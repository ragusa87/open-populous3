//! A challenge (docs/specs/combat.md "Challenge"): one attacker and one target before a melee. The attacker charges,
//! shoves the target away without hurting it, charges again, and then they fight. A coordinator
//! (docs/specs/coordinators.md): both link to it, it ticks before them.

use crate::angle;
use crate::combat::{move_turns, Move};
use crate::coordinator::{Coordinator, Link, Step};
use crate::event::{PushKind, WorldEvent};
use crate::id::Id;
use crate::map::{GameMap, Lcg};
use crate::motion::Velocity;
use crate::time::{Countdown, Tick, Ticks};
use crate::unit::{isqrt, torus_delta, Action, Unit, UnitId};

pub type ChallengeId = Id<Challenge>;

/// An attacker this close to its target challenges it ("Starting a melee" [exe]).
pub const FIGHT_REACH: i32 = 224;
/// A charger this close to its target on both axes has reached it ("Pre-fight" [exe]).
pub const SHOVE_REACH: i32 = 315;
/// How long a charge may last before the challenge is given up ("Pre-fight" [exe]).
pub const CHARGE_TICKS: Ticks = Ticks::new(40);
/// The target is thrown when the shove has this many turns left ("Pre-fight" [exe, measured]).
pub const SHOVE_THROW_LEFT: u32 = 2;
/// The throw's speed per turn: the first plus a draw below the second ("Pre-fight" [exe]).
pub const SHOVE_SPEED: (i32, i32) = (100, 100);
/// The throw's heading, turned from the way to the attacker: the first plus a draw below the second, of
/// `angle::TURN` (a third to two thirds of a turn: away from the attacker) ("Pre-fight" [exe]).
pub const SHOVE_TURN: (u32, u32) = (683, 682);
/// The throw's lift, terrain height units per turn: ours has no sliding on the ground, so a short hop carries the
/// target about as far as measured (0.7 to 2.5 cells) (guess).
pub const SHOVE_LIFT: i16 = 48;

/// How far the challenge is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Charge { left: Countdown },
    Shove,
    Recharge { left: Countdown },
}

/// A unit's part in a challenge (`Action::Challenge`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The target, standing.
    Waiting,
    /// The attacker running at `to` at `speed` per turn on flat ground.
    Charging { to: (u16, u16), speed: i32 },
    /// The attacker shoving, `left` turns to go; then it charges again at `speed`.
    Shoving { left: Countdown, speed: i32 },
    /// The target, thrown: waiting again once still.
    Shoved,
}

/// How a challenge began.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Approach {
    /// The attacker reached its target in the open.
    Open,
    /// The target was pulled out of its `building` by its attacker (`crate::wreck`).
    PulledOut { building: crate::building::BuildingId },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Challenge {
    pub attacker: UnitId,
    pub target: UnitId,
    pub approach: Approach,
    pub phase: Phase,
    /// The last turn the waiting target taunted ("Pre-fight" [exe]): no art or pose for it yet (TODO.md "Units"),
    /// kept for when there is.
    pub taunted: Option<Tick>,
}

/// The target taunts on this chance per turn while waiting ("Pre-fight" [exe], confirmed).
pub const TAUNT_CHANCE: u32 = 8;

/// Whether the draw `r` taunts: 1 chance in `TAUNT_CHANCE`.
pub fn taunts(r: u32) -> bool {
    r.is_multiple_of(TAUNT_CHANCE)
}

impl Challenge {
    pub fn new(attacker: UnitId, target: UnitId) -> Self {
        Challenge { attacker, target, approach: Approach::Open, phase: Phase::Charge { left: Countdown::new(CHARGE_TICKS) }, taunted: None }
    }

    fn end(&self, id: ChallengeId) -> Step {
        let from = Link::Challenge(id);
        Step { finished: true, events: vec![WorldEvent::Release { unit: self.attacker, from }, WorldEvent::Release { unit: self.target, from }] }
    }
}

/// The role `u` plays in a challenge, if it is in one.
pub fn role_of(u: &Unit) -> Option<Role> {
    match u.action {
        Action::Challenge { role, .. } => Some(role),
        _ => None,
    }
}

/// Whether the charger at `a` has reached the target at `t`.
pub fn reached(a: (u16, u16), t: (u16, u16)) -> bool {
    torus_delta(a.0, t.0).abs() < SHOVE_REACH && torus_delta(a.1, t.1).abs() < SHOVE_REACH
}

/// Whether an attacker at `a` is close enough to the target at `t` to challenge it.
pub fn in_reach(a: (u16, u16), t: (u16, u16)) -> bool {
    let (dx, dz) = (torus_delta(a.0, t.0), torus_delta(a.1, t.1));
    dx * dx + dz * dz < FIGHT_REACH * FIGHT_REACH
}

/// The throw of a shove: `speed` per turn from `target`, its heading turned by `turn` from the way to `attacker`.
pub fn shove_throw(attacker: (u16, u16), target: (u16, u16), speed: i32, turn: u32) -> Velocity {
    let (dx, dz) = (torus_delta(target.0, attacker.0), torus_delta(target.1, attacker.1));
    let d = isqrt((dx * dx + dz * dz) as u32).max(1) as i32;
    let (x, z) = angle::rotate((dx * speed / d, dz * speed / d), turn);
    Velocity::new(x as i16, SHOVE_LIFT, z as i16)
}

impl Coordinator for Challenge {
    fn link_of(u: &Unit) -> Option<Id<Self>> {
        match u.action.link()? {
            Link::Challenge(id) => Some(id),
            _ => None,
        }
    }

    fn tick(&mut self, id: Id<Self>, members: &[UnitId], now: Tick, world: &GameMap, rng: &mut Lcg) -> Step {
        let (Some(a), Some(t)) = (world.unit(self.attacker), world.unit(self.target)) else { return self.end(id) };
        let linked = members.contains(&a.id) && members.contains(&t.id);
        if !linked || !a.is_alive() || !t.is_alive() || a.owner == t.owner {
            return self.end(id);
        }
        let (Some(charger), Some(waiting)) = (role_of(a), role_of(t)) else { return self.end(id) };
        let mut events = Vec::new();
        let target_at = (t.x, t.z);
        if waiting == Role::Waiting {
            events.push(WorldEvent::Face { unit: t.id, at: (a.x, a.z) });
            if taunts(rng.next_u32()) {
                self.taunted = Some(now);
            }
        }
        match self.phase {
            Phase::Charge { ref mut left } | Phase::Recharge { ref mut left } => {
                if let Role::Charging { to, .. } = charger
                    && to != target_at
                {
                    events.push(WorldEvent::Placed { unit: a.id, at: target_at });
                }
                if reached((a.x, a.z), target_at) && waiting == Role::Waiting {
                    if matches!(self.phase, Phase::Recharge { .. }) {
                        return Step { finished: true, events: vec![WorldEvent::StartMelee { attacker: a.id, target: t.id }] };
                    }
                    events.push(WorldEvent::Shove { attacker: a.id });
                    self.phase = Phase::Shove;
                } else if left.tick() {
                    return self.end(id);
                }
            }
            Phase::Shove => match charger {
                Role::Shoving { left, .. } if left.left().get() == SHOVE_THROW_LEFT => {
                    let speed = SHOVE_SPEED.0 + (rng.next_u32() % SHOVE_SPEED.1 as u32) as i32;
                    let turn = SHOVE_TURN.0 + rng.next_u32() % SHOVE_TURN.1;
                    let vel = shove_throw((a.x, a.z), target_at, speed, turn);
                    events.push(WorldEvent::Push { unit: t.id, vel, kind: PushKind::Melee });
                }
                Role::Charging { .. } => self.phase = Phase::Recharge { left: Countdown::new(CHARGE_TICKS) },
                _ => {}
            },
        }
        Step { finished: false, events }
    }
}

impl GameMap {
    /// Attackers within reach of a free target challenge it (`Challenge`); the charge's speed is drawn here. A
    /// target already in a fight is left alone for now, a shaman in her cast jump until it ends - a listener
    /// (sermon.rs: locked but linked to no coordinator) is free, so attacking one still takes it out of its sermon.
    pub(crate) fn start_challenges(&mut self) {
        for i in 0..self.units.len() {
            let a = &self.units[i];
            let Action::Attacking { target: crate::unit::Target::Unit(target), .. } = a.action else { continue };
            if a.kind.stats().throws {
                continue;
            }
            let Some(j) = self.unit_index(target) else { continue };
            let t = &self.units[j];
            let free = self.linked(t).is_none();
            if !free || t.thrown() || !crate::attack::may_attack(a, t) || matches!(t.action, Action::Casting { .. }) || !t.in_the_open() || !in_reach((a.x, a.z), (t.x, t.z)) {
                continue;
            }
            let base = a.kind.speed();
            let speed = base + (self.rng.next_u32() % (base as u32 / 4).max(1)) as i32;
            let to = (t.x, t.z);
            let id = self.new_id_of();
            self.coordinators.challenges.insert(id, Challenge::new(self.units[i].id, target));
            self.units[i].join(Action::Challenge { challenge: id, role: Role::Charging { to, speed } });
            self.units[j].join(Action::Challenge { challenge: id, role: Role::Waiting });
        }
    }

    /// The attacker in a challenge starts its shove.
    pub(crate) fn shove(&mut self, attacker: UnitId) {
        let Some(u) = self.unit_mut(attacker) else { return };
        if let Action::Challenge { challenge, role: Role::Charging { speed, .. } } = u.action {
            let left = Countdown::new(Ticks::new(move_turns(u.kind, Move::Shove)));
            u.action = Action::Challenge { challenge, role: Role::Shoving { left, speed } };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::effect::DamageKind;
    use crate::unit::{octant, Order, Target, UnitKind};

    /// A brave of tribe 0 and one of tribe 1 `gap` cells east of it, both near the player's shaman.
    fn pair(gap: i32) -> (GameMap, UnitId, UnitId) {
        let mut map = GameMap::sandbox_units().seeded(3);
        let s = map.shaman_of(0).unwrap();
        let at = (s.x.wrapping_add(1024), s.z.wrapping_add(2048));
        let a = map.spawn(0, UnitKind::Brave, at).id;
        let t = map.spawn(1, UnitKind::Brave, (at.0.wrapping_add((gap * 512) as u16), at.1)).id;
        map.apply(&Command::OrderUnit { player: 0, unit: a, order: Order::Attack { target: Target::Unit(t) } });
        (map, a, t)
    }

    fn role(map: &GameMap, u: UnitId) -> Option<Role> {
        map.unit(u).and_then(role_of)
    }

    fn until(map: &mut GameMap, turns: u32, done: impl Fn(&GameMap) -> bool) -> Option<u32> {
        (1..=turns).find(|_| {
            map.tick();
            done(map)
        })
    }

    #[test]
    fn an_attacker_in_reach_challenges_charges_and_shoves() {
        let (mut map, a, t) = pair(3);
        until(&mut map, 200, |m| role(m, t).is_some()).expect("challenged");
        assert!(matches!(role(&map, a), Some(Role::Charging { .. } | Role::Shoving { .. })), "already in reach: it may shove at once");
        assert_eq!(role(&map, t), Some(Role::Waiting));
        let life = map.unit(t).unwrap().health;
        until(&mut map, 60, |m| role(m, t) == Some(Role::Shoved)).expect("shoved");
        assert!(map.unit(t).unwrap().motion.airborne(), "thrown");
        assert_eq!(map.unit(t).unwrap().health, life, "a shove does not hurt");
        until(&mut map, 60, |m| role(m, t) == Some(Role::Waiting)).expect("stands again");
    }

    #[test]
    fn after_the_second_charge_they_fight() {
        let (mut map, a, t) = pair(3);
        until(&mut map, 200, |m| role(m, t) == Some(Role::Shoved)).expect("shoved");
        until(&mut map, 200, |m| m.coordinators.challenges.is_empty()).expect("over");
        assert_eq!(map.coordinators.melees.len(), 1);
        assert!([a, t].iter().all(|&u| matches!(map.unit(u).unwrap().action, Action::Melee { .. })));
    }

    #[test]
    fn the_throw_goes_away_from_the_attacker() {
        for turn in [SHOVE_TURN.0, SHOVE_TURN.0 + SHOVE_TURN.1 / 2, SHOVE_TURN.0 + SHOVE_TURN.1 - 1] {
            let v = shove_throw((1000, 1000), (1300, 1000), 150, turn);
            assert!(v.x > 0, "away on x at {turn}: {v:?}");
            assert!(v.x as i32 * v.x as i32 + v.z as i32 * v.z as i32 > 140 * 140);
        }
        assert_eq!(shove_throw((1000, 1000), (1300, 1000), 150, 1024), Velocity::new(150, SHOVE_LIFT, 0), "straight away");
    }

    #[test]
    fn a_charge_too_long_or_a_dead_attacker_ends_it() {
        let (mut map, a, t) = pair(3);
        until(&mut map, 200, |m| role(m, t).is_some()).expect("challenged");
        map.damage(a, u16::MAX, DamageKind::Lightning, None);
        map.tick();
        assert!(map.coordinators.challenges.is_empty());
        assert_eq!(map.unit(t).unwrap().action, Action::Idle);
        let (mut map, a, t) = pair(3);
        until(&mut map, 200, |m| role(m, t).is_some()).expect("challenged");
        let far = map.unit(t).map(|u| (u.x.wrapping_add(20 * 512), u.z)).unwrap();
        map.unit_mut(t).unwrap().x = far.0;
        let left = until(&mut map, 100, |m| m.coordinators.challenges.is_empty()).expect("given up");
        assert!(left <= CHARGE_TICKS.get() + 1);
        assert!(role(&map, a).is_none());
    }

    #[test]
    fn a_shaman_casting_in_a_fight_casts_once_then_fights_on() {
        let mut map = GameMap::sandbox_walk();
        let s = map.shaman_of(0).unwrap();
        let (shaman, at) = (s.id, (s.x, s.z.wrapping_add(4 * 512)));
        let w = map.spawn(1, UnitKind::Warrior, (s.x.wrapping_add(512), s.z)).id;
        map.apply(&Command::OrderUnit { player: 1, unit: w, order: Order::Attack { target: Target::Unit(shaman) } });
        until(&mut map, 200, |m| role(m, shaman).is_some()).expect("challenged");
        map.apply(&Command::Cast { player: 0, spell: crate::spell::Spell::Blast { at } });
        map.run(crate::unit::CAST_RELEASE);
        assert_eq!(map.effects.len(), 1, "her jump not cut short");
        until(&mut map, 200, |m| role(m, shaman).is_some()).expect("challenged again after it");
        let casting = |m: &GameMap| matches!(m.unit(shaman).unwrap().action, Action::Casting { .. });
        assert_eq!(until(&mut map, 200, casting), None, "not cast again");
        until(&mut map, 400, |m| matches!(m.unit(shaman).unwrap().action, Action::Melee { .. })).expect("in a melee");
        let effects = map.effects.len();
        map.apply(&Command::Cast { player: 0, spell: crate::spell::Spell::Blast { at } });
        map.run(crate::unit::CAST_RELEASE);
        assert_eq!(map.effects.len(), effects + 1, "blasts in a melee too");
    }

    #[test]
    fn the_same_seed_gives_the_same_challenge() {
        let run = || {
            let (mut map, _, _) = pair(3);
            map.run(Ticks::new(60));
            (map.units, map.coordinators, map.rng)
        };
        assert_eq!(run(), run());
    }

    /// 1 chance in `TAUNT_CHANCE`: exactly one of every `TAUNT_CHANCE` consecutive draws taunts
    /// ("Pre-fight" [exe], confirmed).
    #[test]
    fn taunts_one_draw_in_eight() {
        assert_eq!((0..TAUNT_CHANCE * 4).filter(|&r| taunts(r)).count(), 4);
        assert!(taunts(0));
        assert!(!taunts(1));
    }

    #[test]
    fn a_waiting_target_faces_its_attacker() {
        let (mut map, a, t) = pair(3);
        until(&mut map, 200, |m| role(m, t) == Some(Role::Waiting)).expect("waiting");
        map.tick();
        if role(&map, t) != Some(Role::Waiting) {
            return; // shoved at once: already in reach, nothing left to check this turn
        }
        let (tu, au) = (map.unit(t).unwrap(), map.unit(a).unwrap());
        assert_eq!(tu.facing, octant(torus_delta(tu.x, au.x), torus_delta(tu.z, au.z)));
    }
}
