//! Flatten (pop3-rev-analysis.md "Flatten (effect state 31)"), an effect (`effect::EffectKind::Flatten`): the 45
//! height points around the cast point's cell corner move towards the ground height at the exact cast point over
//! `DURATION` turns, each once a radius growing from the point reaches it, so the middle levels first.

use crate::effect::{Effect, EffectKind, TerrainChange};
use crate::event::WorldEvent;
use crate::spell::Cell;
use crate::terrain::{DirtyRect, Heightmap};
use crate::time::{Countdown, Ticks};
use crate::unit::torus_delta;
use pop3_format::WORLD_UNITS_PER_CELL;
use crate::tribes::Owner;

/// Turns of levelling (the original's 15 turns).
pub const DURATION: u32 = 15;
/// The disc: points within √13 of the corner (offsets with dx² + dz² < 16), the original's 45 points.
pub const RADIUS_SQ: i32 = 13;
/// The radius (world units, from the exact cast point) a point must be within to move, on the first turn.
pub const START_RADIUS: u32 = 380;
/// What the radius grows by over the turns: `SPREAD / turns_left` a turn, taken off what is left of it.
pub const SPREAD: u32 = 5120;
/// The highest ground it levels to.
pub const MAX_HEIGHT: u16 = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flatten {
    /// The exact cast point (world units).
    pub at: (u16, u16),
    /// The ground height interpolated at `at` when cast, clamped to `0..=MAX_HEIGHT`: the level the disc moves to.
    pub height: u16,
    /// Turns done.
    turn: u32,
    radius: u32,
    spread: u32,
}

/// A flatten cast by tribe `owner` at the point `at` (world units) of `terrain`.
pub fn effect(owner: impl Into<Owner>, at: (u16, u16), terrain: &Heightmap) -> Effect {
    let height = terrain.height_at(at.0 as u32, at.1 as u32, WORLD_UNITS_PER_CELL).clamp(0, MAX_HEIGHT as i32) as u16;
    let flatten = Flatten { at, height, turn: 0, radius: START_RADIUS, spread: SPREAD };
    Effect::new(owner, at, Countdown::new(Ticks::new(DURATION)), EffectKind::Flatten(flatten))
}

/// The height point nearest to `at` (world units).
pub fn nearest_point(at: (u16, u16)) -> Cell {
    let point = |c: u16| ((c as u32 + WORLD_UNITS_PER_CELL / 2) / WORLD_UNITS_PER_CELL) as i32;
    (point(at.0), point(at.1))
}

/// The corner the disc is centred on: `at` rounded down to its cell.
pub fn corner(at: (u16, u16)) -> Cell {
    ((at.0 as u32 / WORLD_UNITS_PER_CELL) as i32, (at.1 as u32 / WORLD_UNITS_PER_CELL) as i32)
}

impl Flatten {
    /// One turn: the terrain step it makes now, then its radius grows.
    pub fn turn(&mut self) -> Vec<WorldEvent> {
        if self.turn >= DURATION {
            return Vec::new();
        }
        let change = TerrainChange::Flatten { at: self.at, height: self.height, turn: self.turn, radius: self.radius };
        let step = self.spread / (DURATION - self.turn);
        (self.radius, self.spread) = (self.radius + step, self.spread - step);
        self.turn += 1;
        vec![WorldEvent::Terrain(change)]
    }

    /// Its height points not yet at its level on `terrain`.
    pub fn reshaping(&self, terrain: &Heightmap) -> Vec<Cell> {
        disc(corner(self.at)).filter(|&(x, z)| terrain.get(x, z) != self.height).collect()
    }
}

/// The disc's height points around the corner `at`, unwrapped.
pub fn disc(at: Cell) -> impl Iterator<Item = Cell> {
    (-3..=3).flat_map(move |dz| (-3..=3).filter(move |dx: &i32| dx * dx + dz * dz <= RADIUS_SQ).map(move |dx| (at.0 + dx, at.1 + dz)))
}

/// A height `h` one turn closer to `target`, `turn` turns in: the gap divided by the turns left, truncated, so
/// the last turn lands on it ("Flatten (effect state 31)" [exe]).
pub fn step(h: u16, target: u16, turn: u32) -> u16 {
    let left = DURATION.saturating_sub(turn).max(1) as i32;
    (h as i32 + (target as i32 - h as i32) / left).clamp(0, MAX_HEIGHT as i32) as u16
}

/// Whether the height point `point` is within `radius` of the exact point `at` (world units, across the map's edge).
fn reached(at: (u16, u16), point: Cell, radius: u32) -> bool {
    let world = |c: i32| (c as u32).wrapping_mul(WORLD_UNITS_PER_CELL) as u16;
    let (dx, dz) = (torus_delta(at.0, world(point.0)) as i64, torus_delta(at.1, world(point.1)) as i64);
    dx * dx + dz * dz <= (radius as i64).pow(2)
}

/// One turn of a flatten at `at` on `terrain`: the disc's points within `radius` of it step towards `height`; the
/// region it touched.
pub fn apply(terrain: &mut Heightmap, at: (u16, u16), height: u16, turn: u32, radius: u32) -> DirtyRect {
    let c = corner(at);
    for (x, z) in disc(c).filter(|&p| reached(at, p, radius)) {
        let h = terrain.get(x, z);
        terrain.set(x, z, step(h, height, turn));
    }
    DirtyRect { min: (c.0 - 3, c.1 - 3), max: (c.0 + 4, c.1 + 4) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tribes::TribeId;

    fn run(terrain: &mut Heightmap, f: &mut Flatten, turns: u32) {
        for _ in 0..turns {
            for e in f.turn() {
                if let WorldEvent::Terrain(TerrainChange::Flatten { at, height, turn, radius }) = e {
                    apply(terrain, at, height, turn, radius);
                }
            }
        }
    }

    fn cast(terrain: &Heightmap, at: (u16, u16)) -> Flatten {
        let Effect { kind: EffectKind::Flatten(f), .. } = effect(TribeId::Blue, at, terrain) else { unreachable!() };
        f
    }

    #[test]
    fn the_disc_holds_the_originals_45_points() {
        let points: Vec<Cell> = disc((10, 10)).collect();
        assert_eq!(points.len(), 45);
        assert!(points.contains(&(13, 12)) && points.contains(&(10, 7)) && !points.contains(&(13, 13)));
    }

    #[test]
    fn the_radius_grows_as_the_original_s() {
        let mut f = cast(&Heightmap::new(128), (0, 0));
        let radii: Vec<u32> = (0..8).map(|_| { let r = f.radius; f.turn(); r }).collect();
        assert_eq!(radii, [380, 721, 1062, 1403, 1744, 2085, 2426, 2767]);
    }

    #[test]
    fn aims_at_the_ground_height_at_the_exact_point_clamped() {
        let mut t = Heightmap::new(128);
        t.set(11, 10, 400);
        assert_eq!(cast(&t, (10 * 512 + 128, 10 * 512)).height, 100, "a quarter of the way to the 400 point");
        t.set(10, 10, 2000);
        assert_eq!(cast(&t, (10 * 512, 10 * 512)).height, MAX_HEIGHT);
    }

    #[test]
    fn levels_from_the_middle_outwards_and_all_of_the_disc_by_the_end() {
        let mut t = Heightmap::new(128);
        for (x, z) in disc((10, 10)) {
            t.set(x, z, 600);
        }
        t.set(10, 10, 200);
        let at = (10 * 512 + 100, 10 * 512 + 100);
        let mut f = cast(&t, at);
        let target = f.height;
        run(&mut t, &mut f, 1);
        assert!(t.get(10, 10) > 200 && t.get(11, 10) == 600, "the corner within 380 first");
        run(&mut t, &mut f, 1);
        assert!(t.get(11, 10) < 600 && t.get(13, 10) == 600, "then its neighbours, the rim waits");
        run(&mut t, &mut f, 3);
        assert!(t.get(13, 10) < 600, "the rim joins on turn 5");
        run(&mut t, &mut f, DURATION - 5);
        assert!(disc((10, 10)).all(|(x, z)| t.get(x, z) == target), "every point at the level");
        assert_eq!((t.get(14, 10), f.turn()), (0, Vec::new()), "nothing beyond, and done");
    }

    #[test]
    fn centred_on_the_cell_rounded_down_across_the_map_edge() {
        assert_eq!(corner((5 * 512 + 500, 2 * 512 + 1)), (5, 2));
        let mut t = Heightmap::new(128);
        t.set(0, 0, 300);
        t.set(127, 1, 100);
        let mut f = cast(&t, (0, 0));
        run(&mut t, &mut f, DURATION);
        assert_eq!(t.get(127, 1), 300);
    }

    #[test]
    fn aims_at_the_nearest_point() {
        assert_eq!(nearest_point((255, 256)), (0, 1));
        assert_eq!(nearest_point((5 * 512 + 300, 2 * 512)), (6, 2));
    }
}
