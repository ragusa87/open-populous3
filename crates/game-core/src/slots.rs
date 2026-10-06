//! Standing slots: a unit stops on a cell of its own, never on another unit's or a tree's. A group
//! sent somewhere is dispatched over the free cells around the target (`dispatch`), and a unit
//! arriving on a cell taken meanwhile moves on to the nearest free one (`GameMap::tick`).

use crate::path::Mobility;
use crate::terrain::Heightmap;
use pop3_format::WORLD_UNITS_PER_CELL;
use std::collections::{BTreeSet, VecDeque};

/// How far (cells) from the target free cells are looked for.
pub const SEARCH_RADIUS: i32 = 12;

/// Cell holding a world position.
pub fn cell_of((x, z): (u16, u16)) -> (i32, i32) {
    ((x as u32 / WORLD_UNITS_PER_CELL) as i32, (z as u32 / WORLD_UNITS_PER_CELL) as i32)
}

pub fn cell_centre((cx, cz): (i32, i32)) -> (u16, u16) {
    let c = |v: i32| (v.rem_euclid(128) as u32 * WORLD_UNITS_PER_CELL + WORLD_UNITS_PER_CELL / 2) as u16;
    (c(cx), c(cz))
}

/// The first `n` free cells around `target`, nearest first (ties by row then column): cells a walker
/// reaches from `target` without crossing water or cliffs, within `SEARCH_RADIUS`, not in `taken`.
pub fn free_cells_near(terrain: &Heightmap, target: (i32, i32), taken: &BTreeSet<(i32, i32)>, n: usize) -> Vec<(i32, i32)> {
    let size = terrain.size() as i32;
    let wrap = |(x, z): (i32, i32)| (x.rem_euclid(size), z.rem_euclid(size));
    let target = wrap(target);
    let delta = |a: i32, b: i32| {
        let d = (b - a).rem_euclid(size);
        if d > size / 2 { d - size } else { d }
    };
    let offset = |c: (i32, i32)| (delta(target.0, c.0), delta(target.1, c.1));
    let mut seen = BTreeSet::from([target]);
    let mut queue = VecDeque::from([target]);
    let mut found = Vec::new();
    if !Mobility::Walk.passable(terrain, target) {
        return found;
    }
    while let Some(cell) = queue.pop_front() {
        if !taken.contains(&cell) {
            found.push(cell);
        }
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let next = wrap((cell.0 + dx, cell.1 + dz));
            let (ox, oz) = offset(next);
            if ox.abs().max(oz.abs()) <= SEARCH_RADIUS && Mobility::Walk.passable(terrain, next) && seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    found.sort_by_key(|&c| {
        let (ox, oz) = offset(c);
        (ox * ox + oz * oz, oz, ox)
    });
    found.truncate(n);
    found
}

/// Where each of `units` (id, current position) goes for a move to `to`: one free cell each, the
/// nearest slots first, each slot taken by the closest unit still unplaced (ties by id). The unit on
/// the target cell goes exactly to `to`, the others to their cell's centre. Units without a slot
/// (no room) are left out.
pub fn dispatch(terrain: &Heightmap, units: &[(u32, (u16, u16))], to: (u16, u16), taken: &BTreeSet<(i32, i32)>) -> Vec<(u32, (u16, u16))> {
    let target = cell_of(to);
    let slots = free_cells_near(terrain, target, taken, units.len());
    let mut left: Vec<(u32, (u16, u16))> = units.to_vec();
    let mut out = Vec::new();
    for slot in slots {
        let spot = if slot == target { to } else { cell_centre(slot) };
        let dist = |p: (u16, u16)| {
            let d = |a: u16, b: u16| (b.wrapping_sub(a) as i16 as i64).pow(2);
            d(p.0, spot.0) + d(p.1, spot.1)
        };
        let Some(best) = (0..left.len()).min_by_key(|&i| (dist(left[i].1), left[i].0)) else { break };
        let (id, _) = left.remove(best);
        out.push((id, spot));
    }
    out.sort_by_key(|&(id, _)| id);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn land() -> Heightmap {
        let mut t = Heightmap::new(128);
        for z in 0..128 {
            for x in 0..128 {
                t.set(x, z, 100);
            }
        }
        t
    }

    fn at(cell: (i32, i32)) -> (u16, u16) {
        cell_centre(cell)
    }

    #[test]
    fn nearest_free_cells_first_skipping_taken_ones() {
        let t = land();
        let taken = BTreeSet::from([(10, 10), (11, 10)]);
        let cells = free_cells_near(&t, (10, 10), &taken, 4);
        assert_eq!(cells, vec![(10, 9), (9, 10), (10, 11), (9, 9)]);
    }

    #[test]
    fn slots_stay_on_the_same_shore() {
        let mut t = land();
        for z in 0..128 {
            for x in 12..14 {
                t.set(x, z, 0);
            }
            t.set(14, z, 0);
        }
        let cells = free_cells_near(&t, (11, 10), &BTreeSet::new(), 30);
        assert!(cells.iter().all(|c| c.0 <= 11), "never across the water: {cells:?}");
        assert!(free_cells_near(&t, (12, 10), &BTreeSet::new(), 3).is_empty(), "target in the sea");
    }

    #[test]
    fn a_group_gets_one_cell_each_the_closest_unit_on_the_target() {
        let t = land();
        let units = [(1, at((0, 10))), (2, at((5, 10))), (3, at((1, 10)))];
        let to = (10 * 512 + 100, 10 * 512 + 300);
        let out = dispatch(&t, &units, to, &BTreeSet::new());
        assert_eq!(out.len(), 3);
        let cells: BTreeSet<_> = out.iter().map(|&(_, p)| cell_of(p)).collect();
        assert_eq!(cells.len(), 3, "all different: {out:?}");
        assert_eq!(out.iter().find(|o| o.0 == 2).unwrap().1, to, "the nearest goes exactly where clicked");
        assert!(cells.iter().all(|&(x, z)| (x - 10).abs() <= 1 && (z - 10).abs() <= 1), "packed around the target");
        assert_eq!(out, dispatch(&t, &units, to, &BTreeSet::new()), "deterministic");
    }

    #[test]
    fn a_single_unit_sent_onto_a_taken_cell_stops_next_to_it() {
        let t = land();
        let to = at((20, 20));
        let out = dispatch(&t, &[(7, at((10, 20)))], to, &BTreeSet::from([(20, 20)]));
        assert_eq!(out.len(), 1);
        assert_ne!(cell_of(out[0].1), (20, 20));
        let (x, z) = cell_of(out[0].1);
        assert!((x - 20).abs() <= 1 && (z - 20).abs() <= 1);
    }
}
