//! Ground Shake: the trembling ground under Volcano's own pre-eruption rumble and Earthquake's own tremor
//! (`game_core::effect::GroundShake`, shared between the two the same way `lava` is shared by their aftermaths).
//! A draped overlay patch (`ground_mesh`, the same per-cell draping `lava.rs`'s own patch uses) wearing the real
//! terrain's own baked material (`world::TerrainMaterial`), so it reads as the ground itself trembling rather
//! than a separate effect sitting on top of it. Peak lift (`LIFT`) and the jitter shape are our own invention,
//! picked by eye against a throwaway prototype: the trace gives no magnitude for either's own "rumbling"/"shake".

use crate::camera::{CameraRig, CurveParamsRes};
use crate::grounded::ground_point;
use crate::original_models::MeshData;
use crate::sim_time::{seed, SimNow};
use crate::terrain_mesh::CurveParams;
use crate::views::{sync_views, WorldView};
use crate::world::{CurrentMap, TerrainMaterial};
use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use game_core::effect::{Effect, GroundShake};
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::terrain::Heightmap;
use game_core::time::TICKS_PER_SECOND;
use pop3_format::WORLD_UNITS_PER_CELL;
use std::f32::consts::TAU;

/// World units to the cell they fall in, unwrapped (`game_core::unit::cell_of`, private to that crate).
fn cell_of(v: u16) -> i32 {
    (v as u32 / WORLD_UNITS_PER_CELL) as i32
}

/// Peak lift at full intensity, right at the epicentre (render units): picked by eye against a POC, not an
/// original-game number.
const LIFT: f32 = 0.3;
/// How fast a shaking vertex trembles (Hz): same reasoning as `LIFT`.
const FREQ: f32 = 2.2;
/// How far above the true ground the patch sits (`lava.rs`'s own `LIFT`, same reason: no z-fighting).
const HOVER: f32 = 0.02;

fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}
fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = clamp01((x - a) / (b - a));
    t * t * (3.0 - 2.0 * t)
}

/// How much `at` (cell corner, unwrapped) is still trembling: `shake.intensity` at `centre`, fading to 0 by
/// `shake.radius_cells`.
fn strength_at(shake: GroundShake, centre: (i32, i32), at: (i32, i32)) -> f32 {
    let d = (((at.0 - centre.0).pow(2) + (at.1 - centre.1).pow(2)) as f32).sqrt();
    shake.intensity * (1.0 - smoothstep(shake.radius_cells * 0.45, shake.radius_cells, d))
}

/// A corner's own trembling, in `[-1, 1]`: two summed sines out of phase per corner (`sim_time::seed`, the same
/// stable per-id phase every other view uses) so neighbours don't move in lockstep.
fn jitter(corner: (i32, i32), elapsed_secs: f32) -> f32 {
    let key = (corner.0 as u32).wrapping_mul(0x27D4_EB2F) ^ (corner.1 as u32).wrapping_mul(0x9E37_79B9);
    let phase = seed(key) * TAU;
    0.6 * (elapsed_secs * FREQ * TAU + phase).sin() + 0.4 * (elapsed_secs * FREQ * TAU * 2.13 + phase * 1.7 + 1.3).sin()
}

/// One quad per cell, draped on the real terrain like `lava.rs`'s own `ground_mesh`, each corner lifted by its
/// own trembling on top of the true height. UVs match `terrain_mesh::build`'s own scheme (absolute coordinates
/// over the map's size), not a repeating `0..1` per quad like `lava.rs`'s own patch: this one wears the single,
/// whole-map baked ground texture (`TerrainMaterial`), not a small repeating tile.
fn ground_mesh(cells: &[(i32, i32)], shake: GroundShake, centre: (i32, i32), elapsed_secs: f32, terrain: &Heightmap, focus: Vec2, params: &CurveParams) -> MeshData {
    let size = terrain.size() as f32;
    let mut m = MeshData::default();
    for &(cx, cz) in cells {
        let base = m.positions.len() as u32;
        for (ox, oz) in [(0, 0), (1, 0), (1, 1), (0, 1)] {
            let corner = (cx + ox, cz + oz);
            let lift = LIFT * strength_at(shake, centre, corner) * jitter(corner, elapsed_secs);
            let at = Vec2::new(corner.0 as f32, corner.1 as f32);
            let p = ground_point(terrain, focus, params, at) + Vec3::Y * (HOVER + lift);
            m.positions.push(p.into());
            m.normals.push([0.0, 1.0, 0.0]);
            m.uvs.push([corner.0 as f32 / size, corner.1 as f32 / size]);
        }
        m.indices.extend([base, base + 1, base + 3, base + 1, base + 2, base + 3]);
    }
    m
}

fn set_ground_mesh(mesh: &mut Mesh, data: MeshData) {
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, data.positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, data.uvs);
    mesh.insert_indices(Indices::U32(data.indices));
}

/// The effect's own current shake, its cell footprint, centre and how long it's been running, if it has one.
fn effect_shake(e: &Effect, size: i32) -> Option<(GroundShake, Vec<(i32, i32)>, (i32, i32), f32)> {
    let shake = e.ground_shake()?;
    let elapsed_secs = e.left.done().get() as f32 / TICKS_PER_SECOND as f32;
    let centre = (cell_of(e.x), cell_of(e.z));
    let cells = game_core::lava::disc((e.x, e.z), shake.radius_cells, size);
    Some((shake, cells, centre, elapsed_secs))
}

/// One effect's own trembling patch: a draped mesh rebuilt every frame (like `LavaView`'s own) so it tracks the
/// camera's live focus and the torus wrap.
#[derive(Component)]
pub struct GroundShakeView {
    mesh: Handle<Mesh>,
}

impl WorldView for GroundShakeView {
    type Model = (Id<Effect>, Effect);
    type Params = (Res<'static, CurrentMap>, Res<'static, CameraRig>, Res<'static, CurveParamsRes>, Res<'static, TerrainMaterial>, ResMut<'static, Assets<Mesh>>);

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| e.ground_shake().is_some())
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((_, e): &(Id<Effect>, Effect), view: &mut EntityCommands, (map, rig, params, material, meshes): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let size = map.0.terrain.size() as i32;
        let (shake, cells, centre, elapsed) = effect_shake(e, size)?;
        if cells.is_empty() {
            return None;
        }
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        set_ground_mesh(&mut mesh, ground_mesh(&cells, shake, centre, elapsed, &map.0.terrain, rig.focus, &params.0));
        let mesh = meshes.add(mesh);
        view.insert((Mesh3d(mesh.clone()), MeshMaterial3d(material.0.clone()), Transform::default(), Visibility::Visible, NotShadowCaster, crate::hover::NoOutline));
        Some(GroundShakeView { mesh })
    }

    fn update(&self, (_, e): &(Id<Effect>, Effect), _now: SimNow, (map, rig, params, _material, meshes): &mut SystemParamItem<Self::Params>) {
        let size = map.0.terrain.size() as i32;
        let Some((shake, cells, centre, elapsed)) = effect_shake(e, size) else { return };
        if let Some(mut m) = meshes.get_mut(&self.mesh) {
            set_ground_mesh(&mut m, ground_mesh(&cells, shake, centre, elapsed, &map.0.terrain, rig.focus, &params.0));
        }
    }
}

pub struct GroundShakePlugin;

impl Plugin for GroundShakePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_views::<GroundShakeView>.after(crate::units::run_ticks));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHAKE: GroundShake = GroundShake { radius_cells: 8.0, intensity: 1.0 };

    #[test]
    fn strength_is_full_at_the_centre_and_gone_at_the_radius() {
        assert_eq!(strength_at(SHAKE, (0, 0), (0, 0)), 1.0);
        assert_eq!(strength_at(SHAKE, (0, 0), (8, 0)), 0.0);
        let mid = strength_at(SHAKE, (0, 0), (4, 0));
        assert!(mid > 0.0 && mid < 1.0, "fades in between: {mid}");
    }

    #[test]
    fn strength_scales_with_intensity() {
        let half = GroundShake { radius_cells: 8.0, intensity: 0.5 };
        assert_eq!(strength_at(half, (0, 0), (0, 0)), 0.5);
    }

    #[test]
    fn jitter_stays_in_range_and_differs_between_neighbours() {
        for t in [0.0, 0.3, 1.7, 6.4] {
            let j = jitter((5, 5), t);
            assert!((-1.0..=1.0).contains(&j), "out of range: {j}");
        }
        assert_ne!(jitter((5, 5), 1.0), jitter((6, 5), 1.0), "neighbours don't move in lockstep");
    }

    #[test]
    fn ground_mesh_is_one_quad_per_cell() {
        let terrain = Heightmap::new(16);
        let params = CurveParams::default();
        let m = ground_mesh(&[(0, 0), (1, 0)], SHAKE, (0, 0), 0.0, &terrain, Vec2::ZERO, &params);
        assert_eq!(m.positions.len(), 8);
        assert_eq!(m.indices.len(), 12);
    }
}
