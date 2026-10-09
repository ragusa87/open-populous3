//! Things standing on the ground (site stones, later buildings, trees, units): each part has
//! its own map position and is set on the curved terrain under it, so footprints follow slopes.

use crate::camera::{CameraRig, CurveParamsRes};
use crate::game_frame::{GamePos, GameYaw};
use crate::terrain_mesh::{drop_at, CurveParams};
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::terrain::Heightmap;

/// A part standing on the ground at `at` (cell coordinates, wraps).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Grounded {
    pub at: Vec2,
    /// Half size of the square footprint: the part rests on its lowest corner so it never floats.
    pub half: f32,
}

/// A grounded part that leans with the ground under it (large objects: buildings), turned by `yaw`
/// about its own up: its up follows the ground normal as drawn (slopes and the planet's curve), so
/// no side sinks in.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Tilted {
    /// Half size (cells) of the square the slope is measured over.
    pub half: f32,
    pub yaw: GameYaw,
}

pub struct GroundedPlugin;

impl Plugin for GroundedPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, (place_grounded, tilt_grounded).chain().before(TransformSystems::Propagate));
    }
}

/// Unscaled ground height exactly as the terrain mesh draws it: two triangles per cell,
/// split along the (1,0)-(0,1) diagonal.
pub fn mesh_height(map: &Heightmap, x: f32, z: f32) -> f32 {
    let (x0, z0) = (x.floor(), z.floor());
    let (fx, fz) = (x - x0, z - z0);
    let (x0, z0) = (x0 as i32, z0 as i32);
    let h = |dx, dz| map.get(x0 + dx, z0 + dz) as f32;
    if fx + fz <= 1.0 {
        h(0, 0) + (h(1, 0) - h(0, 0)) * fx + (h(0, 1) - h(0, 0)) * fz
    } else {
        h(1, 1) + (h(0, 1) - h(1, 1)) * (1.0 - fx) + (h(1, 0) - h(1, 1)) * (1.0 - fz)
    }
}

/// Render-space position of a grounded part around the camera focus, taking the shortest way
/// around the torus. None when outside the drawn disc.
pub fn render_pos(map: &Heightmap, g: &Grounded, focus: Vec2, params: &CurveParams) -> Option<Vec3> {
    let size = map.size() as f32;
    let wrap = |d: f32| (d + size / 2.0).rem_euclid(size) - size / 2.0;
    let (dx, dz) = (wrap(g.at.x - focus.x), wrap(g.at.y - focus.y));
    let r = params.radius as f32 - 1.0;
    if dx * dx + dz * dz > r * r {
        return None;
    }
    let corners = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0), (0.0, 0.0)];
    let y = corners
        .iter()
        .map(|&(cx, cz)| {
            let (ox, oz) = (cx * g.half, cz * g.half);
            mesh_height(map, g.at.x + ox, g.at.y + oz) * params.height_scale - drop_at(params, dx + ox, dz + oz)
        })
        .fold(f32::INFINITY, f32::min);
    Some(GamePos::ground(dx, y, dz).into())
}

/// Render point on the ground as the mesh draws it at map position `at` (cells), the short way round
/// the torus from the focus.
pub fn ground_point(map: &Heightmap, focus: Vec2, params: &CurveParams, at: Vec2) -> Vec3 {
    let size = map.size() as f32;
    let wrap = |d: f32| (d + size / 2.0).rem_euclid(size) - size / 2.0;
    let r = Vec3::from(GamePos::ground(wrap(at.x - focus.x), 0.0, wrap(at.y - focus.y)));
    Vec3::new(r.x, ground_y(map, focus, params, r.x, r.z), r.z)
}

/// Render-space ground height at render offset `(dx, dz)` from the focus, as the mesh draws it.
pub fn ground_y(map: &Heightmap, focus: Vec2, params: &CurveParams, dx: f32, dz: f32) -> f32 {
    let at = focus + GamePos::from_render(Vec3::new(dx, 0.0, dz)).xz();
    mesh_height(map, at.x, at.y) * params.height_scale - drop_at(params, dx, dz)
}

/// Map position (cells, wrapped) of the first ground point hit by a render-space ray, None if it
/// leaves the drawn disc first. Marches in small steps, then bisects the crossing.
pub fn pick_ground(map: &Heightmap, focus: Vec2, params: &CurveParams, origin: Vec3, dir: Vec3) -> Option<Vec2> {
    const STEP: f32 = 0.05;
    const MAX_T: f32 = 600.0;
    let r = params.radius as f32 - 1.0;
    let below = |t: f32| {
        let p = origin + dir * t;
        p.y <= ground_y(map, focus, params, p.x, p.z)
    };
    let mut t = 0.0;
    while !below(t + STEP) {
        t += STEP;
        if t > MAX_T {
            return None;
        }
    }
    let (mut lo, mut hi) = (t, t + STEP);
    for _ in 0..16 {
        let mid = (lo + hi) / 2.0;
        if below(mid) { hi = mid } else { lo = mid }
    }
    let p = origin + dir * hi;
    if p.x * p.x + p.z * p.z > r * r {
        return None;
    }
    let size = map.size() as f32;
    Some((focus + GamePos::from_render(p).xz()).rem_euclid(Vec2::splat(size)))
}

/// Ground normal (render space) at offset `(dx, dz)` from the focus, from the drawn heights `half`
/// cells either side along x and z.
pub fn ground_normal(map: &Heightmap, focus: Vec2, params: &CurveParams, dx: f32, dz: f32, half: f32) -> Vec3 {
    let y = |ox: f32, oz: f32| ground_y(map, focus, params, dx + ox, dz + oz);
    let slope_x = (y(half, 0.0) - y(-half, 0.0)) / (2.0 * half);
    let slope_z = (y(0.0, half) - y(0.0, -half)) / (2.0 * half);
    Vec3::new(-slope_x, 1.0, -slope_z).normalize()
}

/// Leans `Tilted` parts with the ground under them (after `place_grounded` put them there).
fn tilt_grounded(
    rig: Res<CameraRig>,
    map: Res<CurrentMap>,
    params: Res<CurveParamsRes>,
    mut q: Query<(&Grounded, &Tilted, &mut Transform)>,
) {
    let size = map.0.terrain.size() as f32;
    let wrap = |d: f32| (d + size / 2.0).rem_euclid(size) - size / 2.0;
    for (g, tilt, mut t) in &mut q {
        let at = Vec3::from(GamePos::ground(wrap(g.at.x - rig.focus.x), 0.0, wrap(g.at.y - rig.focus.y)));
        let normal = ground_normal(&map.0.terrain, rig.focus, &params.0, at.x, at.z, tilt.half);
        t.rotation = Quat::from_rotation_arc(Vec3::Y, normal) * Quat::from(tilt.yaw);
    }
}

fn place_grounded(
    rig: Res<CameraRig>,
    map: Res<CurrentMap>,
    params: Res<CurveParamsRes>,
    mut q: Query<(&Grounded, &mut Transform, &mut Visibility)>,
) {
    for (g, mut t, mut vis) in &mut q {
        match render_pos(&map.0.terrain, g, rig.focus, &params.0) {
            Some(p) => {
                t.translation = p;
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_normal_follows_slopes_and_the_planet_curve() {
        let flat = CurveParams { radius: 10, curvature: 0.0, height_scale: 1.0 };
        let level = Heightmap::new(32);
        assert!(ground_normal(&level, Vec2::splat(16.0), &flat, 3.0, 0.0, 1.0).distance(Vec3::Y) < 1e-6, "flat ground: up");
        let curved = CurveParams { curvature: 0.01, ..flat };
        let n = ground_normal(&level, Vec2::splat(16.0), &curved, 5.0, 0.0, 1.0);
        assert!(n.x > 0.05 && n.z.abs() < 1e-6, "away from the focus the ground bends down: leans outward {n:?}");
        let mut ramp = Heightmap::new(32);
        for z in 0..32 {
            for x in 0..32 {
                ramp.set(x, z, (x * 10) as u16);
            }
        }
        let n = ground_normal(&ramp, Vec2::splat(16.0), &flat, 0.0, 0.0, 1.0);
        assert!(n.x < -0.1, "rising towards +x: leans back {n:?}");
    }

    fn flat_params() -> CurveParams {
        CurveParams { radius: 10, curvature: 0.0, height_scale: 1.0 }
    }

    fn point(at: Vec2) -> Grounded {
        Grounded { at, half: 0.0 }
    }

    #[test]
    fn mesh_height_matches_triangle_split() {
        let mut map = Heightmap::new(8);
        map.set(1, 1, 100);
        assert_eq!(mesh_height(&map, 0.5, 0.5), 0.0, "on the diagonal, (1,1) does not count");
        assert_eq!(mesh_height(&map, 0.75, 0.75), 50.0);
        assert_eq!(mesh_height(&map, 1.0, 1.0), 100.0);
        assert_eq!(mesh_height(&map, -7.0, -7.0), 100.0, "wraps");
    }

    #[test]
    fn render_pos_wraps_around_the_torus() {
        let map = Heightmap::new(128);
        let pos = render_pos(&map, &point(Vec2::new(1.0, 1.0)), Vec2::new(126.0, 127.0), &flat_params()).unwrap();
        assert_eq!((pos.x, pos.z), (3.0, -2.0), "the game's z drawn mirrored");
    }

    #[test]
    fn picking_the_ground_finds_the_place_drawn_there() {
        let map = Heightmap::new(128);
        let (at, focus) = (Vec2::new(12.0, 7.0), Vec2::new(10.0, 10.0));
        let drawn = render_pos(&map, &point(at), focus, &flat_params()).unwrap();
        let hit = pick_ground(&map, focus, &flat_params(), drawn + Vec3::Y * 5.0, Vec3::NEG_Y).unwrap();
        assert!(hit.distance(at) < 0.01, "{hit:?}");
    }

    #[test]
    fn hidden_outside_the_disc() {
        let map = Heightmap::new(128);
        assert!(render_pos(&map, &point(Vec2::new(40.0, 40.0)), Vec2::ZERO, &flat_params()).is_none());
    }

    #[test]
    fn rests_on_lowest_corner_of_a_slope() {
        let mut map = Heightmap::new(16);
        for z in 0..16 {
            for x in 0..16 {
                map.set(x, z, (x * 10) as u16);
            }
        }
        let p = flat_params();
        let at = Vec2::new(5.0, 5.0);
        assert_eq!(render_pos(&map, &point(at), at, &p).unwrap().y, 50.0);
        let wide = Grounded { at, half: 0.5 };
        assert_eq!(render_pos(&map, &wide, at, &p).unwrap().y, 45.0);
    }

    #[test]
    fn picks_the_ground_under_a_ray_and_wraps() {
        let mut map = Heightmap::new(128);
        map.set(3, 0, 10);
        let p = flat_params();
        let hit = pick_ground(&map, Vec2::new(126.0, 1.0), &p, Vec3::new(-2.0, 10.0, 0.0), Vec3::NEG_Y).unwrap();
        assert!((hit - Vec2::new(124.0, 1.0)).length() < 1e-3, "{hit:?}");
        let slanted = pick_ground(&map, Vec2::ZERO, &p, Vec3::new(0.0, 5.0, 0.0), Vec3::new(1.0, -1.0, 0.0).normalize()).unwrap();
        assert!((slanted.x - 25.0 / 11.0).abs() < 1e-3 && slanted.y.abs() < 1e-3, "hits the slope up to (3,0): {slanted:?}");
        assert!(pick_ground(&map, Vec2::ZERO, &p, Vec3::new(0.0, 5.0, 0.0), Vec3::Y).is_none(), "looking at the sky");
    }

    #[test]
    fn follows_the_planet_curve() {
        let map = Heightmap::new(128);
        let p = CurveParams { curvature: 0.01, ..flat_params() };
        let y = render_pos(&map, &point(Vec2::new(5.0, 0.0)), Vec2::ZERO, &p).unwrap().y;
        assert!((y + 0.25).abs() < 1e-6);
    }
}
