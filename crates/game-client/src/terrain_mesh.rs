//! Pure (engine-free) construction of the camera-centred, curved terrain grid.
//!
//! The mesh always sits at the render origin, which is the camera focus. Each frame
//! we sample the wrapping heightmap around the focus and bend vertices down with
//! `y = height - k * (dx² + dz²)`, which makes the flat torus look like a small planet.

use game_core::terrain::Heightmap;

#[derive(Clone, Copy, Debug)]
pub struct CurveParams {
    /// Cells drawn on each side of the focus.
    pub radius: i32,
    pub height_scale: f32,
    pub curvature: f32,
}

impl Default for CurveParams {
    fn default() -> Self {
        CurveParams { radius: 64, height_scale: 1.0 / 384.0, curvature: 0.012 }
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
            g.positions.push([dx, y, dz]);
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
            g.indices.extend([a, c, b, b, c, d]);
        }
    }
    g
}

fn grid_normals(p: &[[f32; 3]], n: usize) -> Vec<[f32; 3]> {
    let at = |i: usize, j: usize| p[j.min(n - 1) * n + i.min(n - 1)];
    let mut out = Vec::with_capacity(p.len());
    for j in 0..n {
        for i in 0..n {
            let (l, r) = (at(i.saturating_sub(1), j), at(i + 1, j));
            let (d, u) = (at(i, j.saturating_sub(1)), at(i, j + 1));
            let ex = [r[0] - l[0], r[1] - l[1], r[2] - l[2]];
            let ez = [u[0] - d[0], u[1] - d[1], u[2] - d[2]];
            let c = [
                ez[1] * ex[2] - ez[2] * ex[1],
                ez[2] * ex[0] - ez[0] * ex[2],
                ez[0] * ex[1] - ez[1] * ex[0],
            ];
            let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt().max(1e-6);
            out.push([c[0] / len, c[1] / len, c[2] / len]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_centred_and_curves_down() {
        let map = Heightmap::new(128);
        let p = CurveParams { radius: 4, ..Default::default() };
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
        let p = CurveParams { radius: 1, curvature: 0.0, ..Default::default() };
        let g = build(&map, (127.0, 127.0), &p);
        assert_eq!(g.positions[8][1], 2.0, "cell (128,128) is cell (0,0)");
    }

    #[test]
    fn flat_ground_normals_point_up() {
        let map = Heightmap::new(16);
        let p = CurveParams { radius: 2, curvature: 0.0, ..Default::default() };
        let g = build(&map, (3.0, 3.0), &p);
        assert!(g.normals.iter().all(|n| (n[1] - 1.0).abs() < 1e-5));
    }
}
