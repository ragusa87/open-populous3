//! Coordinators (docs/specs/coordinators.md): short-lived objects that coordinate several units, like the original's
//! invisible "internal" things (class 10: fights, formations, guard control...). Units link to one through their
//! action, one way; the coordinator keeps no list of them. It ticks before the units, returns `WorldEvent`s, and is
//! removed when its situation ends; its units then find its id gone and are released (`GameMap::release_orphans`).
//! Kinds: `challenge::Challenge`, `melee::Melee`, `armageddon::ArmageddonMatch` (global: no unit ever links to it);
//! a test-only `Gathering` proves the pattern. A sermon is deliberately not one of these (sermon.rs): just state on
//! the preacher and the listener directly, like the original. A formation (formation.rs) is not one either: it does
//! not own what its members are doing, only nudges where they walk, so it keeps its own member list and is its own
//! typed list on `GameMap` instead.

use crate::event::WorldEvent;
use crate::id::{Id, Registry};
use crate::map::{GameMap, Lcg};
use crate::time::Tick;
use crate::unit::{Unit, UnitId};
use std::collections::BTreeMap;

/// What a coordinator's turn gives: whether its situation is over (it is removed this turn), and its events.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Step {
    pub finished: bool,
    pub events: Vec<WorldEvent>,
}

/// One kind of coordinator. It reads the map as it was before any coordinator ticked, changes only itself.
pub trait Coordinator: Sized {
    /// The coordinator of this kind `u` links to, if any.
    fn link_of(u: &Unit) -> Option<Id<Self>>;
    /// Its turn; `members`: the units linked to it, in unit order.
    fn tick(&mut self, id: Id<Self>, members: &[UnitId], now: Tick, world: &GameMap, rng: &mut Lcg) -> Step;
}

/// A unit's link to its coordinator (`Action::link`), one variant per kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    Challenge(crate::challenge::ChallengeId),
    Melee(crate::melee::MeleeId),
    #[cfg(test)]
    Gathering(Id<tests::Gathering>),
}

/// Every coordinator, one registry per kind.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Coordinators {
    pub challenges: Registry<crate::challenge::Challenge>,
    pub melees: Registry<crate::melee::Melee>,
    /// Armageddon's match, if one is active (`armageddon.rs`): global, no unit ever links to it.
    pub armageddons: Registry<crate::armageddon::ArmageddonMatch>,
    #[cfg(test)]
    pub gatherings: Registry<tests::Gathering>,
}

impl Coordinators {
    /// Whether the coordinator `link` points at is still there.
    pub fn exists(&self, link: Link) -> bool {
        match link {
            Link::Challenge(id) => self.challenges.contains(id),
            Link::Melee(id) => self.melees.contains(id),
            #[cfg(test)]
            Link::Gathering(id) => self.gatherings.contains(id),
        }
    }

    /// Every coordinator's turn, kind after kind in this order; the finished ones are removed. Their events, in order.
    #[allow(unused_variables, unused_mut)]
    pub fn tick(&mut self, now: Tick, world: &GameMap, rng: &mut Lcg) -> Vec<WorldEvent> {
        let mut events = Vec::new();
        tick_kind(&mut self.challenges, now, world, rng, &mut events);
        tick_kind(&mut self.melees, now, world, rng, &mut events);
        tick_kind(&mut self.armageddons, now, world, rng, &mut events);
        #[cfg(test)]
        tick_kind(&mut self.gatherings, now, world, rng, &mut events);
        events
    }
}

/// The turn of every coordinator of one kind, in id order, its members found once for all; the finished ones are
/// removed.
pub fn tick_kind<C: Coordinator>(list: &mut Registry<C>, now: Tick, world: &GameMap, rng: &mut Lcg, events: &mut Vec<WorldEvent>) {
    let members = participants(&world.units, C::link_of);
    let mut finished = Vec::new();
    for (id, c) in list.iter_mut() {
        let step = c.tick(id, members.get(&id).map_or(&[], Vec::as_slice), now, world, rng);
        events.extend(step.events);
        if step.finished {
            finished.push(id);
        }
    }
    list.retain(|id, _| !finished.contains(&id));
}

/// The reverse of the units' links: the units (ids, in unit order) linked to each coordinator of one kind.
pub fn participants<C>(units: &[Unit], link_of: impl Fn(&Unit) -> Option<Id<C>>) -> BTreeMap<Id<C>, Vec<UnitId>> {
    let mut by = BTreeMap::<Id<C>, Vec<UnitId>>::new();
    for u in units {
        if let Some(id) = link_of(u) {
            by.entry(id).or_default().push(u.id);
        }
    }
    by
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::effect::DamageKind;
    use crate::time::{Countdown, Ticks};
    use crate::unit::{Action, UnitKind};

    /// A test coordinator: holds its units for `left` turns, hurting each by 1-3 (drawn) every turn when `hurt`, then
    /// ends, releasing them when `release`.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Gathering {
        pub left: Countdown,
        pub hurt: bool,
        pub release: bool,
    }

    fn link_of(u: &Unit) -> Option<Id<Gathering>> {
        <Gathering as Coordinator>::link_of(u)
    }

    impl Coordinator for Gathering {
        fn link_of(u: &Unit) -> Option<Id<Self>> {
            match u.action.link()? {
                Link::Gathering(id) => Some(id),
                _ => None,
            }
        }

        fn tick(&mut self, id: Id<Self>, units: &[UnitId], _now: Tick, _world: &GameMap, rng: &mut Lcg) -> Step {
            let mut events = Vec::new();
            if self.hurt {
                events.extend(units.iter().map(|&unit| WorldEvent::Damage { unit, amount: 1 + (rng.next_u32() % 3) as u16, kind: DamageKind::Blast, from: None }));
            }
            let finished = self.left.tick();
            if finished && self.release {
                events.extend(units.iter().map(|&unit| WorldEvent::Release { unit, from: Link::Gathering(id) }));
            }
            Step { finished, events }
        }
    }

    /// `units` gathered for `turns` turns.
    fn gather(map: &mut GameMap, units: &[UnitId], turns: u32, hurt: bool, release: bool) -> Id<Gathering> {
        let id = map.new_id_of();
        map.coordinators.gatherings.insert(id, Gathering { left: Countdown::new(Ticks::new(turns)), hurt, release });
        for &u in units {
            map.unit_mut(u).unwrap().action = Action::Gathered { gathering: id };
        }
        id
    }

    /// `n` new braves of tribe 0, standing near its shaman.
    fn braves(map: &mut GameMap, n: usize) -> Vec<UnitId> {
        let s = map.shaman_of(0).unwrap();
        let at = (s.x.wrapping_add(1024), s.z);
        (0..n).map(|k| map.spawn(0, UnitKind::Brave, (at.0, at.1.wrapping_add(300 * k as u16))).id).collect()
    }

    fn action(map: &GameMap, id: UnitId) -> Action {
        map.unit(id).unwrap().action
    }

    #[test]
    fn a_gathering_ending_now_is_gone_and_its_units_free_when_they_tick() {
        let mut map = GameMap::sandbox_units();
        let units = braves(&mut map, 3);
        for &u in &units {
            map.damage(u, 1, DamageKind::Blast, None);
        }
        let g = gather(&mut map, &units, 2, false, true);
        map.tick();
        assert!(map.coordinators.gatherings.contains(g));
        assert!(units.iter().all(|&u| action(&map, u) == Action::Gathered { gathering: g }));
        map.tick();
        assert!(!map.coordinators.gatherings.contains(g));
        assert!(units.iter().all(|&u| action(&map, u) == Action::Idle), "released by its events, the same turn");
    }

    #[test]
    fn units_held_by_a_coordinator_heal_like_anyone() {
        let mut map = GameMap::sandbox_units();
        let units = braves(&mut map, 2);
        map.damage(units[0], 100, DamageKind::Blast, None);
        gather(&mut map, &units, 50, false, false);
        let life = |map: &GameMap| map.unit(units[0]).unwrap().health.current();
        let before = life(&map);
        while !crate::unit::HEAL_EVERY.fires(map.now.next(), 0) {
            map.tick();
        }
        map.tick();
        assert_eq!(life(&map), before + UnitKind::Brave.stats().heal.unwrap());
    }

    #[test]
    fn units_whose_gathering_is_gone_are_freed_without_being_told() {
        let mut map = GameMap::sandbox_units();
        let units = braves(&mut map, 2);
        let ends = gather(&mut map, &units[..1], 1, false, false);
        let removed = gather(&mut map, &units[1..], 50, false, false);
        map.coordinators.gatherings.retain(|id, _| id != removed);
        assert_eq!(map.linked(map.unit(units[1]).unwrap()), None, "linked to nothing any more");
        assert_eq!(map.linked(map.unit(units[0]).unwrap()), Some(Link::Gathering(ends)));
        map.tick();
        assert!(units.iter().all(|&u| action(&map, u) == Action::Idle));
    }

    #[test]
    fn participants_are_exactly_the_linked_units() {
        let mut map = GameMap::sandbox_units();
        let units = braves(&mut map, 4);
        let a = gather(&mut map, &units[..2], 5, false, false);
        let b = gather(&mut map, &units[3..], 5, false, false);
        let by = participants(&map.units, link_of);
        assert_eq!(by, BTreeMap::from([(a, units[..2].to_vec()), (b, vec![units[3]])]));
    }

    #[test]
    fn a_release_for_a_free_or_missing_unit_does_nothing() {
        let mut map = GameMap::sandbox_units();
        let free = braves(&mut map, 1)[0];
        let before = map.units.clone();
        let from = Link::Gathering(Id::from_raw(1));
        map.apply_events(vec![WorldEvent::Release { unit: free, from }, WorldEvent::Release { unit: Id::from_raw(99_999), from }]);
        assert_eq!(map.units, before);
    }

    #[test]
    fn its_damage_takes_the_one_hurt_path() {
        let mut map = GameMap::sandbox_units();
        let units = braves(&mut map, 2);
        map.unit_mut(units[1]).unwrap().statuses.shield = crate::status::Timed::steps(crate::status::SHIELD_STEPS);
        gather(&mut map, &units, 5, true, false);
        let max = map.unit(units[0]).unwrap().health.max();
        map.tick();
        assert!(map.unit(units[0]).unwrap().health.current() < max);
        assert_eq!(map.unit(units[1]).unwrap().health.current(), max, "shielded");
    }

    #[test]
    fn the_same_seed_gives_the_same_coordinators_and_units() {
        let run = || {
            let mut map = GameMap::sandbox_units().seeded(7);
            let units = braves(&mut map, 3);
            gather(&mut map, &units, 6, true, true);
            gather(&mut map, &units[..1], 20, true, false);
            map.run(Ticks::new(10));
            (map.units, map.coordinators, map.rng)
        };
        assert_eq!(run(), run());
    }
}
