//! Standing slots: each cell holds `PER_CELL` x `PER_CELL` spots (about a third of a cell apart,
//! a unit's width), and a unit stops on a spot of its own, never on another unit's nor under a tree
//! (a tree takes its whole cell). A group sent somewhere is dispatched over the free spots around the
//! target (`dispatch`), and a unit arriving on a spot taken meanwhile moves on to the nearest free
//! one (`GameMap::tick`).

use crate::path::{Ground, Mobility};
use pop3_format::WORLD_UNITS_PER_CELL;
use std::collections::{BTreeSet, VecDeque};

/// Spots per cell side.
pub const PER_CELL: i32 = 3;
/// How far (cells) from the target free spots are looked for.
pub const SEARCH_RADIUS: i32 = 12;
const SPOT: u32 = WORLD_UNITS_PER_CELL / PER_CELL as u32;
/// Spots per map side (the map is 128 cells).
const SPOTS: i32 = 128 * PER_CELL;

/// A standing spot, in spot coordinates (`PER_CELL` per cell).
pub type Spot = (i32, i32);

/// Spot holding a world position.
pub fn spot_of((x, z): (u16, u16)) -> Spot {
    let s = |v: u16| ((v as u32 / WORLD_UNITS_PER_CELL) * PER_CELL as u32 + (v as u32 % WORLD_UNITS_PER_CELL) * PER_CELL as u32 / WORLD_UNITS_PER_CELL) as i32;
    (s(x), s(z))
}

/// World position of a spot's centre.
pub fn spot_centre((sx, sz): Spot) -> (u16, u16) {
    let c = |v: i32| {
        let v = v.rem_euclid(SPOTS);
        ((v / PER_CELL) as u32 * WORLD_UNITS_PER_CELL + (v % PER_CELL) as u32 * SPOT + SPOT / 2) as u16
    };
    (c(sx), c(sz))
}

/// Cell holding a spot.
pub fn spot_cell((sx, sz): Spot) -> (i32, i32) {
    (sx.rem_euclid(SPOTS).div_euclid(PER_CELL), sz.rem_euclid(SPOTS).div_euclid(PER_CELL))
}

/// World position of a cell's centre (its middle spot).
pub fn cell_centre((cx, cz): (i32, i32)) -> (u16, u16) {
    spot_centre((cx * PER_CELL + PER_CELL / 2, cz * PER_CELL + PER_CELL / 2))
}

/// Every spot of a cell.
pub fn cell_spots((cx, cz): (i32, i32)) -> impl Iterator<Item = Spot> {
    (0..PER_CELL * PER_CELL).map(move |i| (cx * PER_CELL + i % PER_CELL, cz * PER_CELL + i / PER_CELL))
}

/// The first `n` free spots around `target`, nearest first (ties by row then column): spots a walker
/// reaches from `target` without crossing water, cliffs or walls, within `SEARCH_RADIUS` cells, not in `taken`.
pub fn free_spots_near(ground: &impl Ground, target: Spot, taken: &BTreeSet<Spot>, n: usize) -> Vec<Spot> {
    let wrap = |(x, z): Spot| (x.rem_euclid(SPOTS), z.rem_euclid(SPOTS));
    let target = wrap(target);
    let delta = |a: i32, b: i32| {
        let d = (b - a).rem_euclid(SPOTS);
        if d > SPOTS / 2 { d - SPOTS } else { d }
    };
    let offset = |s: Spot| (delta(target.0, s.0), delta(target.1, s.1));
    let walkable = |s: Spot| ground.passable(Mobility::Walk, spot_cell(s));
    let mut found = Vec::new();
    if !walkable(target) {
        return found;
    }
    let mut seen = BTreeSet::from([target]);
    let mut queue = VecDeque::from([target]);
    while let Some(spot) = queue.pop_front() {
        if !taken.contains(&spot) {
            found.push(spot);
        }
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let next = wrap((spot.0 + dx, spot.1 + dz));
            let (ox, oz) = offset(next);
            if ox.abs().max(oz.abs()) <= SEARCH_RADIUS * PER_CELL && walkable(next) && seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    found.sort_by_key(|&s| {
        let (ox, oz) = offset(s);
        (ox * ox + oz * oz, oz, ox)
    });
    found.truncate(n);
    found
}

/// Where each of `units` (id, current position) goes for a move to `to`: one free spot each, the
/// nearest spots first, each taken by the closest unit still unplaced (ties by id). The unit on the
/// target spot goes exactly to `to`, the others to their spot's centre. Units left without a spot
/// (no room) are left out.
pub fn dispatch(ground: &impl Ground, units: &[(u32, (u16, u16))], to: (u16, u16), taken: &BTreeSet<Spot>) -> Vec<(u32, (u16, u16))> {
    let target = spot_of(to);
    let spots = free_spots_near(ground, target, taken, units.len());
    let mut left: Vec<(u32, (u16, u16))> = units.to_vec();
    let mut out = Vec::new();
    for spot in spots {
        let at = if spot == target { to } else { spot_centre(spot) };
        let dist = |p: (u16, u16)| {
            let d = |a: u16, b: u16| (b.wrapping_sub(a) as i16 as i64).pow(2);
            d(p.0, at.0) + d(p.1, at.1)
        };
        let Some(best) = (0..left.len()).min_by_key(|&i| (dist(left[i].1), left[i].0)) else { break };
        let (id, _) = left.remove(best);
        out.push((id, at));
    }
    out.sort_by_key(|&(id, _)| id);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::Heightmap;

    fn land() -> Heightmap {
        let mut t = Heightmap::new(128);
        for z in 0..128 {
            for x in 0..128 {
                t.set(x, z, 100);
            }
        }
        t
    }

    #[test]
    fn spots_split_cells_in_three() {
        assert_eq!(spot_of((0, 0)), (0, 0));
        assert_eq!(spot_of((512 + 200, 512 * 2 + 400)), (4, 8));
        assert_eq!(spot_centre((4, 8)), (512 + 170 + 85, 1024 + 340 + 85));
        assert_eq!(spot_of(spot_centre((4, 8))), (4, 8));
        assert_eq!(spot_cell((4, 8)), (1, 2));
        assert_eq!(cell_spots((1, 2)).count(), 9);
        assert!(cell_spots((1, 2)).all(|s| spot_cell(s) == (1, 2)));
        assert_eq!(spot_of(spot_centre((-1, 0))), (SPOTS - 1, 0), "wraps");
    }

    #[test]
    fn nearest_free_spots_first_skipping_taken_ones() {
        let t = land();
        let taken = BTreeSet::from([(30, 30), (31, 30)]);
        assert_eq!(free_spots_near(&t, (30, 30), &taken, 4), vec![(30, 29), (29, 30), (30, 31), (29, 29)]);
    }

    #[test]
    fn spots_stay_on_the_same_shore() {
        let mut t = land();
        for z in 0..128 {
            for x in 12..15 {
                t.set(x, z, 0);
            }
        }
        let spots = free_spots_near(&t, (11 * PER_CELL + 1, 30), &BTreeSet::new(), 200);
        assert!(spots.iter().all(|&s| spot_cell(s).0 <= 11), "never across the water");
        assert!(free_spots_near(&t, (13 * PER_CELL, 30), &BTreeSet::new(), 3).is_empty(), "target in the sea");
    }

    #[test]
    fn a_group_packs_around_the_target_the_closest_unit_on_it() {
        let t = land();
        let units = [(1, spot_centre((0, 30))), (2, spot_centre((25, 30))), (3, spot_centre((3, 30)))];
        let to = spot_centre((30, 30));
        let out = dispatch(&t, &units, to, &BTreeSet::new());
        let spots: BTreeSet<_> = out.iter().map(|&(_, p)| spot_of(p)).collect();
        assert_eq!((out.len(), spots.len()), (3, 3), "one spot each: {out:?}");
        assert_eq!(out.iter().find(|o| o.0 == 2).unwrap().1, to, "the nearest goes exactly where clicked");
        assert!(spots.iter().all(|&(x, z)| (x - 30).abs() <= 1 && (z - 30).abs() <= 1), "shoulder to shoulder");
        assert_eq!(out, dispatch(&t, &units, to, &BTreeSet::new()), "deterministic");
    }
}
