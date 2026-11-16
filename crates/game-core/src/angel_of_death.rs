//! The Angel of Death (pop3-rev-analysis.md "Angel of Death mechanics", "Per spell" row 13, model 13). Cast like
//! any other spell: an invisible meteor falls (`Cast`, `FALL_TICKS`), and the instant it crosses the ground it
//! triggers a real terrain ripple (`Shockwave`, `apply_shockwave`) and a plain `blast::effect` at the same point
//! (the same 140/98 push a building's own destruction and Lightning share) — then the real creature
//! (`unit::UnitKind::AngelOfDeath`, person state 28 in the original) appears and runs entirely on its own from
//! there (`GameMap::tend_angels_of_death`), never through `Command`: nobody ever selects or orders it
//! (`Action::can_take_orders`'s own exclusion list keeps it, and whoever it grabs, out of reach).
//!
//! Simplified from the trace, tagged here rather than silently guessed at:
//! - Hunting is immediate and unconditional: it locks onto the nearest rival angel if any, else the nearest
//!   eligible enemy anywhere on the map. No "engagement roll"/alert-flag sound cue, no tower-housed-target
//!   standoff distance — `[ours]`: one `GRAB_RANGE`/`FIGHT_RANGE` regardless of what the target stands in or near.
//! - Flight is a straight line at a constant `SPEED`: no "self speed x2 closing in", no zigzag while carrying, no
//!   180-degree wind-up turn before release, no victim-facing throw direction — `[ours]`. The victim is simply
//!   killed where it hangs once `CARRY_TICKS` run out.
//! - `tend_closing`'s own "once close enough to grab" check (`GRAB_RANGE`) is `x`/`z` distance alone: `flight.height`
//!   is never factored into it, nor deliberately lowered towards a grounded victim (only ever clamped to a
//!   *minimum* clearance above terrain, `MIN_CLEARANCE`) — so it grabs someone on the ground from `CRUISE_HEIGHT`
//!   without ever visibly diving down to reach them, user-reported playing it. The trace doesn't resolve this
//!   either way (only the separate "launching/accelerating" climb pose is confirmed, not a literal dive-to-grab
//!   manoeuvre, pop3-rev-analysis.md "Angel of Death mechanics" "Visuals"), so whether the original actually
//!   swoops down before a grab is itself an open question for a future trace session — `[ours]` until then.
//! - The angel-vs-angel exchange (`fight`) is our own reading of "both sides always take damage, scaled by the
//!   stronger side's own health, so the stronger snowballs": both lose a quarter of the winner's current health
//!   each clash, the loser reeling `FIGHT_STAGGER` — `[ours]`, no literal formula was traced.
//! - The firewarrior-bolt ~10% damage factor (confirmed, `SW_BLAST_DAMAGE_AOD`) and the literal kill-count/
//!   duration expiry (`AOD_KILL_COUNT`/`AOD_DURATION`, never located in the trace) are both left unimplemented:
//!   `fights: false` (kind_stats.rs) already keeps it out of the ordinary attack/spotting pipeline entirely, the
//!   simplest reading of "cannot be selected or given orders" extended to "cannot be ordered to attack it
//!   either" — revisit if a future session resolves either open question.
//! - Whether it can attack a building at all, or grab someone riding in a vehicle: left undone, same as the trace.
//! - Still no 2D unit sprite (`UnitKind::AngelOfDeath` is deliberately left out of `UnitKind::ALL`, so nothing in
//!   the boot job or the generated-figure fallback ever tries to draw one): it is drawn instead as a real 3D
//!   object-bank mesh, like a vehicle (`game-client`'s own `angel_of_death.rs`, pop3-rev-analysis.md "Angel of
//!   Death mechanics" "Visuals").

use crate::angle::{self, Angle};
use crate::effect::{DamageKind, Effect, EffectKind};
use crate::event::WorldEvent;
use crate::map::{torus_dist2, GameMap};
use crate::terrain::Heightmap;
use crate::time::{Countdown, Ticks};
use crate::tribes::Owner;
use crate::unit::{torus_delta, Action, Unit, UnitId, UnitKind};
use pop3_format::WORLD_UNITS_PER_CELL;

/// The invisible meteor's own fall: height 2048, -256 world units/tick (pop3-rev-analysis.md), so 8 ticks.
pub const FALL_TICKS: Ticks = Ticks::new(8);
/// The ground shockwave: a 9-tick (0.75 s) outward ring, amplitude 64 down to 0 in steps of 8.
pub const RIPPLE_TICKS: usize = 9;
pub const RIPPLE_AMPLITUDES: [i32; RIPPLE_TICKS] = [64, 56, 48, 40, 32, 24, 16, 8, 0];
/// How long it carries a grabbed victim (28 turns, 2.3 s, exe-confirmed).
pub const CARRY_TICKS: Ticks = Ticks::new(28);
/// How close it gets before grabbing a victim in the open, or one inside a building (measured to its
/// `grab_point`), and before engaging a rival angel, world units (pop3-rev-analysis.md "Angel of Death mechanics").
pub const GRAB_RANGE: i32 = 160;
pub const GRAB_RANGE_HOUSED: i32 = 200;
pub const FIGHT_RANGE: i32 = 312;
/// `[ours]`: the wear a building takes when an angel pulls someone out of it (blast's own `BLAST_DAMAGE_BLDG`).
pub const GRAB_BUILDING_WEAR: u16 = crate::blast::DAMAGE_BLDG;
/// `[ours]`: its own flight speed (`UnitKind::AngelOfDeath::speed`'s single source), world units/tick.
pub const SPEED: i32 = 150;
/// `[ours]`: how high above the ground it cruises, and the least it ever clears it by, terrain height units.
pub const CRUISE_HEIGHT: i32 = 600;
pub const MIN_CLEARANCE: i32 = 100;
/// Forces panic on any eligible enemy this close, every tick (pop3-rev-analysis.md: "area panic... every tick").
pub const PANIC_RADIUS_CELLS: i32 = 8;
/// `[ours]`: knockback distance after an angel-vs-angel exchange, world units.
pub const FIGHT_KNOCKBACK: i32 = 800;
/// How long the loser of a clash reels, neither moving nor striking (the trace's own "~11.5 s" stagger).
pub const FIGHT_STAGGER: Ticks = Ticks::millis(11_500);
/// `[ours]`: a quarter of the winner's own current health, each exchange.
pub const FIGHT_DAMAGE_DIVISOR: u16 = 4;

/// An Angel of Death's own flight: never through the generic `Motion`/physics (no gravity, no touchdown — it
/// never lands).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flight {
    pub heading: Angle,
    pub height: i32,
}

/// An Angel of Death's own state (`Unit::angel`): its flight, the kills it has made and how long it has left to
/// live (`balance::Angels`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Angel {
    pub flight: Flight,
    pub kills: u8,
    pub left: Countdown,
}

impl Angel {
    /// A fresh angel over ground at `ground`, cruising, to live `duration`.
    pub fn new(ground: i32, duration: crate::balance::Turns) -> Angel {
        Angel { flight: Flight { heading: Angle::ZERO, height: ground + CRUISE_HEIGHT }, kills: 0, left: Countdown::new(Ticks::new(duration.0)) }
    }
}

/// The spell's own cast sequence: the meteor falling, invisible throughout. A one-shot controller, not a
/// standing effect — `FALL_TICKS` long, firing everything on its last turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cast {
    turn: u32,
}

/// Cast by tribe `owner` at `at` (world units): the fall, then one more turn for the angel to appear.
pub fn cast(owner: impl Into<Owner>, at: (u16, u16)) -> Effect {
    Effect::new(owner, at, Countdown::new(FALL_TICKS + Ticks::new(1)), EffectKind::AngelOfDeath(Cast::default()))
}

impl Cast {
    /// One turn of the fall. On its last one the meteor crosses the ground and starts the shockwave; the turn after,
    /// once the meteor is gone, the real angel appears (`create_aod`), bringing its own spawn blast with it
    /// (`WorldEvent::SpawnAngelOfDeath`'s handler).
    pub fn turn(&mut self, at: (u16, u16), owner: Owner) -> Vec<WorldEvent> {
        let events = match self.turn + 1 {
            t if t == FALL_TICKS.get() => vec![WorldEvent::SpawnEffect(shockwave(owner, at))],
            t if t == FALL_TICKS.get() + 1 => vec![WorldEvent::SpawnAngelOfDeath { owner, at }],
            _ => Vec::new(),
        };
        self.turn += 1;
        events
    }
}

/// One ground shockwave, rippling outward from `at` for `RIPPLE_TICKS`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shockwave {
    turn: u32,
}

fn shockwave(owner: impl Into<Owner>, at: (u16, u16)) -> Effect {
    Effect::new(owner, at, Countdown::new(Ticks::new(RIPPLE_TICKS as u32)), EffectKind::AngelOfDeathShockwave(Shockwave::default()))
}

impl Shockwave {
    pub fn turn(&mut self, at: (u16, u16)) -> Vec<WorldEvent> {
        let cell = (crate::unit::cell_of(at.0), crate::unit::cell_of(at.1));
        let events = vec![WorldEvent::Terrain(crate::effect::TerrainChange::AngelOfDeathShockwave { at: cell, turn: self.turn })];
        self.turn += 1;
        events
    }
}

/// The ring `radius` cells out from `centre` (a discretized circle, wrapped): every cell whose rounded distance
/// from `centre` is `radius` (the centre itself, for `radius` 0).
fn ring_cells(centre: (i32, i32), radius: u32) -> Vec<(i32, i32)> {
    if radius == 0 {
        return vec![centre];
    }
    let r = radius as i32;
    (-r..=r)
        .flat_map(|dz| (-r..=r).map(move |dx| (dx, dz)))
        .filter(|&(dx, dz)| (((dx * dx + dz * dz) as f32).sqrt().round() as i32) == r)
        .map(|(dx, dz)| (centre.0 + dx, centre.1 + dz))
        .collect()
}

/// One tick of the shockwave: the ring at `turn` cells out rises by `RIPPLE_AMPLITUDES[turn]`, the one at
/// `turn - 1` (if any) settles back down by its own turn's amplitude — the two additive deltas exactly cancel
/// out once both have played, no snapshot needed.
pub fn apply_shockwave(terrain: &mut Heightmap, at: (i32, i32), turn: u32) -> crate::terrain::DirtyRect {
    let mut touched = Vec::new();
    if turn > 0 && let Some(&was) = RIPPLE_AMPLITUDES.get(turn as usize - 1) {
        for (x, z) in ring_cells(at, turn - 1) {
            terrain.set(x, z, (terrain.get(x, z) as i32 - was).max(0) as u16);
            touched.push((x, z));
        }
    }
    if let Some(&now) = RIPPLE_AMPLITUDES.get(turn as usize) {
        for (x, z) in ring_cells(at, turn) {
            terrain.set(x, z, (terrain.get(x, z) as i32 + now).clamp(0, crate::terrain::MAX_HEIGHT as i32) as u16);
            touched.push((x, z));
        }
    }
    crate::terrain::DirtyRect::bounding(touched)
}

impl GameMap {
    /// `GIVE_UP_AND_SULK`'s sweep (`0x41CA10`): every living Angel of Death of `tribe` has its life
    /// rerolled down to a random 32-95 (out of its full life, `balance.units.life`) — crippling, not
    /// killing (ai-scripts.md "Other ON/OFF switches", confirmed).
    pub(crate) fn ai_cripple_angels_of_death(&mut self, tribe: crate::tribes::TribeId) {
        const MIN: u32 = 32;
        const MAX: u32 = 95;
        let ids: Vec<UnitId> = self.units.iter().filter(|u| u.kind == UnitKind::AngelOfDeath && u.is_alive() && u.owner == tribe).map(|u| u.id).collect();
        for id in ids {
            let roll = MIN + self.rng.next_u32() % (MAX - MIN + 1);
            if let Some(u) = self.unit_mut(id) {
                u.health = crate::health::Health::new(roll as u16, u.health.max());
            }
        }
    }

    /// Every living Angel of Death, on its own: hunts (a rival angel first, else the nearest eligible enemy),
    /// closes in, grabs or fights, and panics anyone eligible nearby — all independent of `Command`.
    pub(crate) fn tend_angels_of_death(&mut self) {
        let ids: Vec<UnitId> = self.units.iter().filter(|u| u.kind == UnitKind::AngelOfDeath && u.is_alive()).map(|u| u.id).collect();
        let mut events = Vec::new();
        for id in ids {
            events.extend(self.tend_one_angel(id));
        }
        self.apply_events(events);
    }

    fn tend_one_angel(&mut self, id: UnitId) -> Vec<WorldEvent> {
        let Some(i) = self.unit_index(id) else { return Vec::new() };
        let (owner, pos) = (self.units[i].owner, (self.units[i].x, self.units[i].z));
        let ground = self.terrain.height_at(pos.0 as u32, pos.1 as u32, WORLD_UNITS_PER_CELL);
        let Some(mut angel) = self.units[i].angel else { return Vec::new() };
        let mut flight = angel.flight;
        flight.height = flight.height.max(ground + MIN_CLEARANCE);

        let mut events = self.angel_panic(pos, owner);

        match self.units[i].action {
            Action::Hunting => {
                if let Some(target) = self.angel_target(id, owner) {
                    self.units[i].action = Action::Closing { target };
                }
            }
            Action::Closing { target } => events.extend(self.tend_closing(i, id, owner, &mut flight, target)),
            Action::Carrying { victim, mut left } => {
                if let Some(vi) = self.unit_index(victim) {
                    (self.units[vi].x, self.units[vi].z) = (self.units[i].x, self.units[i].z);
                }
                if left.tick() {
                    events.push(WorldEvent::Damage { unit: victim, amount: u16::MAX, kind: DamageKind::AngelOfDeath, from: Some(owner) });
                    self.units[i].action = Action::Hunting;
                } else {
                    self.units[i].action = Action::Carrying { victim, left };
                }
            }
            Action::Fighting { rival, mut stagger } => {
                if !stagger.tick() {
                    self.units[i].action = Action::Fighting { rival, stagger };
                } else if let Some(r) = self.unit(rival).filter(|r| r.is_alive()).map(|r| (r.x, r.z)) {
                    if torus_dist2(pos, r) <= (FIGHT_RANGE as i64).pow(2) {
                        events.extend(self.angel_fight(i, rival));
                    } else {
                        self.fly_towards(i, &mut flight, r);
                    }
                } else {
                    self.units[i].action = Action::Hunting;
                }
            }
            _ => {}
        }
        angel.flight = flight;
        self.units[i].angel = Some(angel);
        events
    }

    /// Flies towards `target` (its `grab_point`); grabs it (a plain enemy) or starts a fight (a rival angel) once
    /// in range; gives up if it is no longer eligible.
    fn tend_closing(&mut self, i: usize, id: UnitId, owner: Owner, flight: &mut Flight, target: UnitId) -> Vec<WorldEvent> {
        let Some(t) = self.unit(target).filter(|t| angel_target_eligible(owner, t)) else {
            self.units[i].action = Action::Hunting;
            return Vec::new();
        };
        let (aim, housed) = self.grab_point(t);
        let rival_angel = t.kind == UnitKind::AngelOfDeath;
        let range = match (rival_angel, housed) {
            (true, _) => FIGHT_RANGE,
            (false, Some(_)) => GRAB_RANGE_HOUSED,
            (false, None) => GRAB_RANGE,
        };
        if torus_dist2((self.units[i].x, self.units[i].z), aim) > (range as i64).pow(2) {
            self.fly_towards(i, flight, aim);
            return Vec::new();
        }
        if rival_angel {
            self.units[i].action = Action::Fighting { rival: target, stagger: Countdown::new(Ticks::ZERO) };
            return Vec::new();
        }
        self.units[i].action = Action::Carrying { victim: target, left: Countdown::new(CARRY_TICKS) };
        self.grab(id, target, aim, housed)
    }

    /// Where an angel grabs `t`, and the building it is pulled out of, if any: a unit in a drum tower where it
    /// stands at its post, one in any other building at the building's centre — never by the door (user-described,
    /// `[ours]`: the trace found no building path) — anyone else where it is.
    fn grab_point(&self, t: &Unit) -> ((u16, u16), Option<crate::building::BuildingId>) {
        match t.inside.and_then(|inside| self.building(inside.building)) {
            Some(b) if b.kind == crate::building::BuildingKind::DrumTower => ((t.x, t.z), Some(b.id)),
            Some(b) => (b.centre(), Some(b.id)),
            None => ((t.x, t.z), None),
        }
    }

    /// Angel `by` takes `victim` at `at`: out of its building there (which takes `GRAB_BUILDING_WEAR`) or out of
    /// its vehicle, held from now on (pop3-rev-analysis.md: state 40's ENTER case evicts a housed victim).
    fn grab(&mut self, by: UnitId, victim: UnitId, at: (u16, u16), housed: Option<crate::building::BuildingId>) -> Vec<WorldEvent> {
        if let Some(vehicle) = self.unit(victim).and_then(|v| v.aboard) {
            self.evict(vehicle, &[victim]);
        }
        let Some(v) = self.unit_mut(victim) else { return Vec::new() };
        v.inside = None;
        (v.x, v.z) = at;
        v.action = Action::Grabbed { by };
        housed.map(|building| WorldEvent::Wear { building, amount: GRAB_BUILDING_WEAR }).into_iter().collect()
    }

    /// One tick of flight of angel `i` straight at `to`.
    fn fly_towards(&mut self, i: usize, flight: &mut Flight, to: (u16, u16)) {
        let pos = (self.units[i].x, self.units[i].z);
        flight.heading = angle::atan2(torus_delta(pos.1, to.1), torus_delta(pos.0, to.0));
        let (mx, mz) = angle::rotate((SPEED, 0), flight.heading.steps());
        (self.units[i].x, self.units[i].z) = (pos.0.wrapping_add(mx as u16), pos.1.wrapping_add(mz as u16));
    }

    /// The rival angel nearest `id` if any, else the nearest other eligible enemy anywhere on the map
    /// (pop3-rev-analysis.md: "hunts a rival angel first").
    fn angel_target(&self, id: UnitId, owner: Owner) -> Option<UnitId> {
        let pos = self.unit(id).map(|u| (u.x, u.z))?;
        let nearest = |rival_only: bool| {
            self.units
                .iter()
                .filter(|u| u.id != id && angel_target_eligible(owner, u) && (u.kind == UnitKind::AngelOfDeath) == rival_only)
                .min_by_key(|u| torus_dist2(pos, (u.x, u.z)))
                .map(|u| u.id)
        };
        nearest(true).or_else(|| nearest(false))
    }

    /// Any eligible enemy within `PANIC_RADIUS_CELLS` of `pos` panics, independent of whatever it is targeting
    /// (pop3-rev-analysis.md: "every tick, regardless of... area panic"); same exclusions Insect Plague's own
    /// sting uses (shamans and angels, TODO.md "Units"). Only people in the open: someone inside a building or
    /// aboard a vehicle stays put, to be pulled out by the grab itself (`grab_point`) — `[ours]`, the trace doesn't
    /// say whether the original's panic reaches them.
    fn angel_panic(&self, pos: (u16, u16), owner: Owner) -> Vec<WorldEvent> {
        let r2 = (PANIC_RADIUS_CELLS as i64 * WORLD_UNITS_PER_CELL as i64).pow(2);
        self.units
            .iter()
            .filter(|u| u.is_alive() && u.in_the_open() && !matches!(u.kind, UnitKind::Shaman | UnitKind::AngelOfDeath) && u.owner != owner && !self.tribes.allied(owner, u.owner) && torus_dist2(pos, (u.x, u.z)) <= r2)
            .map(|u| WorldEvent::Panic { unit: u.id })
            .collect()
    }

    /// One exchange between angel `i` and its rival `rival`, both in range: both take a quarter of the stronger
    /// one's own current health; the weaker is knocked back `FIGHT_KNOCKBACK` along the stronger's own heading and
    /// reels for `FIGHT_STAGGER`, while the stronger is free to close in again at once. Whichever side processes
    /// first this tick resolves it: the knockback leaves the other out of range, so it never double-applies.
    fn angel_fight(&mut self, i: usize, rival: UnitId) -> Vec<WorldEvent> {
        let Some(ri) = self.unit_index(rival) else {
            self.units[i].action = Action::Hunting;
            return Vec::new();
        };
        let id = self.units[i].id;
        let (my_health, rival_health) = (self.units[i].health.current(), self.units[ri].health.current());
        let (winner, loser) = if my_health >= rival_health { (i, ri) } else { (ri, i) };
        let hit = self.units[winner].health.current() / FIGHT_DAMAGE_DIVISOR;
        let heading = self.units[winner].angel.map_or(Angle::ZERO, |a| a.flight.heading);
        let (kx, kz) = angle::rotate((FIGHT_KNOCKBACK, 0), heading.steps());
        let lp = (self.units[loser].x, self.units[loser].z);
        (self.units[loser].x, self.units[loser].z) = (lp.0.wrapping_add(kx as u16), lp.1.wrapping_add(kz as u16));
        let stagger = |side: usize| Countdown::new(if side == loser { FIGHT_STAGGER } else { Ticks::ZERO });
        (self.units[i].action, self.units[ri].action) = (Action::Fighting { rival, stagger: stagger(i) }, Action::Fighting { rival: id, stagger: stagger(ri) });
        vec![
            WorldEvent::Damage { unit: self.units[i].id, amount: hit, kind: DamageKind::AngelOfDeath, from: Some(self.units[ri].owner) },
            WorldEvent::Damage { unit: self.units[ri].id, amount: hit, kind: DamageKind::AngelOfDeath, from: Some(self.units[i].owner) },
        ]
    }
}

/// Whether `t` is a valid Angel of Death target (rival or victim) for `owner`: alive, not its own tribe nor an
/// ally, and — unless it is itself a rival angel — neither already grabbed by someone else nor being preached to
/// (state 23, skipped by the trace's own scan).
fn angel_target_eligible(owner: Owner, t: &Unit) -> bool {
    t.is_alive() && t.owner != owner && !t.fools(owner) && (t.kind == UnitKind::AngelOfDeath || !matches!(t.action, Action::Grabbed { .. } | Action::Listening { .. }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::spell::Spell;
    use crate::spell_book::{Availability, SpellKind};
    use crate::terrain::Heightmap;
    use crate::tribes::TribeId;
    use crate::unit::Action;

    fn bare() -> GameMap {
        let mut terrain = Heightmap::new(128);
        for z in 0..128 {
            for x in 0..128 {
                terrain.set(x, z, 500);
            }
        }
        GameMap::bare("angel-of-death-test", terrain, Vec::new())
    }

    const AT: (u16, u16) = (64 * WORLD_UNITS_PER_CELL as u16 + 256, 64 * WORLD_UNITS_PER_CELL as u16 + 256);

    /// An angel as `WorldEvent::SpawnAngelOfDeath`'s own handler (map.rs) makes one: hunting, already flying.
    fn spawn_angel(map: &mut GameMap, owner: TribeId, at: (u16, u16)) -> UnitId {
        let u = map.spawn(owner, UnitKind::AngelOfDeath, at);
        let id = u.id;
        u.action = Action::Hunting;
        u.angel = Some(Angel { flight: Flight { heading: Angle::ZERO, height: 2000 }, ..Angel::new(0, crate::balance::Balance::ours().angels.duration) });
        id
    }

    #[test]
    fn give_up_and_sulk_cripples_its_own_angels_only() {
        let mut map = bare().seeded(1);
        let mine = spawn_angel(&mut map, TribeId::Blue, AT);
        let theirs = spawn_angel(&mut map, TribeId::Red, (AT.0.wrapping_add(20), AT.1));
        let full = map.unit(mine).unwrap().health.max();
        map.ai_cripple_angels_of_death(TribeId::Blue);
        let crippled = map.unit(mine).unwrap().health.current();
        assert!((32..=95).contains(&crippled), "{crippled} out of range");
        assert_eq!(map.unit(theirs).unwrap().health.current(), full, "a different tribe's own angel is untouched");
    }

    #[test]
    fn the_fall_ripples_the_ground_then_the_angel_appears_in_its_own_blast() {
        let mut map = bare();
        map.add_effect(cast(TribeId::Blue, AT));
        map.run(FALL_TICKS);
        let has = |map: &GameMap, f: fn(&EffectKind) -> bool| map.effects.entries().any(|(_, e)| f(&e.kind));
        assert!(has(&map, |k| matches!(k, EffectKind::AngelOfDeathShockwave(_))), "the meteor crossed the ground: the ripple");
        assert!(!map.units.iter().any(|u| u.kind == UnitKind::AngelOfDeath), "not yet: the meteor is still there");
        assert!(!has(&map, |k| matches!(k, EffectKind::SimpleBlast(_))), "no blast before the angel");
        map.tick();
        let angel = map.units.iter().find(|u| u.kind == UnitKind::AngelOfDeath).expect("the angel appears");
        assert_eq!((angel.owner, angel.action), (Owner::Tribe(TribeId::Blue), Action::Hunting));
        assert!(angel.angel.is_some(), "already flying, not on the ground");
        let blast = map.effects.entries().find(|(_, e)| matches!(e.kind, EffectKind::SimpleBlast(_))).map(|(_, e)| (e.x, e.z));
        assert_eq!(blast, Some((angel.x, angel.z)), "its own blast, where it appeared");
    }

    #[test]
    fn the_ground_rises_then_settles_back_down_as_the_ripple_travels_out() {
        let mut map = bare();
        let cell = (crate::unit::cell_of(AT.0), crate::unit::cell_of(AT.1));
        let ground = map.terrain.get(cell.0, cell.1);
        map.add_effect(shockwave(TribeId::Blue, AT));
        map.tick();
        assert_eq!(map.terrain.get(cell.0, cell.1), ground + RIPPLE_AMPLITUDES[0] as u16, "the centre rises first");
        map.tick();
        assert_eq!(map.terrain.get(cell.0, cell.1), ground, "back down once the ring has moved past it");
        assert_eq!(map.terrain.get(cell.0 + 1, cell.1), ground + RIPPLE_AMPLITUDES[1] as u16, "the ring now one cell out");
        map.run(Ticks::new(RIPPLE_TICKS as u32));
        for d in 0..RIPPLE_TICKS as i32 {
            assert_eq!(map.terrain.get(cell.0 + d, cell.1), ground, "every ring settled back to flat once it's all over: {d}");
        }
    }

    #[test]
    fn casting_it_through_the_spell_book_adds_the_controller_effect() {
        let mut map = GameMap::sandbox_walk();
        map.spell_books[0].set(SpellKind::AngelOfDeath, Availability::Unlimited);
        let at = (map.units[0].x, map.units[0].z);
        map.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::AngelOfDeath { at } });
        map.run(crate::unit::CAST_RELEASE);
        assert!(map.effects.entries().any(|(_, e)| matches!(e.kind, EffectKind::AngelOfDeath(_))));
    }

    #[test]
    fn hunts_the_nearest_enemy_closes_in_and_grabs_it_then_releases_it_dead() {
        let mut map = bare();
        let angel = spawn_angel(&mut map, TribeId::Blue, AT);
        let far = (AT.0.wrapping_add(50 * WORLD_UNITS_PER_CELL as u16), AT.1);
        let victim = map.spawn(TribeId::Red, UnitKind::Brave, far).id;
        let own = map.spawn(TribeId::Blue, UnitKind::Brave, AT).id;
        for _ in 0..2000 {
            map.tick();
            if map.unit(victim).unwrap().action == (Action::Grabbed { by: angel }) {
                break;
            }
        }
        assert_eq!(map.unit(victim).unwrap().action, Action::Grabbed { by: angel }, "grabbed");
        assert!(matches!(map.unit(angel).unwrap().action, Action::Carrying { victim: v, .. } if v == victim));
        assert!(map.unit(own).unwrap().is_alive(), "its own tribe is never targeted");
        map.run(CARRY_TICKS);
        assert!(!map.unit(victim).unwrap().is_alive(), "released, dead");
        assert_eq!(map.unit(angel).unwrap().action, Action::Hunting, "goes back to hunting");
    }

    /// Runs until `victim` is grabbed by anyone, at most 2000 ticks.
    fn until_grabbed(map: &mut GameMap, victim: UnitId) -> bool {
        (0..2000).any(|_| {
            map.tick();
            matches!(map.unit(victim).unwrap().action, Action::Grabbed { .. })
        })
    }

    fn housed_in(kind: crate::building::BuildingKind) -> (GameMap, UnitId, crate::building::BuildingId) {
        use crate::building::Building;
        let mut map = bare();
        spawn_angel(&mut map, TribeId::Blue, AT);
        let corner = (AT.0.wrapping_add(20 * WORLD_UNITS_PER_CELL as u16), AT.1);
        let building = map.add_building(Building::new(kind, TribeId::Red, corner.0, corner.1, 0));
        let b = map.building(building).unwrap().clone();
        let post = crate::work::work_point(&b, crate::id::Id::from_raw(7));
        let victim = map.spawn(TribeId::Red, UnitKind::Brave, post).id;
        map.unit_mut(victim).unwrap().enter(crate::unit::Inside { building, door: b.door() }, post);
        (map, victim, building)
    }

    #[test]
    fn a_housed_victim_is_pulled_out_at_the_buildings_centre_which_takes_wear() {
        let (mut map, victim, building) = housed_in(crate::building::BuildingKind::Hut { size: 1 });
        let centre = map.building(building).unwrap().centre();
        assert!(until_grabbed(&mut map, victim));
        let v = map.unit(victim).unwrap();
        assert_eq!((v.inside, (v.x, v.z)), (None, centre), "out, at the centre, never by the door");
        assert!(map.building(building).unwrap().wear > 0, "the building took wear");
    }

    #[test]
    fn a_drum_tower_guard_is_grabbed_where_it_stands_at_its_post() {
        let (mut map, victim, _) = housed_in(crate::building::BuildingKind::DrumTower);
        let post = (map.unit(victim).unwrap().x, map.unit(victim).unwrap().z);
        assert!(until_grabbed(&mut map, victim));
        let v = map.unit(victim).unwrap();
        assert_eq!(v.inside, None);
        let angel = map.units.iter().find(|u| u.kind == UnitKind::AngelOfDeath).unwrap();
        assert!(torus_dist2(post, (angel.x, angel.z)) <= (GRAB_RANGE_HOUSED as i64).pow(2), "taken at its post");
    }

    #[test]
    fn a_passenger_is_taken_off_its_vehicle() {
        let mut map = bare();
        spawn_angel(&mut map, TribeId::Blue, AT);
        let at = (AT.0.wrapping_add(20 * WORLD_UNITS_PER_CELL as u16), AT.1);
        let vehicle = map.add_vehicle(crate::vehicle::Vehicle::new(crate::vehicle::VehicleKind::Balloon, TribeId::Red, at, Angle::ZERO));
        let victim = map.spawn(TribeId::Red, UnitKind::Brave, at).id;
        map.unit_mut(victim).unwrap().aboard = Some(vehicle);
        map.vehicles.get_mut(vehicle).unwrap().passengers.push(victim);
        assert!(until_grabbed(&mut map, victim));
        assert_eq!(map.unit(victim).unwrap().aboard, None);
        assert!(!map.vehicles.get(vehicle).unwrap().passengers.contains(&victim));
    }

    #[test]
    fn someone_being_preached_to_is_never_a_target() {
        let mut map = bare();
        let id = map.spawn(TribeId::Red, UnitKind::Brave, AT).id;
        let preacher = map.spawn(TribeId::Green, UnitKind::Preacher, AT).id;
        let owner = Owner::Tribe(TribeId::Blue);
        assert!(angel_target_eligible(owner, map.unit(id).unwrap()));
        map.unit_mut(id).unwrap().action = Action::Listening { preacher, sitting: Countdown::new(Ticks::new(5)), left: Countdown::new(Ticks::new(5)), attempts: 0 };
        assert!(!angel_target_eligible(owner, map.unit(id).unwrap()));
    }

    fn health(map: &GameMap, id: UnitId) -> u16 {
        map.unit(id).unwrap().health.current()
    }

    fn staggered(map: &GameMap, id: UnitId) -> bool {
        matches!(map.unit(id).unwrap().action, Action::Fighting { stagger, .. } if stagger.left() > Ticks::ZERO)
    }

    /// Two rival angels a little apart, `a` the stronger, run until their first clash.
    fn first_clash() -> (GameMap, UnitId, UnitId) {
        let mut map = bare();
        let a = spawn_angel(&mut map, TribeId::Blue, AT);
        let b = spawn_angel(&mut map, TribeId::Red, (AT.0.wrapping_add(FIGHT_RANGE as u16 / 2), AT.1));
        map.unit_mut(b).unwrap().health = crate::health::Health::new(5_000, 10_000);
        for _ in 0..10 {
            map.tick();
            if health(&map, a) < 10_000 {
                return (map, a, b);
            }
        }
        panic!("never clashed");
    }

    #[test]
    fn two_rival_angels_clash_by_health_both_hurt_the_weaker_knocked_away_and_reeling() {
        let (map, a, b) = first_clash();
        let hit = 10_000 / FIGHT_DAMAGE_DIVISOR;
        assert_eq!((health(&map, a), health(&map, b)), (10_000 - hit, 5_000 - hit), "both lose a quarter of the stronger's health");
        assert!(staggered(&map, b), "the loser reels");
        assert!(!staggered(&map, a), "the winner doesn't");
        let (pa, pb) = ((map.unit(a).unwrap().x, map.unit(a).unwrap().z), (map.unit(b).unwrap().x, map.unit(b).unwrap().z));
        assert!(torus_dist2(pa, pb) > (FIGHT_RANGE as i64).pow(2), "knocked out of range");
    }

    #[test]
    fn the_winner_closes_in_again_and_strikes_the_reeling_loser_who_never_moves() {
        let (mut map, _, b) = first_clash();
        let at = (map.unit(b).unwrap().x, map.unit(b).unwrap().z);
        let before = health(&map, b);
        map.tick();
        assert_eq!(health(&map, b), before, "no clash from out of range");
        for _ in 0..20 {
            assert_eq!((map.unit(b).unwrap().x, map.unit(b).unwrap().z), at, "the reeling loser never moves");
            map.tick();
            if health(&map, b) < before {
                break;
            }
        }
        assert!(health(&map, b) < before, "struck again once the winner closed in");
        assert!(FIGHT_STAGGER.get() > 20, "test setup: still reeling throughout");
    }

    #[test]
    fn a_rival_that_dies_ends_the_fight() {
        let (mut map, a, b) = first_clash();
        map.apply_events(vec![WorldEvent::Damage { unit: b, amount: u16::MAX, kind: DamageKind::Blast, from: None }]);
        map.tick();
        assert_eq!(map.unit(a).unwrap().action, Action::Hunting);
    }

    #[test]
    fn rival_angels_fight_to_the_death_the_weaker_dying() {
        let (mut map, a, b) = first_clash();
        for _ in 0..2000 {
            map.tick();
            if !map.unit(b).is_some_and(|u| u.is_alive()) {
                break;
            }
        }
        assert!(!map.unit(b).is_some_and(|u| u.is_alive()), "the weaker died");
        assert!(map.unit(a).unwrap().is_alive(), "the stronger survived");
    }

    #[test]
    fn forces_eligible_enemies_within_reach_to_panic_every_tick_sparing_shamans_angels_and_allies() {
        let mut map = bare();
        let angel = spawn_angel(&mut map, TribeId::Blue, AT);
        let enemy = map.spawn(TribeId::Red, UnitKind::Brave, AT).id;
        let shaman = map.spawn(TribeId::Red, UnitKind::Shaman, AT).id;
        let own = map.spawn(TribeId::Blue, UnitKind::Brave, AT).id;
        map.tick();
        assert_eq!(map.unit(enemy).unwrap().action.name(), "Panicking");
        for id in [shaman, own, angel] {
            assert_ne!(map.unit(id).unwrap().action.name(), "Panicking", "{id:?}");
        }
        // A rival angel is immune to panic too (same exclusion as a shaman) — checked on its own: it is itself
        // processed every tick (kind regardless of action), and would otherwise legitimately panic `own` as
        // Red's own enemy, which this assertion isn't about.
        let rival = spawn_angel(&mut map, TribeId::Red, AT);
        map.tick();
        assert_ne!(map.unit(rival).unwrap().action.name(), "Panicking");
    }
}
