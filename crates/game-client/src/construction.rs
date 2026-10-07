//! Meshes for buildings under construction (docs/specs/buildings.md "Building it"): the wooden
//! structure of the building's shape (its edges as beams), the parts of the real model already
//! built (one band per piece of wood, from the ground up), and where a hut's chimney smokes.
//! Pure functions on `MeshData`, in the building's own frame (cells, y up).

use crate::original_models::MeshData;
use bevy::math::Vec3;
use pop3_format::{Object, WORLD_UNITS_PER_CELL};

/// Beam thickness (cells) of the wooden structure.
pub const BEAM: f32 = 0.022;

/// A segment between two points (cells).
pub type Edge = (Vec3, Vec3);

/// The triangles built with `used` of `of` pieces: triangles sorted by the height of their
/// centre (lowest first, stable), the first `used / of` of them. A building grows from the ground
/// up, one band per piece, and every piece shows something.
pub fn built_part(mesh: &MeshData, used: u8, of: u8) -> MeshData {
    let tris = mesh.indices.len() / 3;
    let keep = if of == 0 { tris } else { (tris * used.min(of) as usize).div_ceil(of as usize) };
    let centre_y = |t: usize| (0..3).map(|k| mesh.positions[mesh.indices[t * 3 + k] as usize][1]).sum::<f32>();
    let mut order: Vec<usize> = (0..tris).collect();
    order.sort_by(|&a, &b| centre_y(a).total_cmp(&centre_y(b)));
    let mut out = MeshData::default();
    for &t in &order[..keep] {
        for k in 0..3 {
            let i = mesh.indices[t * 3 + k] as usize;
            out.indices.push(out.positions.len() as u32);
            out.positions.push(mesh.positions[i]);
            out.normals.push(mesh.normals[i]);
            out.uvs.push(mesh.uvs[i]);
        }
    }
    out
}

/// Gap (cells) between a face and its tribe-coloured inner side (`pushed`).
pub const INNER_GAP: f32 = 0.006;

/// Every triangle moved `by` along its own normal (away from the side its back is seen from). The
/// inner side of a face is drawn from this copy: where the model has two faces back to back on one
/// plane, the inner one then lies behind the textured one instead of fighting it.
pub fn pushed(mesh: &MeshData, by: f32) -> MeshData {
    let mut out = MeshData { positions: Vec::with_capacity(mesh.positions.len()), ..mesh.clone() };
    for (p, n) in mesh.positions.iter().zip(&mesh.normals) {
        out.positions.push((Vec3::from(*p) + Vec3::from(*n) * by).into());
    }
    out
}

/// The edges of an original object's faces, once each (shared edges and quads' diagonals are not
/// doubled), in cells.
pub fn object_edges(obj: &Object) -> Vec<Edge> {
    let mut seen = std::collections::BTreeSet::new();
    let scale = 1.0 / WORLD_UNITS_PER_CELL as f32;
    let point = |i: u16| {
        let p = obj.points[i as usize];
        Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32) * scale
    };
    let mut edges = Vec::new();
    for face in &obj.faces {
        let n = face.points.len();
        for k in 0..n {
            let (a, b) = (face.points[k], face.points[(k + 1) % n]);
            if a != b && seen.insert((a.min(b), a.max(b))) {
                edges.push((point(a), point(b)));
            }
        }
    }
    edges
}

/// A box `size` (cells) standing on the origin, as `layers` stacked rings of side walls and a lid:
/// it grows layer by layer through `built_part`. Stand-in for buildings without a model.
pub fn layered_box(size: Vec3, layers: u8) -> MeshData {
    let layers = layers.max(1);
    let (hx, hz) = (size.x / 2.0, size.z / 2.0);
    let mut m = MeshData::default();
    let mut quad = |c: [Vec3; 4], n: Vec3| {
        for k in [0, 1, 2, 0, 2, 3] {
            m.indices.push(m.positions.len() as u32);
            m.positions.push(c[k].into());
            m.normals.push(n.into());
            m.uvs.push([0.0, 0.0]);
        }
    };
    for l in 0..layers {
        let (y0, y1) = (size.y * l as f32 / layers as f32, size.y * (l + 1) as f32 / layers as f32);
        let corners = [Vec3::new(-hx, 0.0, -hz), Vec3::new(hx, 0.0, -hz), Vec3::new(hx, 0.0, hz), Vec3::new(-hx, 0.0, hz)];
        for k in 0..4 {
            let (a, b) = (corners[k], corners[(k + 1) % 4]);
            let n = (b - a).cross(Vec3::Y).normalize();
            quad([a + Vec3::Y * y1, b + Vec3::Y * y1, b + Vec3::Y * y0, a + Vec3::Y * y0], -n);
        }
    }
    let top = size.y;
    quad([Vec3::new(-hx, top, -hz), Vec3::new(-hx, top, hz), Vec3::new(hx, top, hz), Vec3::new(hx, top, -hz)], Vec3::Y);
    m
}

/// The wooden structure of a `layered_box`: four corner posts and a ring of beams per layer.
pub fn box_frame(size: Vec3, layers: u8) -> Vec<Edge> {
    let (hx, hz) = (size.x / 2.0, size.z / 2.0);
    let corners = [Vec3::new(-hx, 0.0, -hz), Vec3::new(hx, 0.0, -hz), Vec3::new(hx, 0.0, hz), Vec3::new(-hx, 0.0, hz)];
    let mut edges: Vec<Edge> = corners.iter().map(|&c| (c, c + Vec3::Y * size.y)).collect();
    for l in 1..=layers.max(1) {
        let y = Vec3::Y * size.y * l as f32 / layers.max(1) as f32;
        edges.extend((0..4).map(|k| (corners[k] + y, corners[(k + 1) % 4] + y)));
    }
    edges
}

/// Each edge as a square beam `thickness` wide (flat-shaded boxes).
pub fn beams(edges: &[Edge], thickness: f32) -> MeshData {
    let mut m = MeshData::default();
    let h = thickness / 2.0;
    for &(a, b) in edges {
        let d = (b - a).normalize_or_zero();
        if d == Vec3::ZERO {
            continue;
        }
        let side = if d.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
        let u = d.cross(side).normalize() * h;
        let v = d.cross(u).normalize() * h;
        let ring = |p: Vec3| [p - u - v, p + u - v, p + u + v, p - u + v];
        let (ra, rb) = (ring(a), ring(b));
        let mut quad = |c: [Vec3; 4]| {
            let n = (c[1] - c[0]).cross(c[2] - c[0]).normalize_or(Vec3::Y);
            for k in [0, 1, 2, 0, 2, 3] {
                m.indices.push(m.positions.len() as u32);
                m.positions.push(c[k].into());
                m.normals.push(n.into());
                m.uvs.push([0.0, 0.0]);
            }
        };
        for k in 0..4 {
            let j = (k + 1) % 4;
            quad([ra[k], ra[j], rb[j], rb[k]]);
        }
        quad([ra[3], ra[2], ra[1], ra[0]]);
        quad([rb[0], rb[1], rb[2], rb[3]]);
    }
    m
}

/// Where a hut's smoke comes out: the middle of its highest points (the top of the roof).
pub fn chimney(mesh: &MeshData) -> Vec3 {
    let top = mesh.positions.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
    let high: Vec<Vec3> = mesh.positions.iter().map(|&p| Vec3::from(p)).filter(|p| p.y > top - 0.05).collect();
    if high.is_empty() {
        return Vec3::ZERO;
    }
    high.iter().sum::<Vec3>() / high.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use pop3_format::Face;

    fn heights(m: &MeshData) -> Vec<f32> {
        m.indices.chunks(3).map(|t| t.iter().map(|&i| m.positions[i as usize][1]).sum::<f32>() / 3.0).collect()
    }

    #[test]
    fn built_part_grows_from_the_ground() {
        let full = layered_box(Vec3::new(1.0, 1.0, 1.0), 4);
        let tris = full.indices.len() / 3;
        assert_eq!(tris, 4 * 8 + 2);
        assert!(built_part(&full, 0, 4).indices.is_empty());
        assert_eq!(built_part(&full, 4, 4).indices.len(), full.indices.len());
        let quarter = built_part(&full, 1, 4);
        assert_eq!(quarter.indices.len() / 3, tris.div_ceil(4));
        assert!(heights(&quarter).iter().all(|&y| y < 0.5), "the lowest layers first");
        let mut last = 0;
        for used in 0..=4 {
            let n = built_part(&full, used, 4).indices.len();
            assert!(n >= last, "every piece adds parts");
            last = n;
        }
        assert_eq!(built_part(&full, 9, 4).indices.len(), full.indices.len(), "never past the whole");
    }

    #[test]
    fn edges_of_a_quad_without_its_diagonal() {
        let obj = Object {
            points: vec![[0, 0, 0], [512, 0, 0], [512, 512, 0], [0, 512, 0], [0, 0, 512]],
            faces: vec![
                Face { tile: None, colour: 0, points: vec![0, 1, 2, 3], uv: vec![(0, 0); 4] },
                Face { tile: None, colour: 0, points: vec![0, 1, 4], uv: vec![(0, 0); 3] },
            ],
        };
        let edges = object_edges(&obj);
        assert_eq!(edges.len(), 6, "4 of the quad + 2 more of the triangle, 0-1 shared");
        assert!(edges.contains(&(Vec3::ZERO, Vec3::X)));
    }

    #[test]
    fn box_frame_posts_and_rings() {
        let edges = box_frame(Vec3::new(2.0, 1.0, 2.0), 3);
        assert_eq!(edges.len(), 4 + 3 * 4);
        assert!(edges.iter().all(|(a, b)| a.y >= 0.0 && b.y <= 1.0 + 1e-6));
    }

    #[test]
    fn a_beam_is_a_closed_box_along_its_edge() {
        let m = beams(&[(Vec3::ZERO, Vec3::new(0.0, 2.0, 0.0)), (Vec3::ZERO, Vec3::ZERO)], 0.1);
        assert_eq!(m.indices.len(), 6 * 6, "one box, the empty edge skipped");
        assert!(m.positions.iter().all(|p| p[0].abs() <= 0.05 + 1e-6 && p[2].abs() <= 0.05 + 1e-6 && (0.0..=2.0).contains(&p[1])));
    }

    /// Every triangle wound counter-clockwise seen from outside: its winding normal agrees with
    /// its stored normal and points away from `centre`.
    fn faces_out(m: &MeshData, centre: Vec3) {
        for t in m.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(m.positions[t[k] as usize]));
            let wound = (b - a).cross(c - a);
            assert!(wound.dot(Vec3::from(m.normals[t[0] as usize])) > 0.0, "winding against the normal");
            assert!(wound.dot((a + b + c) / 3.0 - centre) > 0.0, "facing in");
        }
    }

    #[test]
    fn shapes_face_outward() {
        faces_out(&layered_box(Vec3::new(1.6, 0.8, 1.6), 3), Vec3::new(0.0, 0.4, 0.0));
        faces_out(&beams(&[(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0))], 0.1), Vec3::new(0.5, 0.0, 0.0));
        faces_out(&beams(&[(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0))], 0.1), Vec3::new(0.0, 0.5, 0.0));
    }

    #[test]
    fn inner_side_pushed_behind_the_face() {
        let m = layered_box(Vec3::ONE, 1);
        let p = pushed(&m, 0.01);
        assert_eq!((p.indices.len(), p.normals.len()), (m.indices.len(), m.normals.len()));
        for ((a, b), n) in m.positions.iter().zip(&p.positions).zip(&m.normals) {
            let d = Vec3::from(*b) - Vec3::from(*a);
            assert!((d - Vec3::from(*n) * 0.01).length() < 1e-6);
        }
    }

    #[test]
    fn chimney_at_the_top() {
        let c = chimney(&layered_box(Vec3::new(2.0, 1.5, 2.0), 2));
        assert!((c - Vec3::new(0.0, 1.5, 0.0)).length() < 1e-5);
    }
}
