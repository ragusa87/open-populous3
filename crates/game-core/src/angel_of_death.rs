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
//! - The angel-vs-angel exchange (`fight`) is our own reading of "both sides always take damage, scaled by the
//!   stronger side's own health, so the stronger snowballs": both lose a quarter of the winner's current health
//!   each clash, `FIGHT_COOLDOWN` apart — `[ours]`, no literal formula was traced.
//! - The firewarrior-bolt ~10% damage factor (confirmed, `SW_BLAST_DAMAGE_AOD`) and the literal kill-count/
//!   duration expiry (`AOD_KILL_COUNT`/`AOD_DURATION`, never located in the trace) are both left unimplemented:
//!   `fights: false` (kind_stats.rs) already keeps it out of the ordinary attack/spotting pipeline entirely, the
//!   simplest reading of "cannot be selected or given orders" extended to "cannot be ordered to attack it
//!   either" — revisit if a future session resolves either open question.
//! - Whether it can attack a building at all, or grab someone riding in a vehicle: left undone, same as the trace.
//! - No sprite of its own yet (`UnitKind::AngelOfDeath` is deliberately left out of `UnitKind::ALL`, so nothing
//!   in the boot job or the generated-figure fallback ever tries to draw one): simulated, not drawn.

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
/// `[ours]`: how close it gets before grabbing a victim, or engaging a rival angel, world units.
pub const GRAB_RANGE: i32 = 200;
pub const FIGHT_RANGE: i32 = 312;
/// `[ours]`: its own flight speed (`UnitKind::AngelOfDeath::speed`'s single source), world units/tick.
pub const SPEED: i32 = 150;
/// `[ours]`: how high above the ground it cruises, and the least it ever clears it by, terrain height units.
pub const CRUISE_HEIGHT: i32 = 600;
pub const MIN_CLEARANCE: i32 = 100;
/// Forces panic on any eligible enemy this close, every tick (pop3-rev-analysis.md: "area panic... every tick").
pub const PANIC_RADIUS_CELLS: i32 = 8;
/// `[ours]`: knockback distance after an angel-vs-angel exchange, world units.
pub const FIGHT_KNOCKBACK: i32 = 800;
/// `[ours]`: how long between exchanges (the trace's own "~11.5 s" stagger/reel).
pub const FIGHT_COOLDOWN: Ticks = Ticks::millis(11_500);
/// `[ours]`: a quarter of the winner's own current health, each exchange.
pub const FIGHT_DAMAGE_DIVISOR: u16 = 4;

/// An Angel of Death's own flight (`Unit::flight`): never through the generic `Motion`/physics (no gravity, no
/// touchdown — it never lands).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flight {
    pub heading: Angle,
    pub height: i32,
}

/// The spell's own cast sequence: the meteor falling, invisible throughout. A one-shot controller, not a
/// standing effect — `FALL_TICKS` long, firing everything on its last turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cast {
    turn: u32,
}

/// Cast by tribe `owner` at `at` (world units).
pub fn cast(owner: impl Into<Owner>, at: (u16, u16)) -> Effect {
    Effect::new(owner, at, Countdown::new(FALL_TICKS), EffectKind::AngelOfDeath(Cast::default()))
}

impl Cast {
    /// One turn of the fall; on the last one, the meteor crosses the ground: its own shockwave and spawn blast
    /// start, and the real angel appears.
    pub fn turn(&mut self, at: (u16, u16), owner: Owner) -> Vec<WorldEvent> {
        let events = if self.turn + 1 == FALL_TICKS.get() {
            vec![WorldEvent::SpawnEffect(shockwave(owner, at)), WorldEvent::SpawnEffect(crate::blast::effect(owner, at)), WorldEvent::SpawnAngelOfDeath { owner, at }]
        } else {
            Vec::new()
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
        let mut flight = self.units[i].flight.unwrap_or(Flight { heading: Angle::ZERO, height: ground + CRUISE_HEIGHT });
        flight.height = flight.height.max(ground + MIN_CLEARANCE);

        let mut events = self.angel_panic(pos, owner);

        match self.units[i].action {
            Action::Hunting => {
                if let Some(target) = self.angel_target(id, owner) {
                    self.units[i].action = Action::Closing { target };
                }
            }
            Action::Closing { target } => self.tend_closing(i, id, owner, &mut flight, target),
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
            Action::Fighting { rival, mut cooldown } => {
                if cooldown.tick() {
                    events.extend(self.angel_fight(i, rival));
                } else {
                    self.units[i].action = Action::Fighting { rival, cooldown };
                }
            }
            _ => {}
        }
        self.units[i].flight = Some(flight);
        events
    }

    /// Flies towards `target`; grabs it (a plain enemy) or starts a fight (a rival angel) once in range; gives up
    /// if it is no longer eligible.
    fn tend_closing(&mut self, i: usize, id: UnitId, owner: Owner, flight: &mut Flight, target: UnitId) {
        let Some(t) = self.unit(target).filter(|t| angel_target_eligible(owner, t)) else {
            self.units[i].action = Action::Hunting;
            return;
        };
        let (tx, tz, rival_angel) = (t.x, t.z, t.kind == UnitKind::AngelOfDeath);
        let pos = (self.units[i].x, self.units[i].z);
        let (dx, dz) = (torus_delta(pos.0, tx), torus_delta(pos.1, tz));
        let range = if rival_angel { FIGHT_RANGE } else { GRAB_RANGE };
        if (dx * dx + dz * dz) as i64 <= (range as i64) * (range as i64) {
            if rival_angel {
                self.units[i].action = Action::Fighting { rival: target, cooldown: Countdown::new(Ticks::ZERO) };
            } else if let Some(vi) = self.unit_index(target) {
                self.units[vi].action = Action::Grabbed { by: id };
                self.units[i].action = Action::Carrying { victim: target, left: Countdown::new(CARRY_TICKS) };
            }
            return;
        }
        flight.heading = angle::atan2(dz, dx);
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
    /// sting uses (shamans and angels, TODO.md "Units").
    fn angel_panic(&self, pos: (u16, u16), owner: Owner) -> Vec<WorldEvent> {
        let r2 = (PANIC_RADIUS_CELLS as i64 * WORLD_UNITS_PER_CELL as i64).pow(2);
        self.units
            .iter()
            .filter(|u| u.is_alive() && !matches!(u.kind, UnitKind::Shaman | UnitKind::AngelOfDeath) && u.owner != owner && !self.tribes.allied(owner, u.owner) && torus_dist2(pos, (u.x, u.z)) <= r2)
            .map(|u| WorldEvent::Panic { unit: u.id })
            .collect()
    }

    /// One exchange between angel `i` and its rival `rival`: both take a quarter of the stronger one's own
    /// current health, the weaker knocked back `FIGHT_KNOCKBACK` along the stronger's facing; both re-armed with
    /// a fresh `FIGHT_COOLDOWN` (resolved from whichever side processes first this tick: see `fight`'s own
    /// comment for why this never double-applies).
    fn angel_fight(&mut self, i: usize, rival: UnitId) -> Vec<WorldEvent> {
        let Some(ri) = self.unit_index(rival) else {
            self.units[i].action = Action::Hunting;
            return Vec::new();
        };
        let id = self.units[i].id;
        let (my_health, rival_health) = (self.units[i].health.current(), self.units[ri].health.current());
        let (winner, loser) = if my_health >= rival_health { (i, ri) } else { (ri, i) };
        let hit = self.units[winner].health.current() / FIGHT_DAMAGE_DIVISOR;
        let (wp, lp) = ((self.units[winner].x, self.units[winner].z), (self.units[loser].x, self.units[loser].z));
        let heading = angle::atan2(torus_delta(wp.1, lp.1), torus_delta(wp.0, lp.0));
        let (kx, kz) = angle::rotate((FIGHT_KNOCKBACK, 0), heading.steps());
        (self.units[loser].x, self.units[loser].z) = (lp.0.wrapping_add(kx as u16), lp.1.wrapping_add(kz as u16));
        (self.units[i].action, self.units[ri].action) =
            (Action::Fighting { rival, cooldown: Countdown::new(FIGHT_COOLDOWN) }, Action::Fighting { rival: id, cooldown: Countdown::new(FIGHT_COOLDOWN) });
        vec![
            WorldEvent::Damage { unit: self.units[i].id, amount: hit, kind: DamageKind::AngelOfDeath, from: Some(self.units[ri].owner) },
            WorldEvent::Damage { unit: self.units[ri].id, amount: hit, kind: DamageKind::AngelOfDeath, from: Some(self.units[i].owner) },
        ]
    }
}

/// Whether `t` is a valid Angel of Death target (rival or victim) for `owner`: alive, not its own tribe nor an
/// ally, and — unless it is itself a rival angel — not already grabbed by someone else.
fn angel_target_eligible(owner: Owner, t: &Unit) -> bool {
    t.is_alive() && t.owner != owner && (t.kind == UnitKind::AngelOfDeath || !matches!(t.action, Action::Grabbed { .. }))
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
        u.flight = Some(Flight { heading: Angle::ZERO, height: 2000 });
        id
    }

    #[test]
    fn the_fall_ripples_the_ground_and_spawns_the_real_angel_and_a_blast() {
        let mut map = bare();
        map.add_effect(cast(TribeId::Blue, AT));
        map.run(Ticks::new(FALL_TICKS.get() - 1));
        assert!(!map.units.iter().any(|u| u.kind == UnitKind::AngelOfDeath), "not yet: still falling");
        map.tick();
        let angel = map.units.iter().find(|u| u.kind == UnitKind::AngelOfDeath).expect("the angel appears");
        assert_eq!(angel.owner, Owner::Tribe(TribeId::Blue));
        assert_eq!(angel.action, Action::Hunting);
        assert!(angel.flight.is_some(), "already flying, not on the ground");
        assert!(map.effects.entries().any(|(_, e)| matches!(e.kind, EffectKind::AngelOfDeathShockwave(_))), "the ground ripple");
        assert!(map.effects.entries().any(|(_, e)| matches!(e.kind, EffectKind::SimpleBlast(_))), "the spawn-moment blast");
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

    #[test]
    fn two_rival_angels_fight_by_health_each_taking_damage_and_being_knocked_apart() {
        let mut map = bare();
        let a = spawn_angel(&mut map, TribeId::Blue, AT);
        let near = (AT.0.wrapping_add(FIGHT_RANGE as u16 / 2), AT.1);
        let b = spawn_angel(&mut map, TribeId::Red, near);
        let (full_a, full_b) = (map.unit(a).unwrap().health.current(), map.unit(b).unwrap().health.current());
        for _ in 0..200 {
            map.tick();
            if matches!(map.unit(a).unwrap().action, Action::Fighting { .. }) {
                break;
            }
        }
        assert!(matches!(map.unit(a).unwrap().action, Action::Fighting { .. }), "locked onto each other, not grabbing");
        map.run(Ticks::millis(11_500) + Ticks::new(1));
        assert!(map.unit(a).unwrap().health.current() < full_a, "the exchange hurt both");
        assert!(map.unit(b).unwrap().health.current() < full_b);
        assert_ne!((map.unit(a).unwrap().x, map.unit(a).unwrap().z), (map.unit(b).unwrap().x, map.unit(b).unwrap().z), "knocked apart");
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
