//! Attacking a building (docs/specs/combat.md "Attacking a building"): the attacker hacks at its door, brings its
//! people out to fight them on the porch, then walks in and wears it down from inside; the building loses a piece of
//! wood each time its wear reaches its limit.

use crate::angle;
use crate::building::{Building, BuildingId, BuildingKind};
use crate::challenge::{Approach, Challenge, Role};
use crate::event::WorldEvent;
use crate::map::GameMap;
use crate::terrain::DirtyRect;
use crate::motion::{Motion, Velocity};
use crate::time::{Countdown, Every, Ticks};
use crate::unit::{isqrt, torus_delta, Action, Order, Target, UnitId};
use pop3_format::WORLD_UNITS_PER_CELL;

/// Hacking at the door before anything else ("Attacking a building" [exe]).
pub const HACK_AT_DOOR: Ticks = Ticks::new(16);
/// A round of hacking: the first plus a draw below the second ("Attacking a building" [exe]).
pub const HACK_TICKS: (u32, u32) = (8, 16);
/// Between rounds the attacker walks to a point this far from the building's middle, inside its walls ("Attacking a
/// building" [exe]).
pub const WANDER: i32 = 360;
/// Draws for a spot clear of other buildings' walls before the attacker settles for the middle.
const WANDER_TRIES: u32 = 8;
/// An attacker this close to where it was going has arrived.
pub const ARRIVED: i32 = 32;
/// Each blow shakes the building for this many turns ("Attacking a building" [exe]).
pub const SHAKE_TICKS: u16 = 6;
/// Wear fades by one on this rhythm ("Attacking a building" [exe]).
pub const WEAR_DECAY: Every = Every::new(4);
/// After a piece of wood is lost to wear, nobody repairs the building for this long (constant.dat value).
pub const REPAIR_DELAY: Ticks = Ticks::new(1200);
/// Someone pulled out of a building is thrown from its door at this speed per turn (exe), lifted by this much so it
/// carries about as far as the original's slowing slide (guess).
pub const PULL_SPEED: i32 = 100;
pub const PULL_LIFT: i16 = 24;

/// Whether `u` has come to `to`.
fn arrived(u: &crate::unit::Unit, to: (u16, u16)) -> bool {
    torus_delta(u.x, to.0).abs() <= ARRIVED && torus_delta(u.z, to.1).abs() <= ARRIVED
}

/// The wear a building takes before it loses a piece of wood, None when it never does ("Attacking a building"
/// [exe]).
pub fn wear_limit(kind: BuildingKind) -> Option<u16> {
    match kind {
        BuildingKind::Vault => None,
        BuildingKind::WallPiece | BuildingKind::Gate => Some(16_000),
        BuildingKind::Prison => Some(22_000),
        _ => Some(2000),
    }
}

/// A building big enough that a lost piece of wood throws its people out ("Attacking a building" [exe]; which
/// kinds count is a guess).
pub fn large(kind: BuildingKind) -> bool {
    !matches!(kind, BuildingKind::Hut { size: 1 | 2 } | BuildingKind::DrumTower | BuildingKind::GuardPost | BuildingKind::WallPiece | BuildingKind::Gate)
}

/// What a turn of wear does to a building.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wear {
    Nothing,
    /// A piece of wood lost; `destroyed` when it was the last.
    Hit { destroyed: bool },
}

impl Building {
    /// `amount` of wear from an attacker; it shakes.
    pub fn wear_down(&mut self, amount: u16) {
        self.wear = self.wear.saturating_add(amount);
        self.shaking = SHAKE_TICKS;
    }

    /// One turn of its wear at turn `now`: it fades, and at its limit costs a piece of wood. Its shaking and repair
    /// delay count down.
    pub fn tend_wear(&mut self, now: crate::time::Tick) -> Wear {
        self.shaking = self.shaking.saturating_sub(1);
        if let Some(delay) = self.repair_delay.as_mut()
            && delay.tick()
        {
            self.repair_delay = None;
        }
        if WEAR_DECAY.fires(now, 0) {
            self.wear = self.wear.saturating_sub(1);
        }
        let Some(limit) = wear_limit(self.kind).filter(|&l| self.wear >= l) else { return Wear::Nothing };
        self.wear -= limit;
        self.repair_delay = Some(Countdown::new(REPAIR_DELAY));
        Wear::Hit { destroyed: self.lose_a_piece() }
    }
}

impl GameMap {
    /// Every building's wear for this turn: lost wood with damage smoke, a large one's people thrown out, a building
    /// with no wood left destroyed.
    pub(crate) fn tend_building_wear(&mut self) -> Vec<DirtyRect> {
        let now = self.now;
        let hits: Vec<(BuildingId, bool)> = self.buildings.iter_mut().filter_map(|b| match b.tend_wear(now) {
            Wear::Hit { destroyed } => Some((b.id, destroyed)),
            Wear::Nothing => None,
        }).collect();
        let (mut events, mut dirty) = (Vec::new(), Vec::new());
        for (id, destroyed) in hits {
            let Some(i) = self.building_index(id) else { continue };
            let b = &self.buildings[i];
            events.push(WorldEvent::SpawnEffect(crate::smoke::damage(b.owner, b.centre())));
            if destroyed {
                events.extend(self.throw_out(id));
                dirty.extend(self.burn_down(id));
            } else if large(b.kind) {
                events.extend(self.throw_out(id));
            }
        }
        dirty.extend(self.apply_events(events));
        dirty
    }

    /// The attackers of buildings: at the door, hacking, pulling people out, walking round it.
    pub(crate) fn tend_wrecking(&mut self) {
        for i in 0..self.units.len() {
            match self.units[i].action {
                Action::Attacking { target: Target::Building(b), to } if !self.units[i].kind.stats().throws && arrived(&self.units[i], to) => {
                    let at_door = self.building(b).is_some_and(|b| b.door() == to);
                    let left = if at_door { HACK_AT_DOOR } else { self.hack_round() };
                    self.units[i].action = Action::Hacking { building: b, left: Countdown::new(left), at_door };
                }
                Action::BreakingIn { building, to } if arrived(&self.units[i], to) => {
                    let left = Countdown::new(self.hack_round());
                    self.units[i].action = Action::Hacking { building, left, at_door: false };
                }
                Action::Hacking { building, left, at_door } => self.hack(i, building, left, at_door),
                _ => {}
            }
        }
    }

    /// A spot `WANDER` from the middle of `building` in a drawn direction, not in another building's walls; a few
    /// draws, then its middle.
    fn wander_spot(&mut self, building: BuildingId, centre: (u16, u16)) -> (u16, u16) {
        let size = self.terrain.size() as i32;
        (0..WANDER_TRIES)
            .find_map(|_| {
                let (x, z) = angle::rotate((WANDER, 0), self.rng.next_u32() % angle::TURN);
                let to = (centre.0.wrapping_add(x as u16), centre.1.wrapping_add(z as u16));
                let cell = ((to.0 as u32 / 512) as i32 % size, (to.1 as u32 / 512) as i32 % size);
                self.walls.at(cell).is_none_or(|b| b == building).then_some(to)
            })
            .unwrap_or(centre)
    }

    /// Whether anyone is inside `building`.
    fn occupied(&self, building: BuildingId) -> bool {
        self.units.iter().any(|u| u.is_alive() && u.inside.is_some_and(|inside| inside.building == building))
    }

    fn hack_round(&mut self) -> Ticks {
        Ticks::new(HACK_TICKS.0 + self.rng.next_u32() % HACK_TICKS.1)
    }

    /// One turn of unit `i` hacking at `building`.
    fn hack(&mut self, i: usize, building: BuildingId, mut left: Countdown, at_door: bool) {
        let attacker = &self.units[i];
        let Some(b) = self.building_index(building).filter(|&b| crate::attack::may_attack_building(attacker, &self.buildings[b])) else {
            self.units[i].action = Action::Idle;
            return;
        };
        let amount = if at_door { 0 } else { attacker.kind.stats().sabotage };
        self.buildings[b].wear_down(amount);
        let centre = self.buildings[b].centre();
        self.units[i].face(centre);
        if !left.tick() {
            self.units[i].action = Action::Hacking { building, left, at_door };
            return;
        }
        if at_door && self.pull_out(i, building) {
            return;
        }
        let door = self.buildings[b].door();
        if !at_door && self.occupied(building) {
            self.units[i].attack(Target::Building(building), door);
            return;
        }
        let to = self.wander_spot(building, centre);
        self.units[i].action = Action::BreakingIn { building, to };
    }

    /// Unit `i` at the door brings everyone inside `building` out onto its porch: the first is pulled out and
    /// challenged, the others come out at the door and attack it. The attacker goes back to the building once the
    /// fight is behind it, those who come out back in. False when nobody is inside.
    fn pull_out(&mut self, i: usize, building: BuildingId) -> bool {
        let Some(b) = self.building(building) else { return false };
        let (door, centre, site) = (b.door(), b.centre(), (b.x, b.z));
        let inside: Vec<usize> = (0..self.units.len()).filter(|&j| self.units[j].is_alive() && self.units[j].inside.is_some_and(|inside| inside.building == building)).collect();
        let Some((&j, others)) = inside.split_first() else { return false };
        let (dx, dz) = (torus_delta(centre.0, door.0), torus_delta(centre.1, door.1));
        let d = isqrt((dx * dx + dz * dz) as u32).max(1) as i32;
        let vel = Velocity::new((dx * PULL_SPEED / d) as i16, PULL_LIFT, (dz * PULL_SPEED / d) as i16);
        let ground = self.terrain.height_at(door.0 as u32, door.1 as u32, WORLD_UNITS_PER_CELL);
        let (attacker, target): (UnitId, UnitId) = (self.units[i].id, self.units[j].id);
        let base = self.units[i].kind.speed();
        let speed = base + (self.rng.next_u32() % (base as u32 / 4).max(1)) as i32;
        let id = self.new_id_of();
        self.coordinators.challenges.insert(id, Challenge { approach: Approach::PulledOut { building }, ..Challenge::new(attacker, target) });
        let pulled = &mut self.units[j];
        pulled.inside = None;
        (pulled.x, pulled.z) = door;
        pulled.join(Action::Challenge { challenge: id, role: Role::Shoved });
        pulled.motion = Motion::flying(vel, ground);
        pulled.return_after_fight(site);
        let a = &mut self.units[i];
        a.join(Action::Challenge { challenge: id, role: Role::Charging { to: door, speed } });
        a.resume_after_fight(Target::Building(building));
        for &k in others {
            self.come_out_to_fight(k, door, site, attacker);
        }
        true
    }

    /// Unit `k` comes out of its building (corner `site`) at its `door` to attack `attacker`, then goes back in.
    fn come_out_to_fight(&mut self, k: usize, door: (u16, u16), site: (u16, u16), attacker: UnitId) {
        let u = &mut self.units[k];
        u.inside = None;
        (u.x, u.z) = door;
        self.start_order(k, Order::Attack { target: Target::Unit(attacker) });
        if matches!(self.units[k].action, Action::Attacking { .. }) {
            self.units[k].return_after_fight(site);
        } else {
            self.start_order(k, Order::Enter { site });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tribes::TribeId;
    use crate::building::Stage;
    use crate::command::Command;
    use crate::time::Tick;
    use crate::unit::UnitKind;

    /// A hut of tribe 1 and a warrior of tribe 0 ordered to attack it.
    fn siege(kind: UnitKind) -> (GameMap, BuildingId, UnitId) {
        let mut map = GameMap::sandbox_units().seeded(9);
        // Only the attacker matters here; `sandbox_units`'s own braves sit close enough to the hut below
        // (cell-wise) that a long-enough run now auto-houses an idle Red one into it
        // (`GameMap::tend_auto_housing`), which crosses the attacker's path and derails the siege - not a
        // bug in that feature, just two unrelated fixtures placed too close together.
        map.units.clear();
        let hut = map.add_building(Building::new(BuildingKind::Hut { size: 1 }, TribeId::Red, 70 * 512, 70 * 512, 0));
        let door = map.building(hut).unwrap().door();
        let a = map.spawn(TribeId::Blue, kind, (door.0.wrapping_add(1024), door.1)).id;
        map.apply(&Command::OrderUnit { player: TribeId::Blue, unit: a, order: Order::Attack { target: Target::Building(hut) } });
        (map, hut, a)
    }

    fn until(map: &mut GameMap, turns: u32, done: impl Fn(&GameMap) -> bool) -> Option<u32> {
        (1..=turns).find(|_| {
            map.tick();
            done(map)
        })
    }

    #[test]
    fn wear_fades_and_costs_a_piece_at_its_limit() {
        let mut hut = Building::new(BuildingKind::Hut { size: 1 }, TribeId::Red, 0, 0, 0);
        let used = hut.used;
        hut.wear_down(10);
        assert_eq!((hut.tend_wear(Tick::from_wire(4)), hut.wear, hut.shaking), (Wear::Nothing, 9, SHAKE_TICKS - 1));
        assert_eq!(hut.tend_wear(Tick::from_wire(5)), Wear::Nothing);
        assert_eq!(hut.wear, 9, "only every 4 turns");
        hut.wear_down(2000);
        assert_eq!(hut.tend_wear(Tick::from_wire(5)), Wear::Hit { destroyed: false });
        assert_eq!((hut.used, hut.wear), (used - 1, 9));
        assert!(hut.repair_delay.is_some());
        let mut vault = Building::new(BuildingKind::Vault, TribeId::Red, 0, 0, 0);
        vault.wear_down(u16::MAX);
        assert_eq!(vault.tend_wear(Tick::from_wire(5)), Wear::Nothing, "never worn down");
    }

    #[test]
    fn an_empty_hut_is_hacked_down_to_nothing() {
        let (mut map, hut, a) = siege(UnitKind::Warrior);
        until(&mut map, 300, |m| matches!(m.unit(a).unwrap().action, Action::Hacking { at_door: false, .. })).expect("hacking");
        let took = until(&mut map, 2000, |m| m.building(hut).is_some_and(|b| b.used < b.kind.wood_cost())).expect("a piece lost");
        assert!(took > 2000 / 12, "about 2000 wear at 12 a turn, walks between rounds");
        let b = map.building(hut).unwrap();
        assert!(matches!(b.stage(), Stage::UnderConstruction { .. }) && b.repair_delay.is_some());
        let cell = map.building(hut).unwrap().walled_cells(128)[0];
        until(&mut map, 6000, |m| m.building(hut).is_none()).expect("destroyed");
        assert!(!map.ashes.buildable(cell), "left in ashes");
        assert_eq!(map.unit(a).unwrap().action, Action::Idle, "nothing left to attack");
    }

    #[test]
    fn hacking_stops_the_instant_the_match_has_ended() {
        let (mut map, hut, a) = siege(UnitKind::Warrior);
        until(&mut map, 300, |m| matches!(m.unit(a).unwrap().action, Action::Hacking { .. })).expect("hacking");
        map.declare_outcome(Some(TribeId::Blue), crate::outcome::Reason::Eliminated);
        assert_eq!(map.unit(a).unwrap().action, Action::Idle, "stood down the instant the match ended");
        let wear = map.building(hut).unwrap().wear;
        map.run(crate::time::Ticks::new(500));
        assert_eq!(map.building(hut).unwrap().wear, wear, "no more wear once the match has ended");
        assert_eq!(map.unit(a).unwrap().action, Action::Idle, "never resumes hacking");
    }

    /// `n` people of `kind` of tribe 1 inside the hut.
    fn garrison(map: &mut GameMap, hut: BuildingId, kind: UnitKind, n: usize) -> Vec<UnitId> {
        let door = map.building(hut).unwrap().door();
        (0..n)
            .map(|_| {
                let id = map.spawn(TribeId::Red, kind, door).id;
                map.unit_mut(id).unwrap().enter(crate::unit::Inside { building: hut, door }, door);
                id
            })
            .collect()
    }

    #[test]
    fn everyone_inside_comes_out_onto_the_porch_to_fight() {
        let (mut map, hut, a) = siege(UnitKind::Brave);
        let inside = garrison(&mut map, hut, UnitKind::Brave, 2);
        until(&mut map, 300, |m| inside.iter().any(|&id| m.unit(id).unwrap().inside.is_none())).expect("brought out");
        assert!(inside.iter().all(|&id| map.unit(id).unwrap().inside.is_none()), "nobody left inside");
        let door = map.building(hut).unwrap().door();
        for id in std::iter::once(a).chain(inside) {
            let u = map.unit(id).unwrap();
            assert!(u.action.link().is_some() || u.action.name() == "Attacking", "{:?} fights", u.action);
            assert!(torus_delta(u.x, door.0).abs() < 512 && torus_delta(u.z, door.1).abs() < 512, "on the porch");
        }
    }

    #[test]
    fn defenders_who_win_go_back_in() {
        let (mut map, hut, a) = siege(UnitKind::Brave);
        let inside = garrison(&mut map, hut, UnitKind::Warrior, 2);
        until(&mut map, 600, |m| m.unit(a).is_none_or(|u| !u.is_alive())).expect("the attacker loses");
        until(&mut map, 300, |m| inside.iter().all(|&id| m.unit(id).unwrap().inside.is_some_and(|i| i.building == hut))).expect("both back in");
    }

    #[test]
    fn the_winner_walks_in_and_hacks_from_inside() {
        let (mut map, hut, a) = siege(UnitKind::Warrior);
        garrison(&mut map, hut, UnitKind::Brave, 1);
        until(&mut map, 1000, |m| matches!(m.unit(a).unwrap().action, Action::Hacking { at_door: false, .. })).expect("hacking inside");
        let u = map.unit(a).unwrap();
        assert!(map.behind_walls((u.x, u.z)), "inside the walls");
        assert!(u.inside.is_none(), "not one of its people");
        let worn = until(&mut map, 300, |m| m.building(hut).unwrap().wear > 0);
        assert!(worn.is_some(), "it wears the hut down from there");
    }

    #[test]
    fn no_repairs_while_the_delay_runs() {
        let mut hut = Building::new(BuildingKind::Hut { size: 1 }, TribeId::Red, 0, 0, 0);
        hut.wear_down(2000);
        hut.tend_wear(Tick::from_wire(1));
        let delay = hut.repair_delay.unwrap().left().get();
        assert_eq!(delay, REPAIR_DELAY.get());
        for t in 0..delay {
            hut.tend_wear(Tick::from_wire(2 + t));
        }
        assert!(hut.repair_delay.is_none());
    }
}


