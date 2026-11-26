//! Convert (pop3-rev-analysis.md "Convert wild" [exe, measured]), an effect (`effect::EffectKind::Convert`) at the
//! cast point that never moves. Every `CHECK_EVERY` turns, if the caster's tribe has room
//! (`headcount::MAX_POPULATION`), it looks at the wildmen in the whole cells around the cast cell (`REACH_CELLS`
//! each way, `COMPUTER_REACH_CELLS` for a computer caster): the nearest one on an all-land cell becomes a brave of
//! the caster (`WorldEvent::ChangeOwner`) and the effect's life goes back to `LIFE`; those on a coast or sea cell
//! are not converted that try but step inland and wait there (`WorldEvent::StepInland`). It lives `LIFE`
//! (`COMPUTER_LIFE` for a computer caster) turns from the cast or the last conversion. Not done: a tribe that
//! "gives up and sulks" ending it at once (no such tribe state here).

use crate::effect::{Effect, EffectKind};
use crate::event::WorldEvent;
use crate::headcount::MAX_POPULATION;
use crate::map::GameMap;
use crate::terrain::Heightmap;
use crate::time::{Countdown, Ticks};
use crate::tribes::Owner;
use crate::unit::{cell_of, torus_delta, Unit, UnitId, UnitKind};
use pop3_format::WORLD_UNITS_PER_CELL;

/// A conversion try every 8 turns ("Convert wild": the effect's own turn counter & 7).
pub const CHECK_EVERY: Ticks = Ticks::new(8);
/// The first try, 2 turns after the cast (measured in both casts on record).
pub const FIRST_TRY: Ticks = Ticks::new(2);
/// Turns it lives from the cast, and again from each conversion.
pub const LIFE: Ticks = Ticks::new(18);
/// From the cast for a computer caster.
pub const COMPUTER_LIFE: Ticks = Ticks::new(22);
/// Whole cells scanned each way around the cast cell: 5 x 5.
pub const REACH_CELLS: i32 = 2;
/// For a computer caster: 7 x 7.
pub const COMPUTER_REACH_CELLS: i32 = 3;
/// How long a wildman sent inland stands still waiting before it wanders again (the original's 64 turns).
pub const INLAND_WAIT: Ticks = Ticks::new(64);
/// How far a wildman sent inland looks for an all-land cell, cells (ours: the original walks away from the
/// coast in 4-turn steps until on land).
pub const INLAND_SEARCH_CELLS: i32 = 2;
/// How far inside that cell it stops, world units (ours: the original stops once on land).
pub const INLAND_STEP_IN: i32 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Convert {
    /// Turns left to the next try.
    next: Countdown,
    /// Whole cells scanned each way around the cast cell.
    reach: i32,
    /// Turns since the cast (its sparkles' clock).
    age: u32,
}

/// Cast by tribe `owner` at `at` (world units); `computer`: a computer-controlled caster's longer first life and
/// wider square.
pub fn effect(owner: impl Into<Owner>, at: (u16, u16), computer: bool) -> Effect {
    let (life, reach) = if computer { (COMPUTER_LIFE, COMPUTER_REACH_CELLS) } else { (LIFE, REACH_CELLS) };
    Effect::new(owner, at, Countdown::new(life), EffectKind::Convert(Convert { next: Countdown::new(FIRST_TRY), reach, age: 0 }))
}

impl Convert {
    /// Whole cells it scans each way around the cast cell.
    pub fn reach(&self) -> i32 {
        self.reach
    }

    /// Turns since the cast.
    pub fn age(&self) -> u32 {
        self.age
    }

    /// One turn at `at`, cast by `owner`: its events, and whether it converted someone (its life starts over).
    pub fn turn(&mut self, at: (u16, u16), owner: Owner, world: &GameMap) -> (Vec<WorldEvent>, bool) {
        self.age += 1;
        if !self.next.tick() {
            return (Vec::new(), false);
        }
        self.next = Countdown::new(CHECK_EVERY);
        let Some(tribe) = owner.tribe() else { return (Vec::new(), false) };
        if world.housing(tribe).population >= MAX_POPULATION {
            return (Vec::new(), false);
        }
        let found = scan(at, self.reach, &world.units, &world.terrain);
        let mut events: Vec<WorldEvent> = found.ashore.iter().filter_map(|&unit| {
            let u = world.unit(unit)?;
            let to = inland_of(&world.terrain, (u.x, u.z))?;
            Some(WorldEvent::StepInland { unit, to })
        }).collect();
        let converted = found.nearest.is_some();
        events.extend(found.nearest.map(|unit| WorldEvent::ChangeOwner { unit, tribe }));
        (events, converted)
    }
}

/// What one try finds around the cast cell.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Scan {
    /// The nearest wildman to the cast point on an all-land cell.
    pub nearest: Option<UnitId>,
    /// The wildmen on a coast or sea cell.
    pub ashore: Vec<UnitId>,
}

/// Every cell corner above sea level: no coast, no sea.
pub fn all_land(terrain: &Heightmap, (x, z): (i32, i32)) -> bool {
    [(0, 0), (1, 0), (0, 1), (1, 1)].iter().all(|&(dx, dz)| !terrain.is_water(x + dx, z + dz))
}

/// The living wildmen in the whole cells within `reach` of the cast cell (a square, wrapping).
pub fn scan(at: (u16, u16), reach: i32, units: &[Unit], terrain: &Heightmap) -> Scan {
    let size = terrain.size() as i32;
    let centre = (cell_of(at.0), cell_of(at.1));
    let off = |a: i32, b: i32| {
        let d = (b - a).rem_euclid(size);
        d.min(size - d)
    };
    let mut found = Scan::default();
    let mut best: Option<(i64, UnitId)> = None;
    for u in units.iter().filter(|u| u.kind == UnitKind::Wildman && u.is_alive() && u.inside.is_none()) {
        let cell = (cell_of(u.x), cell_of(u.z));
        if off(centre.0, cell.0) > reach || off(centre.1, cell.1) > reach {
            continue;
        }
        if !all_land(terrain, cell) {
            found.ashore.push(u.id);
            continue;
        }
        let (dx, dz) = (torus_delta(at.0, u.x) as i64, torus_delta(at.1, u.z) as i64);
        let d2 = dx * dx + dz * dz;
        if best.is_none_or(|(b, _)| d2 < b) {
            best = Some((d2, u.id));
        }
    }
    found.nearest = best.map(|(_, id)| id);
    found
}

/// Just inside the nearest all-land cell to `from` (world units) within `INLAND_SEARCH_CELLS` cells, if any.
pub fn inland_of(terrain: &Heightmap, from: (u16, u16)) -> Option<(u16, u16)> {
    let size = terrain.size() as i32;
    let cell = WORLD_UNITS_PER_CELL as i32;
    let r = INLAND_SEARCH_CELLS;
    let (cx, cz) = (cell_of(from.0), cell_of(from.1));
    let inside = |p: u16, c: i32| {
        let lo = (c * cell) as u16;
        let off = torus_delta(lo, p).clamp(INLAND_STEP_IN, cell - 1 - INLAND_STEP_IN);
        lo.wrapping_add(off as u16)
    };
    (-r..=r)
        .flat_map(|dz| (-r..=r).map(move |dx| (dx, dz)))
        .filter(|&(dx, dz)| (dx, dz) != (0, 0))
        .map(|(dx, dz)| ((cx + dx).rem_euclid(size), (cz + dz).rem_euclid(size)))
        .filter(|&c| all_land(terrain, c))
        .map(|c| (inside(from.0, c.0), inside(from.1, c.1)))
        .min_by_key(|&p| {
            let (dx, dz) = (torus_delta(from.0, p.0) as i64, torus_delta(from.1, p.1) as i64);
            (dx * dx + dz * dz, p)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tribes::TribeId;

    const CELL: u16 = WORLD_UNITS_PER_CELL as u16;
    const AT: (u16, u16) = (40 * CELL + 256, 40 * CELL + 256);

    fn land() -> GameMap {
        let mut terrain = Heightmap::new(128);
        for z in 0..128 {
            for x in 0..128 {
                terrain.set(x, z, 64);
            }
        }
        GameMap::bare("convert-test", terrain, Vec::new())
    }

    fn cells_off(dx: i32, dz: i32) -> (u16, u16) {
        (AT.0.wrapping_add_signed((dx * CELL as i32) as i16), AT.1.wrapping_add_signed((dz * CELL as i32) as i16))
    }

    #[test]
    fn first_tries_2_turns_after_the_cast_then_every_8_the_nearest_first() {
        let mut map = land();
        map.add_effect(effect(TribeId::Blue, AT, false));
        let near = map.spawn(Owner::Wild, UnitKind::Wildman, cells_off(1, 0)).id;
        let next = map.spawn(Owner::Wild, UnitKind::Wildman, cells_off(-2, 2)).id;
        map.run(Ticks::new(1));
        assert_eq!(map.unit(near).unwrap().owner, Owner::Wild, "not yet");
        map.tick();
        assert_eq!(map.unit(near).unwrap().owner, Owner::Tribe(TribeId::Blue), "the nearest, on turn 2");
        assert_eq!(map.unit(near).unwrap().kind, UnitKind::Brave);
        assert_eq!(map.unit(next).unwrap().owner, Owner::Wild, "one a try");
        map.run(Ticks::new(7));
        assert_eq!(map.unit(next).unwrap().owner, Owner::Wild);
        map.tick();
        assert_eq!(map.unit(next).unwrap().owner, Owner::Tribe(TribeId::Blue), "8 turns later, the square's corner");
    }

    #[test]
    fn its_square_is_5_by_5_cells_7_by_7_for_a_computer() {
        let mut units = vec![Unit::new(crate::id::Id::from_raw(1), Owner::Wild, UnitKind::Wildman, cells_off(3, 0))];
        let terrain = land().terrain;
        assert_eq!(scan(AT, REACH_CELLS, &units, &terrain).nearest, None, "3 cells out: outside 5 x 5");
        assert!(scan(AT, COMPUTER_REACH_CELLS, &units, &terrain).nearest.is_some(), "inside 7 x 7");
        units[0].x = cells_off(2, 0).0;
        units[0].z = cells_off(0, -2).1;
        assert!(scan(AT, REACH_CELLS, &units, &terrain).nearest.is_some(), "the corner of 5 x 5");
    }

    #[test]
    fn lives_18_turns_from_its_last_conversion_22_from_a_computer_cast() {
        let mut map = land();
        let id = map.add_effect(effect(TribeId::Blue, AT, false));
        map.spawn(Owner::Wild, UnitKind::Wildman, cells_off(1, 1));
        map.run(FIRST_TRY);
        map.run(Ticks::new(LIFE.get() - 1));
        assert!(map.effects.contains(id), "17 turns after the conversion, still there");
        map.tick();
        assert!(!map.effects.contains(id), "18 turns after it, gone");
        let lone = map.add_effect(effect(TribeId::Blue, AT, true));
        map.run(Ticks::new(COMPUTER_LIFE.get() - 1));
        assert!(map.effects.contains(lone));
        map.tick();
        assert!(!map.effects.contains(lone), "nobody to convert: its first life");
    }

    #[test]
    fn a_wildman_on_the_coast_steps_inland_and_waits_to_be_converted() {
        let mut map = land();
        for z in 30..50 {
            map.terrain.set(42, z, 0);
            map.terrain.set(43, z, 0);
        }
        map.add_effect(effect(TribeId::Blue, AT, false));
        let w = map.spawn(Owner::Wild, UnitKind::Wildman, cells_off(1, 0)).id;
        assert!(!all_land(&map.terrain, (41, 40)), "test setup: on the coast");
        map.run(FIRST_TRY);
        assert_eq!(map.unit(w).unwrap().owner, Owner::Wild, "not converted on the coast");
        map.run(CHECK_EVERY * 2);
        let u = map.unit(w).unwrap();
        assert_eq!(u.owner, Owner::Tribe(TribeId::Blue), "converted once inland");
        assert!(all_land(&map.terrain, (cell_of(u.x), cell_of(u.z))));
    }

    #[test]
    fn inland_is_just_inside_the_nearest_land_cell() {
        let mut terrain = land().terrain;
        for z in 30..50 {
            terrain.set(42, z, 0);
        }
        let from = (41 * CELL + 400, 40 * CELL + 100);
        assert_eq!(inland_of(&terrain, from), Some((41 * CELL - 1 - INLAND_STEP_IN as u16, 40 * CELL + 100)), "the cell west, at its east edge");
    }

    #[test]
    fn never_converts_once_the_tribe_is_full() {
        let mut map = land();
        map.add_effect(effect(TribeId::Blue, AT, false));
        for n in 0..MAX_POPULATION {
            map.spawn(TribeId::Blue, UnitKind::Brave, (0, n));
        }
        let wildman = map.spawn(Owner::Wild, UnitKind::Wildman, cells_off(0, 1)).id;
        map.run(CHECK_EVERY);
        assert_eq!(map.unit(wildman).unwrap().owner, Owner::Wild, "no room");
    }

    #[test]
    fn scan_ignores_the_dead_and_other_kinds() {
        let terrain = land().terrain;
        let mut units = vec![Unit::new(crate::id::Id::from_raw(1), Owner::Wild, UnitKind::Brave, AT), Unit::new(crate::id::Id::from_raw(2), Owner::Wild, UnitKind::Wildman, AT)];
        units[1].action = crate::unit::Action::Dead { left: Countdown::new(Ticks::new(3)) };
        assert_eq!(scan(AT, REACH_CELLS, &units, &terrain), Scan::default());
    }
}
