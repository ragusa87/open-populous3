//! A spy setting a building on fire (docs/specs/units.md "Spies", pop3-rev-analysis.md "Arson: a separate order,
//! kind 15"): it walks up to the middle of the building's wall, crouches, lights a firebrand and leaves it burning
//! on the wall, and the building catches fire as if struck by lightning (`GameMap::set_fire`, lit by its real
//! tribe), then it lingers. The computer's own order (`ai_spy.rs`), and a spy's attack on a building. Knocked out
//! of it by a fight, it starts over, until the building burns, is gone, or the spy dies.

use crate::building::BuildingId;
use crate::effect::{Effect, EffectKind};
use crate::event::WorldEvent;
use crate::map::GameMap;
use crate::time::{Countdown, Ticks};
use crate::tribes::Owner;
use crate::unit::{Action, Order, UnitKind, torus_delta};

/// Settling and facing the door before the windup (exe default, unverified live).
pub const AIM_TICKS: Ticks = Ticks::new(10);
/// Holding the lit firebrand before the throw ([exe, measured]: one live trace).
pub const WINDUP_TICKS: Ticks = Ticks::new(24);
/// The firebrand's flight, before the building catches (exe default, unverified live).
pub const FLIGHT_TICKS: Ticks = Ticks::new(8);
/// Lingering at the scene once it burns (exe default, unverified live).
pub const WITHDRAW_TICKS: Ticks = Ticks::new(24);
/// The firebrand left burning on the wall, a cosmetic effect [exe]: 64 turns.
pub const FIREBRAND_TICKS: Ticks = Ticks::new(64);
/// Close enough to its spot to start aiming, world units: the original's 12 on both axes [exe].
const ARRIVED: i32 = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Aim,
    /// The firebrand in hand.
    Windup,
    /// Thrown, still crouched facing the wall while it catches.
    Flight,
    Withdraw,
}

impl GameMap {
    /// `Order::Arson`, or a spy's attack on a building (user-confirmed in play: a spy sets an enemy building on
    /// fire instead of hacking at it): spy `i` goes to set `building` on fire, if it is another tribe's; anyone
    /// else ignores it.
    pub(crate) fn go_arson(&mut self, i: usize, building: BuildingId) {
        let u = &self.units[i];
        let enemy = self.building(building).is_some_and(|b| b.owner != u.owner && b.owner.tribe().is_some() && !self.tribes.allied(u.owner, b.owner));
        if u.kind == UnitKind::Spy && enemy {
            self.units[i].start(Order::Stop);
            self.units[i].arson = Some(building);
        }
    }

    /// Where a spy stands to set `building` on fire, wherever it comes from: against the middle of its back wall,
    /// opposite the door (the original's one flagged wall cell, pop3-rev-analysis.md "Arson" substate 0); just
    /// outside the walls, since ours cannot be stood on. Its door when that is not free ground.
    pub fn arson_spot(&self, building: BuildingId) -> Option<(u16, u16)> {
        let b = self.building(building)?;
        let f = b.kind.footprint();
        let (ox, oz) = f.offset;
        let back = [crate::building::DOOR_GAP, 512, 768].into_iter().map(|gap| b.local_point((ox, oz + f.half.1 + gap))).find(|&p| self.free_ground(p));
        Some(back.unwrap_or(b.door()))
    }

    /// Ground a spy can stand on: not behind any building's walls, not the sea.
    fn free_ground(&self, p: (u16, u16)) -> bool {
        !self.behind_walls(p) && !crate::unit::is_sea(&self.terrain, (crate::unit::cell_of(p.0), crate::unit::cell_of(p.1)))
    }

    /// Whether `building` can still be set on fire.
    fn arson_target(&self, building: BuildingId) -> bool {
        self.building(building).is_some_and(|b| b.burning.is_none() && !b.kind.fireproof() && b.stage() != crate::building::Stage::Blueprint)
    }

    /// Every spy with an arson order: walks to its spot, then aims, throws and lingers.
    pub(crate) fn tend_arson(&mut self) {
        for i in 0..self.units.len() {
            let Some(building) = self.units[i].arson else { continue };
            let u = &self.units[i];
            if !u.is_alive() {
                self.units[i].arson = None;
                continue;
            }
            match u.action {
                Action::Arson { building, stage, left } => {
                    let events = self.arson_turn(i, building, stage, left);
                    self.apply_events(events);
                }
                Action::Idle if u.in_the_open() => {
                    if !self.arson_target(building) {
                        self.units[i].arson = None;
                        continue;
                    }
                    let Some(spot) = self.arson_spot(building) else { continue };
                    if torus_delta(u.x, spot.0).abs() <= ARRIVED && torus_delta(u.z, spot.1).abs() <= ARRIVED {
                        let centre = self.building(building).map(|b| b.centre()).unwrap_or(spot);
                        self.units[i].face(centre);
                        self.units[i].action = Action::Arson { building, stage: Stage::Aim, left: Countdown::new(AIM_TICKS) };
                    } else {
                        self.units[i].start(Order::MoveTo { x: spot.0, z: spot.1 });
                    }
                }
                _ => {}
            }
        }
    }

    /// One turn of spy `i`'s arson on `building` at `stage`.
    fn arson_turn(&mut self, i: usize, building: BuildingId, stage: Stage, mut left: Countdown) -> Vec<WorldEvent> {
        let Some(b) = self.building(building) else {
            self.end_arson(i);
            return Vec::new();
        };
        let (door, centre, owner) = (b.door(), b.centre(), self.units[i].owner);
        if matches!(stage, Stage::Aim | Stage::Windup) {
            if !self.arson_target(building) {
                self.end_arson(i);
                return Vec::new();
            }
            if self.guarded_door(i, door) {
                self.units[i].reveal();
                self.end_arson(i);
                return Vec::new();
            }
        }
        let mut events = Vec::new();
        if !left.tick() {
            self.units[i].action = Action::Arson { building, stage, left };
            return events;
        }
        let u = &self.units[i];
        let next = match stage {
            Stage::Aim => Some((Stage::Windup, WINDUP_TICKS)),
            Stage::Windup => {
                let wall = self.building(building).map_or(centre, |b| wall_point(b, (u.x, u.z)));
                let firebrand = Effect::new(owner, (u.x, u.z), Countdown::new(FIREBRAND_TICKS), EffectKind::Firebrand { to: wall });
                events.push(WorldEvent::SpawnEffect(firebrand));
                Some((Stage::Flight, FLIGHT_TICKS))
            }
            Stage::Flight => {
                events.push(WorldEvent::SetFire { building, by: owner });
                if self.noticed(owner, building) {
                    self.units[i].reveal();
                }
                Some((Stage::Withdraw, WITHDRAW_TICKS))
            }
            Stage::Withdraw => None,
        };
        match next {
            Some((stage, ticks)) => self.units[i].action = Action::Arson { building, stage, left: Countdown::new(ticks) },
            None => self.end_arson(i),
        }
        events
    }

    /// Whether the owners of `building` notice a spy of tribe `spy` setting it on fire ("Arson" substate 4 [exe]):
    /// rolled only for a human player's spy, outside multiplayer games, against the owner's `SPY_DISCOVER_CHANCE`
    /// (0 for a tribe without a computer player: never).
    fn noticed(&mut self, spy: Owner, building: BuildingId) -> bool {
        let Some(owner) = self.building(building).and_then(|b| b.owner.tribe()) else { return false };
        if self.multiplayer || spy.tribe().is_none_or(|t| self.tribes.is_computer(t)) {
            return false;
        }
        let chance = self.ai_attr(owner, crate::ai_spy::SPY_DISCOVER_CHANCE);
        (self.rng.next_u32() % 100) < chance.max(0) as u32
    }

    /// Whether a genuine member of the tribe spy `i` is disguised as stands at the `door` ("a real one at the door"
    /// [exe]): its disguise would not fool them up close.
    fn guarded_door(&self, i: usize, door: (u16, u16)) -> bool {
        let Some(d) = self.units[i].statuses.disguise else { return false };
        let cell = (crate::unit::cell_of(door.0), crate::unit::cell_of(door.1));
        self.units.iter().any(|u| u.owner == Owner::Tribe(d.apparent) && u.is_alive() && u.in_the_open() && u.cell() == cell)
    }

    fn end_arson(&mut self, i: usize) {
        self.units[i].arson = None;
        if matches!(self.units[i].action, Action::Arson { .. }) {
            self.units[i].action = Action::Idle;
        }
    }
}

/// The middle of `b`'s wall facing `from`: where the spy's firebrand burns.
pub fn wall_point(b: &crate::building::Building, from: (u16, u16)) -> (u16, u16) {
    let (cx, cz) = b.centre();
    let f = b.kind.footprint();
    let (x, z) = crate::terrain::to_local((torus_delta(cx, from.0), torus_delta(cz, from.1)), b.facing / 2);
    let (x, z) = (x - f.offset.0, z - f.offset.1);
    let local = if x.abs() * f.half.1 >= z.abs() * f.half.0 { (f.offset.0 + x.signum() * f.half.0, f.offset.1) } else { (f.offset.0, f.offset.1 + z.signum() * f.half.1) };
    b.local_point(local)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::{Building, BuildingKind};
    use crate::command::Command;
    use crate::tribes::TribeId;
    use crate::unit::UnitId;

    fn scene(kind: UnitKind) -> (GameMap, BuildingId, UnitId) {
        let mut map = GameMap::sandbox_units().seeded(9);
        map.units.clear();
        let hut = map.add_building(Building::new(BuildingKind::Hut { size: 1 }, TribeId::Red, 70 * 512, 70 * 512, 0));
        let door = map.building(hut).unwrap().door();
        let spy = map.spawn(TribeId::Blue, kind, (door.0.wrapping_add(2048), door.1)).id;
        map.apply(&Command::OrderUnit { player: TribeId::Blue, unit: spy, order: Order::Arson { building: hut } });
        (map, hut, spy)
    }

    fn until(map: &mut GameMap, turns: u32, done: impl Fn(&GameMap) -> bool) -> Option<u32> {
        (1..=turns).find(|_| {
            map.tick();
            done(map)
        })
    }

    fn stage(map: &GameMap, spy: UnitId) -> Option<Stage> {
        match map.unit(spy).unwrap().action {
            Action::Arson { stage, .. } => Some(stage),
            _ => None,
        }
    }

    #[test]
    fn its_spot_is_the_middle_of_the_back_wall_opposite_the_door() {
        let (map, hut, _) = scene(UnitKind::Spy);
        let b = map.building(hut).unwrap();
        let (cx, cz) = b.centre();
        let local = |p: (u16, u16)| crate::terrain::to_local((torus_delta(cx, p.0), torus_delta(cz, p.1)), b.facing / 2);
        let f = b.kind.footprint();
        let spot = map.arson_spot(hut).unwrap();
        let (x, z) = local(spot);
        assert!(!map.behind_walls(spot) && b.covers(spot, 1024), "free ground just outside it: {spot:?}");
        assert!(x == f.offset.0 && z > f.offset.1 + f.half.1, "the middle of the back wall: {x}, {z}");
        let (_, door_z) = local(b.door());
        assert!(door_z < f.offset.1, "the door is the other side");
    }

    #[test]
    fn a_human_player_s_spy_is_noticed_at_the_owner_s_own_chance_a_computer_s_never() {
        let run = |chance: Option<i32>, computer_spy: bool| {
            let (mut map, hut, spy) = scene(UnitKind::Spy);
            if let Some(c) = chance {
                map.ai.push(crate::ai::NativeAi::new(TribeId::Red));
                map.ai_set_attr(TribeId::Red, crate::ai_spy::SPY_DISCOVER_CHANCE, c);
            }
            if computer_spy {
                map.tribes = map.tribes.clone().with_ai_scripts([1, 0, 0, 0]);
            }
            map.unit_mut(spy).unwrap().disguise_as(TribeId::Green, Ticks::ZERO);
            until(&mut map, 600, |m| m.building(hut).unwrap().burning.is_some()).expect("lit");
            map.tick();
            map.unit(spy).unwrap().disguised()
        };
        assert!(!run(Some(100), false), "noticed for sure");
        assert!(run(Some(0), false) && run(None, false), "never: no chance, or a human owner");
        assert!(run(Some(100), true), "a computer's spy is never rolled for");
    }

    #[test]
    fn a_spy_ordered_to_attack_an_enemy_building_sets_it_on_fire_instead_of_hacking() {
        let mut map = GameMap::sandbox_units().seeded(9);
        map.units.clear();
        let hut = map.add_building(Building::new(BuildingKind::Hut { size: 1 }, TribeId::Red, 70 * 512, 70 * 512, 0));
        let door = map.building(hut).unwrap().door();
        let spy = map.spawn(TribeId::Blue, UnitKind::Spy, (door.0.wrapping_add(2048), door.1)).id;
        map.apply(&Command::OrderUnit { player: TribeId::Blue, unit: spy, order: Order::Attack { target: crate::unit::Target::Building(hut) } });
        assert_eq!(map.unit(spy).unwrap().arson, Some(hut));
        until(&mut map, 800, |m| m.building(hut).unwrap().burning.is_some()).expect("set on fire");
        let own = map.add_building(Building::new(BuildingKind::Hut { size: 1 }, TribeId::Blue, 60 * 512, 70 * 512, 0));
        map.apply(&Command::OrderUnit { player: TribeId::Blue, unit: spy, order: Order::Arson { building: own } });
        assert_eq!(map.unit(spy).unwrap().arson, None, "never its own");
    }

    #[test]
    fn it_walks_up_aims_throws_and_the_building_burns_lit_by_its_real_tribe() {
        let (mut map, hut, spy) = scene(UnitKind::Spy);
        map.unit_mut(spy).unwrap().disguise_as(TribeId::Red, Ticks::ZERO);
        until(&mut map, 600, |m| stage(m, spy) == Some(Stage::Aim)).expect("arrived and aiming");
        let windup = until(&mut map, 100, |m| stage(m, spy) == Some(Stage::Windup)).unwrap();
        assert_eq!(windup, AIM_TICKS.get());
        let thrown = until(&mut map, 100, |m| stage(m, spy) == Some(Stage::Flight)).unwrap();
        assert_eq!(thrown, WINDUP_TICKS.get());
        assert!(map.effects.iter().any(|(_, e)| matches!(e.kind, EffectKind::Firebrand { .. })), "the firebrand flies");
        assert!(map.building(hut).unwrap().burning.is_none(), "not yet: in flight");
        let lit = until(&mut map, 100, |m| m.building(hut).unwrap().burning.is_some()).unwrap();
        assert_eq!(lit, FLIGHT_TICKS.get());
        assert_eq!(map.building(hut).unwrap().ignited_by, Some(Owner::Tribe(TribeId::Blue)));
        until(&mut map, 100, |m| m.unit(spy).unwrap().action == Action::Idle).expect("done after lingering");
        assert_eq!(map.unit(spy).unwrap().arson, None);
    }

    #[test]
    fn a_real_one_of_the_imitated_tribe_at_the_door_unmasks_it_before_the_throw() {
        let (mut map, hut, spy) = scene(UnitKind::Spy);
        map.unit_mut(spy).unwrap().disguise_as(TribeId::Red, Ticks::ZERO);
        until(&mut map, 600, |m| stage(m, spy) == Some(Stage::Aim)).expect("aiming");
        let door = map.building(hut).unwrap().door();
        map.spawn(TribeId::Red, UnitKind::Preacher, door);
        map.tick();
        let u = map.unit(spy).unwrap();
        assert!(!u.disguised() && u.arson.is_none() && map.building(hut).unwrap().burning.is_none());
    }

    #[test]
    fn interrupted_it_starts_over() {
        let (mut map, hut, spy) = scene(UnitKind::Spy);
        until(&mut map, 600, |m| stage(m, spy) == Some(Stage::Windup)).expect("winding up");
        map.unit_mut(spy).unwrap().action = Action::Idle;
        map.tick();
        assert_eq!(stage(&map, spy), Some(Stage::Aim));
        until(&mut map, 200, |m| m.building(hut).unwrap().burning.is_some()).expect("it burns in the end");
    }

    #[test]
    fn only_a_spy_takes_it_and_another_order_cancels_it() {
        let (map, _, brave) = scene(UnitKind::Brave);
        assert_eq!(map.unit(brave).unwrap().arson, None);
        let (mut map, _, spy) = scene(UnitKind::Spy);
        map.apply(&Command::OrderUnit { player: TribeId::Blue, unit: spy, order: Order::Stop });
        assert_eq!(map.unit(spy).unwrap().arson, None);
    }

    #[test]
    fn the_firebrand_burns_on_the_middle_of_the_wall_facing_the_spy() {
        let (map, hut, _) = scene(UnitKind::Spy);
        let b = map.building(hut).unwrap();
        let f = b.kind.footprint();
        let east = b.local_point((f.offset.0 + f.half.0 + 600, f.offset.1));
        assert_eq!(wall_point(b, east), b.local_point((f.offset.0 + f.half.0, f.offset.1)));
        let back = b.local_point((f.offset.0, f.offset.1 + f.half.1 + 600));
        assert_eq!(wall_point(b, back), b.local_point((f.offset.0, f.offset.1 + f.half.1)));
    }
}
