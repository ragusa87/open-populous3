//! Vehicles (`game_core::vehicle`), a world view (`views::WorldView`) of each: the original boat or airship object
//! (`catalog::BOAT`, `AIRSHIP`) in its tribe's colours, or with `--no-original` our own stand-in (a hull, a balloon
//! over a basket). Turned by its heading, tilted by a sinking boat's pitch and a stranded one's roll, lifted to its
//! altitude (a balloon's hover, a sinking boat's depth), gliding between ticks like the units.

use crate::camera::CurveParamsRes;
use crate::game_frame::GameYaw;
use crate::grounded::Grounded;
use crate::original_models::{object_mesh, solid_part, to_mesh, OriginalObjects};
use crate::sim_time::SimNow;
use crate::units::SimClock;
use crate::views::{sync_views, WorldView};
use crate::world::{CurrentMap, LevelList};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use game_core::angle::{Angle, TURN};
use game_core::map::GameMap;
use game_core::tribes::{Owner, TribeId};
use game_core::vehicle::{State, Vehicle, VehicleId, VehicleKind};
use pop3_format::{catalog, WORLD_UNITS_PER_CELL};

/// Stand-in sizes, cells: the hull (length along the heading, height, width), the balloon's radius and the basket
/// hanging under it.
const HULL: Vec3 = Vec3::new(0.32, 0.12, 0.8);
const BALLOON_RADIUS: f32 = 0.42;
const BASKET: Vec3 = Vec3::new(0.25, 0.16, 0.25);
const ROPES: f32 = 0.18;
const WOOD: Color = Color::srgb(0.45, 0.3, 0.16);

/// Meshes and materials made so far, for each kind and tribe (the original objects are recoloured per tribe).
#[derive(Resource, Default)]
pub struct VehicleLook {
    theme: Option<u8>,
    original: Option<Handle<StandardMaterial>>,
    parts: Vec<((VehicleKind, Owner), Vec<(Handle<Mesh>, Handle<StandardMaterial>, Transform)>)>,
}

/// One vehicle's view: the root (moved along the ground), its hull (lifted, turned and tilted) and what it last
/// drew (to redraw on a change of tribe, and to glide from).
#[derive(Component)]
pub struct VehicleView {
    me: Entity,
    hull: Entity,
}

/// Where the view glides from and to (cells) during the map tick `moved`, when the vehicle last moved, and for which
/// tribe its parts were made.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Drawn {
    from: Vec2,
    to: Vec2,
    moved: game_core::time::Tick,
    owner: Owner,
}

impl Drawn {
    /// Where to draw a vehicle at `at` (cells) when the map is at tick `now`, `alpha` into its step: from where it was
    /// to `at` during the tick it moved in, then still at `at` (never back and forth once it stops).
    pub fn glide(&mut self, at: Vec2, now: game_core::time::Tick, alpha: f32) -> Vec2 {
        if at != self.to {
            (self.from, self.to, self.moved) = (self.to, at, now);
        }
        let far = (self.to - self.from).abs().max_element() > 1.0;
        if far || now != self.moved { self.to } else { self.from.lerp(self.to, alpha) }
    }
}

/// Whether it is going somewhere now (not waiting after a boarding): its people pedal.
pub fn under_way(v: &Vehicle) -> bool {
    match v.state {
        State::Moving { .. } => v.wait.left() == game_core::time::Ticks::ZERO,
        State::Drifting { .. } => true,
        _ => false,
    }
}

/// A vehicle's place in cells.
pub fn cells_at(v: &Vehicle) -> Vec2 {
    Vec2::new(v.x as f32, v.z as f32) / WORLD_UNITS_PER_CELL as f32
}

/// How high over the ground under it the hull is drawn, in render units, `alpha` into the step: a balloon's hover,
/// glided from where its last turn started (`Vehicle::was_at`) so it climbs and sinks smoothly; a sinking boat's depth
/// below the surface (negative).
pub fn lift(map: &GameMap, v: &Vehicle, height_scale: f32, alpha: f32) -> f32 {
    let ground = map.terrain.height_at(v.x as u32, v.z as u32, WORLD_UNITS_PER_CELL).max(0) as f32;
    let depth = match v.state {
        State::Sinking { depth, .. } => depth as f32,
        _ => 0.0,
    };
    let now = v.altitude(&map.terrain) as f32;
    let drawn = match (v.kind, v.was_at) {
        (VehicleKind::Balloon, Some(was)) => was as f32 + (now - was as f32) * alpha.clamp(0.0, 1.0),
        _ => now,
    };
    (drawn - ground - depth) * height_scale
}

/// Where its people stand over the model's origin, render units: the origin is its floor (the original boat's
/// waterline, its keel going below; the airship's basket floor; our stand-ins' bottoms).
const FEET: f32 = 0.03;
/// Seats along a boat, across a basket, cells apart.
const SEAT_GAP: f32 = 0.12;

/// The height its people stand at over the ground under it, render units.
pub fn deck(map: &GameMap, v: &Vehicle, height_scale: f32, alpha: f32) -> f32 {
    lift(map, v, height_scale, alpha) + FEET
}

/// Where seat `k` is from the vehicle's middle, cells: along a boat from bow to stern, side by side in a basket.
pub fn seat_offset(v: &Vehicle, k: usize) -> Vec2 {
    let along = (k as f32 - (v.kind.seats() as f32 - 1.0) / 2.0) * SEAT_GAP;
    let h = radians(v.heading);
    let (ahead, side) = (Vec2::new(h.cos(), h.sin()), Vec2::new(-h.sin(), h.cos()));
    match v.kind {
        VehicleKind::Boat => ahead * along,
        VehicleKind::Balloon => side * along,
    }
}

/// Half the width of the box a vehicle is hovered in, cells.
pub const HALF: f32 = 0.45;
/// How tall a boat stands and how far a balloon's top is over its basket, cells.
const BOAT_TOP: f32 = 0.7;
const BALLOON_TOP: f32 = 1.1;

/// The top of a vehicle's model over the ground under it, render units: for its hover box.
pub fn top(map: &GameMap, v: &Vehicle, height_scale: f32) -> f32 {
    let model = match v.kind {
        VehicleKind::Boat => BOAT_TOP,
        VehicleKind::Balloon => BALLOON_TOP,
    };
    lift(map, v, height_scale, 1.0).max(0.0) + model
}

fn radians(a: Angle) -> f32 {
    a.signed() as f32 * std::f32::consts::TAU / TURN as f32
}

/// The hull's turn: its heading (from +x towards +z; the original boat's bow is along its game +z, render -z), then its pitch (bow up) and
/// roll.
pub fn hull_rotation(v: &Vehicle) -> Quat {
    let yaw = GameYaw(std::f32::consts::FRAC_PI_2 - radians(v.heading));
    let (pitch, roll) = match v.state {
        State::Sinking { pitch, .. } => (radians(pitch), 0.0),
        State::Stranded { roll, .. } => (0.0, radians(roll)),
        _ => (0.0, 0.0),
    };
    Quat::from(yaw) * Quat::from_rotation_x(-pitch) * Quat::from_rotation_z(roll)
}

impl VehicleLook {
    /// The parts of a `kind` of `owner`: the original object when there is one, else our stand-in.
    fn parts(&mut self, kind: VehicleKind, owner: Owner, theme: u8, levels: &LevelList, objects: &OriginalObjects, assets: &mut Assets3d) -> Vec<(Handle<Mesh>, Handle<StandardMaterial>, Transform)> {
        if let Some((_, parts)) = self.parts.iter().find(|(k, _)| *k == (kind, owner)) {
            return parts.clone();
        }
        if self.theme != Some(theme) {
            self.theme = Some(theme);
            self.original = crate::buildings::original_material(levels, theme, &mut assets.images, &mut assets.materials);
            self.parts.clear();
        }
        let index = match kind {
            VehicleKind::Boat => catalog::BOAT,
            VehicleKind::Balloon => catalog::AIRSHIP,
        };
        let original = self.original.clone().zip(objects.0.as_ref().and_then(|bank| bank.get(index)));
        let tribe = owner.tribe().unwrap_or(TribeId::Blue);
        let parts = match original {
            Some((material, obj)) => vec![(assets.meshes.add(to_mesh(object_mesh(&solid_part(obj), tribe))), material, Transform::IDENTITY)],
            None => stand_in(kind, owner, assets),
        };
        self.parts.push(((kind, owner), parts.clone()));
        parts
    }
}

/// Our own look: a hull in the tribe's colour, or a balloon in the tribe's colour over a wooden basket.
fn stand_in(kind: VehicleKind, owner: Owner, assets: &mut Assets3d) -> Vec<(Handle<Mesh>, Handle<StandardMaterial>, Transform)> {
    let colour = assets.materials.add(StandardMaterial { base_color: crate::sites::tribe_color(owner), perceptual_roughness: 0.8, ..default() });
    let wood = assets.materials.add(StandardMaterial { base_color: WOOD, perceptual_roughness: 0.9, ..default() });
    match kind {
        VehicleKind::Boat => vec![
            (assets.meshes.add(Cuboid::from_size(HULL)), wood, Transform::from_xyz(0.0, HULL.y / 2.0, 0.0)),
            (assets.meshes.add(Cuboid::from_size(Vec3::new(HULL.x * 1.05, HULL.y * 0.3, HULL.z * 0.9))), colour, Transform::from_xyz(0.0, HULL.y, 0.0)),
        ],
        VehicleKind::Balloon => vec![
            (assets.meshes.add(Cuboid::from_size(BASKET)), wood, Transform::from_xyz(0.0, BASKET.y / 2.0, 0.0)),
            (assets.meshes.add(Sphere::new(BALLOON_RADIUS)), colour, Transform::from_xyz(0.0, BASKET.y + ROPES + BALLOON_RADIUS, 0.0)),
        ],
    }
}

/// The asset stores a vehicle's parts are made in.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Assets3d<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    images: ResMut<'w, Assets<Image>>,
}

fn spawn_parts(hull: &mut ChildSpawnerCommands, parts: Vec<(Handle<Mesh>, Handle<StandardMaterial>, Transform)>) {
    for (mesh, material, at) in parts {
        hull.spawn((Mesh3d(mesh), MeshMaterial3d(material), at));
    }
}

impl WorldView for VehicleView {
    type Model = (VehicleId, Vehicle);
    type Params = (
        Commands<'static, 'static>,
        (Res<'static, CurrentMap>, Res<'static, CurveParamsRes>, Res<'static, SimClock>, Res<'static, LevelList>, Res<'static, OriginalObjects>),
        ResMut<'static, VehicleLook>,
        Assets3d<'static>,
        Query<'static, 'static, (&'static mut Grounded, &'static mut Drawn)>,
        Query<'static, 'static, &'static mut Transform, Without<Grounded>>,
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &(VehicleId, Vehicle)> {
        map.vehicles.entries()
    }

    fn id((id, _): &(VehicleId, Vehicle)) -> u32 {
        id.raw()
    }

    fn spawn((_, v): &(VehicleId, Vehicle), view: &mut EntityCommands, (_, (map, params, _, levels, objects), look, assets, ..): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let theme = map.0.theme.unwrap_or(crate::buildings::DEFAULT_THEME);
        let parts = look.parts(v.kind, v.owner, theme, levels, objects, assets);
        let me = view.id();
        let at = cells_at(v);
        view.insert((
            Grounded { at, half: 0.0 },
            Drawn { from: at, to: at, moved: map.0.now, owner: v.owner },
            crate::object_scale::Scaled(crate::object_scale::ObjectGroup::Buildings),
            Transform::default(),
            Visibility::Hidden,
            crate::hover::Hoverable::default(),
        ));
        let mut hull = Entity::PLACEHOLDER;
        let pose = Transform::from_xyz(0.0, lift(&map.0, v, params.0.height_scale, 1.0), 0.0).with_rotation(hull_rotation(v));
        view.with_children(|c| {
            hull = c.spawn((pose, Visibility::Inherited)).with_children(|h| spawn_parts(h, parts)).id();
        });
        Some(VehicleView { me, hull })
    }

    fn update(&self, (_, v): &(VehicleId, Vehicle), _now: SimNow, (commands, (map, params, clock, levels, objects), look, assets, grounded, transforms): &mut SystemParamItem<Self::Params>) {
        if let Ok((mut g, mut drawn)) = grounded.get_mut(self.me) {
            g.at = drawn.glide(cells_at(v), map.0.now, clock.alpha());
            if drawn.owner != v.owner {
                drawn.owner = v.owner;
                let theme = map.0.theme.unwrap_or(crate::buildings::DEFAULT_THEME);
                let parts = look.parts(v.kind, v.owner, theme, levels, objects, assets);
                commands.entity(self.hull).despawn_related::<Children>().with_children(|h| spawn_parts(h, parts));
            }
        }
        if let Ok(mut t) = transforms.get_mut(self.hull) {
            t.translation.y = lift(&map.0, v, params.0.height_scale, clock.alpha());
            t.rotation = hull_rotation(v);
        }
    }
}

pub struct VehiclesPlugin;

impl Plugin for VehiclesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VehicleLook>().add_systems(Update, sync_views::<VehicleView>.after(crate::units::run_ticks));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vehicle(kind: VehicleKind, at: (u16, u16)) -> Vehicle {
        Vehicle::new(kind, TribeId::Blue, at, Angle::ZERO)
    }

    #[test]
    fn a_balloon_floats_over_the_ground_a_sinking_boat_under_the_water() {
        let map = GameMap::sandbox_buildings();
        let at = (64 * 512, 64 * 512);
        let ground = map.terrain.height_at(at.0 as u32, at.1 as u32, WORLD_UNITS_PER_CELL);
        assert_eq!(cells_at(&vehicle(VehicleKind::Boat, at)), Vec2::new(64.0, 64.0));
        assert_eq!(lift(&map, &vehicle(VehicleKind::Balloon, at), 0.5, 1.0), game_core::vehicle::HOVER as f32 * 0.5);
        let mut climbing = vehicle(VehicleKind::Balloon, at);
        climbing.was_at = Some(ground as u16 + 100);
        assert_eq!(lift(&map, &climbing, 0.5, 0.25), (100.0 + 0.25 * 100.0) * 0.5, "a quarter of the way up its turn's climb");
        let mut sinking = vehicle(VehicleKind::Boat, (0, 0));
        sinking.state = State::Sinking { depth: 40, pitch: Angle::ZERO };
        assert_eq!(lift(&map, &sinking, 0.5, 0.0), -20.0);
        assert!(ground > 0);
    }

    #[test]
    fn seats_run_along_a_boat_and_across_a_basket() {
        let boat = vehicle(VehicleKind::Boat, (0, 0));
        assert_eq!(seat_offset(&boat, 2), Vec2::ZERO, "the middle of 5");
        assert!((seat_offset(&boat, 4) - Vec2::new(2.0 * SEAT_GAP, 0.0)).length() < 1e-5, "heading +x: along x");
        let balloon = vehicle(VehicleKind::Balloon, (0, 0));
        assert!((seat_offset(&balloon, 0) - Vec2::new(0.0, -SEAT_GAP / 2.0)).length() < 1e-5, "side by side");
    }

    #[test]
    fn it_glides_in_the_tick_it_moved_then_stays_put() {
        let tick = game_core::time::Tick::from_wire;
        let mut d = Drawn { from: Vec2::ZERO, to: Vec2::ZERO, moved: tick(0), owner: Owner::Tribe(TribeId::Blue) };
        assert_eq!(d.glide(Vec2::new(0.2, 0.0), tick(5), 0.5), Vec2::new(0.1, 0.0), "half way through its step");
        assert!((d.glide(Vec2::new(0.2, 0.0), tick(5), 0.9) - Vec2::new(0.18, 0.0)).length() < 1e-6);
        assert_eq!(d.glide(Vec2::new(0.2, 0.0), tick(6), 0.1), Vec2::new(0.2, 0.0), "parked: still, not replaying the step");
        assert_eq!(d.glide(Vec2::new(3.2, 0.0), tick(7), 0.1), Vec2::new(3.2, 0.0), "a jump is not glided");
    }

    #[test]
    fn its_people_pedal_only_while_it_goes() {
        let mut v = vehicle(VehicleKind::Boat, (0, 0));
        assert!(!under_way(&v), "parked");
        v.go((5000, 0));
        assert!(under_way(&v));
        v.seat(game_core::id::Id::from_raw(3), TribeId::Blue);
        assert!(!under_way(&v), "waiting after a boarding");
    }

    #[test]
    fn the_bow_points_where_it_heads() {
        let v = |steps| {
            let mut v = vehicle(VehicleKind::Boat, (0, 0));
            v.heading = Angle::new(steps);
            v
        };
        let bow = |steps| hull_rotation(&v(steps)) * Vec3::NEG_Z;
        let game = |r: Vec3| Vec2::new(r.x, -r.z);
        assert!((game(bow(0)) - Vec2::X).length() < 1e-5, "heading 0 is +x");
        assert!((game(bow(512)) - Vec2::Y).length() < 1e-5, "a quarter turn is +z");
    }
}
