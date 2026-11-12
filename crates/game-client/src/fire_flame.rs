//! The single companion fire that stands wherever something catches fire - a burning tree's own base, a
//! burning building's own door (pop3-rev-analysis.md "Ground fire, ignited trees, and a burning building's
//! flame"): one standing flame (`flame`'s crossed boards, not scaled to whatever it burns), continuous
//! drifting ground smoke while it stands, one last ember `EMBER_BEFORE_END` before it goes out, then a brief
//! settling-ash sink before it disappears. Shared by `burning_trees` and `burning_buildings`: each spawns one
//! of these alongside its own view, keyed by the same model id.

use crate::effects::{Curve, Emit, Puff, Puffs};
use crate::flame::{self, Flame};
use crate::original_models::to_mesh;
use crate::sim_time::{ticks_f32, TICK_SECS};
use bevy::ecs::system::EntityCommands;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::time::Ticks;
use pop3_format::WORLD_UNITS_PER_CELL;

/// Exactly 4 turns before a companion fire's own life ends, it spawns one last ember (pop3-rev-analysis.md,
/// confirmed).
pub const EMBER_BEFORE_END: Ticks = Ticks::new(4);
/// How long its settling-ash finish sinks it into the ground before it is gone: `[ours]`, a guess - the trace
/// only confirms "a visible multi-tick sink", not its length.
pub const SETTLE_TICKS: Ticks = Ticks::new(6);
/// Size (cells) of a companion fire's own standing flame: confirmed the SAME size as an ordinary camp fire
/// (pop3-rev-analysis.md "Ground fire, ignited trees, and a burning building's flame", "Flame billboard size")
/// - this is the same `OBJS0` scenery object, not its own sprite.
const SIZE: f32 = crate::campfire::SIZE;
/// How far (cells) its ground smoke drifts from its own position: `[ours]` scale of the trace's own "jittered
/// about 32 units" (~0.06 cells read too small to see against a cell; stretched so it reads on screen).
const SMOKE_SPREAD: f32 = 0.35;

/// Our own invented ground smoke (not traced, `[ours]`): a few puffs drifting out and fading over 2.4s each,
/// forever while the fire stands (cut off by its own `on` flag once the fire's own life ends). Used without
/// the original game's own data loaded (`smoke_pattern`), same spirit as the rest of `--no-original` mode.
pub const FIRE_SMOKE_OURS: Puffs = Puffs {
    count: 4,
    life: 2.4,
    emit: Emit::Loop,
    spread: (0.0, SMOKE_SPREAD),
    spread_curve: Curve::EaseOut,
    rise: (0.1, 1.1),
    size: (0.1, 0.3),
    opacity: (0.45, 0.0),
    curve: Curve::Linear,
    shrink: 0.0,
    angle: (0.0, 2.4),
};

/// How long one traced ground-fire puff itself lasts (pop3-rev-analysis.md "Ground fire, ignited trees, and a
/// burning building's flame", "The ground-fire's own smoke puff": ~0.58s/7-tick, confirmed).
const FIRE_SMOKE_TRACED_PUFF_TICKS: f32 = 7.0;
/// How far (cells) a traced puff rises over its own life: 20 world units/turn (pop3-rev-analysis.md "The shot
/// 8/6", the same effect) over its `FIRE_SMOKE_TRACED_PUFF_TICKS` ticks.
const FIRE_SMOKE_TRACED_RISE: f32 = 20.0 * FIRE_SMOKE_TRACED_PUFF_TICKS / WORLD_UNITS_PER_CELL as f32;

/// The original's own ground-fire smoke, traced (pop3-rev-analysis.md "Ground fire, ignited trees, and a
/// burning building's flame", "The ground-fire's own smoke puff" - effect 7/0x03, the same generic trail puff
/// a firewarrior's bolt and a small volcano fireball trail also use, `pop3-rev`'s `docs/functions.tsv` rows
/// `0x50bf60`/`0x50bd70`/`0x50beb0`, `docs/effect_states.csv` row 3): a brand-new, short puff almost every
/// tick for as long as the fire lives, jittered by a small roughly-constant offset rather than a few puffs
/// that each drift for 2.4s - `Emit::Run` along `SINKING`'s own pattern, `until` the fire's own `visual_secs`
/// life.
fn fire_smoke_traced(visual_secs: f32) -> Puffs {
    Puffs {
        count: (visual_secs / TICK_SECS).round().max(1.0) as usize,
        life: FIRE_SMOKE_TRACED_PUFF_TICKS * TICK_SECS,
        emit: Emit::Run { gap: TICK_SECS, until: visual_secs },
        spread: (0.06, 0.06),
        spread_curve: Curve::Linear,
        rise: (0.0, FIRE_SMOKE_TRACED_RISE),
        size: (0.08, 0.18),
        opacity: (0.5, 0.0),
        curve: Curve::Linear,
        shrink: 0.0,
        angle: (0.0, 2.4),
    }
}

/// A companion fire's own ground smoke, `visual_secs` into its life: the original's own traced shape
/// (`fire_smoke_traced`) with the original game's own data loaded (`original`), our own invented
/// `FIRE_SMOKE_OURS` otherwise.
pub fn smoke_pattern(original: bool, visual_secs: f32) -> Puffs {
    if original {
        fire_smoke_traced(visual_secs)
    } else {
        FIRE_SMOKE_OURS
    }
}

/// The one last ember a companion fire spawns `EMBER_BEFORE_END` before going out: a brighter, quicker puff.
pub const FIRE_EMBER: Puffs = Puffs {
    count: 1,
    life: 0.9,
    emit: Emit::Burst,
    spread: (0.0, 0.15),
    spread_curve: Curve::EaseOut,
    rise: (0.05, 0.6),
    size: (0.12, 0.05),
    opacity: (0.9, 0.0),
    curve: Curve::EaseOut,
    shrink: 0.0,
    angle: (0.0, 0.0),
};

/// Assets every companion fire shares, made once.
#[derive(Resource)]
pub struct CompanionFireAssets {
    flame_board: Handle<Mesh>,
    puff_ball: Handle<Mesh>,
    smoke: Handle<StandardMaterial>,
    ember: Handle<StandardMaterial>,
}

impl CompanionFireAssets {
    /// The flame's crossed boards, for another standing flame of the same look (`firebrands`).
    pub fn flame_board(&self) -> Handle<Mesh> {
        self.flame_board.clone()
    }
}

pub struct FireFlamePlugin;

impl Plugin for FireFlamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets);
    }
}

fn load_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(CompanionFireAssets {
        flame_board: meshes.add(to_mesh(flame::crossed_boards(SIZE))),
        puff_ball: meshes.add(Sphere::new(0.5).mesh().ico(1).unwrap()),
        smoke: mats.add(StandardMaterial { base_color: Color::srgba(0.35, 0.33, 0.3, 0.5), alpha_mode: AlphaMode::Blend, unlit: true, ..default() }),
        ember: mats.add(StandardMaterial { base_color: Color::srgba(1.0, 0.55, 0.15, 0.9), alpha_mode: AlphaMode::Blend, unlit: true, ..default() }),
    });
}

/// The flame's own height (of full height) `secs` into its life, a life of `visual_secs`: full height until
/// its last `settle_secs`-worth of time, then sinking away to nothing by the end - its settling-ash finish
/// (pop3-rev-analysis.md: confirmed to happen, not its own exact curve, so `[ours]`).
pub fn height(secs: f32, visual_secs: f32, settle_secs: f32) -> f32 {
    if secs > visual_secs {
        return 0.0;
    }
    ((visual_secs - secs.max(0.0)) / settle_secs.max(1e-6)).clamp(0.0, 1.0)
}

/// One companion fire's drawn parts: its flame, its smoke puffs, its one ember puff.
pub struct CompanionFire {
    flame: Entity,
    smoke: Vec<Entity>,
    ember: Entity,
}

impl CompanionFire {
    /// Spawns a companion fire's parts as children of `view`, `offset` frames into the flame loop, its own
    /// ground smoke `original` (the original game's own traced shape) or our own invented one, over a life of
    /// `visual_secs`.
    pub fn spawn(view: &mut EntityCommands, assets: &CompanionFireAssets, flame_frame: Handle<StandardMaterial>, offset: usize, original: bool, visual_secs: f32) -> Self {
        let pattern = smoke_pattern(original, visual_secs);
        let (mut flame, mut smoke, mut ember) = (Entity::PLACEHOLDER, Vec::new(), Entity::PLACEHOLDER);
        view.with_children(|v| {
            flame = v.spawn((Flame { offset }, Mesh3d(assets.flame_board.clone()), MeshMaterial3d(flame_frame), NotShadowCaster, crate::hover::NoOutline, Transform::default())).id();
            for k in 0..pattern.count {
                let puff = (Puff::new(pattern, k, Vec3::ZERO), Mesh3d(assets.puff_ball.clone()), MeshMaterial3d(assets.smoke.clone()), NotShadowCaster, crate::hover::NoOutline);
                smoke.push(v.spawn((puff, Transform::default(), Visibility::Hidden)).id());
            }
            ember = v.spawn((Puff::new(FIRE_EMBER, 0, Vec3::ZERO), Mesh3d(assets.puff_ball.clone()), MeshMaterial3d(assets.ember.clone()), NotShadowCaster, crate::hover::NoOutline, Transform::default(), Visibility::Hidden)).id();
        });
        CompanionFire { flame, smoke, ember }
    }

    /// Poses a companion fire `secs` after it was lit, for a visual life of `visual_secs`: standing full
    /// height through most of its life, settling into the ground over its last `SETTLE_TICKS`, its smoke
    /// running throughout, its one ember `EMBER_BEFORE_END` before the end.
    pub fn update(&self, secs: f32, visual_secs: f32, flames: &mut Query<&mut Transform, With<Flame>>, puffs: &mut Query<&mut Puff>) {
        let settle_secs = ticks_f32(SETTLE_TICKS) * TICK_SECS;
        if let Ok(mut t) = flames.get_mut(self.flame) {
            t.scale.y = height(secs, visual_secs, settle_secs);
        }
        let burning = secs <= visual_secs;
        for &p in &self.smoke {
            if let Ok(mut puff) = puffs.get_mut(p) {
                (puff.secs, puff.on) = (secs, burning);
            }
        }
        let ember_start = visual_secs - ticks_f32(EMBER_BEFORE_END) * TICK_SECS;
        if let Ok(mut puff) = puffs.get_mut(self.ember) {
            (puff.secs, puff.on) = (secs - ember_start, secs >= ember_start);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_pattern_switches_between_our_own_loop_and_the_traced_original_run() {
        let visual_secs = 76.0 * TICK_SECS;
        let ours = smoke_pattern(false, visual_secs);
        assert_eq!(ours.emit, Emit::Loop, "our own invented shape, not traced");

        let traced = smoke_pattern(true, visual_secs);
        let Emit::Run { gap, until } = traced.emit else { panic!("the traced shape is Emit::Run") };
        assert!((gap - TICK_SECS).abs() < 1e-6, "a brand-new puff about every tick");
        assert!((until - visual_secs).abs() < 1e-6, "cut off at the fire's own visual life, not a fixed number");
        assert_eq!(traced.count, 76, "one slot per tick of the fire's own life");
        assert!((traced.life - 7.0 * TICK_SECS).abs() < 1e-6, "each puff itself short-lived, ~0.58s");
        assert_eq!(traced.spread, (0.06, 0.06), "near-flat, not ramping outward like our own guess");
    }

    #[test]
    fn traced_smoke_sizes_itself_to_a_different_fire_s_own_life() {
        let (tree, building) = (smoke_pattern(true, 76.0 * TICK_SECS), smoke_pattern(true, 127.0 * TICK_SECS));
        assert_eq!(tree.count, 76);
        assert_eq!(building.count, 127);
    }

    #[test]
    fn full_height_until_the_last_settle_window_then_sinks_to_nothing() {
        assert_eq!(height(-0.1, 10.0, 2.0), 1.0, "clamped to the start");
        assert_eq!(height(0.0, 10.0, 2.0), 1.0);
        assert_eq!(height(7.9, 10.0, 2.0), 1.0, "still before the settle window");
        let mid = height(9.0, 10.0, 2.0);
        assert!(mid > 0.0 && mid < 1.0, "sinking");
        assert_eq!(height(10.0, 10.0, 2.0), 0.0, "fully sunk by the end");
        assert_eq!(height(10.1, 10.0, 2.0), 0.0, "past its life");
    }
}
