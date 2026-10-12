//! Flatten (pop3-rev-analysis.md "Per spell"), an effect (`effect::EffectKind::Flatten`): the 45 height points of a
//! disc around the target point move each turn towards the target point's height when cast, over `DURATION` turns.

use crate::effect::{Effect, EffectKind, TerrainChange};
use crate::event::WorldEvent;
use crate::spell::Cell;
use crate::terrain::{DirtyRect, Heightmap};
use crate::time::{Countdown, Ticks};
use pop3_format::WORLD_UNITS_PER_CELL;

/// Turns of levelling (the original's 15 turns).
pub const DURATION: u32 = 15;
/// The disc: points within √13 of the centre, the original's 45 points (about 3.5 cells).
pub const RADIUS_SQ: i32 = 13;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flatten {
    pub at: Cell,
    /// The target point's height when cast: the level the disc moves to.
    pub height: u16,
    /// Turns done.
    turn: u32,
}

/// A flatten cast by tribe `owner` on the height point `at` of `terrain`.
pub fn effect(owner: u8, at: Cell, terrain: &Heightmap) -> Effect {
    let flatten = Flatten { at, height: terrain.get(at.0, at.1), turn: 0 };
    let pos = |c: i32| (c.rem_euclid(terrain.size() as i32) as u32 * WORLD_UNITS_PER_CELL) as u16;
    Effect::new(owner, (pos(at.0), pos(at.1)), Countdown::new(Ticks::new(DURATION)), EffectKind::Flatten(flatten))
}

/// The height point nearest to `at` (world units).
pub fn nearest_point(at: (u16, u16)) -> Cell {
    let point = |c: u16| ((c as u32 + WORLD_UNITS_PER_CELL / 2) / WORLD_UNITS_PER_CELL) as i32;
    (point(at.0), point(at.1))
}

impl Flatten {
    /// One turn: the terrain step it makes now.
    pub fn turn(&mut self) -> Vec<WorldEvent> {
        if self.turn >= DURATION {
            return Vec::new();
        }
        let change = TerrainChange::Flatten { at: self.at, height: self.height, turn: self.turn };
        self.turn += 1;
        vec![WorldEvent::Terrain(change)]
    }

    /// Its height points not yet at its level on `terrain`.
    pub fn reshaping(&self, terrain: &Heightmap) -> Vec<Cell> {
        disc(self.at).filter(|&(x, z)| terrain.get(x, z) != self.height).collect()
    }
}

/// The disc's height points around `at`, unwrapped.
pub fn disc(at: Cell) -> impl Iterator<Item = Cell> {
    (-3..=3).flat_map(move |dz| (-3..=3).filter(move |dx: &i32| dx * dx + dz * dz <= RADIUS_SQ).map(move |dx| (at.0 + dx, at.1 + dz)))
}

/// A height `h` one turn closer to `target`, `turn` turns in: the gap divided by the turns left, so the last turn
/// lands on it (the original's curve is not traced: a guess, like the land bridge's without its cap).
pub fn step(h: u16, target: u16, turn: u32) -> u16 {
    let left = DURATION.saturating_sub(turn).max(1) as i32;
    (h as i32 + (target as i32 - h as i32) / left) as u16
}

/// One turn of a flatten on `terrain`; the region it touched.
pub fn apply(terrain: &mut Heightmap, at: Cell, height: u16, turn: u32) -> DirtyRect {
    for (x, z) in disc(at) {
        let h = terrain.get(x, z);
        terrain.set(x, z, step(h, height, turn));
    }
    DirtyRect { min: (at.0 - 3, at.1 - 3), max: (at.0 + 4, at.1 + 4) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(terrain: &mut Heightmap, at: Cell, turns: u32) -> Flatten {
        let Effect { kind: EffectKind::Flatten(mut f), .. } = effect(0, at, terrain) else { unreachable!() };
        for _ in 0..turns {
            for e in f.turn() {
                if let WorldEvent::Terrain(TerrainChange::Flatten { at, height, turn }) = e {
                    apply(terrain, at, height, turn);
                }
            }
        }
        f
    }

    #[test]
    fn the_disc_holds_the_originals_45_points() {
        let points: Vec<Cell> = disc((10, 10)).collect();
        assert_eq!(points.len(), 45);
        assert!(points.contains(&(13, 12)) && points.contains(&(10, 7)) && !points.contains(&(13, 13)));
    }

    #[test]
    fn levels_to_the_target_height_over_its_turns_and_nothing_beyond() {
        let mut terrain = Heightmap::new(32);
        terrain.set(10, 10, 200);
        for (x, z) in [(12, 11), (8, 10), (14, 10)] {
            terrain.set(x, z, 500);
        }
        let Effect { kind: EffectKind::Flatten(mut f), .. } = effect(0, (10, 10), &terrain) else { unreachable!() };
        let mut run = |turns: u32| {
            for _ in 0..turns {
                for e in f.turn() {
                    if let WorldEvent::Terrain(TerrainChange::Flatten { at, height, turn }) = e {
                        apply(&mut terrain, at, height, turn);
                    }
                }
            }
            (f.reshaping(&terrain).len(), terrain.get(12, 11), terrain.get(11, 11))
        };
        let (moving, high, low) = run(7);
        assert!(high < 500 && high > 200 && low > 0 && low < 200, "part-way: {high} {low}");
        assert_eq!(moving, 45 - 1, "only the target point is level yet");
        assert_eq!(run(DURATION - 7), (0, 200, 200), "lowered and raised alike");
        assert_eq!((terrain.get(8, 10), terrain.get(14, 10)), (200, 500), "inside the disc, outside it");
    }

    #[test]
    fn its_turns_end_and_it_crosses_the_map_edge() {
        let mut terrain = Heightmap::new(16);
        terrain.set(0, 0, 300);
        terrain.set(15, 1, 100);
        let mut f = build(&mut terrain, (0, 0), DURATION);
        assert_eq!(terrain.get(15, 1), 300);
        assert_eq!(f.turn(), Vec::new(), "done");
    }

    #[test]
    fn aims_at_the_nearest_point() {
        assert_eq!(nearest_point((255, 256)), (0, 1));
        assert_eq!(nearest_point((5 * 512 + 300, 2 * 512)), (6, 2));
    }
}
