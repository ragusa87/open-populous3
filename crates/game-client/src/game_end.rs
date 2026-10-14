//! The match's end (`docs/specs/outcome.md`): once `GameMap::outcome` is set (automatic elimination, or
//! Armageddon), a short hold lets the final blow finish playing, then a dialog takes over (`AppState::Ended`),
//! reusing the exact freeze mechanism `AppState::Paused` already gets for free (everything in `menu::Gameplay`
//! stops). Dismissing it leaves for the main menu, the same `menu::leave_to_menu` the pause menu uses.

use crate::menu::{leave_to_menu, AppState, GameSetup, MenuNav, DIM, HIGHLIGHT, ITEM, TEXT};
use crate::sim_time::SimNow;
use crate::units::{SimClock, PLAYER};
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::outcome::Reason;
use game_core::time::Tick;

/// How the local player should read the result: did their tribe win, lose, or is there no local tribe to judge it
/// by yet (a future spectator case — `game-net` has no such concept today).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Victory,
    Defeat,
    Draw,
    Announced(u8),
}

/// `local: None` is the future spectator case; today the client always passes `Some(units::PLAYER)`. Compares via
/// `same_team`, not equality, so a tribe whose ally actually won is shown `Victory`, not `Defeat`.
pub fn verdict(winner: Option<u8>, local: Option<u8>, same_team: impl Fn(u8, u8) -> bool) -> Verdict {
    match (winner, local) {
        (None, _) => Verdict::Draw,
        (Some(w), None) => Verdict::Announced(w),
        (Some(w), Some(me)) if same_team(w, me) => Verdict::Victory,
        (Some(_), Some(_)) => Verdict::Defeat,
    }
}

/// What the dialog shows: a headline, why, and every tribe's survivor count (`tribes`, in owner order).
#[derive(Clone, Debug, PartialEq)]
pub struct EndView {
    pub headline: String,
    pub subtext: String,
    pub tribes: Vec<(u8, u16)>,
}

pub fn end_view(verdict: Verdict, reason: Reason, tribes: Vec<(u8, u16)>) -> EndView {
    let headline = match verdict {
        Verdict::Victory => "Victory".to_string(),
        Verdict::Defeat => "Defeat".to_string(),
        Verdict::Draw => "Draw".to_string(),
        Verdict::Announced(tribe) => format!("Tribe {tribe} has won"),
    };
    let subtext = match reason {
        Reason::Eliminated => "Every other tribe has been wiped out.",
        Reason::Armageddon => "Armageddon has settled the match.",
        Reason::Scripted => "The level has ended.",
    }
    .to_string();
    EndView { headline, subtext, tribes }
}

/// Seconds held on the final moment before the dialog appears — long enough for the last blow's animation to
/// finish playing, not a mid-swing cut. The simulation keeps running normally during this window.
const HOLD_SECS: f32 = 2.0;

#[derive(Resource, Default)]
struct EndingWatch {
    game: u32,
    seen: Option<Tick>,
}

fn ready_to_end(seen: Option<Tick>, now: SimNow, hold: f32) -> bool {
    seen.is_some_and(|start| now.secs_since(start) >= hold)
}

#[derive(Component)]
struct EndRoot;
#[derive(Component)]
struct EndHeadline;
#[derive(Component)]
struct EndSubtext;
/// The "Press Enter or click to continue" button: only a click actually on it dismisses the dialog, not anywhere
/// on screen (the backdrop behind it is not itself clickable).
#[derive(Component)]
struct DismissButton;

pub struct GameEndPlugin;

impl Plugin for GameEndPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EndingWatch>()
            .add_systems(Startup, spawn_end_root)
            .add_systems(Update, detect_ending.in_set(crate::menu::Gameplay).run_if(in_state(AppState::Playing)))
            .add_systems(Update, game_end_from_env.in_set(crate::menu::Gameplay).run_if(in_state(AppState::Playing)))
            .add_systems(Update, show_ended.run_if(state_changed::<AppState>))
            .add_systems(Update, (dismiss_keys, dismiss_mouse).run_if(in_state(AppState::Ended)));
    }
}

fn detect_ending(map: Res<CurrentMap>, clock: Res<SimClock>, now: Res<SimNow>, mut watch: ResMut<EndingWatch>, mut state: ResMut<NextState<AppState>>) {
    if watch.game != clock.game {
        *watch = EndingWatch { game: clock.game, seen: None };
    }
    if watch.seen.is_none() {
        watch.seen = map.0.outcome.map(|o| o.at);
    }
    if ready_to_end(watch.seen, *now, HOLD_SECS) {
        state.set(AppState::Ended);
    }
}

fn spawn_end_root(mut commands: Commands, overlay: Single<Entity, With<crate::camera::OverlayCamera>>) {
    commands
        .spawn((
            EndRoot,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(14),
                ..default()
            },
            BackgroundColor(DIM),
            GlobalZIndex(i32::MAX - 10),
            UiTargetCamera(*overlay),
            Visibility::Hidden,
        ))
        .with_children(|root| {
            root.spawn((EndHeadline, Text::new(""), TextFont { font_size: FontSize::Px(44.0), ..default() }, TextColor(TEXT)));
            root.spawn((EndSubtext, Text::new(""), TextFont { font_size: FontSize::Px(16.0), ..default() }, TextColor(TEXT.with_alpha(0.8))));
            root.spawn((
                DismissButton,
                Button,
                Node { margin: UiRect::top(px(10)), padding: UiRect::axes(px(16), px(8)), border: UiRect::all(px(2)), ..default() },
                BackgroundColor(ITEM),
                BorderColor::all(HIGHLIGHT),
            ))
            .with_child((Text::new("Press Enter or click to continue"), TextFont { font_size: FontSize::Px(13.0), ..default() }, TextColor(TEXT)));
        });
}

fn show_ended(state: Res<State<AppState>>, map: Res<CurrentMap>, mut root: Query<&mut Visibility, With<EndRoot>>, mut headline: Query<&mut Text, (With<EndHeadline>, Without<EndSubtext>)>, mut subtext: Query<&mut Text, With<EndSubtext>>) {
    let Ok(mut vis) = root.single_mut() else { return };
    if *state.get() != AppState::Ended {
        *vis = Visibility::Hidden;
        return;
    }
    *vis = Visibility::Inherited;
    let Some(outcome) = map.0.outcome else { return };
    let v = verdict(outcome.winner, Some(PLAYER), |a, b| map.0.tribes.same_team(a, b));
    let tribes: Vec<(u8, u16)> = map.0.tribes.all().map(|t| (t, map.0.headcount(t).total())).collect();
    let view = end_view(v, outcome.reason, tribes);
    if let Ok(mut t) = headline.single_mut() {
        t.0 = view.headline;
    }
    if let Ok(mut t) = subtext.single_mut() {
        t.0 = view.subtext;
    }
}

fn dismiss_keys(keys: Res<ButtonInput<KeyCode>>, mut game: GameSetup, mut nav: ResMut<MenuNav>, mut state: ResMut<NextState<AppState>>) {
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space, KeyCode::Escape]) {
        leave_to_menu(&mut game, &mut nav, &mut state);
    }
}

fn dismiss_mouse(buttons: Query<&Interaction, (With<DismissButton>, Changed<Interaction>)>, mut game: GameSetup, mut nav: ResMut<MenuNav>, mut state: ResMut<NextState<AppState>>) {
    if buttons.iter().any(|i| *i == Interaction::Pressed) {
        leave_to_menu(&mut game, &mut nav, &mut state);
    }
}

/// Dev: `GAME_END=victory|defeat|draw` jumps straight to the dialog with a synthetic verdict, for quick layout
/// review (same `Local<bool>`-guarded shape as `menu::menu_from_env`); `GAME_END=eliminate:<tribe>`
/// (+`GAME_END_TICKS=N`) is the more realistic hook — empties that tribe's units directly and runs N ticks, so
/// game-core's own `check_elimination` produces a *real* `Outcome`, exercising the whole pipeline.
fn game_end_from_env(mut done: Local<bool>, mut map: ResMut<CurrentMap>, mut state: ResMut<NextState<AppState>>) {
    if std::mem::replace(&mut *done, true) {
        return;
    }
    let Ok(value) = std::env::var("GAME_END") else { return };
    if let Some(tribe) = value.strip_prefix("eliminate:").and_then(|t| t.parse::<u8>().ok()) {
        map.0.units.retain(|u| u.owner != tribe);
        let ticks: u32 = std::env::var("GAME_END_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
        map.0.run(game_core::time::Ticks::new(ticks));
        return;
    }
    let winner = match value.as_str() {
        "victory" => Some(PLAYER),
        "defeat" => Some(PLAYER.wrapping_add(1)),
        "draw" => None,
        _ => return,
    };
    map.0.declare_outcome(winner, Reason::Eliminated);
    state.set(AppState::Ended);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_compares_by_team_not_equality() {
        assert_eq!(verdict(Some(0), Some(0), |a, b| a == b), Verdict::Victory);
        assert_eq!(verdict(Some(1), Some(0), |a, b| a == b || (a, b) == (1, 0)), Verdict::Victory, "an ally's win still counts");
        assert_eq!(verdict(Some(1), Some(0), |a, b| a == b), Verdict::Defeat);
        assert_eq!(verdict(None, Some(0), |a, b| a == b), Verdict::Draw);
        assert_eq!(verdict(Some(2), None, |_, _| false), Verdict::Announced(2));
    }

    #[test]
    fn end_view_names_the_reason_and_carries_every_tribes_count() {
        let view = end_view(Verdict::Victory, Reason::Armageddon, vec![(0, 5), (1, 0)]);
        assert_eq!(view.headline, "Victory");
        assert!(view.subtext.contains("Armageddon"));
        assert_eq!(view.tribes, vec![(0, 5), (1, 0)]);
    }

    #[test]
    fn ready_to_end_waits_for_the_hold() {
        let start = Tick::from_wire(0);
        let at = |t: u32| SimNow { tick: Tick::from_wire(t), frac: 0.0 };
        assert!(!ready_to_end(Some(start), at(0), HOLD_SECS));
        assert!(!ready_to_end(None, at(1_000), HOLD_SECS));
        let ticks_for_hold = (HOLD_SECS / crate::sim_time::TICK_SECS).ceil() as u32;
        assert!(ready_to_end(Some(start), at(ticks_for_hold), HOLD_SECS));
    }
}
