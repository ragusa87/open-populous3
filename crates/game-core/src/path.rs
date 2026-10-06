//! Path finding on the cell grid (a torus): A* over the cells a mover can cross (land without
//! cliffs on foot, open sea by boat, anywhere by balloon), then straightened into a few waypoints.
//! Integer-only and deterministic (ties broken by cell index).

use crate::terrain::Heightmap;
use crate::unit::is_sea;
use pop3_format::WORLD_UNITS_PER_CELL;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

const CELL: i32 = WORLD_UNITS_PER_CELL as i32;
const STRAIGHT: u32 = 10;
const DIAGONAL: u32 = 14;
/// Longest straightened leg, in cells: well under half the map, so the shortest way around the
/// torus between two waypoints is the leg that was checked.
const MAX_LEG_CELLS: i32 = 24;
const NEIGHBOURS: [(i32, i32); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];

/// Highest height step between two corners along a cell edge that can still be walked; a step
/// over this is a cliff. 300 makes ~5.5% of the original levels' land cliffs (150: ~15%).
pub const MAX_CLIMB: u16 = 300;

/// How a mover crosses the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mobility {
    /// On foot: land, no cliffs.
    Walk,
    /// Boats: open sea only.
    Sail,
    /// Balloons: anywhere.
    Fly,
}

impl Mobility {
    pub fn passable(self, terrain: &Heightmap, cell: (i32, i32)) -> bool {
        match self {
            Mobility::Walk => !is_sea(terrain, cell) && !is_cliff(terrain, cell),
            Mobility::Sail => is_sea(terrain, cell),
            Mobility::Fly => true,
        }
    }
}

/// A cell edge rises more than `MAX_CLIMB` between its two corners.
pub fn is_cliff(terrain: &Heightmap, (x, z): (i32, i32)) -> bool {
    let h = |dx, dz| terrain.get(x + dx, z + dz);
    let edges = [(h(0, 0), h(1, 0)), (h(0, 1), h(1, 1)), (h(0, 0), h(0, 1)), (h(1, 0), h(1, 1))];
    edges.iter().any(|&(a, b)| a.abs_diff(b) > MAX_CLIMB)
}

/// Waypoints (world units) from `from` to `to` over cells `mob` can cross, ending exactly at
/// `to`; None when `to` cannot be reached. `straighten` merges cells into straight legs; without it
/// the route goes through every cell centre, starting with the centre of the current cell
/// (always safe, used when a straight leg was blocked).
pub fn route(terrain: &Heightmap, mob: Mobility, from: (u16, u16), to: (u16, u16), straighten: bool) -> Option<Vec<(u16, u16)>> {
    let cells = cell_path(terrain, mob, cell_of(from), cell_of(to))?;
    let size = terrain.size() as i32;
    let step = |a: i32, b: i32| (b - a + size / 2).rem_euclid(size) - size / 2;
    // Unwrapped coordinates: the start, every cell centre one step at a time, then the target in
    // the last cell. Legs between consecutive points are always passable.
    let start = (from.0 as i32, from.1 as i32);
    let mut cell = (start.0.div_euclid(CELL), start.1.div_euclid(CELL));
    let mut points = vec![start, centre(cell)];
    for w in cells.windows(2) {
        cell = (cell.0 + step(w[0].0, w[1].0), cell.1 + step(w[0].1, w[1].1));
        points.push(centre(cell));
    }
    points.push((cell.0 * CELL + to.0 as i32 % CELL, cell.1 * CELL + to.1 as i32 % CELL));
    points.dedup();
    let points = if straighten { straighten_legs(terrain, mob, &points) } else { points };
    Some(points.into_iter().skip(1).map(|(x, z)| (x as u16, z as u16)).collect())
}

/// Cells from `from` to `to` (both included), 8-connected without cutting a corner past an
/// impassable cell, shortest by 10/14 steps. None if `to` is impassable or out of reach. The
/// start cell may be impassable (ground raised into a cliff under her): she can step off it.
pub fn cell_path(terrain: &Heightmap, mob: Mobility, from: (i32, i32), to: (i32, i32)) -> Option<Vec<(i32, i32)>> {
    let size = terrain.size() as i32;
    let wrap = |(x, z): (i32, i32)| (x.rem_euclid(size), z.rem_euclid(size));
    let (from, to) = (wrap(from), wrap(to));
    let ok = |c: (i32, i32)| mob.passable(terrain, c);
    if !ok(to) {
        return None;
    }
    let index = |(x, z): (i32, i32)| (z * size + x) as usize;
    let heuristic = |(x, z): (i32, i32)| {
        let d = |a: i32, b: i32| (a - b).rem_euclid(size).min((b - a).rem_euclid(size)) as u32;
        let (dx, dz) = (d(x, to.0), d(z, to.1));
        STRAIGHT * dx.max(dz) + (DIAGONAL - STRAIGHT) * dx.min(dz)
    };
    let n = (size * size) as usize;
    let mut cost = vec![u32::MAX; n];
    let mut came_from = vec![usize::MAX; n];
    let mut open = BinaryHeap::new();
    cost[index(from)] = 0;
    open.push(Reverse((heuristic(from), index(from))));
    while let Some(Reverse((_, i))) = open.pop() {
        let cell = ((i as i32) % size, (i as i32) / size);
        if cell == to {
            let mut path = vec![to];
            let mut i = i;
            while came_from[i] != usize::MAX {
                i = came_from[i];
                path.push(((i as i32) % size, (i as i32) / size));
            }
            path.reverse();
            return Some(path);
        }
        for (dx, dz) in NEIGHBOURS {
            let next = wrap((cell.0 + dx, cell.1 + dz));
            let diagonal = dx != 0 && dz != 0;
            if !ok(next) || diagonal && !(ok(wrap((cell.0 + dx, cell.1))) && ok(wrap((cell.0, cell.1 + dz)))) {
                continue;
            }
            let c = cost[i] + if diagonal { DIAGONAL } else { STRAIGHT };
            let j = index(next);
            if c < cost[j] {
                cost[j] = c;
                came_from[j] = i;
                open.push(Reverse((c + heuristic(next), j)));
            }
        }
    }
    None
}

/// The cell reachable by `mob` from `from` that is closest to `goal` (straight-line distance
/// around the torus, first found on ties): where to go to get next to something on ground it
/// cannot reach, such as a walker boarding a boat on the water, or a boat unloading at a shore.
/// The start cell counts even if impassable.
pub fn nearest_reachable(terrain: &Heightmap, mob: Mobility, from: (i32, i32), goal: (i32, i32)) -> (i32, i32) {
    let size = terrain.size() as i32;
    let wrap = |(x, z): (i32, i32)| (x.rem_euclid(size), z.rem_euclid(size));
    let from = wrap(from);
    let dist2 = |(x, z): (i32, i32)| {
        let d = |a: i32, b: i32| (a - b).rem_euclid(size).min((b - a).rem_euclid(size));
        d(x, goal.0).pow(2) + d(z, goal.1).pow(2)
    };
    let mut seen = vec![false; (size * size) as usize];
    let mut queue = VecDeque::from([from]);
    seen[(from.1 * size + from.0) as usize] = true;
    let mut best = (dist2(from), from);
    while let Some(cell) = queue.pop_front() {
        if dist2(cell) < best.0 {
            best = (dist2(cell), cell);
        }
        for (dx, dz) in NEIGHBOURS {
            let next = wrap((cell.0 + dx, cell.1 + dz));
            let i = (next.1 * size + next.0) as usize;
            if !seen[i] && mob.passable(terrain, next) {
                seen[i] = true;
                queue.push_back(next);
            }
        }
    }
    best.1
}

/// Keep only the turning points: from each kept point, jump to the farthest next point that a
/// straight leg (checked cell by cell) reaches over passable cells.
fn straighten_legs(terrain: &Heightmap, mob: Mobility, points: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let mut out = vec![points[0]];
    let mut at = 0;
    while at + 1 < points.len() {
        let mut next = at + 1;
        while next + 1 < points.len() && leg_is_clear(terrain, mob, points[at], points[next + 1]) {
            next += 1;
        }
        out.push(points[next]);
        at = next;
    }
    out
}

/// Every cell a straight leg touches after its first is passable (where it passes exactly
/// through a cell corner, both side cells must be), and the leg is short enough (see
/// `MAX_LEG_CELLS`).
pub fn leg_is_clear(terrain: &Heightmap, mob: Mobility, a: (i32, i32), b: (i32, i32)) -> bool {
    let (dx, dz) = ((b.0 - a.0) as i64, (b.1 - a.1) as i64);
    if dx.abs().max(dz.abs()) > (MAX_LEG_CELLS * CELL) as i64 {
        return false;
    }
    let (mut cx, mut cz) = (a.0.div_euclid(CELL), a.1.div_euclid(CELL));
    let end = (b.0.div_euclid(CELL), b.1.div_euclid(CELL));
    let (sx, sz) = (dx.signum() as i32, dz.signum() as i32);
    // Distance along each axis to the next cell border, compared as fractions of the leg.
    let border = |p: i32, c: i32, s: i32| if s > 0 { ((c + 1) * CELL - p) as i64 } else { (p - c * CELL) as i64 };
    let (mut nx, mut nz) = (border(a.0, cx, sx), border(a.1, cz, sz));
    let ok = |c: (i32, i32)| mob.passable(terrain, c);
    let max_steps = (end.0 - cx).abs() + (end.1 - cz).abs() + 2;
    for _ in 0..max_steps {
        if (cx, cz) == end {
            return true;
        }
        let (tx, tz) = (nx * dz.abs(), nz * dx.abs());
        let step_x = dx != 0 && (dz == 0 || tx <= tz);
        let step_z = dz != 0 && (dx == 0 || tz <= tx);
        if step_x && step_z && !(ok((cx + sx, cz)) && ok((cx, cz + sz))) {
            return false;
        }
        if step_x {
            cx += sx;
            nx += CELL as i64;
        }
        if step_z {
            cz += sz;
            nz += CELL as i64;
        }
        if !ok((cx, cz)) {
            return false;
        }
    }
    (cx, cz) == end
}

fn cell_of((x, z): (u16, u16)) -> (i32, i32) {
    (x as i32 / CELL, z as i32 / CELL)
}

fn centre((cx, cz): (i32, i32)) -> (i32, i32) {
    (cx * CELL + CELL / 2, cz * CELL + CELL / 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::torus_delta;

    fn offset(a: (u16, u16), b: (u16, u16)) -> (i32, i32) {
        (torus_delta(a.0, b.0), torus_delta(a.1, b.1))
    }

    /// Land at height 100 everywhere except the cells of the picture (top-left at cell 0,0): `#`
    /// made sea, `^` raised to 500 (a plateau ringed by cliffs).
    fn map(rows: &[&str]) -> Heightmap {
        let mut t = Heightmap::new(128);
        for z in 0..128 {
            for x in 0..128 {
                t.set(x, z, 100);
            }
        }
        for (z, row) in rows.iter().enumerate() {
            for (x, c) in row.chars().enumerate() {
                let (x, z) = (x as i32, z as i32);
                match c {
                    '#' => sea(&mut t, (x, z)),
                    '^' => [(0, 0), (1, 0), (0, 1), (1, 1)].iter().for_each(|(dx, dz)| t.set(x + dx, z + dz, 500)),
                    _ => {}
                }
            }
        }
        t
    }

    fn sea(t: &mut Heightmap, (x, z): (i32, i32)) {
        for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            t.set(x + dx, z + dz, 0);
        }
    }

    fn at(cell: (i32, i32)) -> (u16, u16) {
        let c = centre(cell);
        (c.0 as u16, c.1 as u16)
    }

    #[test]
    fn open_ground_is_one_straight_leg() {
        let t = map(&[]);
        assert_eq!(route(&t, Mobility::Walk, at((2, 2)), (9 * 512 + 3, 5 * 512 + 7), true), Some(vec![(9 * 512 + 3, 5 * 512 + 7)]));
        assert_eq!(route(&t, Mobility::Walk, at((2, 2)), at((2, 2)), true), Some(vec![]), "already there");
    }

    #[test]
    fn goes_around_a_lake() {
        let t = map(&["", "", "", "    ####", "    ####", "    ####", "    ####"]);
        let (from, to) = (at((2, 5)), at((10, 5)));
        let path = cell_path(&t, Mobility::Walk, (2, 5), (10, 5)).unwrap();
        assert!(path.iter().all(|&c| Mobility::Walk.passable(&t, c)));
        let legs = route(&t, Mobility::Walk, from, to, true).unwrap();
        assert!(legs.len() >= 2, "turns around the lake: {legs:?}");
        assert_eq!(legs.last(), Some(&to));
        let mut prev = (from.0 as i32, from.1 as i32);
        for &(x, z) in &legs {
            let (dx, dz) = offset((prev.0 as u16, prev.1 as u16), (x, z));
            let next = (prev.0 + dx, prev.1 + dz);
            assert!(leg_is_clear(&t, Mobility::Walk, prev, next), "leg {prev:?} -> {next:?}");
            prev = next;
        }
    }

    #[test]
    fn unreachable_targets() {
        let ring = ["", "", "  #####", "  #...#", "  #...#", "  #...#", "  #####"];
        let t = map(&ring);
        assert_eq!(route(&t, Mobility::Walk, at((0, 0)), at((4, 4)), true), None, "walled-off island");
        assert_eq!(route(&t, Mobility::Walk, at((0, 0)), at((2, 2)), true), None, "sea");
        assert!(route(&t, Mobility::Walk, at((4, 4)), at((4, 5)), true).is_some(), "inside the island");
    }

    #[test]
    fn no_corner_cutting_between_two_sea_cells() {
        let t = map(&["", " #", "  #"]);
        let path = cell_path(&t, Mobility::Walk, (1, 2), (2, 1)).unwrap();
        assert!(path.len() > 3, "diagonal blocked, goes around: {path:?}");
        assert!(!leg_is_clear(&t, Mobility::Walk, centre((1, 2)), centre((2, 1))));
    }

    #[test]
    fn wraps_around_the_map_edge() {
        let mut rows = vec![""; 128];
        let wall = "#".repeat(128);
        rows[10] = &wall;
        let t = map(&rows);
        let path = cell_path(&t, Mobility::Walk, (5, 9), (5, 11)).unwrap();
        assert_eq!(path.len(), 127, "the long way round the torus");
        let legs = route(&t, Mobility::Walk, at((5, 9)), at((5, 11)), true).unwrap();
        assert_eq!(legs.last(), Some(&at((5, 11))));
    }

    #[test]
    fn cell_by_cell_route_starts_at_the_current_cell_centre() {
        let t = map(&[]);
        let from = (2 * 512 + 10, 2 * 512 + 500);
        let legs = route(&t, Mobility::Walk, from, at((4, 2)), false).unwrap();
        assert_eq!(legs, vec![at((2, 2)), at((3, 2)), at((4, 2))]);
        assert_eq!(route(&t, Mobility::Walk, from, (2 * 512 + 300, 2 * 512 + 300), false).unwrap(), vec![at((2, 2)), (2 * 512 + 300, 2 * 512 + 300)]);
    }

    #[test]
    fn cliffs_block_walkers_not_slopes() {
        let mut t = map(&["", "", "  ^^^^", "  ^^^^", "  ^^^^"]);
        assert!(is_cliff(&t, (1, 3)) && !is_cliff(&t, (3, 3)), "the ring around the plateau, not its top");
        assert_eq!(cell_path(&t, Mobility::Walk, (0, 3), (3, 3)), None, "up the cliff");
        assert!(cell_path(&t, Mobility::Walk, (3, 3), (3, 2)).is_some(), "on top");
        assert!(cell_path(&t, Mobility::Walk, (1, 3), (0, 3)).is_some(), "steps off a cliff cell she stands on");
        for x in 10..20 {
            for z in 0..128 {
                t.set(x, z, 100 + (x as u16 - 10) * MAX_CLIMB);
            }
        }
        assert!(cell_path(&t, Mobility::Walk, (8, 3), (21, 3)).is_some(), "steep but climbable ramp");
    }

    #[test]
    fn boats_sail_the_sea_only() {
        let t = map(&["", "", "", "    ####", "    ####", "    ####", "    ####"]);
        assert!(cell_path(&t, Mobility::Sail, (4, 3), (7, 6)).is_some());
        assert_eq!(cell_path(&t, Mobility::Sail, (4, 3), (10, 3)), None, "land");
        assert_eq!(nearest_reachable(&t, Mobility::Sail, (5, 4), (12, 4)), (7, 4), "unload at the east shore");
    }

    #[test]
    fn balloons_fly_straight_over_everything() {
        let t = map(&["", "", "  ^^^^", "  ^^^^", "  #####"]);
        assert_eq!(route(&t, Mobility::Fly, at((0, 3)), at((9, 3)), true), Some(vec![at((9, 3))]));
    }

    #[test]
    fn nearest_reachable_cell_to_something_on_the_water() {
        let t = map(&["", "", "", "    ####", "    ####", "    ####", "    ####"]);
        assert_eq!(nearest_reachable(&t, Mobility::Walk, (0, 0), (6, 4)), (6, 2));
    }
}
