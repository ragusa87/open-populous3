//! The splash of someone falling or walking into the sea (`Action::Drowning`, units.md): a patch of white foam where
//! it went in, droplets thrown up and falling back around it, and rings spreading on the water every `RING_EVERY`
//! while it sinks — sized to read at the usual camera distance. Our own look
//! (`[ours]`): the original's drowning splash is not traced (sprites.md: hfx 1304-1319 "a water splash
//! (drowning?)"). Timed from the tick it went in, read back from how deep it has sunk; on the sea's surface as drawn
//! (`ground_material::drawn_water_lift`).

use crate::grounded::Grounded;
use crate::sim_time::{seed, SimNow};
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::map::GameMap;
use game_core::time::Tick;
use game_core::unit::{Action, Unit, DROWN_SINK};
use pop3_format::WORLD_UNITS_PER_CELL;

const WATER: Color = Color::srgb(0.85, 0.93, 1.0);
const DROPLETS: usize = 20;
/// How long a droplet flies (seconds), how high it goes and how far out it lands (cells).
const DROP_SECS: f32 = 0.7;
const DROP_HEIGHT: f32 = 0.8;
const DROP_REACH: (f32, f32) = (0.2, 0.6);
const DROP_SIZE: f32 = 0.035;
/// A ring spreads to `RING_REACH` cells over `RING_SECS`, a new one every `RING_EVERY` seconds.
const RING_SECS: f32 = 0.9;
const RING_EVERY: f32 = 0.4;
const RING_REACH: f32 = 0.8;
const RINGS: usize = 4;
const RING_ALPHA: f32 = 0.75;
/// The foam where it went in spreads to `FOAM_REACH` cells over `FOAM_SECS`, fading.
const FOAM_SECS: f32 = 0.8;
const FOAM_REACH: f32 = 0.35;

/// The foam's radius (cells) and opacity `secs` after it went in; None once gone.
pub fn foam(secs: f32) -> Option<(f32, f32)> {
    let t = secs / FOAM_SECS;
    (0.0..1.0).contains(&t).then(|| (FOAM_REACH * (0.3 + 0.7 * t.sqrt()), 0.9 * (1.0 - t)))
}

/// Where droplet `k` of a splash seeded `id` is, `secs` after it went in (cells, from its centre on the water), and
/// how opaque; None once it has fallen back.
pub fn droplet(id: u32, k: usize, secs: f32) -> Option<(Vec3, f32)> {
    let t = secs / DROP_SECS;
    if !(0.0..1.0).contains(&t) {
        return None;
    }
    let angle = (k as f32 + seed(id.wrapping_add(k as u32))) / DROPLETS as f32 * std::f32::consts::TAU;
    let reach = DROP_REACH.0 + (DROP_REACH.1 - DROP_REACH.0) * seed(id.wrapping_mul(31).wrapping_add(k as u32));
    let up = DROP_HEIGHT * (0.6 + 0.4 * seed(id.wrapping_mul(7).wrapping_add(k as u32)));
    let at = Vec3::new(angle.cos(), 0.0, angle.sin()) * reach * t + Vec3::Y * up * 4.0 * t * (1.0 - t);
    Some((at, 1.0 - t * t))
}

/// Ring `k`'s radius (cells) and opacity `secs` after it went in, while it is still `drowning`; None before it
/// starts or once faded.
pub fn ring(k: usize, secs: f32) -> Option<(f32, f32)> {
    let t = (secs - k as f32 * RING_EVERY) / RING_SECS;
    (0.0..1.0).contains(&t).then(|| (RING_REACH * t.sqrt(), (1.0 - t) * RING_ALPHA))
}

/// The tick a drowning unit went in, from how deep it has sunk by `now` (one `DROWN_SINK` the tick it went in).
fn went_in(now: Tick, sunk: u16) -> Tick {
    Tick::from_wire(now.to_wire().wrapping_sub((sunk / DROWN_SINK).saturating_sub(1) as u32))
}

#[derive(Resource)]
pub struct SplashAssets {
    drop: Handle<Mesh>,
    ring: Handle<Mesh>,
    foam: Handle<Mesh>,
}

/// One splash: its droplets and rings, each with its own material to fade.
#[derive(Component)]
pub struct SplashView {
    drops: Vec<(Entity, Handle<StandardMaterial>)>,
    rings: Vec<(Entity, Handle<StandardMaterial>)>,
    foam: (Entity, Handle<StandardMaterial>),
}

impl WorldView for SplashView {
    type Model = Unit;
    type Params = (
        Res<'static, CurrentMap>,
        Res<'static, SplashAssets>,
        ResMut<'static, Assets<StandardMaterial>>,
        Query<'static, 'static, (&'static mut Transform, &'static mut Visibility)>,
        (Res<'static, crate::world::TerrainMaterial>, Res<'static, Assets<crate::ground_material::GroundMaterial>>, Res<'static, Assets<Image>>, Res<'static, crate::camera::CurveParamsRes>),
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &Unit> {
        map.units.iter().filter(|u| matches!(u.action, Action::Drowning { .. }))
    }

    fn id(u: &Unit) -> u32 {
        u.id.raw()
    }

    fn spawn(u: &Unit, view: &mut EntityCommands, (_, assets, mats, ..): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let cell = WORLD_UNITS_PER_CELL as f32;
        view.insert((Grounded { at: Vec2::new(u.x as f32, u.z as f32) / cell, half: 0.0 }, Transform::default(), Visibility::Visible));
        let mut part = |v: &mut ChildSpawnerCommands, mesh: &Handle<Mesh>| {
            let m = mats.add(StandardMaterial { base_color: WATER, alpha_mode: AlphaMode::Blend, unlit: true, ..default() });
            (v.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(m.clone()), NotShadowCaster, crate::hover::NoOutline, Transform::default(), Visibility::Hidden)).id(), m)
        };
        let (mut drops, mut rings, mut foam) = (Vec::new(), Vec::new(), None);
        view.with_children(|v| {
            drops = (0..DROPLETS).map(|_| part(v, &assets.drop)).collect();
            rings = (0..RINGS).map(|_| part(v, &assets.ring)).collect();
            foam = Some(part(v, &assets.foam));
        });
        Some(SplashView { drops, rings, foam: foam? })
    }

    fn update(&self, u: &Unit, now: SimNow, (map, _, mats, parts, (ground, grounds, images, curve)): &mut SystemParamItem<Self::Params>) {
        let Action::Drowning { sunk } = u.action else { return };
        let secs = now.secs_since(went_in(map.0.now, sunk)).max(0.0);
        let at = Vec2::new(u.x as f32, u.z as f32) / WORLD_UNITS_PER_CELL as f32;
        let surface = Vec3::Y * crate::ground_material::drawn_water_lift(grounds, images, &ground.0, &map.0.terrain, at) * curve.0.height_scale;
        let mut show = |(e, m): &(Entity, Handle<StandardMaterial>), pose: Option<(Transform, f32)>| {
            if let Ok((mut t, mut vis)) = parts.get_mut(*e) {
                match pose {
                    Some((pose, alpha)) => {
                        (*t, *vis) = (pose, Visibility::Inherited);
                        if let Some(mut mat) = mats.get_mut(m) {
                            mat.base_color = WATER.with_alpha(alpha);
                        }
                    }
                    None => *vis = Visibility::Hidden,
                }
            }
        };
        for (k, part) in self.drops.iter().enumerate() {
            show(part, droplet(u.id.raw(), k, secs).map(|(at, a)| (Transform::from_translation(surface + at).with_scale(Vec3::splat(DROP_SIZE)), a)));
        }
        for (k, part) in self.rings.iter().enumerate() {
            show(part, ring(k, secs).map(|(r, a)| (Transform::from_translation(surface + Vec3::Y * 0.01).with_scale(Vec3::new(r, 0.15, r)), a)));
        }
        show(&self.foam, foam(secs).map(|(r, a)| (Transform::from_translation(surface + Vec3::Y * 0.005).with_scale(Vec3::new(r, 1.0, r)), a)));
    }
}

pub struct SplashPlugin;

impl Plugin for SplashPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets).add_systems(Update, sync_views::<SplashView>.after(crate::units::run_ticks));
    }
}

fn load_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(SplashAssets { drop: meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap()), ring: meshes.add(Torus::new(0.88, 1.0)), foam: meshes.add(Cylinder::new(1.0, 0.01)) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn droplets_fly_up_and_out_then_fall_back_and_fade() {
        let (start, a0) = droplet(7, 3, 0.0).unwrap();
        assert_eq!((start, a0), (Vec3::ZERO, 1.0), "from its centre on the water");
        let (mid, a) = droplet(7, 3, DROP_SECS / 2.0).unwrap();
        assert!(mid.y > 0.0 && mid.xz().length() > 0.0 && a < 1.0);
        assert!(droplet(7, 3, DROP_SECS).is_none(), "fallen back");
        assert_ne!(droplet(7, 3, 0.2), droplet(7, 4, 0.2), "each its own way");
    }

    #[test]
    fn rings_spread_and_fade_one_after_another() {
        assert_eq!(ring(0, 0.0), Some((0.0, RING_ALPHA)));
        assert!(ring(1, 0.1).is_none(), "the next one not yet");
        let (r, a) = ring(1, RING_EVERY + RING_SECS / 2.0).unwrap();
        assert!(r > 0.0 && r < RING_REACH && a < RING_ALPHA);
        assert!(ring(0, RING_SECS).is_none(), "faded");
    }

    #[test]
    fn the_foam_spreads_and_fades_where_it_went_in() {
        let (r0, a0) = foam(0.0).unwrap();
        let (r1, a1) = foam(FOAM_SECS / 2.0).unwrap();
        assert!(r1 > r0 && a1 < a0 && r1 < FOAM_REACH);
        assert!(foam(FOAM_SECS).is_none());
    }

    #[test]
    fn it_went_in_the_tick_it_first_sank() {
        let now = Tick::from_wire(100);
        assert_eq!(went_in(now, DROWN_SINK), now);
        assert_eq!(went_in(now, 5 * DROWN_SINK), Tick::from_wire(96));
    }
}
