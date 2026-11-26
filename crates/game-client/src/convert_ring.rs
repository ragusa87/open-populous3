//! Convert's ring (pop3-rev-analysis.md "Convert wild"): every turn of the effect, 4 gold sparkles at random angles
//! on a circle of 2.5 cells around the cast cell's centre, each twinkling in place 4-7 turns. A world view
//! (`views::WorldView`) of each Convert effect; the sparkles are worked out from the effect's age and id, the
//! levelling's star sprite (`sparkles.rs`).

use crate::camera::{CameraRig, CurveParamsRes};
use crate::grounded::{render_pos, Grounded};
use crate::sim_time::SimNow;
use crate::sparkles::{mix, SparkleAssets, LIFT, SIZE};
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::effect::{Effect, EffectKind};
use game_core::id::Id;
use game_core::map::GameMap;

/// New sparkles a turn.
const PER_TURN: u32 = 4;
/// A sparkle lives `MIN_LIFE` + 0..=3 turns.
const MIN_LIFE: u32 = 4;
const MAX_LIFE: u32 = MIN_LIFE + 3;
/// The circle's radius, cells (1280 world units).
const RADIUS: f32 = 2.5;

/// One sparkle: where on the circle (radians), the turn of the effect it was born on, and its life in turns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RingSparkle {
    pub angle: f32,
    pub born: u32,
    pub life: u32,
}

/// The sparkles of the effect `id` alive on its turn `age`.
pub fn alive(id: u32, age: u32) -> Vec<RingSparkle> {
    (age.saturating_sub(MAX_LIFE - 1).max(1)..=age)
        .flat_map(|born| {
            (0..PER_TURN).map(move |k| {
                let key = mix(id ^ mix(born ^ mix(k)));
                RingSparkle { angle: (key & 2047) as f32 * std::f32::consts::TAU / 2048.0, born, life: MIN_LIFE + (key >> 11 & 3) }
            })
        })
        .filter(|s| age - s.born < s.life)
        .collect()
}

/// A sparkle's size (cells) `age` turns plus `frac` into the effect: it grows and shrinks over its life.
pub fn size(s: &RingSparkle, age: u32, frac: f32) -> f32 {
    let p = (((age - s.born) as f32 + frac) / s.life as f32).clamp(0.0, 1.0);
    let glow = (std::f32::consts::PI * p).sin();
    SIZE * glow * glow
}

#[derive(Component)]
pub struct ConvertRingView {
    id: u32,
    entity: Entity,
}

#[derive(Component)]
pub struct RingStar;

impl WorldView for ConvertRingView {
    type Model = (Id<Effect>, Effect);
    type Params = (
        Commands<'static, 'static>,
        Res<'static, CurrentMap>,
        Res<'static, CameraRig>,
        Res<'static, CurveParamsRes>,
        Res<'static, SparkleAssets>,
        Query<'static, 'static, &'static Children>,
        Query<'static, 'static, &'static mut Transform, With<RingStar>>,
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| matches!(e.kind, EffectKind::Convert(_)))
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((id, _): &(Id<Effect>, Effect), view: &mut EntityCommands, (commands, _, _, _, assets, _, _): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        view.insert((Transform::default(), Visibility::Visible));
        let entity = view.id();
        for _ in 0..PER_TURN * MAX_LIFE {
            commands.spawn((RingStar, Mesh3d(assets.quad.clone()), MeshMaterial3d(assets.material.clone()), Transform::from_scale(Vec3::ZERO), NotShadowCaster, crate::hover::NoOutline, ChildOf(entity)));
        }
        Some(ConvertRingView { id: id.raw(), entity })
    }

    fn update(&self, (_, effect): &(Id<Effect>, Effect), now: SimNow, (_, map, rig, params, _, children, stars): &mut SystemParamItem<Self::Params>) {
        let EffectKind::Convert(convert) = &effect.kind else { return };
        let terrain = &map.0.terrain;
        let age = convert.age();
        let sparkles = alive(self.id, age);
        let centre = Vec2::new((effect.x / 512) as f32 + 0.5, (effect.z / 512) as f32 + 0.5);
        let facing = Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch);
        let have = children.get(self.entity).map(|c| c.to_vec()).unwrap_or_default();
        for (k, &star) in have.iter().enumerate() {
            let Ok(mut t) = stars.get_mut(star) else { continue };
            let pose = sparkles.get(k).and_then(|s| {
                let at = centre + Vec2::from_angle(s.angle) * RADIUS;
                render_pos(terrain, &Grounded { at, half: 0.0 }, rig.focus, &params.0).map(|ground| (ground + Vec3::Y * LIFT, size(s, age, now.frac)))
            });
            *t = match pose {
                Some((at, size)) => Transform { translation: at, rotation: facing, scale: Vec3::splat(size) },
                None => Transform::from_scale(Vec3::ZERO),
            };
        }
    }
}

pub struct ConvertRingPlugin;

impl Plugin for ConvertRingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_views::<ConvertRingView>.after(crate::units::run_ticks));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_new_sparkles_a_turn_each_living_4_to_7_turns() {
        assert!(alive(9, 0).is_empty(), "none before its first turn");
        assert_eq!(alive(9, 1).len(), 4);
        for age in 1..40 {
            let now = alive(9, age);
            assert_eq!(now.iter().filter(|s| s.born == age).count(), 4);
            assert!(now.iter().all(|s| (MIN_LIFE..=MAX_LIFE).contains(&s.life) && age - s.born < s.life));
            assert!(now.len() <= (PER_TURN * MAX_LIFE) as usize);
        }
        let lives: std::collections::BTreeSet<u32> = (1..60).flat_map(|a| alive(9, a)).map(|s| s.life).collect();
        assert_eq!(lives.len(), 4, "all four lives drawn: {lives:?}");
        assert_eq!(alive(9, 20), alive(9, 20), "the same every frame");
        assert_ne!(alive(9, 20), alive(10, 20), "another effect, other angles");
    }

    #[test]
    fn a_sparkle_grows_then_shrinks_away() {
        let s = RingSparkle { angle: 0.0, born: 5, life: 4 };
        assert!(size(&s, 5, 0.0) < 0.01);
        assert!(size(&s, 7, 0.0) > SIZE * 0.9);
        assert!(size(&s, 8, 0.99) < SIZE * 0.1);
    }
}
