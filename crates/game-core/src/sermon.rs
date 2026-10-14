//! Sermons (docs/specs/combat.md "Sermon"): a preacher sits the enemies it sees down and preaches to them; now and
//! then one of them joins the preacher's tribe. A coordinator per listener (docs/specs/coordinators.md), as the
//! original keeps a state per preached person ("Preacher conversion" [exe]).

use crate::coordinator::{Coordinator, Link, Step};
use crate::event::WorldEvent;
use crate::headcount::MAX_POPULATION;
use crate::id::Id;
use crate::map::{GameMap, Lcg};
use crate::time::{Countdown, Tick, Ticks};
use crate::unit::{Action, Target, Unit, UnitId, UnitKind};

pub type SermonId = Id<Sermon>;

/// Turns between two chances of conversion, and before the first (constant.dat value, halved for the first:
/// "Preacher conversion" [exe]).
pub const CONV_CHECK: u32 = 500;
pub const CONV_FIRST_CHECK: u32 = 250;
/// One chance in this many per check ("Preacher conversion" [exe]).
pub const CONV_CHANCE: u32 = 5;
/// Sitting down to listen: the sit-down animation's 6 frames, one turn each ("Combat" [exe]: one turn per frame).
pub const SIT_TICKS: Ticks = Ticks::new(6);

/// One listener's sermon: who preaches, who listens, the turns to its next chance of conversion, and the
/// checks so far that missed (debug overlay: shows it is still trying, not stuck).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sermon {
    pub preacher: UnitId,
    pub listener: UnitId,
    pub left: Countdown,
    pub attempts: u32,
}

/// `base` turns, give or take a sixteenth of them ("Preacher conversion" [exe]).
pub fn jittered(base: u32, rng: &mut Lcg) -> Countdown {
    let spread = base / 16;
    Countdown::new(Ticks::new(base - spread + rng.next_u32() % (2 * spread + 1)))
}

/// Whether `preacher` may preach to `target` ("Preacher conversion" [exe]; who is spared [game]): an enemy that fights,
/// alive, on the ground in the open (never aboard a vehicle), not a preacher or a shaman (those it fights), never a
/// wildman, and not taken by anything else already.
pub fn may_preach(preacher: &Unit, target: &Unit) -> bool {
    preacher.kind == UnitKind::Preacher
        && target.kind.stats().fights
        && !matches!(target.kind, UnitKind::Preacher | UnitKind::Shaman)
        && preacher.owner != target.owner
        && target.is_alive()
        && target.in_the_open()
        && !target.thrown()
        && target.action.link().is_none()
}

/// Whether `u` stands preaching.
fn preaching(u: &Unit) -> bool {
    u.action == Action::Preaching
}

impl Coordinator for Sermon {
    fn link_of(u: &Unit) -> Option<Id<Self>> {
        match u.action.link()? {
            Link::Sermon(id) => Some(id),
            _ => None,
        }
    }

    fn tick(&mut self, id: Id<Self>, members: &[UnitId], _now: Tick, world: &GameMap, rng: &mut Lcg) -> Step {
        let end = |listener| Step { finished: true, events: vec![WorldEvent::Release { unit: listener, from: Link::Sermon(id) }] };
        let (Some(p), Some(l)) = (world.unit(self.preacher), world.unit(self.listener)) else { return end(self.listener) };
        if !members.contains(&l.id) || !l.is_alive() || !p.is_alive() || p.owner == l.owner {
            return end(l.id);
        }
        if p.thrown() || matches!(p.action, Action::Challenge { .. } | Action::Melee { .. }) {
            let mut step = end(l.id);
            step.events.push(WorldEvent::StrikeBack { unit: l.id, at: p.id });
            return step;
        }
        if !preaching(p) {
            return end(l.id);
        }
        if !self.left.tick() {
            return Step::default();
        }
        if rng.next_u32() % CONV_CHANCE == 1 && world.housing(p.owner).population < MAX_POPULATION {
            return Step { finished: true, events: vec![WorldEvent::ChangeOwner { unit: l.id, tribe: p.owner }] };
        }
        self.attempts += 1;
        self.left = jittered(CONV_CHECK, rng);
        Step::default()
    }
}

impl GameMap {
    /// Preachers start sermons: on their look turn, every enemy they may preach to within their look square sits down
    /// to listen [guess: what starts a sermon is not traced]. Unlike anyone else's own look, walking on any order does
    /// not blind it to this [game]: its conversion look is not a melee scan, so it keeps working while it is on its
    /// way somewhere, not only once it arrives. One ordered to preach checks its one target every turn, not gated by
    /// the look rhythm: the gap it still has to close can otherwise vanish into the target's own melee reach before
    /// the preacher's next look turn [game]. A preacher with no listener left stands idle again.
    pub(crate) fn start_sermons(&mut self) {
        let listened: Vec<UnitId> = self.coordinators.sermons.iter().map(|(_, s)| s.preacher).collect();
        for u in self.units.iter_mut().filter(|u| preaching(u) && !listened.contains(&u.id)) {
            u.action = Action::Idle;
        }
        let now = self.now;
        let size = self.terrain.size() as i32;
        for i in 0..self.units.len() {
            let p = &self.units[i];
            if p.kind != UnitKind::Preacher || !p.is_alive() || !p.in_the_open() {
                continue;
            }
            let ordered = match p.action {
                Action::Attacking { target: Target::Unit(t), .. } => self.unit(t).filter(|t| may_preach(p, t)).map(|t| t.id),
                _ => None,
            };
            if let Some(t) = ordered {
                let p = &self.units[i];
                let cells: Vec<(i32, i32)> = crate::spotting::square(p.cell(), p.kind.stats().sight, size).collect();
                if let Some(j) = self.unit_index(t).filter(|&j| cells.contains(&self.units[j].cell())) {
                    self.begin_sermon(i, &[j]);
                    continue;
                }
            }
            let p = &self.units[i];
            let free = matches!(p.action, Action::Idle | Action::Holding { .. } | Action::Walking { .. }) || preaching(p) || ordered.is_some();
            if !free || !p.kind.stats().look_every.fires(now, 0) {
                continue;
            }
            let cells: Vec<(i32, i32)> = crate::spotting::square(p.cell(), p.kind.stats().sight, size).collect();
            let listeners: Vec<usize> = (0..self.units.len()).filter(|&j| cells.contains(&self.units[j].cell()) && may_preach(p, &self.units[j])).collect();
            if listeners.is_empty() {
                continue;
            }
            self.begin_sermon(i, &listeners);
        }
    }

    /// Preacher `i` starts preaching: `listeners` sit down at once, it faces the first.
    fn begin_sermon(&mut self, i: usize, listeners: &[usize]) {
        let p = &self.units[i];
        let at = (p.x, p.z);
        let preacher = p.id;
        let first = (self.units[listeners[0]].x, self.units[listeners[0]].z);
        for &j in listeners {
            let id = self.new_id_of();
            let left = jittered(CONV_FIRST_CHECK, &mut self.rng);
            self.coordinators.sermons.insert(id, Sermon { preacher, listener: self.units[j].id, left, attempts: 0 });
            self.sit_down(j, id, at);
        }
        let p = &mut self.units[i];
        p.face(first);
        p.join_preaching();
    }

    /// Unit `j` sits down to listen to the sermon `id` of the preacher at `preacher`: a brave puts down the wood it
    /// carries [game].
    fn sit_down(&mut self, j: usize, id: SermonId, preacher: (u16, u16)) {
        let u = &mut self.units[j];
        if u.carrying > 0 {
            u.carrying = 0;
            self.wood.push(crate::wood::WoodPiece::new(u.x, u.z));
        }
        let u = &mut self.units[j];
        u.face(preacher);
        u.join(Action::Listening { sermon: id, sitting: Countdown::new(SIT_TICKS) });
    }

    /// The unit `unit` joins `tribe` (`WorldEvent::ChangeOwner`): as the original's new person in its place, it keeps
    /// its kind and place and starts afresh (`Unit::change_owner`).
    pub(crate) fn change_owner(&mut self, unit: UnitId, tribe: u8) {
        if let Some(u) = self.unit_mut(unit).filter(|u| u.is_alive()) {
            u.change_owner(tribe);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A preacher of tribe 0, and `kinds` of tribe 1 one cell east of it, in a corner of the island away from the
    /// sandbox's people.
    fn sermon_scene(kinds: &[UnitKind]) -> (GameMap, UnitId, Vec<UnitId>) {
        let mut map = GameMap::sandbox_units().seeded(11);
        let s = map.shaman_of(0).unwrap();
        let at = (s.x.wrapping_add(1024), s.z.wrapping_sub(15 * 512));
        let p = map.spawn(0, UnitKind::Preacher, at).id;
        let others = kinds.iter().enumerate().map(|(k, &kind)| map.spawn(1, kind, (at.0.wrapping_add(512), at.1.wrapping_add(100 * k as u16))).id).collect();
        (map, p, others)
    }

    fn listening(map: &GameMap, u: UnitId) -> bool {
        matches!(map.unit(u).map(|u| u.action), Some(Action::Listening { .. }))
    }

    fn until(map: &mut GameMap, turns: u32, done: impl Fn(&GameMap) -> bool) -> Option<u32> {
        (1..=turns).find(|_| {
            map.tick();
            done(map)
        })
    }

    #[test]
    fn checks_every_500_turns_give_or_take_a_sixteenth() {
        let mut rng = Lcg(3);
        for _ in 0..50 {
            let left = jittered(CONV_CHECK, &mut rng).left().get();
            assert!((CONV_CHECK - 31..=CONV_CHECK + 31).contains(&left), "{left}");
        }
    }

    #[test]
    fn an_enemy_in_sight_sits_down_and_joins_the_preachers_tribe_in_the_end() {
        let (mut map, p, others) = sermon_scene(&[UnitKind::Warrior]);
        let w = others[0];
        until(&mut map, 4, |m| listening(m, w)).expect("sat down");
        assert_eq!(map.unit(p).unwrap().action, Action::Preaching);
        map.damage(w, 300, crate::effect::DamageKind::Melee, None);
        let took = until(&mut map, 20_000, |m| m.unit(w).is_some_and(|u| u.owner == 0)).expect("converted");
        assert!(took >= CONV_FIRST_CHECK - CONV_FIRST_CHECK / 16, "not before the first check");
        let u = map.unit(w).unwrap();
        assert_eq!((u.kind, u.action, u.health.is_full()), (UnitKind::Warrior, Action::Idle, true), "a warrior still, idle, as new");
    }

    #[test]
    fn a_listener_is_locked_out_of_orders_and_conversion_starts_it_afresh() {
        let (mut map, _, others) = sermon_scene(&[UnitKind::Brave]);
        let b = others[0];
        until(&mut map, 4, |m| listening(m, b)).expect("sat down");
        assert!(map.unit(b).unwrap().locked());
        map.apply(&crate::command::Command::OrderUnit { player: 1, unit: b, order: crate::unit::Order::MoveTo { x: 0, z: 0 } });
        assert!(listening(&map, b), "no order reaches it");
        let u = map.unit_mut(b).unwrap();
        u.health = crate::health::Health::new(10, u.health.max());
        u.statuses.shield = crate::status::Timed::steps(3);
        u.change_owner(0);
        let u = map.unit(b).unwrap();
        assert_eq!((u.owner, u.action, u.health.is_full(), u.statuses.shield.is_on(), u.locked()), (0, Action::Idle, true, false, false));
    }

    #[test]
    fn wildmen_and_people_aboard_are_never_preached_to() {
        let (mut map, _, others) = sermon_scene(&[UnitKind::Wildman, UnitKind::Brave]);
        let brave = others[1];
        let at = map.unit(brave).map(|u| (u.x, u.z)).unwrap();
        let balloon = map.add_vehicle(crate::vehicle::Vehicle::new(crate::vehicle::VehicleKind::Balloon, 1, at, crate::angle::Angle::new(0)));
        map.unit_mut(brave).unwrap().aboard = Some(balloon);
        map.vehicles.get_mut(balloon).unwrap().seat(brave, 1);
        map.run(Ticks::new(10));
        assert!(!listening(&map, others[0]) && !listening(&map, brave));
    }

    #[test]
    fn a_brave_puts_down_its_wood_to_listen() {
        let (mut map, _, others) = sermon_scene(&[UnitKind::Brave]);
        map.unit_mut(others[0]).unwrap().carrying = 1;
        let wood = map.wood.len();
        until(&mut map, 4, |m| listening(m, others[0])).expect("sat down");
        assert_eq!((map.unit(others[0]).unwrap().carrying, map.wood.len()), (0, wood + 1));
    }

    /// A warrior of tribe 2 right next to `target`, attacking it, and the challenge started at once.
    fn challenged(map: &mut GameMap, target: UnitId) {
        let at = map.unit(target).map(|u| (u.x.wrapping_add(100), u.z)).unwrap();
        let foe = map.spawn(2, UnitKind::Warrior, at);
        foe.action = Action::Attacking { target: Target::Unit(target), to: at };
        map.start_challenges();
        assert!(matches!(map.unit(target).unwrap().action, Action::Challenge { .. }), "challenged");
    }

    #[test]
    fn attacking_a_listener_takes_only_it_away_attacking_the_preacher_ends_every_sermon() {
        let (mut map, p, others) = sermon_scene(&[UnitKind::Brave, UnitKind::Brave]);
        until(&mut map, 4, |m| others.iter().all(|&u| listening(m, u))).expect("both listen");
        challenged(&mut map, others[0]);
        map.tick();
        assert_eq!(map.coordinators.sermons.len(), 1, "its sermon ended");
        assert!(listening(&map, others[1]), "the other one listens on");
        challenged(&mut map, p);
        map.tick();
        assert!(map.coordinators.sermons.is_empty() && !listening(&map, others[1]), "the preacher attacked: every sermon ends");
        assert!(matches!(map.unit(others[1]).unwrap().action, Action::Attacking { target: Target::Unit(t), .. } if t == p), "its listener turns on it");
    }

    /// A bolt of tribe `owner`, thrown from 20 cells west, about to strike `target`.
    fn bolt_at(map: &mut GameMap, owner: u8, target: UnitId) {
        let t = map.unit(target).map(|u| (u.x, u.z)).unwrap();
        let thrower = map.spawn(owner, UnitKind::Firewarrior, (t.0.wrapping_sub(20 * 512), t.1)).id;
        let height = map.terrain.height_at(t.0 as u32, t.1 as u32, pop3_format::WORLD_UNITS_PER_CELL) + crate::firebolt::BOLT_RISE;
        let at = (t.0.wrapping_sub(100), t.1);
        let bolt = crate::firebolt::Firebolt { owner, thrower, target: Target::Unit(target), at, height, aim: (t.0, t.1, height), hits: true, bloodlusted: false, from_tower: false };
        let id = map.new_id_of();
        map.bolts.insert(id, bolt);
    }

    #[test]
    /// The brave listens a cell away: by the time it reaches the preacher, the preacher is back on its feet (a bolt only
    /// makes it hop, `firebolt::steady`) and its next look sits the brave down again, about 20 units before the brave
    /// is close enough to challenge it.
    fn a_preacher_struck_by_a_bolt_is_avenged_but_sits_the_avenger_down_again() {
        let mut outcomes = Vec::new();
        for seed in 0..8 {
            let (mut map, p, others) = sermon_scene(&[UnitKind::Brave]);
            map.rng = Lcg(seed);
            let b = others[0];
            until(&mut map, 4, |m| listening(m, b)).expect("sat down");
            bolt_at(&mut map, 1, p);
            until(&mut map, 3, |m| m.unit(p).unwrap().thrown()).expect("thrown by the bolt");
            assert!(matches!(map.unit(b).unwrap().action, Action::Attacking { target: Target::Unit(t), .. } if t == p), "the brave turns on it");
            let fought = |m: &GameMap| matches!(m.unit(p).unwrap().action, Action::Challenge { .. } | Action::Melee { .. });
            let end = until(&mut map, 200, |m| listening(m, b) || fought(m));
            outcomes.push((end.is_some(), listening(&map, b)));
        }
        assert!(outcomes.iter().all(|&(over, sat)| over && sat), "the preacher, back on its feet, sits it down first: {outcomes:?}");
    }

    #[test]
    fn a_brave_listening_right_next_to_a_struck_preacher_reaches_it_first() {
        let (mut map, p, others) = sermon_scene(&[UnitKind::Brave]);
        let b = others[0];
        until(&mut map, 4, |m| listening(m, b)).expect("sat down");
        let at = map.unit(p).map(|u| (u.x.wrapping_add(250), u.z)).unwrap();
        (map.unit_mut(b).unwrap().x, map.unit_mut(b).unwrap().z) = at;
        bolt_at(&mut map, 1, p);
        let fought = |m: &GameMap| matches!(m.unit(p).unwrap().action, Action::Challenge { .. } | Action::Melee { .. });
        until(&mut map, 200, |m| listening(m, b) || fought(m)).expect("one or the other");
        assert!(fought(&map), "a preacher only hops under a bolt: the brave next to it challenges it before its next look");
    }

    #[test]
    fn the_same_seed_gives_the_same_sermons() {
        let run = || {
            let (mut map, _, _) = sermon_scene(&[UnitKind::Brave, UnitKind::Warrior]);
            map.run(Ticks::new(700));
            (map.units, map.coordinators, map.rng)
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn ordered_to_convert_it_does_not_lose_an_off_rhythm_tick_to_the_targets_own_challenge() {
        // On the very first simulated tick (now == 1), the preacher's look_every(2) does not fire (it only fires on
        // even ticks), yet its ordered target is already right next to it, and that target already decided (on its
        // own, earlier) to attack the preacher back: this is the exact race the preacher must not lose.
        let mut map = GameMap::sandbox_units().seeded(11);
        let s = map.shaman_of(0).unwrap();
        let at = (s.x.wrapping_add(1024), s.z.wrapping_sub(15 * 512));
        let p = map.spawn(0, UnitKind::Preacher, at).id;
        let brave = map.spawn(1, UnitKind::Brave, (at.0.wrapping_add(100), at.1)).id;
        map.apply(&crate::command::Command::OrderUnit { player: 0, unit: p, order: crate::unit::Order::Attack { target: Target::Unit(brave) } });
        map.unit_mut(brave).unwrap().action = Action::Attacking { target: Target::Unit(p), to: at };
        map.tick();
        assert!(listening(&map, brave), "the preacher converts it on this very tick, despite being off its look rhythm");
        assert!(!matches!(map.unit(p).unwrap().action, Action::Challenge { .. } | Action::Melee { .. }), "it must not end up fighting it instead");
    }

    #[test]
    fn walked_near_an_idle_enemy_on_a_plain_move_order_it_still_converts_it() {
        let mut map = GameMap::sandbox_units().seeded(11);
        let s = map.shaman_of(0).unwrap();
        let at = (s.x.wrapping_add(1024), s.z.wrapping_sub(15 * 512));
        let p = map.spawn(0, UnitKind::Preacher, at).id;
        let brave = map.spawn(1, UnitKind::Brave, (at.0.wrapping_add(6 * 512), at.1)).id;
        let near = (at.0.wrapping_add(5 * 512), at.1);
        map.apply(&crate::command::Command::OrderUnit { player: 0, unit: p, order: crate::unit::Order::MoveTo { x: near.0, z: near.1 } });
        let fought = |m: &GameMap| matches!(m.unit(p).unwrap().action, Action::Challenge { .. } | Action::Melee { .. });
        until(&mut map, 300, |m| listening(m, brave) || fought(m)).expect("one or the other");
        assert!(listening(&map, brave), "walked close enough to preach on a plain move order, not an attack order: the preacher must still convert it, not fight it");
    }
}
