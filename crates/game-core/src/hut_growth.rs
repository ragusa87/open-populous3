//! A hut's breeding and growth (huts-and-training.md "Huts" rules 5-6, pop3-rev-analysis.md "Huts: breeding and
//! growth", fully traced): two counters per hut, each on its own rhythm, independent of each other.
//!
//! Breeding (`Building::breed`, the tooltip's green bar), every `BREED_EVERY` (4) turns: with room in the
//! tribe's population (`headcount::Housing::room`), `breed` gains `BREED_STEP` x (occupants + 1); at its target
//! (`breed_target`, `HUT<n>_SPROG_TIME` x a population-band percentage), a fresh brave appears at the door and
//! `breed` restarts at 0. Without room, `breed` resets to 0 instead of holding full (confirmed [exe] - not what
//! an earlier, less detailed pass at huts-and-training.md's own rule 5 guessed).
//!
//! Growth (`Building::grow`, the red bar), every `GROW_EVERY` (16) turns, only a small or medium hut (a large one
//! has no next size): empty, nothing happens (kept, not reset); else `grow` gains `GROW_STEP` x occupants; at
//! `GROW_TARGET` (2400, exe-only, no `constant.dat` entry), if the next size's footprint fits
//! (`placement::can_place`, excluding the hut's own current self), the hut becomes that size outright: `kind`
//! changes in place (same id, so whoever is inside just stays inside - "move the people inside into it" falls
//! out for free), seeded with 1 of its `wood_cost` pieces already in (the exe's own "100 wood", matching our
//! piece-based accounting), `grow` back to 0 - the ordinary construction system (`work.rs`) then has braves
//! bring the rest, same as any other building under construction, since the hut is now simply
//! `Stage::UnderConstruction`. If the footprint does not fit, `grow` stays at its clamped target and this just
//! retries every future growth tick - simpler than the trace's own "retried every 128 turns" detail, `[ours]`,
//! since nothing is lost by checking more often.
//!
//! `SPROG_PCT`'s band table: band 0 (30) is the only individually-confirmed value (`constants.md`); band 19
//! (200) is confirmed by the trace's own prose ("30 % for band 0 up to 200 % for band 19") but the file's other
//! 18 values were not individually dumped this session - linearly interpolated between those two confirmed
//! ends, **`[ours]`, not the real stairstep** (the trace's own worked example, "30, 35, 40, 50, 60, ... 190,
//! 195, 200", does not read as perfectly linear - close, not exact). A later session with the full file dump
//! should replace this table outright, not just retune it.

use crate::building::{Building, BuildingKind};
use crate::map::GameMap;
use crate::time::Every;
use crate::unit::UnitKind;

/// `HUT1..3_SPROG_TIME` by level (1-3) [files].
const SPROG_TIME: [u16; 3] = [4000, 3000, 2000];
/// `SPROG%_POP_BAND_00_04%` (band 0) and `_95_99%` (band 19) [files]; the 18 between are `[ours]`, see module docs.
const SPROG_PCT_FIRST: u32 = 30;
const SPROG_PCT_LAST: u32 = 200;
const SPROG_BANDS: u32 = 20;
/// Breeding's own rhythm and step [exe].
pub const BREED_EVERY: Every = Every::new(4);
const BREED_STEP: u16 = 2;
/// Growth's own rhythm, step and target [exe].
pub const GROW_EVERY: Every = Every::new(16);
const GROW_STEP: u16 = 8;
const GROW_TARGET: u16 = 2400;
/// A freshly completed hut's own breed head start: not quite full, "the first brave comes quickly" [exe].
const FRESH_BREED_HEAD_START: u16 = 54;

/// `SPROG%` for population band `band` (0..=19, already clamped): linear between the two confirmed ends.
fn sprog_pct(band: u32) -> u32 {
    SPROG_PCT_FIRST + (SPROG_PCT_LAST - SPROG_PCT_FIRST) * band.min(SPROG_BANDS - 1) / (SPROG_BANDS - 1)
}

/// The population band (0..=19, 10 people each) a tribe of `population` falls in [exe, confirmed]: not a % of
/// its cap, flat bands of 10 regardless of how many huts it has.
fn band(population: u16) -> u32 {
    (((population as u32) + 1) / 10).min(SPROG_BANDS - 1)
}

/// How much `breed` must reach for a hut of `level` (1-3) at this population `band` to produce a brave.
fn breed_target(level: u8, band: u32) -> u16 {
    (SPROG_TIME[level.clamp(1, 3) as usize - 1] as u32 * sprog_pct(band) / 100) as u16
}

/// The size a hut grows into next, None for a large one (no next size).
fn next_size(size: u8) -> Option<u8> {
    (size < 3).then_some(size + 1)
}

impl GameMap {
    /// Every built hut's breeding and growth for this turn (huts-and-training.md "Huts" rules 5-6).
    pub(crate) fn tend_huts(&mut self) {
        let now = self.now;
        let breeding = BREED_EVERY.fires(now, 0);
        let growing = GROW_EVERY.fires(now, 0);
        if !breeding && !growing {
            return;
        }
        let huts: Vec<usize> = (0..self.buildings.len())
            .filter(|&i| matches!(self.buildings[i].kind, BuildingKind::Hut { .. }) && self.buildings[i].stage() == crate::building::Stage::Built)
            .collect();
        for i in huts {
            if breeding {
                self.tend_breeding(i);
            }
            // A hut `tend_breeding` just grew may have changed kind (no longer `Built`): growth only ever
            // applies to an already-stable hut, so re-check its stage rather than growing a brand new site.
            if growing && self.buildings[i].stage() == crate::building::Stage::Built {
                self.tend_growing(i);
            }
        }
    }

    fn tend_breeding(&mut self, i: usize) {
        let Some(owner) = self.buildings[i].owner.tribe() else { return };
        let housing = self.housing(owner);
        if housing.population as u32 + 1 >= housing.room() as u32 {
            self.buildings[i].breed = 0;
            return;
        }
        let b = &mut self.buildings[i];
        let BuildingKind::Hut { size } = b.kind else { return };
        b.breed = b.breed.saturating_add(BREED_STEP * (b.inside as u16 + 1));
        if b.breed >= breed_target(size, band(housing.population)) {
            b.breed = 0;
            let door = b.door();
            self.spawn(owner, UnitKind::Brave, door);
        }
    }

    fn tend_growing(&mut self, i: usize) {
        let b = &self.buildings[i];
        let BuildingKind::Hut { size } = b.kind else { return };
        let Some(bigger_size) = next_size(size) else { return };
        if b.inside == 0 {
            return;
        }
        let grow = b.grow.saturating_add(GROW_STEP * b.inside as u16).min(GROW_TARGET);
        self.buildings[i].grow = grow;
        if grow < GROW_TARGET {
            return;
        }
        let id = self.buildings[i].id;
        let bigger = Building { kind: BuildingKind::Hut { size: bigger_size }, ..self.buildings[i].clone() };
        if !crate::placement::can_place(self, &bigger, Some(id)) {
            return; // stays clamped at GROW_TARGET, tried again next growth tick
        }
        let b = &mut self.buildings[i];
        b.kind = BuildingKind::Hut { size: bigger_size };
        b.used = 1;
        b.grow = 0;
    }
}

/// Set once a hut's construction completes (`work::work_event`'s own `UnitEvent::Built` handler): "the first
/// brave comes quickly" rather than instantly or from a cold start.
pub(crate) fn on_completed(b: &mut Building) {
    if let BuildingKind::Hut { size } = b.kind {
        // The population band at this exact moment is not threaded through from here; using band 0 (the
        // fastest) is a deliberately small, documented simplification - the head start only shaves a handful
        // of turns off either way, and the next breeding tick immediately recomputes the real target with the
        // tribe's actual population.
        b.breed = breed_target(size, 0).saturating_sub(FRESH_BREED_HEAD_START);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tribes::TribeId;
    use crate::building::Stage;
    use crate::terrain::Heightmap;
    use crate::time::Ticks;
    use crate::unit::Unit;

    fn bare() -> GameMap {
        let mut terrain = Heightmap::new(64);
        for z in 0..64 {
            for x in 0..64 {
                terrain.set(x, z, 64);
            }
        }
        GameMap::bare("hut-growth-test", terrain, Vec::new())
    }

    fn built_hut(map: &mut GameMap, size: u8, at: (u16, u16)) -> crate::building::BuildingId {
        let mut b = Building::new(BuildingKind::Hut { size }, TribeId::Blue, at.0, at.1, 0);
        b.flat = true;
        map.add_building(b)
    }

    /// `n` real occupants, so `GameMap::count_inside` (run every tick) keeps the hut's own `inside` at `n`
    /// instead of wiping a hand-set value back to 0 on the very next tick.
    fn house_people(map: &mut GameMap, hut: crate::building::BuildingId, n: u8) {
        let door = map.building(hut).unwrap().door();
        for _ in 0..n {
            let id = map.spawn(TribeId::Blue, UnitKind::Brave, door).id;
            map.unit_mut(id).unwrap().inside = Some(crate::unit::Inside { building: hut, door });
        }
        map.count_inside();
    }

    #[test]
    fn sprog_pct_matches_the_two_confirmed_ends() {
        assert_eq!(sprog_pct(0), 30);
        assert_eq!(sprog_pct(19), 200);
    }

    #[test]
    fn band_is_flat_tens_not_a_fraction_of_the_cap() {
        assert_eq!(band(0), 0);
        assert_eq!(band(8), 0);
        assert_eq!(band(9), 1);
        assert_eq!(band(195), 19);
        assert_eq!(band(10_000), 19, "clamped");
    }

    #[test]
    fn a_hut_breeds_a_brave_once_its_bar_is_full() {
        let mut map = bare();
        let hut = built_hut(&mut map, 1, (20 * 512, 20 * 512));
        let before = map.units.len();
        let target = breed_target(1, 0);
        let turns = target.div_ceil(BREED_STEP) * BREED_EVERY.period() as u16;
        map.run(Ticks::new(turns as u32));
        assert!(map.units.len() > before, "a brave was born");
        assert_eq!(map.building(hut).unwrap().breed, 0, "restarted");
    }

    #[test]
    fn breeding_stops_and_resets_once_the_tribe_has_no_room() {
        let mut map = bare();
        let hut = built_hut(&mut map, 1, (10 * 512, 10 * 512));
        map.buildings.iter_mut().find(|b| b.id == hut).unwrap().breed = 50;
        for p in 0..crate::headcount::SUPPLY[0] {
            map.units.push(Unit::new(crate::id::Id::from_raw(1000 + p as u32), TribeId::Blue, UnitKind::Brave, (0, 0)));
        }
        map.units.sort_by_key(|u| u.id);
        map.run(Ticks::new(BREED_EVERY.period()));
        assert_eq!(map.building(hut).unwrap().breed, 0, "no room: reset, not held full");
    }

    #[test]
    fn a_small_hut_grows_into_a_medium_one_once_its_bar_is_full_and_there_is_room() {
        let mut map = bare();
        let hut = built_hut(&mut map, 1, (20 * 512, 20 * 512));
        house_people(&mut map, hut, 3);
        let turns = GROW_TARGET.div_ceil(GROW_STEP * 3) * GROW_EVERY.period() as u16;
        map.run(Ticks::new(turns as u32 + GROW_EVERY.period()));
        let b = map.building(hut).unwrap();
        assert_eq!(b.kind, BuildingKind::Hut { size: 2 }, "grown");
        assert_eq!((b.used, b.grow, b.stage()), (1, 0, Stage::UnderConstruction { used: 1, of: 3 }));
    }

    #[test]
    fn an_empty_hut_never_grows_but_keeps_what_it_had() {
        let mut map = bare();
        let hut = built_hut(&mut map, 1, (20 * 512, 20 * 512));
        map.buildings.iter_mut().find(|b| b.id == hut).unwrap().grow = GROW_TARGET - 1;
        map.run(Ticks::new(GROW_EVERY.period() * 5));
        let b = map.building(hut).unwrap();
        assert_eq!((b.kind, b.grow), (BuildingKind::Hut { size: 1 }, GROW_TARGET - 1), "empty: kept, not reset, not grown");
    }

    #[test]
    fn a_large_hut_never_grows() {
        let mut map = bare();
        let hut = built_hut(&mut map, 3, (20 * 512, 20 * 512));
        house_people(&mut map, hut, 5);
        map.run(Ticks::new(GROW_EVERY.period() * 50));
        assert_eq!(map.building(hut).unwrap().kind, BuildingKind::Hut { size: 3 });
    }

    #[test]
    fn growth_is_blocked_when_the_bigger_footprint_does_not_fit() {
        let mut map = bare();
        let hut = built_hut(&mut map, 1, (20 * 512, 20 * 512));
        built_hut(&mut map, 1, (21 * 512, 21 * 512));
        house_people(&mut map, hut, 3);
        let turns = GROW_TARGET.div_ceil(GROW_STEP * 3) * GROW_EVERY.period() as u16;
        map.run(Ticks::new(turns as u32 + GROW_EVERY.period() * 5));
        let b = map.building(hut).unwrap();
        assert_eq!(b.kind, BuildingKind::Hut { size: 1 }, "still small: nowhere to grow into");
        assert_eq!(b.grow, GROW_TARGET, "stays clamped, ready to retry");
    }

    #[test]
    fn on_completed_gives_a_small_head_start_not_an_instant_full_bar() {
        let mut b = Building::new(BuildingKind::Hut { size: 1 }, TribeId::Blue, 0, 0, 0);
        on_completed(&mut b);
        assert_eq!(b.breed, breed_target(1, 0) - FRESH_BREED_HEAD_START);
        assert!(b.breed > 0 && b.breed < breed_target(1, 0), "close to full, not instant");
    }
}
