//! Where a new building may stand (the blueprint's red parts, later `Command::PlaceBuilding`):
//! never on the sea, another building, a reincarnation site's platform, a camp fire's cell or a
//! tree that still has wood, nor on ground too steep (the braves level small unevenness). A boat hut also needs the sea
//! a cell past its jetty side (local +z: the middle and a corner at least) and dry land at its door
//! (local -z): in the levels, all 11 boat huts have only sea within 2.5 cells past their +z side
//! and land past their -z side.
//! Integers only.

use crate::building::{Building, BuildingKind};
use crate::map::GameMap;
use crate::terrain::to_world;
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
    Campfire,
    Totem,
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
    } else if map.campfires.iter().any(|f| f.cell() == cell) {
        Some(Blocked::Campfire)
    } else if map.totem_at(p).is_some() {
        Some(Blocked::Totem)
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

/// The points (world units) a cell past the middle and the corners of `b`'s side at local
/// `z_sign` (+1 the jetty side, -1 the door side): what lies right outside that side.
fn beyond_side(b: &Building, z_sign: i32) -> [(u16, u16); 3] {
    let f = b.kind.footprint();
    let unit = WORLD_UNITS_PER_CELL as i32;
    let z = f.offset.1 + z_sign * (f.half.1 + unit);
    let (cx, cz) = b.centre();
    [-f.half.0, 0, f.half.0].map(|x| {
        let (wx, wz) = to_world((f.offset.0 + x, z), b.facing / 2);
        (cx.wrapping_add(wx as u16), cz.wrapping_add(wz as u16))
    })
}

/// Whether `b` stands right by the water the way its kind needs: a boat hut with the sea past the
/// middle and at least one corner of its jetty side (a loaded level's levelling can lift one corner
/// off the sea) and no sea past its door side; any other building always.
pub fn shore_ok(map: &GameMap, b: &Building) -> bool {
    if b.kind != BuildingKind::BoatHut {
        return true;
    }
    let sea = |p: (u16, u16)| map.terrain.is_water_at(p.0, p.1);
    let [left, middle, right] = beyond_side(b, 1).map(sea);
    middle && (left || right) && !beyond_side(b, -1).into_iter().any(sea)
}

/// The facing to show a blueprint with: `preferred`, else the next quarter turn whose shore fits
/// (`shore_ok`, boat huts), else `preferred` (shown red).
pub fn best_facing(map: &GameMap, b: &Building, preferred: u8) -> u8 {
    (0..4u8)
        .map(|k| (preferred + 2 * k) % 8)
        .find(|&facing| shore_ok(map, &Building { facing, ..b.clone() }))
        .unwrap_or(preferred)
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
    free &= !too_steep(map, b) && shore_ok(map, b);
    free
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::BuildingKind;
    use crate::tree::Tree;

    fn hut(cell: (u16, u16)) -> Building {
        Building::new(BuildingKind::Hut { size: 1 }, 0, cell.0 * 512, cell.1 * 512, 0)
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
    fn sea_buildings_sites_trees_fires_and_totems_block() {
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
        m.campfires.push(crate::campfire::Campfire::new(1, 0, (60, 60)));
        assert_eq!(blocked_at(&m, (60 * 512 + 10, 60 * 512 + 500)), Some(Blocked::Campfire));
        assert!(!can_place(&m, &hut((60, 60))), "over a camp fire");
        m.totems.push(crate::totem::Totem::new(crate::totem::TotemKind::Totem, (70 * 512 + 256, 70 * 512 + 256)));
        assert_eq!(blocked_at(&m, (70 * 512 + 300, 70 * 512 + 200)), Some(Blocked::Totem));
        assert!(!can_place(&m, &hut((70, 70))), "over a totem");
        assert!(!crate::campfire::can_place(&m, (70, 70)), "no camp fire on a totem");
    }

    fn boat_hut(cell: (u16, u16), facing: u8) -> Building {
        Building { kind: BuildingKind::BoatHut, facing, ..hut(cell) }
    }

    #[test]
    fn boat_huts_need_the_sea_past_the_jetty_and_land_at_the_door() {
        let m = map();
        assert!(!shore_ok(&m, &boat_hut((40, 30), 0)), "inland");
        assert!(shore_ok(&m, &hut((40, 30))), "other kinds do not care");
        // The sea is at x < 10: the jetty (+z) must point to -x, a quarter turn the other way.
        assert!(shore_ok(&m, &boat_hut((10, 30), 6)), "jetty to -x, door to +x");
        assert!(!shore_ok(&m, &boat_hut((10, 30), 2)), "door on the water");
        assert!(can_place(&m, &boat_hut((10, 30), 6)), "the hut itself on the shore");
        assert_eq!(best_facing(&m, &boat_hut((10, 30), 0), 0), 6, "turned to the water");
        assert_eq!(best_facing(&m, &boat_hut((10, 30), 6), 6), 6, "kept when it fits");
        assert_eq!(best_facing(&m, &boat_hut((40, 30), 2), 2), 2, "nothing fits: as asked");
        assert_eq!(to_world(crate::terrain::to_local((300, -700), 3), 3), (300, -700));
    }

    /// Land at height 100 inside x 20..=60, z 20..=60 (vertices), sea all around.
    fn island() -> GameMap {
        let mut m = map();
        for z in 0..128 {
            for x in 0..128 {
                let land = (20..=60).contains(&x) && (20..=60).contains(&z);
                m.terrain.set(x, z, if land { 100 } else { 0 });
            }
        }
        m.sites.clear();
        m
    }

    #[test]
    fn a_boat_hut_fits_each_coast_with_its_jetty_out_to_sea() {
        let m = island();
        // Corner cells right inside each coast, and the facing that points the jetty (+z) to sea.
        let coasts = [((40, 59), 0), ((59, 40), 2), ((40, 20), 4), ((20, 40), 6)];
        for ((x, z), facing) in coasts {
            let b = boat_hut((x, z), facing);
            assert!(can_place(&m, &b), "coast at ({x}, {z}) facing {facing}");
            for wrong in [0, 2, 4, 6].into_iter().filter(|&f| f != facing) {
                assert!(!can_place(&m, &boat_hut((x, z), wrong)), "({x}, {z}) facing {wrong}: jetty on land or door in the sea");
            }
            for start in [0, 2, 4, 6] {
                assert_eq!(best_facing(&m, &boat_hut((x, z), start), start), facing, "turns itself from {start}");
            }
        }
    }

    #[test]
    fn a_boat_hut_is_refused_inland_in_the_sea_and_on_a_thin_spit() {
        let mut m = island();
        assert!(!can_place(&m, &boat_hut((40, 40), 0)), "inland: no sea past the jetty");
        assert!(!can_place(&m, &boat_hut((40, 70), 0)), "in the sea");
        assert!(!can_place(&m, &boat_hut((40, 61), 0)), "the hut itself off the coast");
        // A spit one cell wide: sea on both ends of every turn, the door always gets wet.
        for z in 20..=60 {
            for x in 20..=60 {
                m.terrain.set(x, z, if x == 40 || x == 41 { 100 } else { 0 });
            }
        }
        for facing in [0, 2, 4, 6] {
            assert!(!shore_ok(&m, &boat_hut((40, 40), facing)), "spit, facing {facing}");
        }
    }

    #[test]
    fn the_jetty_needs_its_middle_and_one_corner_over_the_sea() {
        let b = boat_hut((40, 59), 0);
        // The height point nearest each test point: raising it puts that point on land only.
        let [left, middle, right] = beyond_side(&b, 1).map(|p| (((p.0 as u32 + 256) / 512) as i32, ((p.1 as u32 + 256) / 512) as i32));
        let raise = |m: &mut GameMap, (x, z): (i32, i32)| m.terrain.set(x, z, 100);
        let mut m = island();
        raise(&mut m, left);
        assert!(shore_ok(&m, &b), "one corner on land is fine");
        raise(&mut m, right);
        assert!(!shore_ok(&m, &b), "both corners on land");
        let mut m = island();
        raise(&mut m, middle);
        assert!(!shore_ok(&m, &b), "the middle on land");
    }

    #[test]
    fn a_boat_hut_on_the_shore_still_minds_other_blockers() {
        let mut m = island();
        assert!(can_place(&m, &boat_hut((40, 59), 0)));
        m.buildings.push(hut((41, 57)));
        assert!(!can_place(&m, &boat_hut((40, 59), 0)), "overlaps a hut");
        m.buildings.clear();
        m.trees.push(Tree::new((40, 59), 0, 3));
        assert!(!can_place(&m, &boat_hut((40, 59), 0)), "a tree with wood");
        assert!(shore_ok(&m, &boat_hut((40, 59), 0)), "the shore itself is fine");
    }

    /// Every boat hut of the original levels fits the rule (skipped without an install).
    #[test]
    fn the_original_levels_boat_huts_fit_the_shore_rule() {
        let Some(levels) = pop3_format::install::find_install().map(|i| pop3_format::install::subdir(&i, "levels")) else { return };
        let Ok(dir) = std::fs::read_dir(&levels) else { return };
        let mut checked = 0;
        for path in dir.flatten().map(|e| e.path()) {
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            if !(name.starts_with("levl") && name.ends_with(".dat")) {
                continue;
            }
            let Ok(m) = GameMap::load_original(&path) else { continue };
            for b in m.buildings.iter().filter(|b| b.kind == BuildingKind::BoatHut) {
                assert!(shore_ok(&m, b), "{name}: boat hut at {:?} facing {}", (b.x / 512, b.z / 512), b.facing);
                assert_eq!(best_facing(&m, b, b.facing), b.facing, "{name}: the level's facing is kept");
                checked += 1;
            }
        }
        assert!(checked == 0 || checked >= 11, "found {checked} boat huts");
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
