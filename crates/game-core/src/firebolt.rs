//! Firebolts (docs/specs/combat.md "Firebolts"): a firewarrior's attack. In reach of its target it throws two homing
//! bolts, one from each hand, of which only the right one hits; then it reloads. A bolt hurts every enemy in the cell
//! it lands in, the target more, throws them, sets trees and the targeted building on fire.

use crate::cast_range::{ALT_BAND_PERCENT, BAND_HEIGHT};
use crate::effect::DamageKind;
use crate::event::{PushKind, WorldEvent};
use crate::id::Id;
use crate::map::GameMap;
use crate::motion::Velocity;
use crate::time::{Countdown, Tick, Ticks};
use crate::unit::{isqrt, torus_delta, Action, Target, Unit, UnitId, UnitKind};
use pop3_format::WORLD_UNITS_PER_CELL;

pub type FireboltId = Id<Firebolt>;

/// A firewarrior's reach in cells at the lowest altitude band's 100 %, before the band's percent ("Firewarrior
/// fire" [exe]).
pub const REACH_CELLS: u32 = 11;
/// Cells a drum tower adds to the reach ("Firewarrior fire" [exe]).
pub const TOWER_CELLS: u32 = 4;
/// A bolt's speed, world units per turn, in 3D ("Firewarrior fire" [exe]).
pub const BOLT_SPEED: i32 = 512;
/// The bolts start this far to each side of the thrower, this high over its feet ("Firewarrior fire" [exe]).
pub const BOLT_SIDE: i32 = 96;
pub const BOLT_RISE: i32 = 16;
/// The throw's length ("Firewarrior fire" [exe]).
pub const THROW_TICKS: Ticks = Ticks::new(5);
/// Turns between throws, from a drum tower, and the shift a bloodlusted thrower's reload takes (constant.dat values).
pub const RELOAD: u8 = 25;
pub const RELOAD_IN_TOWER: u8 = 36;
pub const BLOODLUST_RELOAD_SHIFT: u8 = 3;
/// Every enemy in the bolt's cell loses this; the target more, its kind's share of the second (from a drum tower, the
/// third) ("Firewarrior fire" [exe], constant.dat values).
pub const CELL_DAMAGE: u16 = 100;
pub const TARGET_DAMAGE: u16 = 100;
pub const TARGET_DAMAGE_FROM_TOWER: u16 = 400;
/// Bloodlust multiplies a bolt's damage (constant.dat value).
pub const BLOODLUST_DAMAGE: u16 = 3;
/// Those hit are thrown away from the thrower and up; a steady kind only hops ("Firewarrior fire" [exe]).
pub const THROW_OUT: i32 = 64;
pub const THROW_UP: i16 = 110;
pub const HOP: i16 = 36;

/// One firebolt in flight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Firebolt {
    pub owner: u8,
    pub thrower: UnitId,
    pub target: Target,
    /// Where it is: world units and height.
    pub at: (u16, u16),
    pub height: i32,
    /// Where it last aimed (kept when the target is gone).
    pub aim: (u16, u16, i32),
    /// Only one of a throw's two bolts hits.
    pub hits: bool,
    pub bloodlusted: bool,
    pub from_tower: bool,
}

/// How far a firewarrior at `height` reaches on each axis, world units: its cells by the altitude band's percent,
/// a drum tower's more, made odd ("Firewarrior fire" [exe]).
pub fn bolt_reach(height: u16, in_tower: bool) -> i32 {
    let band = (height as u32 / BAND_HEIGHT).min(7) as usize;
    let cells = (REACH_CELLS * ALT_BAND_PERCENT[band] / 100 + if in_tower { TOWER_CELLS } else { 0 }) | 1;
    (cells * WORLD_UNITS_PER_CELL / 2 + 56) as i32
}

/// The reach in whole cells across, as the spotting square counts them.
pub fn reach_cells(height: u16, in_tower: bool) -> u8 {
    ((bolt_reach(height, in_tower) - 56) * 2 / WORLD_UNITS_PER_CELL as i32) as u8
}

/// Whether `to` is within `reach` of `from` on both axes.
pub fn in_reach(from: (u16, u16), to: (u16, u16), reach: i32) -> bool {
    torus_delta(from.0, to.0).abs() <= reach && torus_delta(from.1, to.1).abs() <= reach
}

/// A thrower's turns to reload: shorter bloodlusted, longer from a tower.
pub fn reload(bloodlusted: bool, in_tower: bool) -> u8 {
    let base = if in_tower { RELOAD_IN_TOWER } else { RELOAD };
    if bloodlusted { base >> BLOODLUST_RELOAD_SHIFT } else { base }
}

/// One turn of a reload at `now`: down by one, its last step only on a turn of four ("Firewarrior fire" [exe]).
pub fn reload_step(left: u8, now: Tick) -> u8 {
    match left {
        0 => 0,
        1 if now.to_wire() & 3 != 0 => 1,
        n => n - 1,
    }
}

/// A target's share of `TARGET_DAMAGE`, out of 256 (constant.dat values; the warrior's 5 % kept as 12/256).
pub fn bolt_share(kind: UnitKind) -> u16 {
    match kind {
        UnitKind::Warrior => 12,
        _ => 256,
    }
}

/// Kinds that are only nudged, alive, by a bolt (warrior and preacher, "Firewarrior fire" [exe]).
pub fn steady(kind: UnitKind) -> bool {
    matches!(kind, UnitKind::Warrior | UnitKind::Preacher)
}

/// Whether a firewarrior hit by a bolt fires back: unless it is already attacking, in a fight or past orders
/// (whatever else it does, walking or working, it drops).
pub fn fires_back(u: &Unit) -> bool {
    u.kind == UnitKind::Firewarrior
        && u.is_alive()
        && u.action.link().is_none()
        && u.action.can_take_orders()
        && !matches!(u.action, Action::Attacking { .. } | Action::Throwing { .. })
}

/// The step a bolt at `at` (height `h`) makes towards `aim`: its next place, and whether it got there.
pub fn fly(at: (u16, u16), h: i32, aim: (u16, u16, i32)) -> ((u16, u16), i32, bool) {
    let (dx, dz, dy) = (torus_delta(at.0, aim.0), torus_delta(at.1, aim.1), aim.2 - h);
    let d2 = (dx as i64).pow(2) + (dz as i64).pow(2) + (dy as i64).pow(2);
    if d2 <= (BOLT_SPEED as i64).pow(2) {
        return ((aim.0, aim.1), aim.2, true);
    }
    let d = isqrt(d2.min(u32::MAX as i64) as u32).max(1) as i32;
    let step = |v: i32| v * BOLT_SPEED / d;
    ((at.0.wrapping_add(step(dx) as u16), at.1.wrapping_add(step(dz) as u16)), h + step(dy), false)
}

impl GameMap {
    /// Where a bolt aimed at `target` goes: the person's middle, a building's door on the ground.
    fn bolt_aim(&self, target: Target) -> Option<(u16, u16, i32)> {
        let ground = |p: (u16, u16)| self.terrain.height_at(p.0 as u32, p.1 as u32, WORLD_UNITS_PER_CELL);
        match target {
            Target::Unit(id) => self.unit(id).map(|u| (u.x, u.z, u.motion.height.unwrap_or_else(|| ground((u.x, u.z))) + BOLT_RISE)),
            Target::Building(id) => self.building(id).map(|b| {
                let door = b.door();
                (door.0, door.1, ground(door))
            }),
        }
    }

    /// Unit `i`, a firewarrior in reach of `target`, throws: two bolts, the right one hitting; then it reloads.
    pub(crate) fn throw_fire(&mut self, i: usize, target: Target) {
        let Some(aim) = self.bolt_aim(target) else { return };
        let u = &self.units[i];
        let (dx, dz) = (torus_delta(u.x, aim.0), torus_delta(u.z, aim.1));
        let d = isqrt((dx * dx + dz * dz) as u32).max(1) as i32;
        let side = (-dz * BOLT_SIDE / d, dx * BOLT_SIDE / d);
        let height = self.altitude(u) as i32 + BOLT_RISE;
        let (bloodlusted, from_tower) = (u.statuses.bloodlust.is_on(), self.in_tower(u));
        let (owner, thrower, at) = (u.owner, u.id, (u.x, u.z));
        for (sign, hits) in [(1, false), (-1, true)] {
            let start = (at.0.wrapping_add((sign * side.0) as u16), at.1.wrapping_add((sign * side.1) as u16));
            let id = self.new_id_of();
            self.bolts.insert(id, Firebolt { owner, thrower, target, at: start, height, aim, hits, bloodlusted, from_tower });
        }
        let u = &mut self.units[i];
        u.face((aim.0, aim.1));
        u.reload = reload(bloodlusted, from_tower);
        u.action = Action::Throwing { target, left: Countdown::new(THROW_TICKS) };
    }

    /// Whether `u` is in a finished drum tower: it throws further, harder, slower ("Firewarrior fire" [exe]).
    pub fn in_tower(&self, u: &Unit) -> bool {
        u.inside.and_then(|i| self.building(i.building)).is_some_and(|b| b.kind == crate::building::BuildingKind::DrumTower && b.stage() == crate::building::Stage::Built)
    }

    /// How far `u` throws on each axis from where it is: its altitude (aboard: its vehicle's), a drum tower's more.
    pub fn bolt_reach_of(&self, u: &Unit) -> i32 {
        bolt_reach(self.altitude(u), self.in_tower(u))
    }

    /// Every bolt flies a turn towards its target; those arriving hit and are gone. The events of the hits.
    pub(crate) fn tend_bolts(&mut self) -> Vec<WorldEvent> {
        let mut bolts = std::mem::take(&mut self.bolts);
        let mut arrived = Vec::new();
        for (id, b) in bolts.iter_mut() {
            if let Some(aim) = self.bolt_aim(b.target) {
                b.aim = aim;
            }
            let (at, height, there) = fly(b.at, b.height, b.aim);
            (b.at, b.height) = (at, height);
            if there {
                arrived.push(id);
            }
        }
        let hits: Vec<Firebolt> = bolts.iter().filter(|(id, b)| arrived.contains(id) && b.hits).map(|(_, b)| b.clone()).collect();
        bolts.retain(|id, _| !arrived.contains(&id));
        self.bolts = bolts;
        hits.iter().flat_map(|b| self.bolt_hit(b)).collect()
    }

    /// What a hitting bolt does where it lands.
    fn bolt_hit(&mut self, b: &Firebolt) -> Vec<WorldEvent> {
        let cell = ((b.at.0 as u32 / WORLD_UNITS_PER_CELL) as i32, (b.at.1 as u32 / WORLD_UNITS_PER_CELL) as i32);
        let thrower = self.unit(b.thrower).map_or(b.at, |t| (t.x, t.z));
        let boost = if b.bloodlusted { BLOODLUST_DAMAGE } else { 1 };
        let mut events = vec![WorldEvent::BurnTrees { cell }];
        let vehicles = self.vehicles.iter().filter(|(_, v)| v.owner != b.owner && v.cell() == cell);
        events.extend(vehicles.map(|(vehicle, _)| WorldEvent::DamageVehicle { vehicle, amount: CELL_DAMAGE, from: b.owner }));
        let hit: Vec<&Unit> = self.units.iter().filter(|u| u.is_alive() && u.in_the_open() && u.owner != b.owner && u.cell() == cell).collect();
        for u in hit {
            let mut amount = CELL_DAMAGE.saturating_mul(boost);
            if b.target == Target::Unit(u.id) {
                let full = if b.from_tower { TARGET_DAMAGE_FROM_TOWER } else { TARGET_DAMAGE };
                amount = amount.saturating_add((full as u32 * bolt_share(u.kind) as u32 / 256) as u16);
            }
            events.push(WorldEvent::Damage { unit: u.id, amount, kind: DamageKind::Fire, from: Some(b.owner) });
            let vel = if steady(u.kind) && amount < u.health.current() {
                Velocity::new(0, HOP, 0)
            } else {
                let (dx, dz) = (torus_delta(thrower.0, u.x), torus_delta(thrower.1, u.z));
                let d = isqrt((dx * dx + dz * dz) as u32).max(1) as i32;
                Velocity::new((dx * THROW_OUT / d) as i16, THROW_UP, (dz * THROW_OUT / d) as i16)
            };
            events.push(WorldEvent::Push { unit: u.id, vel, kind: PushKind::Melee });
        }
        match b.target {
            Target::Building(id) => {
                if let Some(building) = self.building_index(id) {
                    self.buildings[building].wear_down(if b.from_tower { TARGET_DAMAGE_FROM_TOWER } else { TARGET_DAMAGE });
                    events.push(WorldEvent::SetFire { building: id });
                }
            }
            Target::Unit(id) => {
                if self.unit(id).is_some_and(fires_back) {
                    events.push(WorldEvent::StrikeBack { unit: id, at: b.thrower });
                }
            }
        }
        events
    }

    /// Every thrower's reload goes on.
    pub(crate) fn tend_reloads(&mut self) {
        let now = self.now;
        self.units.iter_mut().filter(|u| u.reload > 0).for_each(|u| u.reload = reload_step(u.reload, now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::unit::Order;

    #[test]
    fn reach_by_altitude_band() {
        let cells: Vec<u8> = (0..8).map(|b| reach_cells(b * 128, false)).collect();
        assert_eq!(cells, [9, 9, 11, 13, 13, 15, 15, 17]);
        assert_eq!((bolt_reach(0, false), bolt_reach(7 * 128, false)), (2360, 4408), "cells × 256 + 56");
        assert_eq!(reach_cells(0, true), 13, "a drum tower adds 4 before the odd step");
    }

    #[test]
    fn a_reload_takes_25_to_28_turns() {
        let length = |start: u32| {
            let mut left = RELOAD;
            (start..).take_while(|&t| {
                left = reload_step(left, Tick::from_wire(t));
                left > 0
            }).count() + 1
        };
        let lengths: Vec<usize> = (0..4).map(length).collect();
        assert!(lengths.iter().all(|&n| (25..=28).contains(&n)), "{lengths:?}");
        assert_eq!(reload(true, false), 3, "bloodlusted");
    }

    #[test]
    fn a_bolt_homes_in_one_cell_a_turn() {
        let (at, h, there) = fly((0, 0), 0, (2048, 0, 0));
        assert_eq!((at, h, there), ((512, 0), 0, false));
        assert!(fly((65500, 0), 0, (100, 0, 30)).2, "across the edge, within a turn");
    }

    /// A firewarrior of tribe 0 and a `kind` of tribe 1 `gap` cells east, away from the sandbox's people.
    fn duel(kind: UnitKind, gap: i32) -> (GameMap, UnitId, UnitId) {
        let mut map = GameMap::sandbox_units().seeded(4);
        let s = map.shaman_of(0).unwrap();
        let at = (s.x.wrapping_add(1024), s.z.wrapping_sub(15 * 512));
        let a = map.spawn(0, UnitKind::Firewarrior, at).id;
        let t = map.spawn(1, kind, (at.0.wrapping_add((gap * 512) as u16), at.1)).id;
        map.apply(&Command::OrderUnit { player: 0, unit: a, order: Order::Attack { target: Target::Unit(t) } });
        (map, a, t)
    }

    fn life(map: &GameMap, u: UnitId) -> u16 {
        map.unit(u).map_or(0, |u| u.health.current())
    }

    #[test]
    fn a_brave_loses_200_a_warrior_104_and_the_target_is_thrown() {
        for (kind, lost) in [(UnitKind::Brave, 200), (UnitKind::Warrior, 104)] {
            let (mut map, a, t) = duel(kind, 3);
            let full = life(&map, t);
            let hit = (1..60).find(|_| {
                map.tick();
                life(&map, t) < full
            });
            assert!(hit.is_some(), "{kind:?} hit");
            assert_eq!(full - life(&map, t), lost, "{kind:?}");
            assert!(map.unit(a).unwrap().reload > 0);
        }
    }

    #[test]
    fn from_a_drum_tower_it_throws_further_harder_and_slower() {
        let (mut map, a, t) = duel(UnitKind::Brave, 6);
        let at = map.unit(a).map(|u| (u.x, u.z)).unwrap();
        let tower = map.add_building(crate::building::Building::new(crate::building::BuildingKind::DrumTower, 0, at.0.wrapping_sub(256), at.1.wrapping_sub(256), 0));
        let door = map.building(tower).unwrap().door();
        map.unit_mut(a).unwrap().inside = Some(crate::unit::Inside { building: tower, door });
        let u = map.unit(a).unwrap().clone();
        assert!(map.in_tower(&u));
        assert_eq!(map.bolt_reach_of(&u), bolt_reach(map.altitude(&u), false) + 4 * 256, "4 cells more across");
        map.apply(&Command::OrderUnit { player: 0, unit: a, order: Order::Attack { target: Target::Unit(t) } });
        let full = life(&map, t);
        (1..60).find(|_| {
            map.tick();
            life(&map, t) < full
        })
        .expect("hit from the tower, 6 cells away");
        assert_eq!(full - life(&map, t), CELL_DAMAGE + TARGET_DAMAGE_FROM_TOWER);
        assert!(map.unit(a).unwrap().inside.is_some(), "it never left the tower");
        assert!(map.unit(a).unwrap().reload > RELOAD, "a tower's slower reload");
    }

    #[test]
    fn from_a_boat_it_throws_at_the_shore() {
        let mut map = GameMap::sandbox_vehicles();
        let (boat, at) = map.vehicles.iter().find(|(_, v)| v.kind == crate::vehicle::VehicleKind::Boat && !v.passengers.is_empty()).map(|(id, v)| (id, (v.x, v.z))).unwrap();
        let crew = map.vehicle(boat).unwrap().passengers[0];
        map.unit_mut(crew).unwrap().kind = UnitKind::Firewarrior;
        let foe = map.spawn(1, UnitKind::Brave, (at.0, at.1.wrapping_sub(2 * 512))).id;
        map.apply(&Command::OrderUnit { player: 0, unit: crew, order: Order::Attack { target: Target::Unit(foe) } });
        let full = life(&map, foe);
        (1..60).find(|_| {
            map.tick();
            life(&map, foe) < full
        })
        .expect("hit from the boat");
        assert_eq!(map.unit(crew).unwrap().aboard, Some(boat), "still aboard");
    }

    #[test]
    fn two_firewarriors_on_one_brave_take_400() {
        let (mut map, a, t) = duel(UnitKind::Brave, 3);
        let at = map.unit(a).map(|u| (u.x, u.z.wrapping_add(200))).unwrap();
        let b = map.spawn(0, UnitKind::Firewarrior, at).id;
        map.apply(&Command::OrderUnit { player: 0, unit: b, order: Order::Attack { target: Target::Unit(t) } });
        let full = life(&map, t);
        (0..20).for_each(|_| _ = map.tick());
        assert_eq!(full - life(&map, t), 400);
    }

    #[test]
    fn out_of_reach_it_walks_closer_and_throws_twice_a_reload_apart() {
        let (mut map, a, _) = duel(UnitKind::Spy, 12);
        let mut throws = Vec::new();
        let mut last = 0;
        for turn in 0..120 {
            map.tick();
            let reload = map.unit(a).unwrap().reload;
            if reload > last {
                throws.push(turn);
            }
            last = reload;
        }
        assert!(throws.len() >= 2, "{throws:?}");
        assert!((25..=29).contains(&(throws[1] - throws[0])), "{throws:?}");
    }

    #[test]
    fn a_firewarrior_hit_fires_back_even_walking() {
        for walking in [false, true] {
            let (mut map, a, t) = duel(UnitKind::Firewarrior, 5);
            if walking {
                let to = map.unit(t).map(|u| (u.x, u.z.wrapping_add(8 * 512))).unwrap();
                map.apply(&Command::OrderUnit { player: 1, unit: t, order: Order::MoveTo { x: to.0, z: to.1 } });
            }
            let full = life(&map, t);
            let fired_back = (0..80).any(|_| {
                map.tick();
                life(&map, t) < full && matches!(map.unit(t).map(|u| u.action), Some(Action::Attacking { target: Target::Unit(x), .. } | Action::Throwing { target: Target::Unit(x), .. }) if x == a)
            });
            assert!(fired_back, "walking: {walking}");
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_bolts() {
        let run = || {
            let (mut map, _, _) = duel(UnitKind::Warrior, 5);
            map.run(Ticks::new(80));
            (map.units, map.bolts, map.rng)
        };
        assert_eq!(run(), run());
    }
}
