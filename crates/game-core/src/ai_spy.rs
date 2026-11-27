//! A computer tribe's spies (docs/specs/units.md "Spies", pop3-rev-analysis.md "Arson" and "What un-disguises a
//! spy"): it sends its idle spies to set enemy buildings on fire, and catches other tribes' spies passing as its
//! own people. Reaches the simulation only through commands, like the rest of `ai.rs`.

use crate::building::BuildingId;
use crate::command::Command;
use crate::map::GameMap;
use crate::time::Every;
use crate::tribes::TribeId;
use crate::unit::{Action, Order, UnitId, UnitKind};
use pop3_ai_script::field::InternalVar;

/// `INT_ATTR_MAX_SPY_ATTACKS`: how many sabotage raids (one spy each) it runs at once (pop3-rev-analysis.md
/// "Arson", sabotage raids [exe]).
pub(crate) const MAX_SPY_ATTACKS: InternalVar = InternalVar(1023);
/// `INT_ATTR_ENEMY_SPY_MAX_STAND`: a spy idle longer than this in turns is caught for sure (live 255).
pub(crate) const ENEMY_SPY_MAX_STAND: InternalVar = InternalVar(1024);
/// `INT_ATTR_SPY_DISCOVER_CHANCE`: percent chance to catch each spy on a check (live 45).
pub(crate) const SPY_DISCOVER_CHANCE: InternalVar = InternalVar(1040);
/// How often a tribe starts sabotage raids: its "decide on a new goal" pass, about every 64 turns [exe].
const TASK_EVERY: Every = Every::new(64);
/// How often a tribe looks for spies passing as its own: the discovery task's cadence is not traced. `[guess]`
/// (`INT_ATTR_SPY_CHECK_FREQUENCY` is an attack party's re-disguise period, not this.)
const CHECK_EVERY: Every = Every::new(32);
/// Cells around a caught spy within which every other spy passing as the tribe is unmasked too: its 3 × 3 cells
/// ("What un-disguises a spy" [exe]).
const SWEEP_CELLS: i32 = 1;

/// Whether a spy passing as the checking tribe is caught: the roll under `chance` percent, or standing idle longer
/// than `max_stand` turns.
pub fn caught(roll: u32, chance: i32, idle: u16, max_stand: i32) -> bool {
    (roll % 100) < chance.max(0) as u32 || i32::from(idle) > max_stand
}

impl GameMap {
    /// Every computer tribe with `STATE_SABOTAGE` on sends its idle spies against enemy buildings, and every one
    /// checks for spies passing as its own.
    pub(crate) fn tend_ai_spies(&mut self) {
        let owners: Vec<TribeId> = self.ai.iter().map(|a| a.owner).collect();
        for owner in owners {
            let phase = owner.as_u8() as u32;
            if TASK_EVERY.fires(self.now, phase) && self.ai_state(owner, crate::ai::state::SABOTAGE) {
                self.ai_task_spies(owner);
            }
            if CHECK_EVERY.fires(self.now, phase) {
                self.ai_find_spies(owner);
            }
        }
    }

    /// A sabotage raid's target ("Arson", sabotage raids [exe]): a random enemy tribe, then a random one of its
    /// buildings, walls excluded (and, ours, what a spy cannot set on fire: fireproof, a plan, burning already).
    fn ai_raid_target(&mut self, owner: TribeId) -> Option<BuildingId> {
        let enemies: Vec<TribeId> = self.tribes.all().filter(|&t| t != owner && !self.tribes.same_team(t, owner)).collect();
        if enemies.is_empty() {
            return None;
        }
        let tribe = enemies[self.rng.next_u32() as usize % enemies.len()];
        let targets: Vec<BuildingId> = self
            .buildings
            .iter()
            .filter(|b| b.owner == tribe && b.kind != crate::building::BuildingKind::WallPiece && !b.kind.fireproof() && b.burning.is_none() && b.stage() != crate::building::Stage::Blueprint)
            .map(|b| b.id)
            .collect();
        (!targets.is_empty()).then(|| targets[self.rng.next_u32() as usize % targets.len()])
    }

    fn ai_task_spies(&mut self, owner: TribeId) {
        let out = self.units.iter().filter(|u| u.owner == owner && u.arson.is_some()).count() as i32;
        let room = (self.ai_attr(owner, MAX_SPY_ATTACKS) - out).max(0) as usize;
        let idle: Vec<UnitId> = self
            .units
            .iter()
            .filter(|u| u.owner == owner && u.kind == UnitKind::Spy && u.is_alive() && u.is_free() && u.in_the_open() && u.arson.is_none() && u.queued_patrol().is_empty())
            .map(|u| u.id)
            .take(room)
            .collect();
        for spy in idle {
            let Some(target) = self.ai_raid_target(owner) else { continue };
            let Some(tribe) = self.building(target).and_then(|b| b.owner.tribe()) else { continue };
            if self.unit(spy).and_then(|u| u.statuses.disguise).map(|d| d.apparent) != Some(tribe) {
                self.apply(&Command::OrderUnit { player: owner, unit: spy, order: Order::Disguise { tribe } });
            }
            self.apply(&Command::OrderUnit { player: owner, unit: spy, order: Order::Arson { building: target } });
        }
    }

    /// Other tribes' spies passing as `owner`'s people inside its settlement area (`in_settlement`): each caught by
    /// a roll or for loitering unmasks every one passing as `owner` within `SWEEP_CELLS` of it. Never a non-spy:
    /// only a disguise can pass as `owner`.
    fn ai_find_spies(&mut self, owner: TribeId) {
        let (chance, max_stand) = (self.ai_attr(owner, SPY_DISCOVER_CHANCE), self.ai_attr(owner, ENEMY_SPY_MAX_STAND));
        let suspects: Vec<usize> = (0..self.units.len()).filter(|&i| self.units[i].is_alive() && self.units[i].fools(owner) && self.in_settlement(owner, self.units[i].cell())).collect();
        for i in suspects {
            if !self.units[i].fools(owner) {
                continue;
            }
            let idle = if self.units[i].action == Action::Idle { self.units[i].idle_ticks() } else { 0 };
            if !caught(self.rng.next_u32(), chance, idle, max_stand) {
                continue;
            }
            let (cx, cz) = self.units[i].cell();
            let size = self.terrain.size() as i32;
            let near = |c: (i32, i32)| {
                let d = |a: i32, b: i32| (a - b).rem_euclid(size).min((b - a).rem_euclid(size));
                d(c.0, cx) <= SWEEP_CELLS && d(c.1, cz) <= SWEEP_CELLS
            };
            for u in self.units.iter_mut().filter(|u| u.fools(owner) && near(u.cell())) {
                u.reveal();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{NativeAi, state};
    use crate::building::{Building, BuildingKind};
    use crate::time::Ticks;

    #[test]
    fn caught_by_the_roll_or_for_loitering() {
        assert!(caught(44, 45, 0, 255));
        assert!(!caught(45, 45, 0, 255));
        assert!(caught(99, 45, 256, 255), "idle too long");
        assert!(!caught(99, 45, 255, 255));
    }

    fn red_ai_and_blue_hut() -> (GameMap, BuildingId) {
        let mut map = GameMap::sandbox_units().seeded(3);
        map.units.clear();
        map.buildings.clear();
        map.ai.push(NativeAi::new(TribeId::Red));
        let hut = map.add_building(Building::new(BuildingKind::Hut { size: 1 }, TribeId::Blue, 66 * 512, 70 * 512, 0));
        (map, hut)
    }

    #[test]
    fn with_sabotage_on_it_disguises_an_idle_spy_and_sends_it_to_burn_an_enemy_hut() {
        let (mut map, hut) = red_ai_and_blue_hut();
        let spy = map.spawn(TribeId::Red, UnitKind::Spy, (62 * 512, 66 * 512)).id;
        map.run(Ticks::new(TASK_EVERY.period()));
        assert_eq!(map.unit(spy).unwrap().arson, None, "sabotage off");
        map.ai_set_state(TribeId::Red, state::SABOTAGE, true);
        map.run(Ticks::new(TASK_EVERY.period()));
        let u = map.unit(spy).unwrap();
        assert_eq!(u.arson, Some(hut));
        assert_eq!(u.statuses.disguise.map(|d| d.apparent), Some(TribeId::Blue));
        let burnt = (0..1500).any(|_| {
            map.tick();
            map.building(hut).is_some_and(|b| b.burning.is_some())
        });
        assert!(burnt);
    }

    #[test]
    fn only_inside_its_settlement_area_and_the_sweep_takes_the_3x3_cells_around_the_caught_one() {
        let (mut map, _) = red_ai_and_blue_hut();
        map.add_building(Building::new(BuildingKind::Hut { size: 1 }, TribeId::Red, 64 * 512, 54 * 512, 0));
        let spy = |map: &mut GameMap, cell: (u16, u16)| {
            let id = map.spawn(TribeId::Blue, UnitKind::Spy, (cell.0 * 512 + 256, cell.1 * 512 + 256)).id;
            map.unit_mut(id).unwrap().disguise_as(TribeId::Red, Ticks::ZERO);
            id
        };
        let far = spy(&mut map, (60, 66));
        let caught = spy(&mut map, (64, 60));
        let beside = spy(&mut map, (64, 61));
        map.ai_set_attr(TribeId::Red, SPY_DISCOVER_CHANCE, 0);
        map.ai_set_attr(TribeId::Red, ENEMY_SPY_MAX_STAND, 100_000);
        map.ai_set_defence_radius(TribeId::Red, 7);
        assert!(map.in_settlement(TribeId::Red, (64, 60)) && !map.in_settlement(TribeId::Red, (64, 61)), "radius 6");
        map.unit_mut(caught).unwrap().action = Action::Idle;
        map.ai_set_attr(TribeId::Red, ENEMY_SPY_MAX_STAND, -1);
        map.run(Ticks::new(CHECK_EVERY.period()));
        assert!(!map.unit(caught).unwrap().disguised(), "loitering in its area");
        assert!(!map.unit(beside).unwrap().disguised(), "in the 3 x 3 cells around it, though outside the area");
        assert!(map.unit(far).unwrap().disguised(), "outside every disc: never rolled for");
        assert_eq!(crate::ai::settlement_radius(9), 8);
        assert_eq!(crate::ai::settlement_radius(24), crate::ai::SETTLEMENT_RADIUS);
    }

    #[test]
    fn a_raid_goes_for_a_building_of_an_enemy_tribe_never_a_wall() {
        let (mut map, hut) = red_ai_and_blue_hut();
        map.add_building(Building::new(BuildingKind::WallPiece, TribeId::Blue, 80 * 512, 80 * 512, 0));
        map.add_building(Building::new(BuildingKind::Hut { size: 1 }, TribeId::Red, 40 * 512, 40 * 512, 0));
        assert!((0..50).all(|_| map.ai_raid_target(TribeId::Red) == Some(hut)));
    }

    #[test]
    fn no_more_spies_out_than_its_max_spy_attacks() {
        let (mut map, _) = red_ai_and_blue_hut();
        let spies: Vec<UnitId> = (0..3).map(|k| map.spawn(TribeId::Red, UnitKind::Spy, (62 * 512, (66 + k) * 512)).id).collect();
        map.ai_set_state(TribeId::Red, state::SABOTAGE, true);
        map.ai_set_attr(TribeId::Red, MAX_SPY_ATTACKS, 2);
        map.run(Ticks::new(TASK_EVERY.period()));
        assert_eq!(spies.iter().filter(|&&s| map.unit(s).unwrap().arson.is_some()).count(), 2);
    }

    #[test]
    fn it_unmasks_a_spy_passing_as_its_own_never_one_passing_as_another_tribe() {
        let (mut map, _) = red_ai_and_blue_hut();
        map.add_building(Building::new(BuildingKind::Hut { size: 1 }, TribeId::Red, 60 * 512, 64 * 512, 0));
        map.ai_set_attr(TribeId::Red, SPY_DISCOVER_CHANCE, 100);
        let as_red = map.spawn(TribeId::Blue, UnitKind::Spy, (64 * 512, 64 * 512)).id;
        map.unit_mut(as_red).unwrap().disguise_as(TribeId::Red, Ticks::ZERO);
        let as_green = map.spawn(TribeId::Blue, UnitKind::Spy, (64 * 512, 60 * 512)).id;
        map.unit_mut(as_green).unwrap().disguise_as(TribeId::Green, Ticks::ZERO);
        map.run(Ticks::new(CHECK_EVERY.period()));
        assert!(!map.unit(as_red).unwrap().disguised());
        assert!(map.unit(as_green).unwrap().disguised());
    }

    #[test]
    fn in_the_played_units_sandbox_red_burns_the_blue_hut() {
        let mut map = GameMap::sandbox_units_with_spies().seeded(1);
        let hut = map.buildings.iter().find(|b| b.owner == TribeId::Blue).unwrap().id;
        let burnt = (0..2000).any(|_| {
            map.tick();
            map.building(hut).is_some_and(|b| b.burning.is_some())
        });
        assert!(burnt);
    }
}
