//! Where a new building may stand (the blueprint's red parts, later `Command::PlaceBuilding`):
//! never on the sea, another building, a reincarnation site's platform or a tree that still has
//! wood, nor on ground too steep (the braves level small unevenness). Integers only.

use crate::building::Building;
use crate::map::GameMap;
use crate::site::SPAWN_FLAT_RADIUS;
use crate::unit::torus_delta;
use pop3_format::WORLD_UNITS_PER_CELL;

/// Largest height spread over a footprint the braves will level (placeholder to tune).
pub const STEEP_SPREAD: u16 = 200;

/// Why a ground point cannot be built on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocked {
    Sea,
    Building,
    Site,
    Tree,
}

/// What blocks building on the world point `p`, if anything (the slope is checked per building,
/// `too_steep`).
pub fn blocked_at(map: &GameMap, p: (u16, u16)) -> Option<Blocked> {
    let unit = WORLD_UNITS_PER_CELL as i64;
    let cell = ((p.0 as u32 / WORLD_UNITS_PER_CELL) as i32, (p.1 as u32 / WORLD_UNITS_PER_CELL) as i32);
    if map.terrain.is_water_at(p.0, p.1) {
        Some(Blocked::Sea)
    } else if map.buildings.iter().any(|b| b.covers(p, 0)) {
        Some(Blocked::Building)
    } else if map.sites.iter().any(|s| {
        let (dx, dz) = (torus_delta(s.x, p.0) as i64, torus_delta(s.z, p.1) as i64);
        dx * dx + dz * dz <= (SPAWN_FLAT_RADIUS as i64 * unit).pow(2)
    }) {
        Some(Blocked::Site)
    } else if map.trees.iter().any(|t| t.is_visible() && t.cell() == cell) {
        Some(Blocked::Tree)
    } else {
        None
    }
}

/// Whether the height points under `b`'s footprint differ by more than `STEEP_SPREAD`.
pub fn too_steep(map: &GameMap, b: &Building) -> bool {
    let unit = WORLD_UNITS_PER_CELL as i32;
    let f = b.kind.footprint();
    let r = (f.half.0.abs() + f.offset.0.abs()).max(f.half.1.abs() + f.offset.1.abs()) / unit + 1;
    let (cx, cz) = b.centre();
    let (cx, cz) = (cx as i32 / unit, cz as i32 / unit);
    let size = map.terrain.size() as i32;
    let heights = (cz - r..=cz + r + 1)
        .flat_map(|z| (cx - r..=cx + r + 1).map(move |x| (x, z)))
        .filter(|&(x, z)| b.covers(((x.rem_euclid(size) * unit) as u16, (z.rem_euclid(size) * unit) as u16), 0))
        .map(|(x, z)| map.terrain.get(x, z));
    let (lo, hi) = heights.fold((u16::MAX, 0), |(lo, hi), h| (lo.min(h), hi.max(h)));
    hi > lo && hi - lo > STEEP_SPREAD
}

/// Whether `b` could be placed: steady ground and every point of its footprint free (sampled every
/// quarter cell).
pub fn can_place(map: &GameMap, b: &Building) -> bool {
    let f = b.kind.footprint();
    let step = WORLD_UNITS_PER_CELL as i32 / 4;
    let (cx, cz) = b.centre();
    let span = (f.half.0.abs() + f.offset.0.abs()).max(f.half.1.abs() + f.offset.1.abs());
    let points = (-span..=span).step_by(step as usize).flat_map(|dz| (-span..=span).step_by(step as usize).map(move |dx| (dx, dz)));
    let mut free = points.map(|(dx, dz)| (cx.wrapping_add(dx as u16), cz.wrapping_add(dz as u16))).filter(|&p| b.covers(p, 0)).all(|p| blocked_at(map, p).is_none());
    free &= !too_steep(map, b);
    free
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::BuildingKind;
    use crate::tree::Tree;

    fn hut(cell: (u16, u16)) -> Building {
        Building { kind: BuildingKind::Hut { size: 1 }, owner: 0, x: cell.0 * 512, z: cell.1 * 512, facing: 0 }
    }

    /// Flat land at height 100 except a sea strip x < 10; the site at the centre.
    fn map() -> GameMap {
        let mut m = GameMap::sandbox_buildings();
        m.buildings.clear();
        m.trees.clear();
        for z in 0..128 {
            for x in 0..128 {
                m.terrain.set(x, z, if x < 10 { 0 } else { 100 });
            }
        }
        m
    }

    #[test]
    fn free_flat_land_takes_a_building() {
        let m = map();
        assert!(can_place(&m, &hut((30, 30))));
        assert_eq!(blocked_at(&m, (30 * 512, 30 * 512)), None);
    }

    #[test]
    fn sea_buildings_sites_and_trees_block() {
        let mut m = map();
        assert_eq!(blocked_at(&m, (5 * 512, 30 * 512)), Some(Blocked::Sea));
        assert!(!can_place(&m, &hut((9, 30))), "half on the sea");
        m.buildings.push(hut((40, 40)));
        assert_eq!(blocked_at(&m, (40 * 512 + 256, 40 * 512 + 256)), Some(Blocked::Building));
        assert!(!can_place(&m, &hut((41, 41))), "overlaps");
        let (sx, sz) = (m.sites[0].x, m.sites[0].z);
        assert_eq!(blocked_at(&m, (sx + 512, sz)), Some(Blocked::Site));
        m.trees.push(Tree::new((50, 50), 0, 2));
        assert_eq!(blocked_at(&m, (50 * 512 + 100, 50 * 512 + 100)), Some(Blocked::Tree));
        m.trees[0] = Tree::new((50, 50), 0, 0);
        assert_eq!(blocked_at(&m, (50 * 512 + 100, 50 * 512 + 100)), None, "a cut down tree does not block");
    }

    #[test]
    fn steep_ground_blocks_the_whole_building() {
        let mut m = map();
        assert!(!too_steep(&m, &hut((30, 30))));
        m.terrain.set(31, 31, 100 + STEEP_SPREAD + 1);
        assert!(too_steep(&m, &hut((30, 30))));
        assert!(!can_place(&m, &hut((30, 30))));
    }
}
