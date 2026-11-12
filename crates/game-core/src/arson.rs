//! A spy setting a building on fire (docs/specs/units.md "Spies", pop3-rev-analysis.md "Arson: a separate order,
//! kind 15"): it walks up to the building's wall, aims, winds up with a lit firebrand, throws it, and the building
//! catches fire as if struck by lightning (`GameMap::set_fire`, lit by its real tribe), then it lingers. Only the
//! computer gives this order (`ai.rs`); the original never lets a player arm it. Knocked out of it by a fight, it
//! starts over, until the building burns, is gone, or the spy dies.

use crate::building::BuildingId;
use crate::effect::{Effect, EffectKind};
use crate::event::WorldEvent;
use crate::map::{GameMap, torus_dist2};
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
/// The thrown firebrand's own life, a cosmetic effect [exe].
pub const FIREBRAND_TICKS: Ticks = Ticks::new(64);
/// The firebrand's speed, world units a turn [exe].
pub const FIREBRAND_SPEED: i32 = 80;
/// Percent chance the owners notice once it burns and the spy is revealed: the original reads a per-tribe table
/// whose numbers were never read. `[guess]`.
pub const NOTICED_PERCENT: u32 = 25;
/// Close enough to its spot to start aiming, world units: the original's 12 on both axes [exe].
const ARRIVED: i32 = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Aim,
    /// The firebrand in hand.
    Windup,
    /// Thrown, turned away while it flies.
    Flight,
    Withdraw,
}

impl GameMap {
    /// `Order::Arson`: spy `i` goes to set `building` on fire; anyone else ignores it.
    pub(crate) fn go_arson(&mut self, i: usize, building: BuildingId) {
        if self.units[i].kind == UnitKind::Spy && self.building(building).is_some() {
            self.units[i].start(Order::Stop);
            self.units[i].arson = Some(building);
        }
    }

    /// Where a spy stands to set `building` on fire from `from`: the free cell along its walls nearest to it, else
    /// its door. The original picks a flagged wall cell of the footprint itself; ours cannot be stood on, so the
    /// cell just outside it. `[ours]`.
    pub fn arson_spot(&self, building: BuildingId, from: (u16, u16)) -> Option<(u16, u16)> {
        let b = self.building(building)?;
        let size = self.terrain.size() as i32;
        let walls = b.walled_cells(size);
        let around = walls.iter().flat_map(|&(x, z)| [(x + 1, z), (x - 1, z), (x, z + 1), (x, z - 1)]).map(|(x, z)| (x.rem_euclid(size), z.rem_euclid(size)));
        let spot = around
            .filter(|c| !walls.contains(c))
            .map(crate::slots::cell_centre)
            .filter(|&p| !self.behind_walls(p) && !crate::unit::is_sea(&self.terrain, ((p.0 / 512) as i32, (p.1 / 512) as i32)))
            .min_by_key(|&p| (torus_dist2(from, p), p));
        Some(spot.unwrap_or(b.door()))
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
                    let Some(spot) = self.arson_spot(building, (u.x, u.z)) else { continue };
                    if torus_delta(u.x, spot.0).abs() <= ARRIVED && torus_delta(u.z, spot.1).abs() <= ARRIVED {
                        let door = self.building(building).map(|b| b.door()).unwrap_or(spot);
                        self.units[i].face(door);
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
        let (door, owner) = (b.door(), self.units[i].owner);
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
                let firebrand = Effect::new(owner, (u.x, u.z), Countdown::new(FIREBRAND_TICKS), EffectKind::Firebrand { to: door });
                events.push(WorldEvent::SpawnEffect(firebrand));
                let away = (u.x.wrapping_sub(door.0.wrapping_sub(u.x)), u.z.wrapping_sub(door.1.wrapping_sub(u.z)));
                self.units[i].face(away);
                Some((Stage::Flight, FLIGHT_TICKS))
            }
            Stage::Flight => {
                self.units[i].face(door);
                events.push(WorldEvent::SetFire { building, by: owner });
                if self.rng.next_u32() % 100 < NOTICED_PERCENT {
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

/// Where a firebrand thrown from `from` towards `to` is after `done` turns: straight at `FIREBRAND_SPEED`, stopping
/// there.
pub fn firebrand_at(from: (u16, u16), to: (u16, u16), done: Ticks) -> (u16, u16) {
    let (dx, dz) = (torus_delta(from.0, to.0), torus_delta(from.1, to.1));
    let d = crate::unit::isqrt((dx * dx + dz * dz) as u32) as i32;
    let flown = (done.get() as i32 * FIREBRAND_SPEED).min(d);
    if d == 0 {
        return to;
    }
    (from.0.wrapping_add((dx * flown / d) as u16), from.1.wrapping_add((dz * flown / d) as u16))
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
    fn its_spot_is_beside_the_wall_nearest_to_it() {
        let (map, hut, spy) = scene(UnitKind::Spy);
        let u = map.unit(spy).unwrap();
        let spot = map.arson_spot(hut, (u.x, u.z)).unwrap();
        let b = map.building(hut).unwrap();
        assert!(!b.covers(spot, 0) && b.covers(spot, 512), "just outside its footprint: {spot:?}");
        assert!(torus_dist2((u.x, u.z), spot) < torus_dist2((u.x, u.z), b.door()), "the side facing the spy, not the door");
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
    fn the_firebrand_flies_straight_and_stops_at_its_mark() {
        assert_eq!(firebrand_at((1000, 1000), (1400, 1000), Ticks::new(1)), (1080, 1000));
        assert_eq!(firebrand_at((1000, 1000), (1400, 1000), Ticks::new(10)), (1400, 1000));
        assert_eq!(firebrand_at((65500, 0), (100, 0), Ticks::new(1)), (44, 0), "wraps");
    }
}
