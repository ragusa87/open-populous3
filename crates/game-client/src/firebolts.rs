//! Firebolts in flight (`game_core::firebolt`), a world view (`views::WorldView`) of each: a small glowing ball at the
//! bolt's place and height. Our own look.

use crate::camera::CurveParamsRes;
use crate::grounded::Grounded;
use crate::sim_time::SimNow;
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::firebolt::{Firebolt, FireboltId};
use game_core::map::GameMap;
use pop3_format::WORLD_UNITS_PER_CELL;

/// The ball's radius, cells, and its colour.
const RADIUS: f32 = 0.09;
const GLOW: Color = Color::srgb(1.0, 0.75, 0.25);

#[derive(Resource)]
pub struct BoltLook {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

/// One bolt's view: the view itself (moved along the ground) and its ball (lifted to the bolt's height).
#[derive(Component)]
pub struct FireboltView {
    me: Entity,
    ball: Entity,
}

/// A bolt's place in cells.
pub fn cells_at(b: &Firebolt) -> Vec2 {
    Vec2::new(b.at.0 as f32, b.at.1 as f32) / WORLD_UNITS_PER_CELL as f32
}

/// How high over the ground under it the bolt flies, in render units.
pub fn lift(map: &GameMap, b: &Firebolt, height_scale: f32) -> f32 {
    let ground = map.terrain.height_at(b.at.0 as u32, b.at.1 as u32, WORLD_UNITS_PER_CELL);
    (b.height - ground).max(0) as f32 * height_scale
}

impl WorldView for FireboltView {
    type Model = (FireboltId, Firebolt);
    type Params = (
        Res<'static, CurrentMap>,
        Res<'static, CurveParamsRes>,
        Res<'static, BoltLook>,
        Query<'static, 'static, &'static mut Grounded>,
        Query<'static, 'static, &'static mut Transform, Without<Grounded>>,
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &(FireboltId, Firebolt)> {
        map.bolts.entries()
    }

    fn id((id, _): &(FireboltId, Firebolt)) -> u32 {
        id.raw()
    }

    fn spawn((_, b): &(FireboltId, Firebolt), view: &mut EntityCommands, (map, params, look, ..): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let me = view.id();
        view.insert((Grounded { at: cells_at(b), half: 0.0 }, Transform::default(), Visibility::Visible));
        let mut ball = Entity::PLACEHOLDER;
        let up = lift(&map.0, b, params.0.height_scale);
        view.with_children(|v| {
            ball = v.spawn((Mesh3d(look.mesh.clone()), MeshMaterial3d(look.material.clone()), NotShadowCaster, crate::hover::NoOutline, Transform::from_xyz(0.0, up, 0.0))).id();
        });
        Some(FireboltView { me, ball })
    }

    fn update(&self, (_, b): &(FireboltId, Firebolt), _now: SimNow, (map, params, _, grounded, transforms): &mut SystemParamItem<Self::Params>) {
        if let Ok(mut g) = grounded.get_mut(self.me) {
            g.at = cells_at(b);
        }
        if let Ok(mut t) = transforms.get_mut(self.ball) {
            t.translation.y = lift(&map.0, b, params.0.height_scale);
        }
    }
}

pub struct FireboltsPlugin;

impl Plugin for FireboltsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_look).add_systems(Update, sync_views::<FireboltView>.after(crate::units::run_ticks));
    }
}

fn load_look(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let material = mats.add(StandardMaterial { base_color: GLOW, emissive: LinearRgba::from(GLOW) * 4.0, unlit: true, ..default() });
    commands.insert_resource(BoltLook { mesh: meshes.add(Sphere::new(RADIUS)), material });
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::unit::Target;

    #[test]
    fn the_ball_flies_over_the_ground_never_under_it() {
        let map = GameMap::sandbox_units();
        let b = Firebolt { owner: game_core::tribes::Owner::Tribe(game_core::tribes::TribeId::Blue), thrower: game_core::id::Id::from_raw(1), target: Target::Unit(game_core::id::Id::from_raw(2)), at: (1024, 512), height: 80, aim: (0, 0, 0), hits: true, bloodlusted: false, from_tower: false };
        assert_eq!(cells_at(&b), Vec2::new(2.0, 1.0));
        let ground = map.terrain.height_at(1024, 512, WORLD_UNITS_PER_CELL);
        assert_eq!(lift(&map, &b, 0.5), ((80 - ground).max(0)) as f32 * 0.5);
        assert_eq!(lift(&map, &Firebolt { height: ground - 10, ..b }, 0.5), 0.0);
    }
}
