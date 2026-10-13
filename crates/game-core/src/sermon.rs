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

/// One listener's sermon: who preaches, who listens, and the turns to its next chance of conversion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sermon {
    pub preacher: UnitId,
    pub listener: UnitId,
    pub left: Countdown,
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
        if !members.contains(&l.id) || !l.is_alive() || !p.is_alive() || !preaching(p) || p.owner == l.owner {
            return end(l.id);
        }
        if !self.left.tick() {
            return Step::default();
        }
        if rng.next_u32() % CONV_CHANCE == 1 && world.housing(p.owner).population < MAX_POPULATION {
            return Step { finished: true, events: vec![WorldEvent::ChangeOwner { unit: l.id, tribe: p.owner }] };
        }
        self.left = jittered(CONV_CHECK, rng);
        Step::default()
    }
}

impl GameMap {
    /// Preachers start sermons: on their look turn, every enemy they may preach to within their look square sits down
    /// to listen [guess: what starts a sermon is not traced]. One ordered to preach walks until its target is in that
    /// square. A preacher with no listener left stands idle again.
    pub(crate) fn start_sermons(&mut self) {
        let listened: Vec<UnitId> = self.coordinators.sermons.iter().map(|(_, s)| s.preacher).collect();
        for u in self.units.iter_mut().filter(|u| preaching(u) && !listened.contains(&u.id)) {
            u.action = Action::Idle;
        }
        let now = self.now;
        let size = self.terrain.size() as i32;
        for i in 0..self.units.len() {
            let p = &self.units[i];
            let ordered = matches!(p.action, Action::Attacking { target: Target::Unit(t), .. } if self.unit(t).is_some_and(|t| may_preach(p, t)));
            let free = matches!(p.action, Action::Idle | Action::Holding { .. }) || preaching(p) || ordered;
            if p.kind != UnitKind::Preacher || !p.is_alive() || !p.in_the_open() || !free || !p.kind.stats().look_every.fires(now, 0) {
                continue;
            }
            let cells: Vec<(i32, i32)> = crate::spotting::square(p.cell(), p.kind.stats().sight, size).collect();
            let listeners: Vec<usize> = (0..self.units.len()).filter(|&j| cells.contains(&self.units[j].cell()) && may_preach(p, &self.units[j])).collect();
            if listeners.is_empty() {
                continue;
            }
            let at = (p.x, p.z);
            let preacher = p.id;
            let first = (self.units[listeners[0]].x, self.units[listeners[0]].z);
            for j in listeners {
                let id = self.new_id_of();
                let left = jittered(CONV_FIRST_CHECK, &mut self.rng);
                self.coordinators.sermons.insert(id, Sermon { preacher, listener: self.units[j].id, left });
                self.sit_down(j, id, at);
            }
            let p = &mut self.units[i];
            p.face(first);
            p.join_preaching();
        }
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
        u.join(Action::Listening { sermon: id });
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
}
