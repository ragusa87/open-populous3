//! Volcano's decorative rolling rocks (pop3-rev-analysis.md "Volcano rocks"): one born each turn `FIRST_TURN` to
//! `FIRST_TURN + COUNT` of a Volcano effect (`game_core::volcano`'s own doc: "drawn client-side only ... nothing
//! for the simulation to do" — no `EffectKind` data of its own, everything here is derived from the effect's id
//! and elapsed turns alone, so every client draws the same rocks). Placement matches the trace (a random angle and
//! distance `MIN_DIST`-`MAX_DIST` from the centre, 3-5 cells, the cone's outer slope); motion is our own simplified
//! feel, not the original's per-triangle slope physics (no original art or physics to replicate exactly): each one
//! appears already on the ground (skipping the trace's short, barely-visible vertical drop) and rolls a short
//! distance away from the peak before settling, then fades after `REST_SECS` ("about 10.7 s", the trace's own
//! number for its own rest-before-deletion timer).

use crate::grounded::Grounded;
use crate::sim_time::{seed, SimNow};
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::effect::{Effect, EffectKind};
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::time::Tick;
use pop3_format::WORLD_UNITS_PER_CELL;
use std::f32::consts::TAU;

/// One rock born per turn in this window (pop3-rev-analysis.md "Volcano rocks": turns 105-130, 26 in all).
const FIRST_TURN: u32 = 105;
const COUNT: u32 = 26;
/// Distance from the centre, world units (pop3-rev-analysis.md: `(rand & 0x1FF) * 2 + 0x600`, 1536-2558, 3-5 cells).
const MIN_DIST: f32 = 1536.0;
const MAX_DIST: f32 = 2558.0;
/// How long it rolls away from the peak before settling, and how far (a guess: our own feel).
const ROLL_SECS: f32 = 1.2;
const ROLL_DIST: f32 = 0.4;
/// At rest this long before it vanishes (pop3-rev-analysis.md: "128 turns to live", 10.7 s).
const REST_SECS: f32 = 128.0 / 12.0;
/// Its own size, render units (cells): small, a fraction of a cell.
const SIZE: f32 = 0.22;

const ROCK_COLOUR: Color = Color::srgb(0.24, 0.21, 0.2);

/// One rock's fixed-at-spawn placement and timing.
#[derive(Component, Clone, Copy)]
pub struct Rock {
    /// Seconds into the volcano's own life that it's born.
    born: f32,
    /// Landing point (cells).
    at: Vec2,
    /// Unit direction it rolls away from the peak.
    roll_dir: Vec2,
    scale: f32,
    /// A phase so not every rock spins in step.
    spin_phase: f32,
}

impl Rock {
    fn age_bounds(&self) -> (f32, f32) {
        (self.born, self.born + ROLL_SECS + REST_SECS)
    }
}

#[derive(Resource)]
pub struct RockAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

/// One volcano's rocks: the view itself holds nothing but which children are its own.
#[derive(Component)]
pub struct VolcanoRocks {
    rocks: Vec<Entity>,
}

impl WorldView for VolcanoRocks {
    type Model = (Id<Effect>, Effect);
    #[allow(clippy::type_complexity)]
    type Params = (Res<'static, CurrentMap>, Res<'static, RockAssets>, Query<'static, 'static, (&'static Rock, &'static mut Grounded, &'static mut Transform, &'static mut Visibility)>);

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| matches!(e.kind, EffectKind::Volcano(_)))
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((id, e): &(Id<Effect>, Effect), view: &mut EntityCommands, (_, assets, _): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let raw = id.raw();
        let centre = Vec2::new(e.x as f32, e.z as f32) / WORLD_UNITS_PER_CELL as f32;
        view.insert((Transform::default(), Visibility::Inherited));
        let mut rocks = Vec::with_capacity(COUNT as usize);
        view.with_children(|v| {
            for i in 0..COUNT {
                let s = |salt: u32| seed(raw.wrapping_mul(71).wrapping_add(i.wrapping_mul(151)).wrapping_add(salt));
                let angle = s(1) * TAU;
                let dist = (MIN_DIST + s(2) * (MAX_DIST - MIN_DIST)) / WORLD_UNITS_PER_CELL as f32;
                let dir = Vec2::new(angle.cos(), angle.sin());
                let rock = Rock {
                    born: (FIRST_TURN + i) as f32 / game_core::time::TICKS_PER_SECOND as f32,
                    at: centre + dir * dist,
                    roll_dir: dir,
                    scale: SIZE * (0.75 + s(3) * 0.5),
                    spin_phase: s(4) * TAU,
                };
                let id = v
                    .spawn((
                        Mesh3d(assets.mesh.clone()),
                        MeshMaterial3d(assets.material.clone()),
                        Transform::from_scale(Vec3::splat(rock.scale)),
                        Grounded { at: rock.at, half: 0.0 },
                        Visibility::Hidden,
                        NotShadowCaster,
                        rock,
                    ))
                    .id();
                rocks.push(id);
            }
        });
        Some(VolcanoRocks { rocks })
    }

    fn update(&self, (_, e): &(Id<Effect>, Effect), now: SimNow, (map, _, rocks): &mut SystemParamItem<Self::Params>) {
        let start = Tick::from_wire(map.0.now.to_wire().wrapping_sub(e.left.done().get()));
        let age = now.secs_since(start);
        for &entity in &self.rocks {
            let Ok((rock, mut grounded, mut transform, mut vis)) = rocks.get_mut(entity) else { continue };
            let (born, gone) = rock.age_bounds();
            let since_born = age - born;
            if since_born < 0.0 || age > gone {
                *vis = Visibility::Hidden;
                continue;
            }
            *vis = Visibility::Visible;
            let roll_t = (since_born / ROLL_SECS).clamp(0.0, 1.0);
            let eased = 1.0 - (1.0 - roll_t) * (1.0 - roll_t);
            grounded.at = rock.at + rock.roll_dir * ROLL_DIST * eased;
            let spin = if roll_t < 1.0 { since_born * TAU } else { 0.0 };
            transform.rotation = Quat::from_axis_angle(Vec3::new(-rock.roll_dir.y, 0.0, rock.roll_dir.x), rock.spin_phase + spin);
        }
    }
}

pub struct VolcanoRocksPlugin;

impl Plugin for VolcanoRocksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets).add_systems(Update, sync_views::<VolcanoRocks>.after(crate::units::run_ticks));
    }
}

fn load_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mesh = meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap_or_else(|_| Sphere::new(1.0).mesh().uv(8, 6)));
    let material = materials.add(StandardMaterial { base_color: ROCK_COLOUR, perceptual_roughness: 1.0, ..default() });
    commands.insert_resource(RockAssets { mesh, material });
}
