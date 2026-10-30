//! Shaman preview at the top of the panel: her current sprite (same pose and view as on the
//! map, drawn x2), what she is doing and her health. Clicking it selects her alone and looks at her.

use super::panel::{ShamanPreview, INK};
use crate::camera::CameraRig;
use crate::units::selection::{selectable, Selection};
use crate::units::{health_color, player_shaman_cell, SimClock, UnitSprites, PLAYER};
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::unit::{Action, Unit};

const SCALE: f32 = 2.0;
/// Feet line, from the top of the box.
const BASELINE: f32 = 98.0;
const BAR_WIDTH: f32 = 90.0;

/// What the preview says about the shaman. Pure, so it is unit-tested.
#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    pub title: String,
    pub health: f32,
    pub health_text: String,
}

pub fn status(unit: Option<&Unit>) -> Status {
    let Some(u) = unit else {
        return Status { title: "No shaman".into(), health: 0.0, health_text: String::new() };
    };
    let title = match u.action {
        Action::Dead { left } => format!("Reincarnating in {}s", crate::sim_time::secs_left(left)),
        a => a.name().to_string(),
    };
    Status {
        title,
        health: u.health.current() as f32 / u.health.max() as f32,
        health_text: format!("{}/{}", u.health.current(), u.health.max()),
    }
}

#[derive(Component)]
struct PreviewSprite;
#[derive(Component)]
struct PreviewTitle;
#[derive(Component)]
struct PreviewHealth;
#[derive(Component)]
struct PreviewHealthFill;

pub struct ShamanPreviewPlugin;

impl Plugin for ShamanPreviewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_preview).add_systems(Update, (update_preview, preview_click.in_set(crate::menu::Gameplay)));
    }
}

fn spawn_preview(mut commands: Commands, boxes: Query<Entity, With<ShamanPreview>>) {
    let font = |size| TextFont { font_size: FontSize::Px(size), ..default() };
    for b in &boxes {
        commands.entity(b).with_children(|p| {
            p.spawn((PreviewSprite, ImageNode::default(), Node { position_type: PositionType::Absolute, ..default() }));
            p.spawn((PreviewTitle, Text::new(""), font(12.0), Node { position_type: PositionType::Absolute, left: px(5), top: px(3), ..default() }));
            p.spawn(Node {
                position_type: PositionType::Absolute,
                left: px(5),
                right: px(5),
                bottom: px(4),
                column_gap: px(6),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                row.spawn((Node { width: px(BAR_WIDTH), height: px(7), padding: UiRect::all(px(1)), ..default() }, BackgroundColor(INK)))
                    .with_child((PreviewHealthFill, Node { height: percent(100), width: percent(100), ..default() }, BackgroundColor(health_color(1.0))));
                row.spawn((PreviewHealth, Text::new(""), font(11.0)));
            });
        });
    }
}

#[allow(clippy::type_complexity)]
fn update_preview(
    map: Res<CurrentMap>,
    rig: Res<CameraRig>,
    clock: Res<SimClock>,
    now: Res<crate::sim_time::SimNow>,
    sprites: Res<UnitSprites>,
    boxes: Query<&ComputedNode, With<ShamanPreview>>,
    mut sprite: Query<(&mut ImageNode, &mut Node, &mut Visibility), With<PreviewSprite>>,
    mut title: Query<&mut Text, (With<PreviewTitle>, Without<PreviewHealth>)>,
    mut health: Query<&mut Text, (With<PreviewHealth>, Without<PreviewTitle>)>,
    mut fill: Query<(&mut Node, &mut BackgroundColor), (With<PreviewHealthFill>, Without<PreviewSprite>)>,
) {
    let unit = map.0.shaman_of(PLAYER);
    let s = status(unit);
    let width = boxes.iter().next().map_or(190.0, |c| c.size().x * c.inverse_scale_factor());
    for (mut image, mut node, mut vis) in &mut sprite {
        match unit.and_then(|u| sprites.frame_for(u, None, !u.aboard.and_then(|v| map.0.vehicle(v)).is_some_and(crate::vehicles::under_way), rig.yaw, &clock, *now, map.0.outcome.is_some())) {
            Some(f) => {
                image.image = f.image.clone();
                image.rect = Some(f.rect);
                node.width = px(f.size.x * SCALE);
                node.height = px(f.size.y * SCALE);
                node.left = px(width / 2.0 - f.origin.x * SCALE);
                node.top = px(BASELINE - f.origin.y * SCALE);
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
    for mut t in &mut title {
        t.0.clone_from(&s.title);
    }
    for mut t in &mut health {
        t.0.clone_from(&s.health_text);
    }
    for (mut node, mut bg) in &mut fill {
        node.width = percent(s.health * 100.0);
        bg.0 = health_color(s.health);
    }
}

/// What a click on the preview does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewClick {
    /// Left: selects her alone, the camera stays.
    Select,
    /// Right: the camera flies to her, the selection stays.
    Look,
}

/// The click on the preview, from the buttons pressed this frame (left wins).
pub fn preview_click_of(left: bool, right: bool) -> Option<PreviewClick> {
    if left {
        Some(PreviewClick::Select)
    } else if right {
        Some(PreviewClick::Look)
    } else {
        None
    }
}

fn preview_click(
    q: Query<&Interaction, With<ShamanPreview>>,
    mouse: Res<ButtonInput<MouseButton>>,
    map: Res<CurrentMap>,
    clock: Res<SimClock>,
    mut rig: ResMut<CameraRig>,
    mut selection: ResMut<Selection>,
) {
    if q.iter().all(|i| *i == Interaction::None) {
        return;
    }
    match preview_click_of(mouse.just_pressed(MouseButton::Left), mouse.just_pressed(MouseButton::Right)) {
        Some(PreviewClick::Select) => {
            if let Some(u) = map.0.shaman_of(PLAYER).filter(|u| selectable(u).is_some()) {
                selection.select_only(u.id);
            }
        }
        Some(PreviewClick::Look) => {
            if let Some(cell) = player_shaman_cell(&map.0, &clock) {
                rig.fly_to(cell);
            }
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::site::ReincarnationSite;

    #[test]
    fn left_selects_right_looks() {
        assert_eq!(preview_click_of(true, false), Some(PreviewClick::Select));
        assert_eq!(preview_click_of(false, true), Some(PreviewClick::Look));
        assert_eq!(preview_click_of(true, true), Some(PreviewClick::Select));
        assert_eq!(preview_click_of(false, false), None);
    }

    #[test]
    fn status_shows_action_and_health() {
        let mut u = Unit::shaman(game_core::id::Id::from_raw(1), &ReincarnationSite::at_cell(0, (1, 1), 0));
        u.health = game_core::health::Health::new(800, u.health.max());
        let s = status(Some(&u));
        assert_eq!((s.title.as_str(), s.health, s.health_text.as_str()), ("Idle", 0.4, "800/2000"));
        u.action = Action::Dead { left: game_core::time::Countdown::with_left(game_core::unit::RESPAWN_TICKS, game_core::time::Ticks::new(13)) };
        assert_eq!(status(Some(&u)).title, "Reincarnating in 2s");
        assert_eq!(status(None).title, "No shaman");
    }
}
