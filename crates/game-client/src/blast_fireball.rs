//! Blast's own fireball in flight (`game_core::blast::Fireball`, `GameMap::fireballs`): a small flame flying in a
//! straight line from the shaman to the cast point (TODO.md "Blast": "its trailing-fireball/self-flame visuals
//! (client-side, not simulation)"). A single flickering ball, warmer and a touch bigger than `firebolts.rs`'s own
//! bolt — no sprite traced for it, our own invented look, same spirit as `fireball_shot.rs`'s flame colours.

use crate::camera::CurveParamsRes;
use crate::grounded::Grounded;
use crate::sim_time::{seed, SimNow};
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::blast::{Fireball, FireballId};
use game_core::map::GameMap;
use pop3_format::WORLD_UNITS_PER_CELL;

/// The flame's base size (cells) and how much it pulses, and how fast (full cycles per second): a flicker, not a
/// steady ball.
const RADIUS: f32 = 0.17;
const FLICKER_AMOUNT: f32 = 0.18;
const FLICKER_HZ: f32 = 9.0;
const FLAME_COLOUR: Color = Color::srgb(1.0, 0.55, 0.15);
const GLOW: f32 = 4.5;

/// A fireball's place in cells.
pub fn cells_at(f: &Fireball) -> Vec2 {
    Vec2::new(f.at.0 as f32, f.at.1 as f32) / WORLD_UNITS_PER_CELL as f32
}

/// How high over the ground under it the fireball flies, in render units (same shape as `firebolts::lift`).
pub fn lift(map: &GameMap, f: &Fireball, height_scale: f32) -> f32 {
    let ground = map.terrain.height_at(f.at.0 as u32, f.at.1 as u32, WORLD_UNITS_PER_CELL);
    (f.height - ground).max(0) as f32 * height_scale
}

/// The flame's own scale at `secs`, a seeded flicker so several fireballs don't pulse in lockstep.
pub fn flicker(secs: f32, seed: f32) -> f32 {
    RADIUS * (1.0 + FLICKER_AMOUNT * (secs * FLICKER_HZ * std::f32::consts::TAU + seed * std::f32::consts::TAU).sin())
}

#[derive(Resource)]
pub struct FlameLook {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

/// One fireball's view: the view itself (moved over the ground) and its flame (lifted to the fireball's height).
#[derive(Component)]
pub struct BlastFireballView {
    me: Entity,
    flame: Entity,
    seed: f32,
}

impl WorldView for BlastFireballView {
    type Model = (FireballId, Fireball);
    type Params = (
        Res<'static, CurrentMap>,
        Res<'static, CurveParamsRes>,
        Res<'static, FlameLook>,
        Query<'static, 'static, &'static mut Grounded>,
        Query<'static, 'static, &'static mut Transform, Without<Grounded>>,
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &(FireballId, Fireball)> {
        map.fireballs.entries()
    }

    fn id((id, _): &(FireballId, Fireball)) -> u32 {
        id.raw()
    }

    fn spawn((id, f): &(FireballId, Fireball), view: &mut EntityCommands, (map, params, look, ..): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let me = view.id();
        view.insert((Grounded { at: cells_at(f), half: 0.0 }, Transform::default(), Visibility::Visible));
        let mut flame = Entity::PLACEHOLDER;
        let up = lift(&map.0, f, params.0.height_scale);
        view.with_children(|v| {
            flame = v.spawn((Mesh3d(look.mesh.clone()), MeshMaterial3d(look.material.clone()), NotShadowCaster, crate::hover::NoOutline, Transform::from_xyz(0.0, up, 0.0).with_scale(Vec3::splat(RADIUS)))).id();
        });
        Some(BlastFireballView { me, flame, seed: seed(id.raw()) })
    }

    fn update(&self, (_, f): &(FireballId, Fireball), now: SimNow, (map, params, _, grounded, transforms): &mut SystemParamItem<Self::Params>) {
        if let Ok(mut g) = grounded.get_mut(self.me) {
            g.at = cells_at(f);
        }
        if let Ok(mut t) = transforms.get_mut(self.flame) {
            t.translation.y = lift(&map.0, f, params.0.height_scale);
            t.scale = Vec3::splat(flicker(now.secs_since(game_core::time::Tick::ZERO), self.seed));
        }
    }
}

pub struct BlastFireballPlugin;

impl Plugin for BlastFireballPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_look).add_systems(Update, sync_views::<BlastFireballView>.after(crate::units::run_ticks));
    }
}

fn load_look(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let material = mats.add(StandardMaterial { base_color: FLAME_COLOUR, emissive: LinearRgba::from(FLAME_COLOUR) * GLOW, unlit: true, ..default() });
    commands.insert_resource(FlameLook { mesh: meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap()), material });
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::tribes::TribeId;

    #[test]
    fn the_flame_flies_over_the_ground_never_under_it() {
        let map = GameMap::sandbox_units();
        let f = Fireball { owner: TribeId::Blue.into(), at: (1024, 512), height: 80, aim: (0, 0, 0) };
        assert_eq!(cells_at(&f), Vec2::new(2.0, 1.0));
        let ground = map.terrain.height_at(1024, 512, WORLD_UNITS_PER_CELL);
        assert_eq!(lift(&map, &f, 0.5), ((80 - ground).max(0)) as f32 * 0.5);
        assert_eq!(lift(&map, &Fireball { height: ground - 10, ..f }, 0.5), 0.0);
    }

    #[test]
    fn the_flame_flickers_around_its_base_size_seeded_by_id() {
        let sizes: Vec<f32> = (0..50).map(|i| flicker(i as f32 * 0.1, 0.3)).collect();
        assert!(sizes.iter().any(|&s| s > RADIUS) && sizes.iter().any(|&s| s < RADIUS), "pulses around the base size");
        assert!((sizes.iter().cloned().fold(0.0, f32::max) - RADIUS).abs() <= RADIUS * FLICKER_AMOUNT + 1e-6);
        assert_ne!(flicker(0.05, 0.0), flicker(0.05, 0.3), "a different seed, a different phase");
    }
}
