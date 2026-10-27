//! A melee (docs/specs/combat.md "Melee"): one person in the middle against up to five in a ring around it, trading
//! blows until one side is left. A coordinator (docs/specs/coordinators.md): fighters link to it with their place
//! and their stance, it picks who strikes whom and how.

use crate::angle;
use crate::combat::{move_turns, strength, Blow, Move};
use crate::coordinator::{Coordinator, Link, Step};
use crate::effect::DamageKind;
use crate::event::WorldEvent;
use crate::id::Id;
use crate::map::{GameMap, Lcg};
use crate::motion::Velocity;
use crate::time::{Countdown, Every, Tick, Ticks};
use crate::unit::{isqrt, torus_delta, Action, Unit, UnitId, UnitKind};

pub type MeleeId = Id<Melee>;

/// The ring's radius around the middle, world units ("Fight" [exe, measured]).
pub const RING_RADIUS: i32 = 180;
/// A fighter farther than this from the middle has left the melee ("Fight" [exe]).
pub const MELEE_LEASH: i32 = 2048;
/// A fighter this close to its place is on guard ("Fight" [exe]).
pub const PLACE_REACH: i32 = 12;
/// A busy opponent this close to its place may still be struck ("Fight" [exe]).
pub const BUSY_REACH: i32 = 360;
/// The ring turns half a turn on this rhythm, then maybe a little more ("Fight" [exe]).
pub const RING_TURN: Every = Every::new(32);
/// The extra turn: the first plus a draw below the second, of `angle::TURN` (20° to 80°) ("Fight" [exe]).
pub const RING_EXTRA: (u32, u32) = (113, 341);
/// A knocked fighter is thrown back at the first plus a draw below the second per turn ("Fight" [exe, measured]).
pub const KNOCK_SPEED: (i32, i32) = (35, 70);
/// The most people in one melee ("Fight" [exe]).
pub const MAX_FIGHTERS: usize = 6;

/// A fighter's own cycle (`Action::Melee`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stance {
    /// Walking to its place.
    ToPlace,
    /// At its place, ready to strike or be struck.
    OnGuard,
    /// Dealing `blow`, `left` turns to go.
    Striking { blow: Blow, left: Countdown },
    /// Struck, `left` turns to go; then thrown back at `knock`, if any.
    Reeling { left: Countdown, knock: Option<Velocity> },
    /// Thrown back: walking to its place again once still.
    Staggering,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Melee {
    pub tribes: (u8, u8),
    /// The middle of the ring: where the one alone on its side stands.
    pub at: (u16, u16),
    /// The ring's turn, of `angle::TURN`.
    pub angle: u32,
    pub centre: Option<UnitId>,
}

/// A fighter as the melee sees it this turn.
#[derive(Clone, Copy)]
struct Fighter<'a> {
    unit: &'a Unit,
    place: (u16, u16),
    stance: Stance,
}

fn fighter(u: &Unit) -> Option<Fighter<'_>> {
    match u.action {
        Action::Melee { place, stance, .. } => Some(Fighter { unit: u, place, stance }),
        _ => None,
    }
}

fn distance(a: (u16, u16), b: (u16, u16)) -> i32 {
    let (dx, dz) = (torus_delta(a.0, b.0), torus_delta(a.1, b.1));
    isqrt((dx * dx + dz * dz) as u32) as i32
}

/// Where the ring's `k`-th person stands with `count` in the melee: around `at` at the ring's `angle`; the 5th and
/// 6th in the middle ("Fight" [exe]).
pub fn ring_place(at: (u16, u16), angle: u32, count: usize, k: usize) -> (u16, u16) {
    let offsets: &[u32] = match count {
        0..=2 => &[0],
        3 => &[0, angle::TURN / 4],
        _ => &[0, angle::TURN / 3, 2 * angle::TURN / 3],
    };
    let Some(offset) = offsets.get(k) else { return at };
    let (x, z) = angle::rotate((RING_RADIUS, 0), angle + offset);
    (at.0.wrapping_add(x as u16), at.1.wrapping_add(z as u16))
}

/// The blow `attacker` deals for the draw `r` (0..16) to an opponent on guard ("Fight" [exe]): the shaman never trades.
pub fn blow_on_guard(attacker: UnitKind, r: u32) -> Blow {
    match (attacker, r) {
        (UnitKind::Shaman, 0..=6) => Blow::Knock,
        (UnitKind::Shaman, _) => Blow::Clean,
        (_, 0..=4) => Blow::Knock,
        (_, 5..=13) => Blow::Clean,
        _ => Blow::Trade,
    }
}

/// The blow, if any, `attacker` deals for the draw `r` to a busy opponent near its place ("Fight" [exe]).
pub fn blow_on_busy(r: u32, crowded: bool) -> Option<Blow> {
    match (r, crowded) {
        (4.., _) => None,
        (_, false) => Some(Blow::Clean),
        (r, true) if r % 2 == 0 => Some(Blow::Clean),
        _ => Some(Blow::Trade),
    }
}

impl Melee {
    /// A new melee where `target` stands, `attacker` against it.
    pub fn new(attacker: &Unit, target: &Unit, angle: u32) -> Self {
        Melee { tribes: (attacker.owner, target.owner), at: (target.x, target.z), angle, centre: Some(target.id) }
    }

    /// The fighter alone on its side, the last centre first.
    fn find_centre(&self, fighters: &[Fighter]) -> Option<UnitId> {
        let alone = |f: &&Fighter| fighters.iter().filter(|o| o.unit.owner == f.unit.owner).count() == 1;
        let candidates: Vec<&Fighter> = fighters.iter().filter(alone).collect();
        candidates.iter().find(|f| Some(f.unit.id) == self.centre).or(candidates.first()).map(|f| f.unit.id)
    }
}

impl Coordinator for Melee {
    fn link_of(u: &Unit) -> Option<Id<Self>> {
        match u.action.link()? {
            Link::Melee(id) => Some(id),
            _ => None,
        }
    }

    fn tick(&mut self, id: Id<Self>, members: &[UnitId], now: Tick, world: &GameMap, rng: &mut Lcg) -> Step {
        let mut events = Vec::new();
        let mut fighters = Vec::new();
        for &m in members {
            let Some(f) = world.unit(m).filter(|u| u.is_alive()).and_then(fighter) else { continue };
            if distance((f.unit.x, f.unit.z), self.at) > MELEE_LEASH {
                events.push(WorldEvent::Release { unit: m, from: Link::Melee(id) });
            } else {
                fighters.push(f);
            }
        }
        let sides = fighters.iter().filter(|f| f.unit.owner != fighters[0].unit.owner).count();
        if fighters.len() < 2 || sides == 0 {
            events.extend(fighters.iter().map(|f| WorldEvent::Release { unit: f.unit.id, from: Link::Melee(id) }));
            return Step { finished: true, events };
        }
        self.centre = self.find_centre(&fighters);
        if let Some(c) = self.centre.and_then(|c| world.unit(c)) {
            self.at = (c.x, c.z);
        }
        if RING_TURN.fires(now, id.raw()) {
            self.angle += angle::TURN / 2;
            if rng.next_u32().is_multiple_of(2) {
                let extra = RING_EXTRA.0 + rng.next_u32() % RING_EXTRA.1;
                self.angle = if rng.next_u32().is_multiple_of(2) { self.angle + extra } else { self.angle + angle::TURN - extra };
            }
            self.angle %= angle::TURN;
        }
        let ring: Vec<Fighter> = fighters.iter().copied().filter(|f| Some(f.unit.id) != self.centre).collect();
        for (k, f) in ring.iter().enumerate() {
            let place = ring_place(self.at, self.angle, fighters.len(), k);
            if place != f.place {
                events.push(WorldEvent::Placed { unit: f.unit.id, at: place });
            }
        }
        if let Some(c) = fighters.iter().find(|f| Some(f.unit.id) == self.centre).filter(|c| c.place != self.at) {
            events.push(WorldEvent::Placed { unit: c.unit.id, at: self.at });
        }
        events.extend(self.exchange(&fighters, &ring, rng));
        Step { finished: false, events }
    }
}

impl Melee {
    /// This turn's blows: each fighter on guard picks its opponent and maybe a blow. Lives lost this turn count for
    /// the next blows, and someone struck is busy for the rest of the turn.
    fn exchange(&self, fighters: &[Fighter], ring: &[Fighter], rng: &mut Lcg) -> Vec<WorldEvent> {
        let mut events = Vec::new();
        let mut lost: Vec<(UnitId, u16)> = Vec::new();
        let mut busy: Vec<UnitId> = Vec::new();
        let life = |u: &Unit, lost: &[(UnitId, u16)]| u.health.current().saturating_sub(lost.iter().filter(|l| l.0 == u.id).map(|l| l.1).sum());
        for f in fighters {
            if f.stance != Stance::OnGuard || busy.contains(&f.unit.id) {
                continue;
            }
            let r = rng.next_u32() & 15;
            let opponent = if Some(f.unit.id) == self.centre {
                ring[(rng.next_u32() % ring.len() as u32) as usize]
            } else {
                match fighters.iter().find(|o| Some(o.unit.id) == self.centre) {
                    Some(&c) => c,
                    None => continue,
                }
            };
            let ready = opponent.stance == Stance::OnGuard && !busy.contains(&opponent.unit.id);
            let blow = if ready {
                blow_on_guard(f.unit.kind, r)
            } else if distance((opponent.unit.x, opponent.unit.z), opponent.place) <= BUSY_REACH {
                match blow_on_busy(r, fighters.len() > 2) {
                    Some(b) => b,
                    None => continue,
                }
            } else {
                continue;
            };
            let (a, t) = (f.unit, opponent.unit);
            let (a_life, t_life) = (life(a, &lost), life(t, &lost));
            let hit = strength(a.kind, a_life, a.health.max(), a.statuses.bloodlust.is_on());
            events.push(WorldEvent::Damage { unit: t.id, amount: hit, kind: DamageKind::Melee, from: Some(a.owner) });
            lost.push((t.id, hit));
            if blow != Blow::Clean {
                let back = strength(t.kind, t_life, t.health.max(), t.statuses.bloodlust.is_on());
                events.push(WorldEvent::Damage { unit: a.id, amount: back, kind: DamageKind::Melee, from: Some(t.owner) });
                lost.push((a.id, back));
            }
            let knock = (ready && blow == Blow::Knock && t.kind != UnitKind::Shaman).then(|| {
                let speed = KNOCK_SPEED.0 + (rng.next_u32() % KNOCK_SPEED.1 as u32) as i32;
                knock_back((a.x, a.z), (t.x, t.z), speed)
            });
            events.push(WorldEvent::Struck { attacker: a.id, target: t.id, blow, knock });
            busy.extend([a.id, t.id]);
        }
        events
    }
}

/// A knock back at `speed` per turn from `target`, straight away from `attacker`, along the ground.
pub fn knock_back(attacker: (u16, u16), target: (u16, u16), speed: i32) -> Velocity {
    let (dx, dz) = (torus_delta(attacker.0, target.0), torus_delta(attacker.1, target.1));
    let d = isqrt((dx * dx + dz * dz) as u32).max(1) as i32;
    Velocity::new((dx * speed / d) as i16, 0, (dz * speed / d) as i16)
}

impl GameMap {
    /// A challenge won its second charge: a melee at the target's place, both in it.
    pub(crate) fn start_melee(&mut self, attacker: UnitId, target: UnitId) {
        let angle = self.rng.next_u32() % angle::TURN;
        let (Some(a), Some(t)) = (self.unit(attacker), self.unit(target)) else { return };
        if !(a.is_alive() && t.is_alive()) {
            return;
        }
        let melee = Melee::new(a, t, angle);
        let ring = ring_place(melee.at, melee.angle, 2, 0);
        let id = self.new_id_of();
        let at = melee.at;
        self.coordinators.melees.insert(id, melee);
        for (unit, place) in [(attacker, ring), (target, at)] {
            if let Some(u) = self.unit_mut(unit) {
                u.join(Action::Melee { melee: id, place, stance: Stance::ToPlace });
            }
        }
    }

    /// A blow dealt (`WorldEvent::Struck`): the attacker strikes, a target on guard reels, both face each other.
    pub(crate) fn struck(&mut self, attacker: UnitId, target: UnitId, blow: Blow, knock: Option<Velocity>) {
        let (Some(a), Some(t)) = (self.unit_index(attacker), self.unit_index(target)) else { return };
        let (pa, pt) = ((self.units[a].x, self.units[a].z), (self.units[t].x, self.units[t].z));
        let a = &mut self.units[a];
        a.face(pt);
        if let Action::Melee { melee, place, .. } = a.action {
            let left = Countdown::new(Ticks::new(move_turns(a.kind, Move::Strike(blow))));
            a.action = Action::Melee { melee, place, stance: Stance::Striking { blow, left } };
        }
        let t = &mut self.units[t];
        t.face(pa);
        if let Action::Melee { melee, place, stance: Stance::OnGuard } = t.action {
            let left = Countdown::new(Ticks::new(move_turns(t.kind, Move::Reel { knocked: knock.is_some() })));
            t.action = Action::Melee { melee, place, stance: Stance::Reeling { left, knock } };
        }
    }
}

impl GameMap {
    /// A third attacker closing on either member of an ongoing challenge turns it into a melee at once, instead of
    /// standing in reach doing nothing while the duel plays out its own charge-shove-charge cycle on its own time
    /// (combat.md "Spotting", TODO.md "Joining a fight already starting"): guessed, not traced against the
    /// original - `join_melees`, called right after, is what actually brings this attacker (and anyone else in
    /// reach) into the fresh melee, under its usual tribe/rank/capacity rules.
    pub(crate) fn escalate_challenges(&mut self) {
        for i in 0..self.units.len() {
            let a = &self.units[i];
            let Action::Attacking { target: crate::unit::Target::Unit(target), .. } = a.action else { continue };
            let Some(j) = self.unit_index(target) else { continue };
            let t = &self.units[j];
            let Some(Link::Challenge(id)) = self.linked(t) else { continue };
            if t.thrown() || !crate::attack::may_attack(a, t) || !t.in_the_open() || !crate::challenge::in_reach((a.x, a.z), (t.x, t.z)) {
                continue;
            }
            let Some(challenge) = self.coordinators.challenges.get(id) else { continue };
            let (attacker, target) = (challenge.attacker, challenge.target);
            self.coordinators.challenges.retain(|cid, _| cid != id);
            self.start_melee(attacker, target);
        }
    }

    /// Attackers within reach of a target already in a melee join it ("Fight" [exe]): one of its two tribes only,
    /// never a firewarrior, up to `MAX_FIGHTERS`; a tribe with 3 or more there is joined only by replacing one of
    /// lower rank, who leaves. Then the melee splits if both sides have 2 or more.
    pub(crate) fn join_melees(&mut self) {
        for i in 0..self.units.len() {
            let a = &self.units[i];
            let Action::Attacking { target: crate::unit::Target::Unit(target), .. } = a.action else { continue };
            let Some(t) = self.unit(target) else { continue };
            let Some(Link::Melee(id)) = self.linked(t) else { continue };
            let Some(melee) = self.coordinators.melees.get(id) else { continue };
            if !a.kind.stats().joins_melees || ![melee.tribes.0, melee.tribes.1].contains(&a.owner) || !crate::challenge::in_reach((a.x, a.z), (t.x, t.z)) {
                continue;
            }
            let members: Vec<&Unit> = self.units.iter().filter(|u| <Melee as Coordinator>::link_of(u) == Some(id)).collect();
            let own: Vec<&&Unit> = members.iter().filter(|u| u.owner == a.owner).collect();
            let rank = a.kind.stats().rank;
            let leaving = if own.len() >= 3 {
                match own.iter().filter(|u| u.kind.stats().rank < rank).min_by_key(|u| (u.kind.stats().rank, u.id)) {
                    Some(u) => Some(u.id),
                    None => continue,
                }
            } else if members.len() >= MAX_FIGHTERS {
                continue;
            } else {
                None
            };
            if let Some(u) = leaving.and_then(|u| self.unit_mut(u)) {
                u.release();
            }
            let a = &mut self.units[i];
            let place = (a.x, a.z);
            a.join(Action::Melee { melee: id, place, stance: Stance::ToPlace });
            self.split(id);
        }
    }

    /// Both sides of the melee `id` with 2 or more: the last of each leaves for a new one-to-one melee at the same
    /// place ("Fight" [exe]).
    fn split(&mut self, id: MeleeId) {
        let Some(melee) = self.coordinators.melees.get(id) else { return };
        let (tribes, at) = (melee.tribes, melee.at);
        let side = |tribe: u8| self.units.iter().filter(|u| u.owner == tribe && <Melee as Coordinator>::link_of(u) == Some(id)).map(|u| u.id).collect::<Vec<_>>();
        let (first, second) = (side(tribes.0), side(tribes.1));
        if first.len() < 2 || second.len() < 2 {
            return;
        }
        let pair = [*first.last().unwrap(), *second.last().unwrap()];
        let angle = self.rng.next_u32() % angle::TURN;
        let new = self.new_id_of();
        self.coordinators.melees.insert(new, Melee { tribes, at, angle, centre: None });
        for u in pair {
            if let Some(u) = self.unit_mut(u) {
                let place = (u.x, u.z);
                u.join(Action::Melee { melee: new, place, stance: Stance::ToPlace });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::unit::{Order, Target};

    fn until(map: &mut GameMap, turns: u32, done: impl Fn(&GameMap) -> bool) -> Option<u32> {
        (1..=turns).find(|_| {
            map.tick();
            done(map)
        })
    }

    /// Two braves of tribes 0 and 1 already in a melee, a cell apart.
    fn duel(attacker: UnitKind, target: UnitKind) -> (GameMap, UnitId, UnitId) {
        let mut map = GameMap::sandbox_units().seeded(5);
        let s = map.shaman_of(0).unwrap();
        let at = (s.x.wrapping_add(1024), s.z.wrapping_add(2048));
        let a = map.spawn(0, attacker, at).id;
        let t = map.spawn(1, target, (at.0.wrapping_add(200), at.1)).id;
        map.start_melee(a, t);
        (map, a, t)
    }

    fn stance(map: &GameMap, u: UnitId) -> Option<Stance> {
        match map.unit(u)?.action {
            Action::Melee { stance, .. } => Some(stance),
            _ => None,
        }
    }

    /// A warrior's blow (360 at full life) so outweighs a shaman's (60) that it wins every one of 200 seeds tried
    /// (never the shaman, never a draw), in well under 200 turns - far too fast for her healing (32 every 8
    /// turns, `HEAL_EVERY`) to keep up. Not traced against the original (combat.md "Fight numbers" gives no
    /// winner's odds): this only checks our own numbers play out the way they read, consistently.
    #[test]
    fn a_warrior_beats_a_shaman_in_a_fair_fight() {
        let (mut map, warrior, shaman) = duel(UnitKind::Warrior, UnitKind::Shaman);
        until(&mut map, 300, |m| m.coordinators.melees.is_empty()).expect("the fight ends");
        assert!(map.unit(warrior).is_some_and(Unit::is_alive), "the warrior's far harder blow should win before she can out-heal it");
        assert!(!map.unit(shaman).is_some_and(Unit::is_alive), "the shaman should fall");
    }

    #[test]
    fn places_around_the_middle() {
        let at = (1000, 1000);
        assert_eq!(ring_place(at, 0, 2, 0), (1180, 1000));
        assert_eq!(ring_place(at, 0, 3, 1), (1000, 1180));
        assert_eq!(ring_place(at, 0, 6, 3), at, "the 5th and 6th stand in the middle");
        for k in 0..3 {
            assert!((RING_RADIUS - 1..=RING_RADIUS).contains(&distance(ring_place(at, 300, 4, k), at)), "integer turns round down");
        }
    }

    #[test]
    fn the_odds_of_the_blows() {
        let count = |kind, blow| (0..16).filter(|&r| blow_on_guard(kind, r) == blow).count();
        assert_eq!([Blow::Knock, Blow::Clean, Blow::Trade].map(|b| count(UnitKind::Brave, b)), [5, 9, 2]);
        assert_eq!([Blow::Knock, Blow::Clean, Blow::Trade].map(|b| count(UnitKind::Shaman, b)), [7, 9, 0]);
        assert_eq!((0..16).filter_map(|r| blow_on_busy(r, false)).count(), 4);
    }

    #[test]
    fn a_duel_goes_on_until_one_falls_and_the_winner_is_free() {
        let (mut map, a, t) = duel(UnitKind::Brave, UnitKind::Brave);
        assert_eq!(map.coordinators.melees.len(), 1);
        let ring = map.coordinators.melees.iter().next().unwrap().1.at;
        let p = map.unit(a).map(|u| match u.action {
            Action::Melee { place, .. } => place,
            _ => unreachable!(),
        });
        assert!(p.is_some_and(|p| (RING_RADIUS - 1..=RING_RADIUS).contains(&distance(p, ring))));
        let mut struck = 0;
        for _ in 0..3000 {
            map.tick();
            if matches!(stance(&map, a), Some(Stance::Striking { .. })) || matches!(stance(&map, t), Some(Stance::Striking { .. })) {
                struck += 1;
            }
            if map.coordinators.melees.is_empty() {
                break;
            }
        }
        assert!(struck > 0, "blows were dealt");
        assert!(map.coordinators.melees.is_empty(), "over");
        let alive = [a, t].iter().filter(|&&u| map.unit(u).is_some_and(Unit::is_alive)).count();
        assert_eq!(alive, 1, "one fell");
        assert!([a, t].iter().filter_map(|&u| map.unit(u)).filter(|u| u.is_alive()).all(|u| u.action == Action::Idle));
    }

    #[test]
    fn a_clean_blow_hurts_only_the_one_struck() {
        let (map, a, t) = duel(UnitKind::Warrior, UnitKind::Brave);
        let melee = map.coordinators.melees.iter().next().unwrap().1.clone();
        let fighters: Vec<Fighter> = [a, t].iter().map(|&u| Fighter { stance: Stance::OnGuard, ..fighter(map.unit(u).unwrap()).unwrap() }).collect();
        let ring = vec![fighters[0]];
        for seed in 0..40 {
            let mut rng = Lcg(seed);
            let events = melee.exchange(&fighters, &ring, &mut rng);
            let damaged: Vec<UnitId> = events.iter().filter_map(|e| if let WorldEvent::Damage { unit, .. } = e { Some(*unit) } else { None }).collect();
            for e in &events {
                if let WorldEvent::Struck { attacker, target, blow, .. } = *e {
                    let hurt_back = damaged.contains(&attacker);
                    assert_eq!(hurt_back, blow != Blow::Clean, "{blow:?}");
                    assert!(damaged.contains(&target));
                }
            }
        }
    }

    /// `attacker` and `target` already locked in a challenge, `target`'s place.
    fn challenging(map: &mut GameMap, attacker: UnitId, target: UnitId) -> crate::challenge::ChallengeId {
        let id = map.new_id_of();
        map.coordinators.challenges.insert(id, crate::challenge::Challenge::new(attacker, target));
        let to = map.unit(target).map(|u| (u.x, u.z)).unwrap();
        map.unit_mut(attacker).unwrap().action = Action::Challenge { challenge: id, role: crate::challenge::Role::Charging { to, speed: 100 } };
        map.unit_mut(target).unwrap().action = Action::Challenge { challenge: id, role: crate::challenge::Role::Waiting };
        id
    }

    #[test]
    fn a_third_attacker_turns_an_ongoing_challenge_into_a_melee_at_once() {
        let mut map = GameMap::sandbox_units().seeded(5);
        let s = map.shaman_of(0).unwrap();
        let at = (s.x.wrapping_add(1024), s.z.wrapping_add(2048));
        let a = map.spawn(0, UnitKind::Brave, at).id;
        let t = map.spawn(1, UnitKind::Brave, (at.0.wrapping_add(100), at.1)).id;
        challenging(&mut map, a, t);
        let b = attacker(&mut map, 0, UnitKind::Brave, t);
        map.escalate_challenges();
        assert!(map.coordinators.challenges.is_empty(), "the challenge escalated into a melee at once");
        assert_eq!(map.coordinators.melees.len(), 1);
        assert!([a, t].iter().all(|&u| matches!(map.unit(u).unwrap().action, Action::Melee { .. })));
        map.join_melees();
        let ring = melee_of(&map, t);
        assert_eq!(melee_of(&map, b), ring, "the third joins the same turn, no standing around in reach");
    }

    #[test]
    fn a_distant_attacker_does_not_escalate_a_challenge() {
        let mut map = GameMap::sandbox_units().seeded(5);
        let s = map.shaman_of(0).unwrap();
        let at = (s.x.wrapping_add(1024), s.z.wrapping_add(2048));
        let a = map.spawn(0, UnitKind::Brave, at).id;
        let t = map.spawn(1, UnitKind::Brave, (at.0.wrapping_add(100), at.1)).id;
        let id = challenging(&mut map, a, t);
        let far = map.spawn(0, UnitKind::Brave, (at.0.wrapping_add(20 * 512), at.1)).id;
        map.unit_mut(far).unwrap().action = Action::Attacking { target: crate::unit::Target::Unit(t), to: (at.0.wrapping_add(20 * 512), at.1) };
        map.escalate_challenges();
        assert!(map.coordinators.challenges.contains(id), "out of reach: the challenge runs on");
        assert!(map.coordinators.melees.is_empty());
    }

    #[test]
    fn a_fighter_walked_far_away_leaves_and_fighters_heal() {
        let (mut map, a, t) = duel(UnitKind::Brave, UnitKind::Brave);
        map.unit_mut(a).unwrap().x = map.unit(a).unwrap().x.wrapping_add(10 * 512);
        map.tick();
        assert!(map.coordinators.melees.is_empty());
        assert_eq!(map.unit(t).unwrap().action, Action::Idle);
        let (map, a, t) = duel(UnitKind::Firewarrior, UnitKind::Brave);
        assert!([a, t].iter().all(|&u| map.unit(u).unwrap().heals()), "fighting, they heal"); 
    }

    #[test]
    fn a_fighter_pushed_out_of_leash_resumes_its_attack_order() {
        let (mut map, _, t) = duel(UnitKind::Brave, UnitKind::Brave);
        let b = attacker(&mut map, 0, UnitKind::Brave, t);
        map.join_melees();
        assert!(melee_of(&map, b).is_some(), "joined the melee from its own attack order");
        map.unit_mut(b).unwrap().x = map.unit(b).unwrap().x.wrapping_add(10 * 512);
        map.tick();
        assert_eq!(melee_of(&map, b), None, "farther than MELEE_LEASH: left");
        assert!(
            matches!(map.unit(b).unwrap().action, Action::Attacking { target: crate::unit::Target::Unit(id), .. } if id == t),
            "the attack order survived the fight and resumed, not idle: {:?}",
            map.unit(b).unwrap().action
        );
    }

    /// pop3-rev-analysis.md "Resuming a fight cut short by a push": a blast knocks the attacker clean out of its
    /// own melee and its own auto-scan square, yet it walks straight back to the same target once it is back on
    /// its feet - its own standing attack order, quietly surviving the fight, not a fresh one from spotting the
    /// defender again. The caster is a separate, uninvolved shaman: casting is its own order and would otherwise
    /// take a fighting shaman out of her own melee too (combat.md "Challenge"), a distraction from this test.
    #[test]
    fn a_warrior_blasted_out_of_sight_resumes_its_attack_order() {
        let mut map = GameMap::sandbox_walk().seeded(1);
        let defender = map.spawn(0, UnitKind::Brave, (map.shaman_of(0).unwrap().x.wrapping_add(1024), map.shaman_of(0).unwrap().z)).id;
        let attacker = map.spawn(1, UnitKind::Warrior, (map.unit(defender).unwrap().x.wrapping_add(512), map.unit(defender).unwrap().z)).id;
        map.apply(&Command::OrderUnit { player: 1, unit: attacker, order: Order::Attack { target: Target::Unit(defender) } });
        until(&mut map, 400, |m| matches!(m.unit(attacker).unwrap().action, Action::Melee { .. })).expect("in a melee");

        // Beyond the attacker, away from the defender along the same line: the push (away from `blast_at`)
        // carries the attacker further from its target, not back onto it.
        let blast_at = {
            let (defender, attacker) = (map.unit(defender).unwrap(), map.unit(attacker).unwrap());
            let (dx, dz) = (torus_delta(defender.x, attacker.x), torus_delta(defender.z, attacker.z));
            (attacker.x.wrapping_add(dx as u16), attacker.z.wrapping_add(dz as u16))
        };
        map.apply(&Command::Cast { player: 0, spell: crate::spell::Spell::Blast { at: blast_at } });
        map.run(crate::unit::CAST_RELEASE);
        until(&mut map, 300, |m| !matches!(m.unit(attacker).unwrap().action, Action::Tumbling | Action::Melee { .. } | Action::Challenge { .. }))
            .expect("back on its feet");

        let (attacker_cell, defender_cell) = (map.unit(attacker).unwrap().cell(), map.unit(defender).unwrap().cell());
        let size = map.terrain.size() as i32;
        let seen = crate::spotting::square(attacker_cell, UnitKind::Warrior.stats().sight, size).any(|c| c == defender_cell);
        assert!(!seen, "blasted past its own auto-scan square: attacker {attacker_cell:?}, defender {defender_cell:?}");
        assert!(
            matches!(map.unit(attacker).unwrap().action, Action::Attacking { target: Target::Unit(id), .. } if id == defender),
            "the attack order survived the fight and resumed on its own, not idle: {:?}",
            map.unit(attacker).unwrap().action
        );
    }

    /// The resume is for an order the fight put on hold, not a reflex of being blasted: a warrior that was never
    /// given one (idle, never fought) has nothing to pick back up, same as the common case in pop3-rev-analysis.md
    /// "Resuming a fight cut short by a push" ("a target with no order of its own just goes idle instead").
    #[test]
    fn an_idle_warrior_blasted_away_has_nothing_to_resume() {
        let mut map = GameMap::sandbox_walk().seeded(1);
        let s = map.shaman_of(0).unwrap();
        let w = map.spawn(1, UnitKind::Warrior, (s.x.wrapping_add(1024), s.z)).id;
        let blast_at = { let u = map.unit(w).unwrap(); (u.x.wrapping_add(512), u.z) };
        map.apply(&Command::Cast { player: 0, spell: crate::spell::Spell::Blast { at: blast_at } });
        map.run(crate::unit::CAST_RELEASE);
        until(&mut map, 300, |m| !matches!(m.unit(w).unwrap().action, Action::Tumbling)).expect("back on its feet");
        assert_eq!(map.unit(w).unwrap().action, Action::Idle, "nothing to resume: stays idle, not Attacking anyone");
    }

    /// `kind` of `tribe` going for `target`, standing next to it.
    fn attacker(map: &mut GameMap, tribe: u8, kind: UnitKind, target: UnitId) -> UnitId {
        let t = map.unit(target).unwrap();
        let at = (t.x.wrapping_add(100), t.z.wrapping_add(50));
        let u = map.spawn(tribe, kind, at);
        u.action = Action::Attacking { target: crate::unit::Target::Unit(target), to: at };
        u.id
    }

    fn melee_of(map: &GameMap, u: UnitId) -> Option<MeleeId> {
        map.unit(u).and_then(<Melee as Coordinator>::link_of)
    }

    #[test]
    fn a_newcomer_joins_but_never_a_firewarrior() {
        let (mut map, a, t) = duel(UnitKind::Brave, UnitKind::Brave);
        let ring = melee_of(&map, t);
        let fire = attacker(&mut map, 0, UnitKind::Firewarrior, t);
        let brave = attacker(&mut map, 0, UnitKind::Brave, t);
        map.join_melees();
        assert_eq!(melee_of(&map, fire), None, "a firewarrior never joins");
        assert_eq!((melee_of(&map, brave), melee_of(&map, a)), (ring, ring), "1 against 2");
    }

    #[test]
    fn two_against_two_splits_in_two_duels() {
        let (mut map, a, t) = duel(UnitKind::Brave, UnitKind::Brave);
        let b = attacker(&mut map, 0, UnitKind::Brave, t);
        map.join_melees();
        let c = attacker(&mut map, 1, UnitKind::Brave, a);
        map.join_melees();
        assert_eq!(map.coordinators.melees.len(), 2);
        let per: Vec<usize> = map.coordinators.melees.ids().map(|id| [a, t, b, c].iter().filter(|&&u| melee_of(&map, u) == Some(id)).count()).collect();
        assert_eq!(per, [2, 2]);
    }

    #[test]
    fn a_fourth_of_a_tribe_replaces_one_of_lower_rank() {
        let (mut map, _, t) = duel(UnitKind::Brave, UnitKind::Brave);
        let ring = melee_of(&map, t);
        for _ in 0..2 {
            attacker(&mut map, 0, UnitKind::Brave, t);
            map.join_melees();
        }
        let in_ring = |map: &GameMap| map.units.iter().filter(|u| melee_of(map, u.id) == ring).count();
        assert_eq!(in_ring(&map), 4, "3 braves against 1");
        let brave = attacker(&mut map, 0, UnitKind::Brave, t);
        map.join_melees();
        assert_eq!(melee_of(&map, brave), None, "no one of lower rank to replace");
        let warrior = attacker(&mut map, 0, UnitKind::Warrior, t);
        map.join_melees();
        assert_eq!((melee_of(&map, warrior), in_ring(&map)), (ring, 4), "a brave gave way");
    }

    #[test]
    fn an_attacked_preacher_strikes_back() {
        let (mut map, _, preacher) = duel(UnitKind::Brave, UnitKind::Preacher);
        let struck = (0..400).any(|_| {
            map.tick();
            matches!(stance(&map, preacher), Some(Stance::Striking { .. }))
        });
        assert!(struck, "it defends itself like anyone");
    }

    #[test]
    fn a_firewarrior_strikes_twice_as_long() {
        assert_eq!(move_turns(UnitKind::Firewarrior, Move::Strike(Blow::Knock)), 2 * move_turns(UnitKind::Brave, Move::Strike(Blow::Knock)));
    }

    #[test]
    fn the_same_seed_gives_the_same_melee() {
        let run = || {
            let (mut map, _, _) = duel(UnitKind::Brave, UnitKind::Warrior);
            map.run(Ticks::new(200));
            (map.units, map.coordinators, map.rng)
        };
        assert_eq!(run(), run());
    }
}
