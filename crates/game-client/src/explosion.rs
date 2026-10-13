//! A building blowing up, drawn only (the simulation just removes it): its model's polygons fly off on their own,
//! spinning, out from the middle and up, fall back, lie on the ground a moment and shrink away. Started by a
//! `BuildingExploded` message while the building's view is still drawn: its meshes are copied, so the explosion
//! looks like whatever drew the building. Poses follow simulation time from the start tick, seeded by the
//! building's id. In the buildings sandbox, its row of `SANDBOX_BLASTS` blows up in a loop.

use crate::buildings::{DrawnBuilding, RespawnBuildings};
use crate::flame::Flame;
use crate::grounded::{Grounded, Tilted};
use crate::original_models::{to_mesh, MeshData};
use crate::sim_time::SimNow;
use crate::units::SimClock;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;
use crate::world::CurrentMap;
use game_core::building::{Building, BuildingId};
use game_core::map::{sandbox_blast_corner, GameMap, SANDBOX_BLASTS, SANDBOX_BUILDINGS};
use game_core::time::{Tick, Ticks};

/// Seconds from the blast until the last polygon is gone.
pub const DURATION: f32 = 3.0;
/// Seconds over which the polygons shrink away, at the end.
const FADE: f32 = 0.8;
/// Gravity, cells per second squared.
const GRAVITY: f32 = 12.0;
/// Speed out from the middle, cells per second: the first plus up to the second.
const OUT_SPEED: (f32, f32) = (0.6, 1.4);
/// Speed up, cells per second: the first plus up to the second.
const UP_SPEED: (f32, f32) = (2.5, 4.0);
/// Spin, radians per second: the first plus up to the second.
const SPIN: (f32, f32) = (4.0, 10.0);
/// In the sandbox loop, how long the row stands between two explosions.
const STANDING: Ticks = Ticks::secs(2);

/// The building `building` blows up at `start`.
#[derive(Message, Clone, Copy, Debug)]
pub struct BuildingExploded {
    pub building: BuildingId,
    pub start: Tick,
}

/// One flying polygon: its triangles' corners around its middle, in the model's frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Shard {
    pub middle: Vec3,
    /// Each corner: offset from `middle`, normal, uv, colour.
    pub corners: Vec<(Vec3, Vec3, [f32; 2], [f32; 4])>,
}

/// One drawn part of the exploding building: its polygons, and the mesh redrawn each frame.
#[derive(Component)]
struct Pieces {
    shards: Vec<Shard>,
    mesh: Handle<Mesh>,
}

#[derive(Component)]
struct Explosion {
    start: Tick,
    seed: u32,
    game: u32,
}

pub struct ExplosionPlugin;

impl Plugin for ExplosionPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BuildingExploded>()
            .add_systems(Update, (sandbox_loop, start_explosions.before(RespawnBuildings)).chain())
            .add_systems(Update, fly);
    }
}

/// The polygons of `mesh`: triangles sharing a corner (a quad split in two, a flat-shaded face) fly together.
pub fn shatter(mesh: &MeshData) -> Vec<Shard> {
    let n = mesh.positions.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for tri in mesh.indices.chunks_exact(3) {
        let a = root(&mut parent, tri[0] as usize);
        for &v in &tri[1..] {
            let b = root(&mut parent, v as usize);
            parent[b] = a;
        }
    }
    let mut groups: Vec<(usize, Vec<u32>)> = Vec::new();
    for tri in mesh.indices.chunks_exact(3) {
        let r = root(&mut parent, tri[0] as usize);
        match groups.iter_mut().find(|g| g.0 == r) {
            Some(g) => g.1.extend_from_slice(tri),
            None => groups.push((r, tri.to_vec())),
        }
    }
    groups
        .into_iter()
        .map(|(_, corners)| {
            let at = |i: u32| Vec3::from(mesh.positions[i as usize]);
            let middle = corners.iter().map(|&i| at(i)).sum::<Vec3>() / corners.len() as f32;
            let corners = corners
                .iter()
                .map(|&i| {
                    let i = i as usize;
                    let uv = mesh.uvs.get(i).copied().unwrap_or_default();
                    let colour = mesh.colors.get(i).copied().unwrap_or([1.0; 4]);
                    (Vec3::from(mesh.positions[i]) - middle, Vec3::from(mesh.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0])), uv, colour)
                })
                .collect();
            Shard { middle, corners }
        })
        .collect()
}

/// A number in 0..1 from `seed`, the polygon `k` and which draw `salt` it is.
fn draw(seed: u32, k: usize, salt: u32) -> f32 {
    let mut h = seed ^ (k as u32).wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

fn between((low, span): (f32, f32), r: f32) -> f32 {
    low + span * r
}

/// Where polygon `k` (its middle at `middle`) is `t` seconds after the blast, and how it is turned and scaled
/// about its middle: thrown out from the middle and up, spinning, until it lands (stays as it landed), then
/// shrinking away over the last `FADE`.
pub fn pose(middle: Vec3, seed: u32, k: usize, t: f32) -> (Vec3, Quat, f32) {
    let t = t.max(0.0);
    let flat = Vec2::new(middle.x, middle.z);
    let out = if flat.length() > 1e-3 { flat.normalize() } else { Vec2::from_angle(draw(seed, k, 0) * std::f32::consts::TAU) };
    let out = Vec2::from_angle((draw(seed, k, 1) - 0.5) * 0.8).rotate(out) * between(OUT_SPEED, draw(seed, k, 2));
    let up = between(UP_SPEED, draw(seed, k, 3));
    let height = middle.y.max(0.0);
    let landing = (up + (up * up + 2.0 * GRAVITY * height).sqrt()) / GRAVITY;
    let flying = t.min(landing);
    let at = middle + Vec3::new(out.x * flying, up * flying - 0.5 * GRAVITY * flying * flying, out.y * flying);
    let axis = Vec3::new(draw(seed, k, 4) - 0.5, draw(seed, k, 5) - 0.5, draw(seed, k, 6) - 0.5).normalize_or(Vec3::X);
    let turn = Quat::from_axis_angle(axis, between(SPIN, draw(seed, k, 7)) * flying);
    let scale = ((DURATION - t) / FADE).clamp(0.0, 1.0);
    (at, turn, scale)
}

/// The polygons `t` seconds after the blast, as one mesh's data.
pub fn blown(shards: &[Shard], seed: u32, t: f32) -> MeshData {
    let mut data = MeshData::default();
    for (k, s) in shards.iter().enumerate() {
        let (at, turn, scale) = pose(s.middle, seed, k, t);
        for &(offset, normal, uv, colour) in &s.corners {
            data.indices.push(data.positions.len() as u32);
            data.positions.push((at + turn * offset * scale).into());
            data.normals.push((turn * normal).into());
            data.uvs.push(uv);
            data.colors.push(colour);
        }
    }
    data
}

/// The mesh asset's triangles, None without positions or indices.
fn mesh_data(mesh: &Mesh) -> Option<MeshData> {
    let VertexAttributeValues::Float32x3(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)? else { return None };
    let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(n)) => n.clone(),
        _ => Vec::new(),
    };
    let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
        Some(VertexAttributeValues::Float32x2(u)) => u.clone(),
        _ => Vec::new(),
    };
    let colors = match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
        Some(VertexAttributeValues::Float32x4(c)) => c.clone(),
        _ => Vec::new(),
    };
    let indices = match mesh.indices()? {
        Indices::U32(i) => i.clone(),
        Indices::U16(i) => i.iter().map(|&i| i as u32).collect(),
    };
    (!indices.is_empty()).then(|| MeshData { positions: positions.clone(), normals, uvs, indices, colors })
}

/// Each exploded building's view copied into an explosion: its place, and its model's parts shattered, seen from
/// both sides.
#[allow(clippy::too_many_arguments)]
fn start_explosions(
    mut commands: Commands,
    mut exploded: MessageReader<BuildingExploded>,
    clock: Res<SimClock>,
    views: Query<(Entity, &DrawnBuilding, &Grounded, &Tilted, &Transform, Option<&crate::object_scale::Scaled>)>,
    children: Query<&Children>,
    parts: Query<(&Mesh3d, &MeshMaterial3d<StandardMaterial>), Without<Flame>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    for e in exploded.read() {
        let Some((view, _, grounded, tilted, transform, scaled)) = views.iter().find(|v| v.1 .0 == e.building) else { continue };
        let mut pieces = Vec::new();
        for part in children.iter_descendants(view) {
            let Ok((mesh, mat)) = parts.get(part) else { continue };
            let Some(data) = meshes.get(&mesh.0).and_then(mesh_data) else { continue };
            let shards = shatter(&data);
            if shards.is_empty() {
                continue;
            }
            let both_sides = mats.get(&mat.0).cloned().map(|m| StandardMaterial { cull_mode: None, double_sided: true, ..m });
            let mat = both_sides.map_or(mat.0.clone(), |m| mats.add(m));
            let mesh = meshes.add(to_mesh(blown(&shards, e.building.raw(), 0.0)));
            pieces.push((Pieces { shards, mesh: mesh.clone() }, Mesh3d(mesh), MeshMaterial3d(mat), NoFrustumCulling));
        }
        let mut root = commands.spawn((Explosion { start: e.start, seed: e.building.raw(), game: clock.game }, *grounded, *tilted, *transform, Visibility::Inherited));
        if let Some(scaled) = scaled {
            root.insert(*scaled);
        }
        root.with_children(|r| {
            for p in pieces {
                r.spawn(p);
            }
        });
    }
}

/// Poses each explosion's polygons at the drawn moment; gone once over, or with its game.
fn fly(
    mut commands: Commands,
    now: Res<SimNow>,
    clock: Res<SimClock>,
    explosions: Query<(Entity, &Explosion, &Children)>,
    pieces: Query<&Pieces>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (e, explosion, children) in &explosions {
        let t = now.secs_since(explosion.start);
        if t > DURATION || explosion.game != clock.game {
            commands.entity(e).despawn();
            continue;
        }
        for p in pieces.iter_many(children) {
            let data = blown(&p.shards, explosion.seed, t);
            if let Some(mut mesh) = meshes.get_mut(&p.mesh) {
                mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, data.positions);
                mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals);
            }
        }
    }
}

/// The buildings sandbox's row blown up in a loop, by index; empty on any other map.
pub fn blast_row(map: &GameMap) -> Vec<usize> {
    if map.name != SANDBOX_BUILDINGS {
        return Vec::new();
    }
    let spots: Vec<_> = SANDBOX_BLASTS.iter().enumerate().map(|(i, &kind)| (kind, sandbox_blast_corner(i))).collect();
    (0..map.buildings.len()).filter(|&i| spots.contains(&(map.buildings[i].kind, (map.buildings[i].x, map.buildings[i].z)))).collect()
}

/// The sandbox loop: the row blown up (removed, its explosion drawn) once it has stood `STANDING`, built again
/// whole (new ids: each round flies its own way) once the explosion is over.
#[derive(Default)]
struct Round {
    game: u32,
    next: Option<Tick>,
    blown: Vec<Building>,
    since: Tick,
}

fn sandbox_loop(mut map: ResMut<CurrentMap>, now: Res<SimNow>, clock: Res<SimClock>, mut round: Local<Round>, mut exploded: MessageWriter<BuildingExploded>) {
    if round.game != clock.game {
        *round = Round { game: clock.game, ..default() };
    }
    if round.blown.is_empty() {
        let next = *round.next.get_or_insert(now.tick + STANDING);
        let row = blast_row(&map.0);
        if now.secs_since(next) < 0.0 || row.is_empty() {
            return;
        }
        round.blown = row.iter().map(|&i| map.0.buildings[i].clone()).collect();
        round.since = now.tick;
        for b in &round.blown {
            exploded.write(BuildingExploded { building: b.id, start: now.tick });
        }
        let gone: Vec<BuildingId> = round.blown.iter().map(|b| b.id).collect();
        map.0.buildings.retain(|b| !gone.contains(&b.id));
    } else if now.secs_since(round.since) > DURATION {
        for b in std::mem::take(&mut round.blown) {
            map.0.add_building(b);
        }
        round.next = Some(now.tick + STANDING);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad_and_triangle() -> MeshData {
        let positions = vec![[0.0, 1.0, 0.0], [1.0, 1.0, 0.0], [1.0, 2.0, 0.0], [0.0, 2.0, 0.0], [3.0, 0.0, 0.0], [4.0, 0.0, 0.0], [3.0, 0.0, 1.0]];
        MeshData { normals: vec![[0.0, 0.0, 1.0]; 7], uvs: vec![[0.0; 2]; 7], indices: vec![0, 1, 2, 0, 2, 3, 4, 5, 6], positions, colors: Vec::new() }
    }

    #[test]
    fn the_sandbox_row_is_found_on_the_sandbox_only() {
        let map = GameMap::sandbox_buildings();
        let row = blast_row(&map);
        assert_eq!(row.iter().map(|&i| map.buildings[i].kind).collect::<Vec<_>>(), SANDBOX_BLASTS);
        assert!(row.iter().all(|&i| map.buildings[i].stage() == game_core::building::Stage::Built));
        assert!(blast_row(&GameMap::sandbox_units()).is_empty());
    }

    #[test]
    fn a_quad_flies_as_one_polygon() {
        let shards = shatter(&quad_and_triangle());
        assert_eq!(shards.len(), 2);
        assert_eq!(shards[0].corners.len(), 6, "both triangles of the quad");
        assert_eq!(shards[0].middle, Vec3::new(0.5, 1.5, 0.0));
        assert_eq!(shards[1].corners.len(), 3);
    }

    #[test]
    fn at_the_blast_the_building_is_whole() {
        let data = quad_and_triangle();
        let shards = shatter(&data);
        let whole = blown(&shards, 7, 0.0);
        let mut corners: Vec<[i32; 3]> = whole.positions.iter().map(|p| p.map(|c| (c * 1000.0).round() as i32)).collect();
        corners.sort_unstable();
        corners.dedup();
        let mut expected: Vec<[i32; 3]> = data.positions.iter().map(|p| p.map(|c| (c * 1000.0).round() as i32)).collect();
        expected.sort_unstable();
        assert_eq!(corners, expected);
    }

    #[test]
    fn polygons_fly_out_and_up_then_lie_on_the_ground() {
        let middle = Vec3::new(1.0, 1.0, 0.0);
        let (early, _, _) = pose(middle, 3, 0, 0.1);
        assert!(early.x > middle.x && early.y > middle.y, "out and up: {early}");
        let (landed, turn, _) = pose(middle, 3, 0, 2.0);
        assert!(landed.y.abs() < 1e-3, "on the ground: {landed}");
        assert_eq!(pose(middle, 3, 0, 2.5).1, turn, "no longer spinning");
    }

    #[test]
    fn they_shrink_away_at_the_end() {
        let middle = Vec3::ONE;
        assert_eq!(pose(middle, 1, 0, 0.5).2, 1.0);
        assert!(pose(middle, 1, 0, DURATION - FADE / 2.0).2 < 1.0);
        assert_eq!(pose(middle, 1, 0, DURATION).2, 0.0);
    }

    #[test]
    fn each_building_and_polygon_its_own_throw() {
        let middle = Vec3::new(1.0, 1.0, 1.0);
        assert_eq!(pose(middle, 5, 2, 0.3), pose(middle, 5, 2, 0.3), "same seed, same pose");
        assert_ne!(pose(middle, 5, 2, 0.3).0, pose(middle, 6, 2, 0.3).0);
        assert_ne!(pose(middle, 5, 2, 0.3).0, pose(middle, 5, 3, 0.3).0);
    }
}
