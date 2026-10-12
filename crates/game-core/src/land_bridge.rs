//! The land bridge (pop3-rev-analysis.md "Per spell"), an effect (`effect::EffectKind::LandBridge`): from the
//! caster to the target, a strip 4 height points wide whose heights move each turn towards a straight slope
//! between both ends' heights (at least `MIN_HEIGHT`), raising sea and lowering hills in its way, over
//! `DURATION` turns.

use crate::effect::{Effect, EffectKind, TerrainChange};
use crate::event::WorldEvent;
use crate::spell::Cell;
use crate::terrain::{DirtyRect, Heightmap, MAX_HEIGHT};
use crate::time::{Countdown, Ticks};
use pop3_format::WORLD_UNITS_PER_CELL;

/// `LAND_BRIDGE_MAX_CHANGE` (exe and file 256): the most a height point moves in one turn, before the division.
pub const MAX_CHANGE: i32 = 256;
/// `LAND_BRIDGE_DURATION` (file 64): turns of building, the step's divisor counting down to 1.
pub const DURATION: u32 = 64;
/// The bridge never goes lower than this (the original's minimum, ours was 32).
pub const MIN_HEIGHT: u16 = 90;
/// Offsets across the line: one point on one side, two on the other.
pub const ACROSS: [i32; 4] = [-1, 0, 1, 2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LandBridge {
    pub from: Cell,
    pub to: Cell,
    /// Both ends' heights when cast, at least `MIN_HEIGHT`: the slope the bridge moves towards.
    pub heights: (u16, u16),
    /// Turns done.
    turn: u32,
}

/// A land bridge cast by tribe `owner` from `from` to `to` (height points) on `terrain`.
pub fn effect(owner: u8, from: Cell, to: Cell, terrain: &Heightmap) -> Effect {
    let end = |(x, z): Cell| terrain.get(x, z).max(MIN_HEIGHT);
    let bridge = LandBridge { from, to, heights: (end(from), end(to)), turn: 0 };
    let at = |c: i32| (c.rem_euclid(terrain.size() as i32) as u32 * WORLD_UNITS_PER_CELL + WORLD_UNITS_PER_CELL / 2) as u16;
    Effect::new(owner, (at(from.0), at(from.1)), Countdown::new(Ticks::new(DURATION)), EffectKind::LandBridge(bridge))
}

impl LandBridge {
    /// One turn: the terrain step it makes now.
    pub fn turn(&mut self) -> Vec<WorldEvent> {
        if self.turn >= DURATION {
            return Vec::new();
        }
        let change = TerrainChange::Bridge { from: self.from, to: self.to, heights: self.heights, turn: self.turn };
        self.turn += 1;
        vec![WorldEvent::Terrain(change)]
    }
}

/// The bridge's height points (unwrapped) with their target heights: along the line the short way around the
/// torus of `size` points, each with its `ACROSS` neighbours on the minor axis.
pub fn targets(size: usize, from: Cell, to: Cell, heights: (u16, u16)) -> Vec<(Cell, u16)> {
    let s = size as i32;
    let wrap_delta = |d: i32| (d + s / 2).rem_euclid(s) - s / 2;
    let (dx, dz) = (wrap_delta(to.0 - from.0), wrap_delta(to.1 - from.1));
    let steps = dx.abs().max(dz.abs()).max(1);
    let (ha, hb) = (heights.0 as i32, heights.1 as i32);
    let mut points: Vec<(Cell, u16)> = Vec::new();
    for i in 0..=steps {
        let (x, z) = (from.0 + dx * i / steps, from.1 + dz * i / steps);
        let h = (ha + (hb - ha) * i / steps).max(MIN_HEIGHT as i32) as u16;
        for o in ACROSS {
            let p = if dx.abs() >= dz.abs() { (x, z + o) } else { (x + o, z) };
            if !points.iter().any(|(q, _)| (q.0 - p.0).rem_euclid(s) == 0 && (q.1 - p.1).rem_euclid(s) == 0) {
                points.push((p, h));
            }
        }
    }
    points
}

/// A height `h` one turn closer to `target`, `turn` turns into the bridge: `clamp(target − h, ±MAX_CHANGE)` divided
/// by the turns left, so the last turn lands on it.
pub fn step(h: u16, target: u16, turn: u32) -> u16 {
    let left = DURATION.saturating_sub(turn).max(1) as i32;
    let moved = h as i32 + (target as i32 - h as i32).clamp(-MAX_CHANGE, MAX_CHANGE) / left;
    moved.clamp(0, MAX_HEIGHT as i32) as u16
}

/// One turn of a bridge on `terrain`; the region it touched.
pub fn apply(terrain: &mut Heightmap, from: Cell, to: Cell, heights: (u16, u16), turn: u32) -> DirtyRect {
    let points = targets(terrain.size(), from, to, heights);
    for &((x, z), target) in &points {
        let h = terrain.get(x, z);
        terrain.set(x, z, step(h, target, turn));
    }
    let xs = points.iter().map(|p| p.0 .0);
    let zs = points.iter().map(|p| p.0 .1);
    DirtyRect { min: (xs.clone().min().unwrap_or(0), zs.clone().min().unwrap_or(0)), max: (xs.max().unwrap_or(0) + 1, zs.max().unwrap_or(0) + 1) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(terrain: &mut Heightmap, from: Cell, to: Cell, turns: u32) -> LandBridge {
        let Effect { kind: EffectKind::LandBridge(mut bridge), .. } = effect(0, from, to, terrain) else { unreachable!() };
        for _ in 0..turns {
            for e in bridge.turn() {
                if let WorldEvent::Terrain(TerrainChange::Bridge { from, to, heights, turn }) = e {
                    apply(terrain, from, to, heights, turn);
                }
            }
        }
        bridge
    }

    #[test]
    fn four_points_wide_on_a_slope_never_below_the_minimum() {
        let t = targets(64, (2, 10), (12, 10), (300, 0));
        assert_eq!(t.len(), 11 * 4);
        let at = |p: Cell| t.iter().find(|(q, _)| *q == p).map(|&(_, h)| h);
        assert_eq!((at((2, 9)), at((2, 10)), at((2, 11)), at((2, 12)), at((2, 8))), (Some(300), Some(300), Some(300), Some(300), None));
        assert_eq!(at((7, 10)), Some(150), "halfway down");
        assert_eq!(at((12, 10)), Some(MIN_HEIGHT), "the sea end held up");
        let steep = targets(64, (5, 0), (6, 20), (100, 100));
        assert!(steep.iter().any(|(p, _)| *p == (4, 0)) && steep.iter().any(|(p, _)| *p == (7, 0)), "across the x axis");
    }

    #[test]
    fn steps_close_in_and_land_on_the_last_turn() {
        assert_eq!(step(0, 1000, 0), 4, "256 / 64");
        assert_eq!(step(1000, 0, 0), 996, "lowers too");
        assert_eq!(step(100, 200, 62), 150);
        assert_eq!(step(100, 200, 63), 200);
        let mut h = 0;
        for turn in 0..DURATION {
            h = step(h, 1024, turn);
        }
        assert_eq!(h, 1024, "even from the sea to the highest ground");
    }

    #[test]
    fn rises_from_the_sea_over_its_turns() {
        let mut terrain = Heightmap::new(64);
        terrain.set(2, 10, 300);
        build(&mut terrain, (2, 10), (12, 10), 32);
        let half = terrain.get(12, 10);
        assert!(half > 0 && half < MIN_HEIGHT, "part-way after 32 turns: {half}");
        let bridge = build(&mut terrain, (2, 10), (12, 10), 0);
        assert_eq!(bridge.heights, (300, MIN_HEIGHT));
        let mut terrain = Heightmap::new(64);
        terrain.set(2, 10, 300);
        let mut bridge = build(&mut terrain, (2, 10), (12, 10), DURATION);
        assert_eq!((terrain.get(7, 10), terrain.get(12, 11), terrain.get(2, 12)), (195, MIN_HEIGHT, 300));
        assert_eq!(bridge.turn(), Vec::new(), "done");
    }

    #[test]
    fn crosses_the_torus_edge() {
        let mut terrain = Heightmap::new(16);
        build(&mut terrain, (14, 2), (2, 2), DURATION);
        assert_eq!((terrain.get(15, 2), terrain.get(0, 2), terrain.get(8, 2)), (MIN_HEIGHT, MIN_HEIGHT, 0));
    }
}
