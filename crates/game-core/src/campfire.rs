//! Camp fires: a tribe lights one in a single click (no wood, no braves) at a cell's centre, on
//! flat free ground (`can_place`). Units sent to it walk to its ring and go round it
//! (`unit::Action::AroundFire`). One with nobody of its tribe around goes out after
//! `ABANDON_TICKS`. See docs/specs/buildings.md "Camp fire". Integers only.

use crate::map::GameMap;
use crate::placement::blocked_at;
use crate::time::Ticks;
use pop3_format::WORLD_UNITS_PER_CELL;

/// Ticks a camp fire burns with nobody around before it goes out (placeholder to tune).
pub const ABANDON_TICKS: Ticks = Ticks::secs(60);
/// Radius (world units) of the ring the units go round.
pub const RING: i32 = 400;
/// Points on the ring, evenly spread (`RING_DIRS`).
pub const RING_POINTS: u8 = 16;
/// Largest height spread over the cell's corners: a camp fire needs flat ground (placeholder to tune).
pub const FLAT_SPREAD: u16 = 24;

/// The ring's directions, cos and sin of k/16 of a turn in 1024ths (+x first, towards +z).
const RING_DIRS: [(i32, i32); RING_POINTS as usize] = [
    (1024, 0),
    (946, 392),
    (724, 724),
    (392, 946),
    (0, 1024),
    (-392, 946),
    (-724, 724),
    (-946, 392),
    (-1024, 0),
    (-946, -392),
    (-724, -724),
    (-392, -946),
    (0, -1024),
    (392, -946),
    (724, -724),
    (946, -392),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Campfire {
    /// Unique on its map (`GameMap::place_campfire`).
    pub id: u32,
    pub owner: u8,
    /// World units, a cell's centre.
    pub x: u16,
    pub z: u16,
    /// Ticks in a row with nobody of its tribe going to it or round it.
    pub unattended: u16,
}

impl Campfire {
    /// A camp fire at the centre of `cell`.
    pub fn new(id: u32, owner: u8, (cx, cz): (i32, i32)) -> Self {
        let centre = |c: i32| (c.rem_euclid(128) as u32 * WORLD_UNITS_PER_CELL + WORLD_UNITS_PER_CELL / 2) as u16;
        Campfire { id, owner, x: centre(cx), z: centre(cz), unattended: 0 }
    }

    pub fn cell(&self) -> (i32, i32) {
        cell_of((self.x, self.z))
    }

    pub fn centre(&self) -> (u16, u16) {
        (self.x, self.z)
    }

    /// Burnt out: nobody came for `ABANDON_TICKS`.
    pub fn is_out(&self) -> bool {
        self.unattended as u32 >= ABANDON_TICKS.get()
    }
}

fn cell_of((x, z): (u16, u16)) -> (i32, i32) {
    ((x as u32 / WORLD_UNITS_PER_CELL) as i32, (z as u32 / WORLD_UNITS_PER_CELL) as i32)
}

/// Point `k` (any value, taken modulo `RING_POINTS`) of the ring around `centre`.
pub fn ring_point(centre: (u16, u16), k: u8) -> (u16, u16) {
    let (dx, dz) = RING_DIRS[(k % RING_POINTS) as usize];
    (centre.0.wrapping_add((dx * RING / 1024) as u16), centre.1.wrapping_add((dz * RING / 1024) as u16))
}

/// The ring point of `centre` closest to `from` (shortest way around the torus).
pub fn nearest_point(centre: (u16, u16), from: (u16, u16)) -> u8 {
    let (dx, dz) = (crate::unit::torus_delta(centre.0, from.0) as i64, crate::unit::torus_delta(centre.1, from.1) as i64);
    (0..RING_POINTS).max_by_key(|&k| {
        let (cx, cz) = RING_DIRS[k as usize];
        cx as i64 * dx + cz as i64 * dz
    })
    .unwrap_or(0)
}

/// The cell a camp fire lit at world point `p` takes.
pub fn cell_at(p: (u16, u16)) -> (i32, i32) {
    cell_of(p)
}

/// Whether a camp fire can be lit in `cell`: land on all its corners, flat (`FLAT_SPREAD`), and
/// nothing on it (a building, a site's platform, a tree with wood, another camp fire), checked at
/// its centre and a quarter cell in from each corner.
pub fn can_place(map: &GameMap, (cx, cz): (i32, i32)) -> bool {
    let corners = [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(dx, dz)| (cx + dx, cz + dz));
    let heights = corners.map(|(x, z)| map.terrain.get(x, z));
    let (lo, hi) = (heights.iter().min().copied().unwrap_or(0), heights.iter().max().copied().unwrap_or(0));
    if corners.iter().any(|&(x, z)| map.terrain.is_water(x, z)) || hi - lo > FLAT_SPREAD {
        return false;
    }
    let unit = WORLD_UNITS_PER_CELL as i32;
    let at = |fx: i32, fz: i32| (((cx * unit + fx).rem_euclid(128 * unit)) as u16, ((cz * unit + fz).rem_euclid(128 * unit)) as u16);
    let q = unit / 4;
    [at(unit / 2, unit / 2), at(q, q), at(unit - q, q), at(q, unit - q), at(unit - q, unit - q)].into_iter().all(|p| blocked_at(map, p).is_none())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::{Building, BuildingKind};
    use crate::tree::Tree;

    /// Flat land at height 100, sea for x < 10, no buildings nor trees.
    fn map() -> GameMap {
        let mut m = GameMap::sandbox_buildings();
        m.buildings.clear();
        m.trees.clear();
        m.campfires.clear();
        for z in 0..128 {
            for x in 0..128 {
                m.terrain.set(x, z, if x < 10 { 0 } else { 100 });
            }
        }
        m
    }

    #[test]
    fn the_ring_is_a_circle_of_its_radius() {
        for (cx, cz) in RING_DIRS {
            let r = ((cx * cx + cz * cz) as f64).sqrt();
            assert!((r - 1024.0).abs() < 1.5, "{cx},{cz}");
        }
        let c = (1000, 1000);
        assert_eq!(ring_point(c, 0), (1000 + RING as u16, 1000));
        assert_eq!(ring_point(c, 4), (1000, 1000 + RING as u16));
        assert_eq!(ring_point(c, 20), ring_point(c, 4), "wraps around the ring");
        assert_eq!(ring_point((10, 10), 8).0, 10u16.wrapping_sub(RING as u16), "wraps around the map");
    }

    #[test]
    fn nearest_point_faces_the_unit() {
        let c = (5000, 5000);
        assert_eq!(nearest_point(c, (9000, 5000)), 0);
        assert_eq!(nearest_point(c, (5000, 1000)), 12);
        assert_eq!(nearest_point(c, (2000, 8000)), 6);
    }

    #[test]
    fn lit_at_a_cell_centre() {
        let f = Campfire::new(1, 0, (30, 40));
        assert_eq!((f.x, f.z, f.cell()), (30 * 512 + 256, 40 * 512 + 256, (30, 40)));
        assert_eq!(Campfire::new(1, 0, (-1, 128)).cell(), (127, 0), "wraps");
        assert_eq!(cell_at((30 * 512 + 511, 40 * 512)), (30, 40));
    }

    #[test]
    fn needs_flat_free_land() {
        let mut m = map();
        assert!(can_place(&m, (30, 30)));
        assert!(!can_place(&m, (5, 30)), "sea");
        assert!(!can_place(&m, (9, 30)), "a corner on the shore");
        m.terrain.set(41, 30, 100 + FLAT_SPREAD + 1);
        assert!(!can_place(&m, (40, 30)), "a slope");
        assert!(can_place(&m, (42, 32)), "away from it");
        m.trees.push(Tree::new((50, 50), 0, 3));
        assert!(!can_place(&m, (50, 50)), "a tree");
        m.buildings.push(Building::new(BuildingKind::Hut { size: 1 }, 0, 60 * 512, 60 * 512, 0));
        assert!(!can_place(&m, (60, 60)), "a building");
        let (sx, sz) = (m.sites[0].x / 512, m.sites[0].z / 512);
        assert!(!can_place(&m, (sx as i32, sz as i32)), "the site");
        m.campfires.push(Campfire::new(1, 0, (70, 70)));
        assert!(!can_place(&m, (70, 70)), "another camp fire");
        assert!(can_place(&m, (71, 70)), "next to it");
    }
}
