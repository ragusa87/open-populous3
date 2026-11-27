//! The discovery cinematic (pop3-rev-analysis.md "Discovery cinematic: the reward flies to the panel"): a player's
//! pending discovery (`game_core::discovery`) flies from the vault to its panel button, from its start turn to the
//! turn it lands. The exe counts drawn frames; ours spreads its frames over the flight's turns, on `SimNow`, so it
//! stops while paused and arrives on the landing turn.
//! A spell: its panel glyph flies to the middle of the view growing to 8× and turning, then onto its tile. A building:
//! `discovery_building`.

use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::grounded::ground_point;
use crate::hud::{ActiveTab, PANEL_WIDTH};
use crate::sim_time::{SimNow, TICK_SECS};
use crate::units::PLAYER;
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::building::Reward;
use game_core::discovery::{flight, Discovery};
use game_core::spell_book::SpellKind;
use game_core::time::Tick;

/// The spell flight's stages, drawn frames [exe]: aim, to the middle, a pause; then to the panel [ours: fixed, the
/// exe's runs until it reaches the button].
const SPELL_AIM: u32 = 1;
const SPELL_TO_MIDDLE: u32 = 13;
const SPELL_PAUSE: u32 = 4;
const SPELL_TO_PANEL: u32 = 18;
pub const SPELL_FRAMES: u32 = SPELL_AIM + SPELL_TO_MIDDLE + SPELL_PAUSE + SPELL_TO_PANEL;
/// Scales in 32nds [exe]: at the start and on the button, and in the middle.
const SMALL: f32 = 52.0;
const LARGE: f32 = 256.0;
/// The original's screen height, for its pixel speeds.
const ORIGINAL_HEIGHT: f32 = 480.0;
/// The glyph's height on its tile (pixels), drawn at `SMALL`.
const GLYPH_HEIGHT: f32 = 30.0;
/// The stage-4 speed: from 2 pixels a frame, times this each frame, up to `TOP_SPEED` [exe]; ours spreads that
/// profile over the distance left.
const SPEED_UP: f32 = 1.5;
const TOP_SPEED: f32 = 80.0;

/// The glyph's place (pixels, from the window's top left), scale (1 on the tile) and turn (radians, clockwise).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphPose {
    pub at: Vec2,
    pub scale: f32,
    pub turn: f32,
}

#[derive(Clone, Copy)]
struct Step {
    at: Vec2,
    scale: f32,
    angle: f32,
}

/// The spell flight frame by frame (`SPELL_FRAMES + 1` steps), from `start` to `middle` to `button`; `px` is one of
/// the original's pixels in ours.
fn spell_steps(start: Vec2, middle: Vec2, button: Vec2, px: f32) -> Vec<Step> {
    let mut s = Step { at: start, scale: SMALL, angle: 0.0 };
    let mut steps = vec![s];
    let mut spin = 0.0;
    for _ in 0..SPELL_AIM {
        steps.push(s);
    }
    let (dir, d) = ((middle - start).normalize_or_zero(), start.distance(middle));
    for t in 0..SPELL_TO_MIDDLE {
        let speed = if t < 5 { d * 36.0 / 256.0 } else { (d * 36.0 / 256.0 * (11.0 - t as f32) / 7.0).max(2.0 * px) };
        s.at += dir * speed;
        s.scale = SMALL + (LARGE - SMALL) * ((t + 1).min(4) as f32 / 4.0);
        if t < 8 {
            spin = 1934.0 / 8.0;
        } else {
            spin -= 51.0;
        }
        s.angle += spin;
        steps.push(s);
    }
    for _ in 0..SPELL_PAUSE {
        s.at += dir * 2.0 * px;
        spin -= 45.0;
        s.angle += spin;
        steps.push(s);
    }
    let from = s.at;
    let speeds: Vec<f32> = (1..=SPELL_TO_PANEL).map(|t| (2.0 * SPEED_UP.powi(t as i32)).min(TOP_SPEED)).collect();
    let whole: f32 = speeds.iter().sum();
    let mut gone = 0.0;
    for t in 1..=SPELL_TO_PANEL {
        gone += speeds[t as usize - 1];
        s.at = from.lerp(button, gone / whole);
        s.scale = (s.scale - 20.0).max(SMALL);
        spin -= match t {
            1 => 28.0,
            2..=5 => 34.0,
            6 => 17.0,
            _ => 22.0,
        };
        s.angle += spin;
        steps.push(s);
    }
    steps
}

/// The glyph `frame` drawn frames into the flight (between two frames, between their steps).
pub fn spell_pose(frame: f32, start: Vec2, middle: Vec2, button: Vec2, px: f32) -> GlyphPose {
    let steps = spell_steps(start, middle, button, px);
    let f = frame.clamp(0.0, SPELL_FRAMES as f32);
    let (k, a) = (f.floor() as usize, f.fract());
    let (s0, s1) = (steps[k], steps[(k + 1).min(steps.len() - 1)]);
    GlyphPose {
        at: s0.at.lerp(s1.at, a),
        scale: (s0.scale + (s1.scale - s0.scale) * a) / SMALL,
        turn: (s0.angle + (s1.angle - s0.angle) * a) / 2048.0 * std::f32::consts::TAU,
    }
}

/// Whether `frame` is in the flight to the panel (the side panel then shows the spells).
pub fn to_the_panel(frame: f32) -> bool {
    frame >= (SPELL_AIM + SPELL_TO_MIDDLE + SPELL_PAUSE) as f32
}

/// Drawn frames `now` into discovery `d`'s cinematic of `frames` frames, None before it starts or once it landed.
pub fn frame_of(d: &Discovery, now: SimNow, frames: u32) -> Option<f32> {
    let length = flight(d.reward)?.get() as f32 * TICK_SECS;
    let secs = now.secs_since(d.started());
    (secs >= 0.0 && secs < length).then(|| secs / length * frames as f32)
}

/// A cinematic on screen: the discovery it shows (its grant turn and reward) and where it started.
#[derive(Component)]
struct Flight {
    granted: Tick,
    reward: Reward,
    start: Vec2,
}

pub struct DiscoveryPlugin;

impl Plugin for DiscoveryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (start_and_end, fly_spells).chain().after(crate::units::SimStep));
    }
}

fn spell_of(reward: Reward) -> Option<SpellKind> {
    match reward {
        Reward::Spell(k) | Reward::OneShot(k) => Some(k),
        _ => None,
    }
}

/// Where a discovery's reward starts on screen (logical pixels): over the vault (its icon's height) when that is in
/// view, else the middle of the side panel (the exe's falls back on a panel widget, which one untraced).
#[derive(bevy::ecs::system::SystemParam)]
pub struct StartPoints<'w, 's> {
    map: Res<'w, CurrentMap>,
    rig: Res<'w, CameraRig>,
    params: Res<'w, CurveParamsRes>,
    camera: Query<'w, 's, (&'static Camera, &'static GlobalTransform), With<GameCamera>>,
}

impl StartPoints<'_, '_> {
    pub fn of(&self, d: &Discovery) -> Option<Vec2> {
        let (cam, cam_t) = self.camera.single().ok()?;
        let size = cam.logical_viewport_size()?;
        let at = Vec2::new(d.from.0 as f32, d.from.1 as f32) / pop3_format::WORLD_UNITS_PER_CELL as f32;
        let over = ground_point(&self.map.0.terrain, self.rig.focus, &self.params.0, at) + Vec3::Y * crate::vault_icon::ICON_CELLS;
        let seen = cam.world_to_viewport(cam_t, over).ok().filter(|p| p.x >= PANEL_WIDTH && p.x <= size.x && p.y >= 0.0 && p.y <= size.y);
        Some(seen.unwrap_or(Vec2::new(PANEL_WIDTH / 2.0, size.y / 2.0)))
    }
}

fn start_and_end(
    mut commands: Commands,
    map: Res<CurrentMap>,
    now: Res<SimNow>,
    flights: Query<(Entity, &Flight)>,
    starts: StartPoints,
    mut glyphs: crate::hud::spells::FlyingGlyphs,
    mut sounds: MessageWriter<crate::audio::PlaySound>,
) {
    let mine: Vec<&Discovery> = map.0.discoveries.iter().filter(|d| d.owner == PLAYER && frame_of(d, *now, SPELL_FRAMES).is_some()).collect();
    for (e, f) in &flights {
        if !mine.iter().any(|d| d.granted == f.granted && d.reward == f.reward) {
            commands.entity(e).despawn();
        }
    }
    for d in mine {
        if flights.iter().any(|(_, f)| d.granted == f.granted && d.reward == f.reward) {
            continue;
        }
        let Some(kind) = spell_of(d.reward) else { continue };
        let Some(start) = starts.of(d) else { continue };
        let Some((image, color)) = glyphs.glyph(kind) else { continue };
        commands.spawn((
            Flight { granted: d.granted, reward: d.reward, start },
            ImageNode::new(image).with_color(color),
            Node { position_type: PositionType::Absolute, height: px(GLYPH_HEIGHT), ..default() },
            UiTransform::IDENTITY,
            GlobalZIndex(50),
            Pickable::IGNORE,
        ));
        sounds.write(crate::audio::PlaySound::flat(crate::audio::SoundCue::DiscoverySpell));
    }
}

fn fly_spells(
    map: Res<CurrentMap>,
    now: Res<SimNow>,
    mut flights: Query<(&Flight, &mut Node, &mut UiTransform)>,
    tiles: crate::hud::spells::SpellTiles,
    camera: Query<&Camera, With<GameCamera>>,
    mut tab: ResMut<ActiveTab>,
) {
    let Some(size) = camera.single().ok().and_then(|c| c.logical_viewport_size()) else { return };
    let middle = Vec2::new((PANEL_WIDTH + size.x) / 2.0, size.y / 2.0);
    for (f, mut node, mut transform) in &mut flights {
        let Some(kind) = spell_of(f.reward) else { continue };
        let Some(d) = map.0.discoveries.iter().find(|d| d.granted == f.granted && d.reward == f.reward) else { continue };
        let Some(frame) = frame_of(d, *now, SPELL_FRAMES) else { continue };
        if to_the_panel(frame) {
            tab.set_if_neq(ActiveTab(0));
        }
        let button = tiles.centre(kind).unwrap_or(Vec2::new(PANEL_WIDTH / 2.0, size.y / 2.0));
        let pose = spell_pose(frame, f.start, middle, button, size.y / ORIGINAL_HEIGHT);
        node.left = px(pose.at.x - GLYPH_HEIGHT / 2.0);
        node.top = px(pose.at.y - GLYPH_HEIGHT / 2.0);
        *transform = UiTransform { scale: Vec2::splat(pose.scale), rotation: Rot2::radians(pose.turn), ..UiTransform::IDENTITY };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::time::Ticks;
    use game_core::tribes::TribeId;

    const START: Vec2 = Vec2::new(900.0, 500.0);
    const MIDDLE: Vec2 = Vec2::new(700.0, 360.0);
    const BUTTON: Vec2 = Vec2::new(60.0, 300.0);

    #[test]
    fn the_glyph_grows_to_8x_in_the_middle_and_lands_on_its_tile_small() {
        let pose = |f: f32| spell_pose(f, START, MIDDLE, BUTTON, 1.5);
        assert_eq!(pose(0.0).at, START);
        assert_eq!(pose(0.0).scale, 1.0);
        let middle = pose((SPELL_AIM + SPELL_TO_MIDDLE) as f32);
        assert!(middle.at.distance(MIDDLE) < START.distance(MIDDLE) * 0.15, "about the middle, a slight overshoot: {:?}", middle.at);
        assert!((middle.scale - LARGE / SMALL).abs() < 1e-3);
        let end = pose(SPELL_FRAMES as f32);
        assert!(end.at.distance(BUTTON) < 1e-3 && (end.scale - 1.0).abs() < 1e-3);
        assert_ne!(pose(10.0).turn, 0.0, "turning");
    }

    #[test]
    fn it_runs_from_its_start_turn_to_its_landing() {
        let d = Discovery { owner: TribeId::Blue, reward: Reward::Spell(SpellKind::Swarm), from: (0, 0), granted: Tick::ZERO };
        let at = |t: Tick| frame_of(&d, SimNow { tick: t, frac: 0.0 }, SPELL_FRAMES);
        assert_eq!(at(Tick::ZERO), None, "hidden at first");
        assert_eq!(at(d.started()), Some(0.0));
        let half = d.started() + Ticks::new(game_core::discovery::SPELL_FLIGHT.get() / 2);
        assert!(at(half).is_some_and(|f| (f - SPELL_FRAMES as f32 * 7.0 / 15.0).abs() < 1e-3));
        assert_eq!(at(d.lands()), None, "landed");
        assert!(to_the_panel(SPELL_FRAMES as f32 - 1.0) && !to_the_panel(1.0));
    }
}
