//! Pure construction of the camera-centred, curved terrain grid, in render space (`game_frame`: the game's z
//! drawn mirrored).
//!
//! The mesh always sits at the render origin, which is the camera focus. Each frame
//! we sample the wrapping heightmap around the focus and bend vertices down with
//! `y = height - k * (dx² + dz²)`, which makes the flat torus look like a small planet.

use crate::game_frame::{mirrored, GamePos};
use bevy::math::Vec3;
use game_core::terrain::{DirtyRect, Heightmap};

#[derive(Clone, Copy, Debug)]
pub struct CurveParams {
    /// Cells drawn on each side of the focus.
    pub radius: i32,
    pub height_scale: f32,
    pub curvature: f32,
}

/// Height units per render unit at relief x1: the original's, 1024 = 4 cells (pop3-rev-analysis.md "Landscape
/// projection").
pub const BASE_HEIGHT_SCALE: f32 = 1.0 / 256.0;
/// Default relief: the original's.
pub const DEFAULT_RELIEF: f32 = 1.0;
/// Default planet bend (`drop = curvature * distance²`): the original's 0.01097 per cell².
pub const DEFAULT_CURVATURE: f32 = 0.011;

impl Default for CurveParams {
    fn default() -> Self {
        CurveParams { radius: 64, height_scale: BASE_HEIGHT_SCALE * DEFAULT_RELIEF, curvature: DEFAULT_CURVATURE }
    }
}

impl CurveParams {
    /// Relief multiplier relative to the original height ratio.
    pub fn relief(&self) -> f32 {
        self.height_scale / BASE_HEIGHT_SCALE
    }

    pub fn with_relief(self, relief: f32) -> Self {
        CurveParams { height_scale: BASE_HEIGHT_SCALE * relief, ..self }
    }
}

#[derive(Default)]
pub struct TerrainGeometry {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Absolute map coordinates / map size: tiles with a repeating sampler.
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

pub fn drop_at(params: &CurveParams, dx: f32, dz: f32) -> f32 {
    params.curvature * (dx * dx + dz * dz)
}

/// Rendered height of the focus point (used to aim the camera).
pub fn focus_height(map: &Heightmap, focus: (f32, f32), params: &CurveParams) -> f32 {
    map.sample(focus.0, focus.1) * params.height_scale
}

pub fn build(map: &Heightmap, focus: (f32, f32), params: &CurveParams) -> TerrainGeometry {
    let r = params.radius;
    let n = (2 * r + 1) as usize;
    let (bx, bz) = (focus.0.floor() as i32, focus.1.floor() as i32);
    let (fx, fz) = (focus.0 - bx as f32, focus.1 - bz as f32);
    let mut g = TerrainGeometry {
        positions: Vec::with_capacity(n * n),
        ..Default::default()
    };
    for j in -r..=r {
        for i in -r..=r {
            let (dx, dz) = (i as f32 - fx, j as f32 - fz);
            let h = map.get(bx + i, bz + j);
            let y = h as f32 * params.height_scale - drop_at(params, dx, dz);
            g.positions.push(Vec3::from(GamePos::ground(dx, y, dz)).into());
            let size = map.size() as f32;
            g.uvs.push([(bx + i) as f32 / size, (bz + j) as f32 / size]);
        }
    }
    g.normals = grid_normals(&g.positions, n);
    for j in 0..n - 1 {
        for i in 0..n - 1 {
            let (cx, cz) = (i as f32 + 0.5 - r as f32, j as f32 + 0.5 - r as f32);
            if cx * cx + cz * cz > (r * r) as f32 {
                continue;
            }
            let a = (j * n + i) as u32;
            let (b, c, d) = (a + 1, a + n as u32, a + n as u32 + 1);
            g.indices.extend(mirrored([a, c, b]).into_iter().chain(mirrored([b, c, d])));
        }
    }
    g
}

fn grid_normals(p: &[[f32; 3]], n: usize) -> Vec<[f32; 3]> {
    let mut out = Vec::with_capacity(p.len());
    for j in 0..n {
        for i in 0..n {
            out.push(vertex_normal(p, n, i, j));
        }
    }
    out
}

/// One vertex's normal at grid index `(i, j)` (`grid_normals`'s own per-vertex formula, reused by `patch` to
/// recompute just the handful of normals a dirty rect touches).
fn vertex_normal(p: &[[f32; 3]], n: usize, i: usize, j: usize) -> [f32; 3] {
    let at = |i: usize, j: usize| p[j.min(n - 1) * n + i.min(n - 1)];
    let (l, r) = (at(i.saturating_sub(1), j), at(i + 1, j));
    let (d, u) = (at(i, j.saturating_sub(1)), at(i, j + 1));
    let ex = [r[0] - l[0], r[1] - l[1], r[2] - l[2]];
    let ez = [u[0] - d[0], u[1] - d[1], u[2] - d[2]];
    // Rows run along the game's z, drawn towards -z: ex x ez points up.
    let c = [ex[1] * ez[2] - ex[2] * ez[1], ex[2] * ez[0] - ex[0] * ez[2], ex[0] * ez[1] - ex[1] * ez[0]];
    let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt().max(1e-6);
    [c[0] / len, c[1] / len, c[2] / len]
}

/// A grid offset (from `anchor`) landing cell `c` within `anchor ± radius`, wrapping at most once either way
/// (the disc's own radius never spans more than one map width).
fn offsets_in_range(c: i32, anchor: i32, radius: i32, size: i32) -> impl Iterator<Item = i32> {
    let base = c - anchor;
    (-1..=1).map(move |k| base + k * size).filter(move |i| i.abs() <= radius)
}

/// Recomputes just the vertices `dirty` touches, in place: `build`'s own grid, without walking the whole disc
/// again. `g` must already be the grid for this `map`/`focus`/`params` (the camera hasn't moved since, nothing
/// else changed it): a terrain edit's `DirtyRect` (`apply_brush`, `volcano::apply`, ...) is a few cells, so most
/// of the disc's `(2r+1)^2` vertices never need touching — the win that matters while an ongoing effect (Volcano's
/// rise, its lava flow) dirties the ground almost every tick. UVs and indices never change here: they depend only
/// on the anchor and the grid's own topology, neither of which moved. Returns whether anything in the drawn disc
/// was actually touched, so a caller can skip re-uploading the mesh entirely when every dirty cell lies outside it
/// (an effect running off-screen).
pub fn patch(g: &mut TerrainGeometry, map: &Heightmap, focus: (f32, f32), params: &CurveParams, dirty: &[DirtyRect]) -> bool {
    let r = params.radius;
    let n = (2 * r + 1) as usize;
    let size = map.size() as i32;
    let (bx, bz) = (focus.0.floor() as i32, focus.1.floor() as i32);
    let (fx, fz) = (focus.0 - bx as f32, focus.1 - bz as f32);

    let mut touched: Vec<(i32, i32)> = Vec::new();
    for rect in dirty {
        for z in rect.min.1..=rect.max.1 {
            for x in rect.min.0..=rect.max.0 {
                for i in offsets_in_range(x, bx, r, size) {
                    for j in offsets_in_range(z, bz, r, size) {
                        touched.push((i, j));
                    }
                }
            }
        }
    }
    if touched.is_empty() {
        return false;
    }

    for &(i, j) in &touched {
        let (dx, dz) = (i as f32 - fx, j as f32 - fz);
        let h = map.get(bx + i, bz + j);
        let y = h as f32 * params.height_scale - drop_at(params, dx, dz);
        g.positions[(j + r) as usize * n + (i + r) as usize] = Vec3::from(GamePos::ground(dx, y, dz)).into();
    }

    // A vertex's normal also depends on its 4 neighbours: recompute every touched vertex's own and theirs.
    let mut dirty_normals: std::collections::BTreeSet<(usize, usize)> = std::collections::BTreeSet::new();
    for &(i, j) in &touched {
        let (gi, gj) = (i + r, j + r);
        for (di, dj) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (ni, nj) = (gi + di, gj + dj);
            if (0..n as i32).contains(&ni) && (0..n as i32).contains(&nj) {
                dirty_normals.insert((ni as usize, nj as usize));
            }
        }
    }
    for (gi, gj) in dirty_normals {
        g.normals[gj * n + gi] = vertex_normal(&g.positions, n, gi, gj);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_centred_and_curves_down() {
        let map = Heightmap::new(128);
        let p = CurveParams { radius: 4, curvature: 0.012, ..Default::default() };
        let g = build(&map, (10.0, 10.0), &p);
        assert_eq!(g.positions.len(), 81);
        assert!(g.indices.len() < 8 * 8 * 6, "clipped to a disc");
        let centre = g.positions[40];
        assert_eq!(centre, [0.0, 0.0, 0.0]);
        assert!(g.positions[0][1] < -0.3, "corners bend below the horizon");
    }

    #[test]
    fn wraps_across_map_edge() {
        let mut map = Heightmap::new(128);
        map.set(0, 0, 768);
        let p = CurveParams { radius: 1, curvature: 0.0, ..Default::default() }.with_relief(1.0);
        let g = build(&map, (127.0, 127.0), &p);
        assert_eq!(g.positions[8][1], 3.0, "cell (128,128) is cell (0,0): 768 = 3 cells");
    }

    #[test]
    fn default_relief_is_twice_the_original_ratio() {
        assert_eq!(CurveParams::default().relief(), DEFAULT_RELIEF);
        assert_eq!(CurveParams::default().with_relief(1.0).height_scale, BASE_HEIGHT_SCALE);
    }

    #[test]
    fn flat_ground_normals_point_up() {
        let map = Heightmap::new(16);
        let p = CurveParams { radius: 2, curvature: 0.0, ..Default::default() };
        let g = build(&map, (3.0, 3.0), &p);
        assert!(g.normals.iter().all(|n| (n[1] - 1.0).abs() < 1e-5));
    }

    #[test]
    fn the_mirrored_grid_faces_up_and_leans_away_from_a_rise() {
        let mut map = Heightmap::new(16);
        for z in 0..16 {
            for x in 0..16 {
                map.set(x, z, (z * 10) as u16);
            }
        }
        let p = CurveParams { radius: 2, curvature: 0.0, height_scale: 0.01 };
        let g = build(&map, (3.0, 3.0), &p);
        let n = g.normals[12];
        assert!(n[2] > 0.05, "rising along the game's z, drawn towards -z: leans to +z {n:?}");
        let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(g.positions[g.indices[k] as usize]));
        assert!((b - a).cross(c - a).y > 0.0, "front faces up");
    }

    #[test]
    fn offsets_in_range_wraps_at_most_once_either_way() {
        assert_eq!(offsets_in_range(5, 10, 4, 128).collect::<Vec<_>>(), Vec::<i32>::new(), "out of range, no wrap needed");
        assert_eq!(offsets_in_range(127, 2, 4, 128).collect::<Vec<_>>(), [-3], "wraps backward across the edge");
        assert_eq!(offsets_in_range(0, 126, 4, 128).collect::<Vec<_>>(), [2], "wraps forward across the edge");
        assert_eq!(offsets_in_range(10, 10, 4, 128).collect::<Vec<_>>(), [0]);
    }

    #[test]
    fn patch_reproduces_a_full_rebuild_after_the_same_terrain_edit() {
        let mut map = Heightmap::new(32);
        for z in 0..32 {
            for x in 0..32 {
                map.set(x, z, 200);
            }
        }
        let p = CurveParams { radius: 6, curvature: 0.01, ..Default::default() };
        let focus = (16.0, 16.0);
        let mut g = build(&map, focus, &p);
        map.set(17, 15, 900);
        let dirty = [DirtyRect { min: (17, 15), max: (17, 15) }];
        assert!(patch(&mut g, &map, focus, &p, &dirty), "the edit lands inside the drawn disc");
        let rebuilt = build(&map, focus, &p);
        assert_eq!(g.positions, rebuilt.positions);
        assert_eq!(g.normals, rebuilt.normals);
        assert_eq!(g.uvs, rebuilt.uvs, "never touched: the anchor didn't move");
        assert_eq!(g.indices, rebuilt.indices, "never touched: same topology");
    }

    #[test]
    fn patch_does_nothing_for_a_dirty_rect_outside_the_drawn_disc() {
        let map = Heightmap::new(128);
        let p = CurveParams { radius: 4, ..Default::default() };
        let focus = (64.0, 64.0);
        let mut g = build(&map, focus, &p);
        let before = (g.positions.clone(), g.normals.clone());
        let dirty = [DirtyRect { min: (0, 0), max: (0, 0) }];
        assert!(!patch(&mut g, &map, focus, &p, &dirty), "far outside the radius-4 disc");
        assert_eq!((g.positions, g.normals), before, "left untouched");
    }

    #[test]
    fn patch_wraps_the_same_way_build_does() {
        let mut map = Heightmap::new(16);
        let p = CurveParams { radius: 2, curvature: 0.0, ..Default::default() }.with_relief(1.0);
        let focus = (15.0, 8.0);
        let mut g = build(&map, focus, &p);
        map.set(0, 8, 300); // cell (16, 8) from the focus's own side: wraps to (0, 8)
        let dirty = [DirtyRect { min: (0, 8), max: (0, 8) }];
        assert!(patch(&mut g, &map, focus, &p, &dirty));
        let rebuilt = build(&map, focus, &p);
        assert_eq!(g.positions, rebuilt.positions, "matches a full rebuild across the wrap");
        assert_eq!(g.normals, rebuilt.normals);
    }
}
