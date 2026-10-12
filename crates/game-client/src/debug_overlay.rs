//! The debug overlay (dev mode only, F4 or `DEBUG_OVERLAY=1`): outlines on the ground of the distances the
//! simulation decides by, drawn with gizmos (no meshes). Our own colours; nothing here is part of the game's look.

use crate::camera::{CameraRig, CurveParamsRes};
use crate::grounded::ground_point;
use crate::keymap::{DevMode, Shortcut, Shortcuts};
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::challenge::{FIGHT_REACH, SHOVE_REACH};
use game_core::melee::{MELEE_LEASH, RING_RADIUS};
use game_core::unit::Action;
use pop3_format::WORLD_UNITS_PER_CELL;
use std::f32::consts::TAU;

/// Lift above the ground, cells.
const LIFT: f32 = 0.03;
/// Points per circle, and per side of a square.
const CIRCLE_POINTS: usize = 48;
const SIDE_POINTS: usize = 8;
const CHALLENGE: Color = Color::srgb(1.0, 0.55, 0.1);
const MELEE: Color = Color::srgb(0.95, 0.15, 0.15);
const WEAR: Color = Color::srgb(0.85, 0.1, 0.1);
const SIGHT: Color = Color::srgb(1.0, 0.9, 0.2);
const BOLT: Color = Color::srgb(0.3, 0.6, 1.0);
const ASHES: Color = Color::srgb(0.6, 0.6, 0.6);
const WEAR_BACK: Color = Color::srgb(0.3, 0.3, 0.3);
/// The wear bar: its height over the ground and its length, cells.
const BAR_LIFT: f32 = 1.2;
const BAR_LENGTH: f32 = 1.0;
/// A fighter's place: a small circle, cells.
const PLACE_MARK: f32 = 0.06;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowOverlay(pub bool);

pub struct DebugOverlayPlugin;

impl Plugin for DebugOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ShowOverlay(std::env::var("DEBUG_OVERLAY").is_ok_and(|v| v == "1"))).add_systems(Update, (toggle, draw).chain());
    }
}

fn toggle(shortcuts: Shortcuts, mut show: ResMut<ShowOverlay>) {
    if shortcuts.just_pressed(Shortcut::DebugOverlay) {
        show.0 = !show.0;
    }
}

/// A world position (world units) in cells.
pub fn cells((x, z): (u16, u16)) -> Vec2 {
    Vec2::new(x as f32, z as f32) / WORLD_UNITS_PER_CELL as f32
}

/// A world distance in cells.
pub fn cells_of(units: i32) -> f32 {
    units as f32 / WORLD_UNITS_PER_CELL as f32
}

/// The closed outline of a circle of `radius` around `centre`, first point repeated last.
pub fn outline_circle(centre: Vec2, radius: f32) -> Vec<Vec2> {
    (0..=CIRCLE_POINTS).map(|k| centre + Vec2::from_angle(k as f32 * TAU / CIRCLE_POINTS as f32) * radius).collect()
}

/// The closed outline of the square `half` each side of `centre`, sampled along its sides to follow the ground.
pub fn outline_square(centre: Vec2, half: f32) -> Vec<Vec2> {
    let corners = [Vec2::new(-half, -half), Vec2::new(half, -half), Vec2::new(half, half), Vec2::new(-half, half), Vec2::new(-half, -half)];
    let mut points: Vec<Vec2> = corners.windows(2).flat_map(|w| (0..SIDE_POINTS).map(move |k| centre + w[0].lerp(w[1], k as f32 / SIDE_POINTS as f32))).collect();
    points.push(centre + corners[0]);
    points
}

fn draw(
    show: Res<ShowOverlay>,
    dev: Res<DevMode>,
    map: Res<CurrentMap>,
    rig: Res<CameraRig>,
    params: Res<CurveParamsRes>,
    mut gizmos: Gizmos,
) {
    if !(show.0 && dev.0) {
        return;
    }
    let map = &map.0;
    let on_ground = |p: Vec2| ground_point(&map.terrain, rig.focus, &params.0, p) + Vec3::Y * LIFT;
    let mut outline = |points: Vec<Vec2>, color: Color| gizmos.linestrip(points.into_iter().map(on_ground), color);
    for (_, c) in map.coordinators.challenges.iter() {
        let (Some(a), Some(t)) = (map.unit(c.attacker), map.unit(c.target)) else { continue };
        let (a, t) = (cells((a.x, a.z)), cells((t.x, t.z)));
        let toward = a + torus_delta_cells(a, t);
        outline(vec![a, toward], CHALLENGE);
        outline(outline_circle(t, cells_of(FIGHT_REACH)), CHALLENGE);
        outline(outline_square(t, cells_of(SHOVE_REACH)), CHALLENGE.with_alpha(0.5));
    }
    for b in map.buildings.iter().filter(|b| b.wear > 0) {
        let Some(limit) = game_core::wreck::wear_limit(b.kind) else { continue };
        let (start, full) = bar(cells(b.centre()), b.wear, limit);
        let lift = |p: Vec2| on_ground(p) + Vec3::Y * BAR_LIFT;
        gizmos.line(lift(start), lift(start + Vec2::X * BAR_LENGTH), WEAR_BACK);
        gizmos.line(lift(start), lift(start + Vec2::X * full), WEAR);
    }
    let mut outline = |points: Vec<Vec2>, color: Color| gizmos.linestrip(points.into_iter().map(on_ground), color);
    for u in &map.units {
        let (x, z) = u.cell();
        let cell = Vec2::new(x as f32 + 0.5, z as f32 + 0.5);
        if u.kind.stats().throws && u.is_alive() {
            outline(outline_square(cells((u.x, u.z)), cells_of(game_core::firebolt::bolt_reach(map.altitude(u), false))), BOLT.with_alpha(0.5));
        }
        if let Some(sight) = map.looking(u).filter(|&s| s > 0) {
            outline(outline_square(cell, sight_half(sight)), SIGHT);
        }
    }
    for (_, b) in map.bolts.iter() {
        let at = cells(b.at);
        outline(vec![at, at + torus_delta_cells(at, cells((b.aim.0, b.aim.1)))], BOLT);
    }
    for p in &map.ashes.patches {
        let fade = (p.darkness - game_core::ashes::FADED) as f32 / (game_core::ashes::DARK - game_core::ashes::FADED) as f32;
        for &(x, z) in &p.cells {
            outline(outline_square(Vec2::new(x as f32 + 0.5, z as f32 + 0.5), 0.5), ASHES.with_alpha(0.3 + 0.7 * fade));
        }
    }
    for (id, m) in map.coordinators.melees.iter() {
        let at = cells(m.at);
        outline(outline_circle(at, cells_of(RING_RADIUS)), MELEE);
        outline(outline_circle(at, cells_of(MELEE_LEASH)), MELEE.with_alpha(0.4));
        for u in &map.units {
            if let Action::Melee { melee, place, .. } = u.action
                && melee == id
            {
                outline(outline_circle(cells(place), PLACE_MARK), MELEE);
                outline(vec![cells((u.x, u.z)), cells((u.x, u.z)) + torus_delta_cells(cells((u.x, u.z)), cells(place))], MELEE.with_alpha(0.6));
            }
        }
    }
}

/// Half the side of the square of cells `sight` looks over, cells.
pub fn sight_half(sight: u8) -> f32 {
    (sight / 2) as f32 + 0.5
}

/// A wear bar centred over `centre`: where it starts, and the length of its worn part out of `BAR_LENGTH`.
pub fn bar(centre: Vec2, wear: u16, limit: u16) -> (Vec2, f32) {
    (centre - Vec2::X * BAR_LENGTH / 2.0, BAR_LENGTH * (wear as f32 / limit.max(1) as f32).min(1.0))
}

/// From `a` to `b` (cells) the short way round the torus.
fn torus_delta_cells(a: Vec2, b: Vec2) -> Vec2 {
    let size = 65536.0 / WORLD_UNITS_PER_CELL as f32;
    let wrap = |d: f32| (d + size / 2.0).rem_euclid(size) - size / 2.0;
    Vec2::new(wrap(b.x - a.x), wrap(b.y - a.y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circles_and_squares_close_on_themselves() {
        let c = outline_circle(Vec2::new(10.0, 10.0), 2.0);
        assert!(c.iter().all(|p| (p.distance(Vec2::new(10.0, 10.0)) - 2.0).abs() < 1e-4));
        assert!(c[0].distance(*c.last().unwrap()) < 1e-4);
        let s = outline_square(Vec2::ZERO, 1.0);
        assert_eq!(s.len(), 4 * SIDE_POINTS + 1);
        assert!(s.iter().all(|p| p.x.abs().max(p.y.abs()) == 1.0), "on the square's sides");
        assert_eq!((s[0], *s.last().unwrap()), (Vec2::new(-1.0, -1.0), Vec2::new(-1.0, -1.0)));
    }

    #[test]
    fn the_sight_square_covers_whole_cells() {
        assert_eq!([0, 1, 3, 5].map(sight_half), [0.5, 0.5, 1.5, 2.5]);
    }

    #[test]
    fn the_wear_bar_fills_up_to_the_limit() {
        assert_eq!(bar(Vec2::new(5.0, 5.0), 500, 2000), (Vec2::new(4.5, 5.0), 0.25));
        assert_eq!(bar(Vec2::ZERO, 5000, 2000).1, BAR_LENGTH);
    }

    #[test]
    fn the_short_way_across_the_edge() {
        assert_eq!(torus_delta_cells(Vec2::new(127.0, 0.0), Vec2::new(1.0, 0.0)), Vec2::new(2.0, 0.0));
    }
}
