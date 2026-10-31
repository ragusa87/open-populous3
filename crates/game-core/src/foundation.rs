//! A building's own foundation (pop3-rev-analysis.md "Terrain changes: what reacts"): on its own rhythm (every
//! `CHECK_EVERY` turns, `CHECK_EVERY_WATER` for a water-sited kind), it pulls its footprint back towards its own
//! base height, or collapses - too-steep ground shatters it on the spot, flooding sinks it over
//! `SINK_TICKS` ("Building sinking (flooded destruction)"). Neither leaves ashes (`crate::ashes`), unlike wear
//! or fire destruction (`crate::wreck`, `crate::fire`).
//!
//! [ours]: a periodic per-building scan rather than a reactive hook on every terrain write (buildings.md
//! "Damage and repair" names `WorldEvent::Terrain` as the original plan) - this reads the ground fresh each time
//! its own rhythm fires, so it reacts the same way whatever changed it (a spell, erosion, construction settling,
//! a reincarnation site's own re-levelling), without chasing down every direct `Heightmap` write.

use crate::building::{Building, BuildingId, BuildingKind, Stage};
use crate::terrain::{DirtyRect, Heightmap, SEA_LEVEL};
use crate::time::{Countdown, Every, Ticks};

/// How often an ordinary building rechecks its footprint [exe].
pub const CHECK_EVERY: Every = Every::new(4);
/// A water-sited building (a boat hut) rechecks every turn instead [exe].
pub const CHECK_EVERY_WATER: Every = Every::new(1);
/// Its footprint's height points (coarse grid, `Building::ground_points`) spread more than this (highest
/// minus lowest) destroys it on the spot ("too steep"), matching `placement::too_steep`'s own placement-time
/// check; within it, every point is instead pulled towards the footprint's average [exe/ours: the trace's own
/// wording ("the first cell found more than 200 away from its target") reads as a per-point deviation from
/// the recomputed average, which a live case showed does not actually destroy a building whose two most
/// extreme points alone already differ by this much - a footprint-wide spread, not a deviation from a shared
/// target, is what matches, so that is what this checks].
pub const PULL_RANGE: i32 = 200;
/// How fast a footprint height point is pulled towards its target, a turn [exe].
pub const PULL_RATE: i32 = 2;
/// More than this fraction of the footprint's fine cells at or below sea level floods it (ordinary
/// buildings only) [exe].
pub const FLOOD_FRACTION: (u32, u32) = (5, 8);
/// A water-sited building's base never drops below this; unlike an ordinary building's, it is never
/// recomputed from the ground [exe].
pub const WATER_BASE_MIN: u16 = 1;
/// The flooded-destruction stand-in's lifetime (pop3-rev-analysis.md "Building sinking (flooded
/// destruction)") [exe].
pub const SINK_TICKS: Ticks = Ticks::new(80);

/// A building on its way out from its own foundation check, never from combat or fire (`crate::wreck`,
/// `crate::fire`) at the same time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Collapse {
    /// Too-steep ground: shatters into debris and blows up (the ordinary explosion visual), removed
    /// the tick after this is set - one tick where it is still here for the client to catch it.
    Shattering,
    /// Flooded: sinks and rocks for `SINK_TICKS`, then gone.
    Sinking { left: Countdown },
}

impl BuildingKind {
    /// Sits over water (today: the boat hut only): its foundation check never averages a target height
    /// from its footprint (no single one to diverge from), so it is never destroyed for "uneven
    /// ground" nor for flooding either - only its base's floor (`WATER_BASE_MIN`) ever changes it.
    pub fn is_water_sited(self) -> bool {
        matches!(self, BuildingKind::BoatHut)
    }
}

impl Building {
    /// One footprint height point's target: sea level for a water-sited building's own water cells,
    /// its base otherwise.
    fn pull_target(&self, terrain: &Heightmap, cell: (i32, i32)) -> u16 {
        if self.kind.is_water_sited() && terrain.is_water(cell.0, cell.1) {
            SEA_LEVEL
        } else {
            self.level
        }
    }
}

impl crate::map::GameMap {
    /// Buildings' own foundation checks. First, what an earlier tick already started: a building
    /// `Shattering` is removed now (people thrown out, no ashes); a `Sinking` one whose countdown just
    /// ran out too. Then, on its own rhythm, every other standing building re-levels its footprint
    /// towards its base, or starts collapsing one of the two ways.
    pub(crate) fn tend_building_foundations(&mut self) -> Vec<DirtyRect> {
        let mut dirty = Vec::new();
        let mut events = Vec::new();

        let shattered: Vec<BuildingId> = self.buildings.iter().filter(|b| b.collapse == Some(Collapse::Shattering)).map(|b| b.id).collect();
        let mut sunk = Vec::new();
        for b in &mut self.buildings {
            if let Some(Collapse::Sinking { left }) = &mut b.collapse
                && left.tick()
            {
                sunk.push(b.id);
            }
        }
        for id in shattered.into_iter().chain(sunk) {
            let Some(i) = self.building_index(id) else { continue };
            events.extend(self.throw_out(id));
            self.remove_building(i);
        }

        let now = self.now;
        let size = self.terrain.size() as i32;
        for i in 0..self.buildings.len() {
            if self.buildings[i].collapse.is_some() || self.buildings[i].stage() != Stage::Built {
                continue;
            }
            let water = self.buildings[i].kind.is_water_sited();
            if !(if water { CHECK_EVERY_WATER } else { CHECK_EVERY }).fires(now, 0) {
                continue;
            }
            if !water {
                let cells = self.buildings[i].walled_cells(size);
                let flooded = cells.iter().filter(|&&(x, z)| self.terrain.get(x, z) == SEA_LEVEL).count();
                if flooded as u32 * FLOOD_FRACTION.1 > cells.len() as u32 * FLOOD_FRACTION.0 {
                    self.buildings[i].collapse = Some(Collapse::Sinking { left: Countdown::new(SINK_TICKS) });
                    continue;
                }
            }
            let points = self.buildings[i].ground_points();
            let heights: Vec<i32> = points.iter().map(|&(x, z)| self.terrain.get(x, z) as i32).collect();
            if !water && heights.iter().max().unwrap_or(&0) - heights.iter().min().unwrap_or(&0) > PULL_RANGE {
                self.buildings[i].collapse = Some(Collapse::Shattering);
                continue;
            }
            self.buildings[i].level = if water {
                self.buildings[i].level.max(WATER_BASE_MIN)
            } else {
                (heights.iter().sum::<i32>() / heights.len().max(1) as i32) as u16
            };
            let mut touched = Vec::new();
            for &(x, z) in &points {
                let target = self.buildings[i].pull_target(&self.terrain, (x, z)) as i32;
                let h = self.terrain.get(x, z) as i32;
                let diff = target - h;
                if diff != 0 {
                    self.terrain.set(x, z, (h + diff.signum() * diff.abs().min(PULL_RATE)) as u16);
                    touched.push((x, z));
                }
            }
            if !touched.is_empty() {
                dirty.push(DirtyRect::bounding(touched));
            }
        }

        dirty.extend(self.apply_events(events));
        dirty
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tribes::TribeId;
    use crate::map::GameMap;

    #[test]
    fn is_water_sited_is_the_boat_hut_only() {
        assert!(BuildingKind::BoatHut.is_water_sited());
        assert!(!BuildingKind::AirshipHut.is_water_sited());
        assert!(!BuildingKind::Hut { size: 1 }.is_water_sited());
    }

    #[test]
    fn pull_target_is_its_base_or_sea_level_for_a_water_siteds_own_water() {
        let mut m = GameMap::sandbox_walk();
        m.terrain.set(20, 20, 150);
        let mut hut = Building::new(BuildingKind::Hut { size: 1 }, TribeId::Blue, 20 * 512, 20 * 512, 0);
        hut.level = 100;
        assert_eq!(hut.pull_target(&m.terrain, (20, 20)), 100, "an ordinary building always targets its base");
        let mut boat = Building::new(BuildingKind::BoatHut, TribeId::Blue, 20 * 512, 20 * 512, 0);
        boat.level = 100;
        assert_eq!(boat.pull_target(&m.terrain, (20, 20)), 100, "its land side still targets its base");
        m.terrain.set(20, 20, 0);
        assert_eq!(boat.pull_target(&m.terrain, (20, 20)), 0, "its own water side targets sea level instead");
    }

    fn run(m: &mut GameMap, ticks: u32) {
        for _ in 0..ticks {
            m.tick();
        }
    }

    /// Every footprint height point of `id` set to `base`, except its first point, set to `spike`: far enough
    /// from any plausible average to land outside `PULL_RANGE` whatever the footprint's size.
    fn spike_one_corner(m: &mut GameMap, id: BuildingId, base: u16, spike: u16) {
        let points = m.building(id).unwrap().ground_points();
        for &(x, z) in &points {
            m.terrain.set(x, z, base);
        }
        let (x, z) = points[0];
        m.terrain.set(x, z, spike);
    }

    #[test]
    fn a_terrain_change_away_from_its_footprint_leaves_a_building_alone() {
        // "Hill, tower downhill. A flatten spell on the top edge: the building should resist."
        let mut m = GameMap::sandbox_walk();
        let tower = m.add_building(Building::new(BuildingKind::DrumTower, TribeId::Blue, 40 * 512, 40 * 512, 0));
        for &(x, z) in &m.building(tower).unwrap().ground_points() {
            m.terrain.set(x, z, 100);
        }
        m.terrain.set(60, 60, 900); // the hill's top edge, well clear of the tower's own footprint
        run(&mut m, 40);
        assert!(m.building(tower).is_some_and(|b| b.collapse.is_none()), "untouched footprint: stands");
    }

    #[test]
    fn uneven_ground_shatters_a_tower_and_a_hut_alike() {
        // "Hill tower on top, a bridge calculated so the tower ends up on a slope: should explode. Same for
        // other buildings."
        for kind in [BuildingKind::DrumTower, BuildingKind::Hut { size: 1 }] {
            let mut m = GameMap::sandbox_walk();
            let id = m.add_building(Building::new(kind, TribeId::Blue, 40 * 512, 40 * 512, 0));
            let cell = m.building(id).unwrap().walled_cells(m.terrain.size() as i32)[0];
            spike_one_corner(&mut m, id, 100, 900);
            run(&mut m, 4);
            assert!(m.building(id).is_some_and(|b| b.collapse == Some(Collapse::Shattering)), "{kind:?}: marked to shatter at once");
            run(&mut m, 1);
            assert!(m.building(id).is_none(), "{kind:?}: gone the tick after");
            assert!(m.ashes.buildable(cell), "{kind:?}: no ashes, unlike wear or fire destruction");
        }
    }

    #[test]
    fn a_spread_over_200_shatters_even_though_neither_point_alone_strays_from_the_average() {
        // A reported live case: footprint corners (95,60)=128, (94,61)=266, (94,59)=64 - the two extremes
        // 202 apart from each other, though each stays within 200 of the recomputed average (~153, with
        // enough other points near 128): still too uneven, caught by the footprint-wide spread, not a
        // per-point deviation from that average.
        let mut m = GameMap::sandbox_walk();
        let id = m.add_building(Building::new(BuildingKind::Temple, TribeId::Blue, 95 * 512, 60 * 512, 0));
        let points = m.building(id).unwrap().ground_points();
        assert!([(95, 60), (94, 61), (94, 59)].iter().all(|p| points.contains(p)), "{points:?}");
        for &(x, z) in &points {
            m.terrain.set(x, z, 128);
        }
        m.terrain.set(94, 61, 266);
        m.terrain.set(94, 59, 64);
        run(&mut m, 4);
        assert_eq!(m.building(id).unwrap().collapse, Some(Collapse::Shattering));
    }

    #[test]
    fn a_small_gap_pulls_the_footprint_flat_again_instead_of_destroying_it() {
        // "A bridge crossing half the house from a bit higher terrain: the house should survive and flatten
        // with time."
        let mut m = GameMap::sandbox_walk();
        let hut = m.add_building(Building::new(BuildingKind::Hut { size: 1 }, TribeId::Blue, 40 * 512, 40 * 512, 0));
        let points = m.building(hut).unwrap().ground_points();
        for (i, &(x, z)) in points.iter().enumerate() {
            m.terrain.set(x, z, if i % 2 == 0 { 100 } else { 150 });
        }
        run(&mut m, 8);
        assert!(m.building(hut).is_some(), "well within range: survives");
        let heights: Vec<u16> = points.iter().map(|&(x, z)| m.terrain.get(x, z)).collect();
        assert!(heights.iter().any(|&h| h != heights[0]), "not flat yet");
        run(&mut m, 300);
        assert!(m.building(hut).is_some_and(|b| b.collapse.is_none()), "never collapsed");
        let flat: Vec<u16> = points.iter().map(|&(x, z)| m.terrain.get(x, z)).collect();
        assert!(flat.iter().all(|&h| h == flat[0]), "flattened with time: {flat:?}");
    }

    #[test]
    fn flooding_sinks_an_ordinary_building_over_80_turns_no_ashes() {
        let mut m = GameMap::sandbox_walk();
        let hut = m.add_building(Building::new(BuildingKind::Hut { size: 1 }, TribeId::Blue, 40 * 512, 40 * 512, 0));
        let size = m.terrain.size() as i32;
        let cells = m.building(hut).unwrap().walled_cells(size);
        let flooded = cells.len() * FLOOD_FRACTION.0 as usize / FLOOD_FRACTION.1 as usize + 1;
        for &(x, z) in cells.iter().take(flooded) {
            m.terrain.set(x, z, SEA_LEVEL);
        }
        for &(x, z) in cells.iter().skip(flooded) {
            m.terrain.set(x, z, 100);
        }
        run(&mut m, 4);
        assert!(matches!(m.building(hut).unwrap().collapse, Some(Collapse::Sinking { .. })), "flooded: sinks");
        run(&mut m, SINK_TICKS.get() - 1);
        assert!(m.building(hut).is_some(), "still sinking");
        run(&mut m, 1);
        assert!(m.building(hut).is_none(), "gone after 80 turns");
        assert!(m.ashes.buildable((cells[0].0, cells[0].1)), "no ashes, unlike wear or fire destruction");
    }

    #[test]
    fn a_water_sited_building_is_immune_to_flooding_and_uneven_ground() {
        let mut m = GameMap::sandbox_walk();
        let hut = m.add_building(Building::new(BuildingKind::BoatHut, TribeId::Blue, 40 * 512, 40 * 512, 0));
        for &(x, z) in &m.building(hut).unwrap().walled_cells(m.terrain.size() as i32) {
            m.terrain.set(x, z, SEA_LEVEL);
        }
        spike_one_corner(&mut m, hut, 0, 900);
        run(&mut m, 40);
        let b = m.building(hut).unwrap();
        assert!(b.collapse.is_none(), "never destroyed: no single target height to diverge from");
        assert!(b.level >= WATER_BASE_MIN, "its base only ever clamped up, never averaged");
    }
}
