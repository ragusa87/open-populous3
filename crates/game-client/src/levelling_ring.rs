//! A reincarnation site's levelling sparkles (pop3-rev-analysis.md "Levelling"): 32 gold sparkles evenly round the
//! site, 320 world units out, each turn stepping 91/2048 of a turn round it and 160 outward (up to 2048, 4 cells),
//! each dropping a small sparkle where it stands that twinkles out over 6 turns. A world view (`views::WorldView`)
//! of each shaman levelling her site (`SiteBuildState::Levelling`), worked out from how far the levelling is, with
//! the levelling's star sprite (`sparkles.rs`). Their height over the ground is ours.

use crate::camera::{CameraRig, CurveParamsRes};
use crate::grounded::{render_pos, Grounded};
use crate::sim_time::SimNow;
use crate::sparkles::{mix, twinkle_at, SparkleAssets, LIFT, SIZE};
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::map::GameMap;
use game_core::site::SiteBuildState;
use game_core::time::Ticks;
use game_core::unit::{Action, Unit};

const SPARKLES: u32 = 32;
/// Turn round the site per step, 2048ths.
const STEP_ANGLE: u32 = 91;
/// First ring radius, growth a step and the cap, world units.
const START_RADIUS: f32 = 320.0;
const GROWTH: f32 = 160.0;
const MAX_RADIUS: f32 = 2048.0;
/// A small sparkle's life, turns.
const SMALL_LIFE: u32 = 6;
/// The big sparkles' twinkle loop (the original's 16 frames, one a drawn frame).
const TWINKLE: Ticks = Ticks::millis(500);
/// Over the ground, render units (ours).
const HEIGHT: f32 = 0.3;

/// Where sparkle `k` stands `step` turns (fractional) into the levelling: its angle (radians) and distance from the
/// site (cells).
pub fn ring_point(k: u32, step: f32) -> (f32, f32) {
    let angle = (k * (2048 / SPARKLES)) as f32 + STEP_ANGLE as f32 * step;
    let radius = (START_RADIUS + GROWTH * step).min(MAX_RADIUS);
    (angle * std::f32::consts::TAU / 2048.0, radius / 512.0)
}

/// A small sparkle's size (cells), `age` turns (fractional) after it was dropped: it twinkles out.
pub fn small_size(age: f32) -> f32 {
    let p = (age / SMALL_LIFE as f32).clamp(0.0, 1.0);
    SIZE * 0.6 * (1.0 - p)
}

/// The levelling's turns done, when `unit` is levelling its site.
fn levelling(unit: &Unit) -> Option<u32> {
    match unit.action {
        Action::BuildingSite(SiteBuildState::Levelling(left)) => Some(left.done().get()),
        _ => None,
    }
}

#[derive(Component)]
pub struct LevellingRingView {
    id: u32,
    entity: Entity,
}

#[derive(Component)]
pub struct LevellingStar;

impl WorldView for LevellingRingView {
    type Model = Unit;
    type Params = (
        Commands<'static, 'static>,
        Res<'static, CurrentMap>,
        Res<'static, CameraRig>,
        Res<'static, CurveParamsRes>,
        Res<'static, SparkleAssets>,
        Query<'static, 'static, &'static Children>,
        Query<'static, 'static, &'static mut Transform, With<LevellingStar>>,
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &Unit> {
        map.units.iter().filter(|u| levelling(u).is_some())
    }

    fn id(unit: &Unit) -> u32 {
        unit.id.raw()
    }

    fn spawn(unit: &Unit, view: &mut EntityCommands, (commands, _, _, _, assets, _, _): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        view.insert((Transform::default(), Visibility::Visible));
        let entity = view.id();
        for _ in 0..SPARKLES * (1 + SMALL_LIFE) {
            commands.spawn((LevellingStar, Mesh3d(assets.quad.clone()), MeshMaterial3d(assets.material.clone()), Transform::from_scale(Vec3::ZERO), NotShadowCaster, crate::hover::NoOutline, ChildOf(entity)));
        }
        Some(LevellingRingView { id: unit.id.raw(), entity })
    }

    fn update(&self, unit: &Unit, now: SimNow, (_, map, rig, params, _, children, stars): &mut SystemParamItem<Self::Params>) {
        let Some(done) = levelling(unit) else { return };
        let Some(site) = map.0.sites.iter().find(|s| unit.owner.tribe() == Some(s.owner)) else { return };
        let terrain = &map.0.terrain;
        let centre = Vec2::new(site.x as f32, site.z as f32) / 512.0;
        let step = done as f32 + now.frac;
        let place = |k: u32, at_step: f32| {
            let (angle, r) = ring_point(k, at_step);
            render_pos(terrain, &Grounded { at: centre + Vec2::from_angle(angle) * r, half: 0.0 }, rig.focus, &params.0)
        };
        let mut poses: Vec<(Vec3, f32)> = Vec::new();
        for k in 0..SPARKLES {
            let size = twinkle_at(mix(self.id ^ mix(k)), TWINKLE, now).size.max(SIZE * 0.35);
            poses.extend(place(k, step).map(|p| (p + Vec3::Y * (LIFT + HEIGHT), size)));
            for back in 0..SMALL_LIFE.min(done + 1) {
                let dropped = done - back;
                poses.extend(place(k, dropped as f32).map(|p| (p + Vec3::Y * (LIFT + HEIGHT), small_size(back as f32 + now.frac))));
            }
        }
        let facing = Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch);
        let have = children.get(self.entity).map(|c| c.to_vec()).unwrap_or_default();
        for (k, &star) in have.iter().enumerate() {
            let Ok(mut t) = stars.get_mut(star) else { continue };
            *t = match poses.get(k) {
                Some(&(at, size)) => Transform { translation: at, rotation: facing, scale: Vec3::splat(size) },
                None => Transform::from_scale(Vec3::ZERO),
            };
        }
    }
}

pub struct LevellingRingPlugin;

impl Plugin for LevellingRingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_views::<LevellingRingView>.after(crate::units::run_ticks));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_turns_16_degrees_and_widens_160_a_turn_to_4_cells() {
        let (a0, r0) = ring_point(0, 0.0);
        let (a1, r1) = ring_point(0, 1.0);
        assert_eq!((a0, r0), (0.0, 320.0 / 512.0));
        assert!(((a1 - a0).to_degrees() - 91.0 * 360.0 / 2048.0).abs() < 1e-3, "16 degrees");
        assert_eq!(r1, 480.0 / 512.0);
        assert_eq!(ring_point(0, 20.0).1, 4.0, "capped at 2048");
        let (b, _) = ring_point(1, 0.0);
        assert!((b.to_degrees() - 360.0 / 32.0).abs() < 1e-3, "32 evenly round");
    }

    #[test]
    fn a_small_sparkle_twinkles_out_over_6_turns() {
        assert!(small_size(0.0) > small_size(3.0));
        assert_eq!(small_size(6.0), 0.0);
    }
}
