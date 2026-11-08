//! The build/rebuild ritual's own shots in flight (`game_core::site::SiteShot`): a small glowing bullet
//! travelling from the shaman to its pillar slot, tethered to her by a thin glowing rope the whole way -
//! our own look (pop3-rev-analysis.md gives the shot's speed, not what it looks like).

use crate::camera::{CameraRig, CurveParamsRes};
use crate::grounded::{render_pos, Grounded};
use crate::sim_time::SimNow;
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::site::SiteShot;
use game_core::tribes::TribeId;
use pop3_format::WORLD_UNITS_PER_CELL;

/// The bullet's radius, and the rope's (both cells), and their shared colour.
const BALL_RADIUS: f32 = 0.09;
const ROPE_THICKNESS: f32 = 0.035;
const GLOW: Color = Color::srgb(0.55, 0.85, 1.0);

#[derive(Resource)]
pub struct SiteShotLook {
    ball: Handle<Mesh>,
    /// A 1×1×1 cuboid at the origin, stretched to the rope's own length and thickness (`rope_transform`).
    rope: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

/// One shot's view: the travelling bullet and the rope tethering it to the shaman, both children of the
/// same (otherwise untransformed) parent.
#[derive(Component)]
pub struct SiteShotView {
    ball: Entity,
    rope: Entity,
}

/// A shot's own place, in cells.
fn cells_at(s: &SiteShot) -> Vec2 {
    Vec2::new(s.at.0 as f32, s.at.1 as f32) / WORLD_UNITS_PER_CELL as f32
}

/// How high over the ground under it something at `(x, z)`/`height` flies, in render units (`firebolts::lift`'s
/// own shape).
fn lift(map: &GameMap, (x, z): (u16, u16), height: i32, height_scale: f32) -> f32 {
    let ground = map.terrain.height_at(x as u32, z as u32, WORLD_UNITS_PER_CELL);
    (height - ground).max(0) as f32 * height_scale
}

/// The shaman of `owner`'s own render-space position (ground plus her own lift), `None` if she's gone or
/// outside the drawn disc.
fn shaman_pos(map: &GameMap, owner: TribeId, rig: &CameraRig, params: &crate::terrain_mesh::CurveParams) -> Option<Vec3> {
    let u = map.shaman_of(owner)?;
    let at = Vec2::new(u.x as f32, u.z as f32) / WORLD_UNITS_PER_CELL as f32;
    let ground = render_pos(&map.terrain, &Grounded { at, half: 0.0 }, rig.focus, params)?;
    Some(ground + Vec3::Y * lift(map, (u.x, u.z), map.altitude(u) as i32, params.height_scale))
}

/// The shot's own render-space position (ground plus its own lift), `None` outside the drawn disc.
fn shot_pos(map: &GameMap, s: &SiteShot, rig: &CameraRig, params: &crate::terrain_mesh::CurveParams) -> Option<Vec3> {
    let ground = render_pos(&map.terrain, &Grounded { at: cells_at(s), half: 0.0 }, rig.focus, params)?;
    Some(ground + Vec3::Y * lift(map, s.at, s.height, params.height_scale))
}

/// A `Transform` stretching the shared unit rope mesh (a 1×1×1 cuboid at the origin, long along Z) to span
/// `a` to `b`, `thickness` wide on the other two axes.
pub fn rope_transform(a: Vec3, b: Vec3, thickness: f32) -> Transform {
    let delta = b - a;
    let len = delta.length();
    let axis = if len > 1e-5 { delta / len } else { Vec3::Z };
    Transform { translation: (a + b) / 2.0, rotation: Quat::from_rotation_arc(Vec3::Z, axis), scale: Vec3::new(thickness, thickness, len) }
}

impl WorldView for SiteShotView {
    type Model = (Id<SiteShot>, SiteShot);
    type Params = (Res<'static, CurrentMap>, Res<'static, CameraRig>, Res<'static, CurveParamsRes>, Res<'static, SiteShotLook>, Query<'static, 'static, &'static mut Transform>);

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<SiteShot>, SiteShot)> {
        map.site_shots.entries()
    }

    fn id((id, _): &(Id<SiteShot>, SiteShot)) -> u32 {
        id.raw()
    }

    fn spawn(_: &(Id<SiteShot>, SiteShot), view: &mut EntityCommands, (.., look, _): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        view.insert((Transform::default(), Visibility::Visible));
        let (mut ball, mut rope) = (Entity::PLACEHOLDER, Entity::PLACEHOLDER);
        view.with_children(|v| {
            rope = v.spawn((Mesh3d(look.rope.clone()), MeshMaterial3d(look.material.clone()), NotShadowCaster, crate::hover::NoOutline, Transform::from_scale(Vec3::ZERO))).id();
            ball = v.spawn((Mesh3d(look.ball.clone()), MeshMaterial3d(look.material.clone()), NotShadowCaster, crate::hover::NoOutline, Transform::from_scale(Vec3::ZERO))).id();
        });
        Some(SiteShotView { ball, rope })
    }

    fn update(&self, (_, s): &(Id<SiteShot>, SiteShot), _now: SimNow, (map, rig, params, _, transforms): &mut SystemParamItem<Self::Params>) {
        let (origin, bullet) = (shaman_pos(&map.0, s.owner, rig, &params.0), shot_pos(&map.0, s, rig, &params.0));
        if let Ok(mut t) = transforms.get_mut(self.rope) {
            *t = match (origin, bullet) {
                (Some(a), Some(b)) => rope_transform(a, b, ROPE_THICKNESS),
                _ => Transform::from_scale(Vec3::ZERO),
            };
        }
        if let Ok(mut t) = transforms.get_mut(self.ball) {
            *t = match bullet {
                Some(at) => Transform::from_translation(at),
                None => Transform::from_scale(Vec3::ZERO),
            };
        }
    }
}

pub struct SiteShotPlugin;

impl Plugin for SiteShotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_look).add_systems(Update, sync_views::<SiteShotView>.after(crate::units::run_ticks));
    }
}

fn load_look(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let material = mats.add(StandardMaterial { base_color: GLOW, emissive: LinearRgba::from(GLOW) * 4.0, unlit: true, ..default() });
    commands.insert_resource(SiteShotLook { ball: meshes.add(Sphere::new(BALL_RADIUS)), rope: meshes.add(Cuboid::from_size(Vec3::ONE)), material });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rope_spans_its_two_ends_and_collapses_at_zero_length() {
        let (a, b) = (Vec3::new(0.0, 1.0, 0.0), Vec3::new(4.0, 1.0, 0.0));
        let t = rope_transform(a, b, 0.1);
        assert_eq!(t.translation, Vec3::new(2.0, 1.0, 0.0));
        assert_eq!(t.scale, Vec3::new(0.1, 0.1, 4.0));
        assert!((t.rotation * Vec3::Z).distance((b - a).normalize()) < 1e-5);
        let same = rope_transform(a, a, 0.1);
        assert_eq!(same.scale.z, 0.0);
    }

    #[test]
    fn lift_never_goes_below_the_ground() {
        let map = GameMap::sandbox_units();
        let ground = map.terrain.height_at(1024, 512, WORLD_UNITS_PER_CELL);
        assert_eq!(lift(&map, (1024, 512), ground + 80, 0.5), 80.0 * 0.5);
        assert_eq!(lift(&map, (1024, 512), ground - 10, 0.5), 0.0);
    }
}
