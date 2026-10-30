//! Hypnotism (pop3-rev-analysis.md "Per spell" 7), an effect (`effect::EffectKind::Hypnotism`): aimed at a point
//! like a blast. On its one turn, the `HYPNO_NUM_PEOPLE` nearest people of other tribes in its 3 × 3 cells
//! (`nearest::in_block`, pop3-rev-analysis.md "Components" "Nearest-N selector") — not a wildman, not a shaman, not
//! a shielded one — join the caster's tribe for `status::HYPNO_STEPS` steps (`WorldEvent::Hypnotize`,
//! `GameMap::hypnotize`); one not already hypnotised sorts before one that is, regardless of distance
//! (`nearest::nearest_n_fresh_first`, the shared selector, pop3-rev-analysis.md 0x515E30): a cast reaches fresh
//! captives first, recapturing an already-hypnotised one only to fill what's left (confirmed, docs/specs/spells.md
//! "Magical Shield"). Once `HYPNO_STEPS` runs out, each one reverts to its own *original* tribe
//! (`GameMap::revert_expired_hypnosis`) — the one before its first capture, even through several recaptures by
//! different tribes in between (confirmed: recasting on an already-hypnotised unit changes who controls it and
//! restarts its timer, but never who it reverts to) — unless that tribe is wiped out by then, when the change
//! stays permanent (confirmed: "if their tribe is gone, they stay").

use crate::effect::{Effect, EffectKind};
use crate::event::WorldEvent;
use crate::map::GameMap;
use crate::nearest::{in_block, nearest_n_fresh_first};
use crate::status::HYPNO_NUM_PEOPLE;
use crate::time::{Countdown, Ticks};
use crate::unit::{Unit, UnitKind};

/// A single turn: the spell's whole effect happens at once, nothing lingers after.
const LIFETIME: Ticks = Ticks::new(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hypnotism;

/// Cast by tribe `owner` at `at` (world units).
pub fn effect(owner: u8, at: (u16, u16)) -> Effect {
    Effect::new(owner, at, Countdown::new(LIFETIME), EffectKind::Hypnotism(Hypnotism))
}

impl Hypnotism {
    /// Its one turn at `at`, cast by `owner`: the nearest `HYPNO_NUM_PEOPLE` eligible people within its 3 × 3 cells
    /// join `owner`'s tribe, a not-yet-hypnotised one taking priority over one already captured by somebody.
    pub fn turn(&mut self, at: (u16, u16), owner: u8, world: &GameMap) -> Vec<WorldEvent> {
        let size = world.terrain.size() as i32;
        let pred = |u: &Unit| {
            u.owner != owner && u.kind != UnitKind::Wildman && u.kind != UnitKind::Shaman && u.is_alive() && !u.statuses.shield.is_on() && in_block(at, (u.x, u.z), size)
        };
        let fresh = |u: &Unit| !u.statuses.hypnotized.is_on();
        nearest_n_fresh_first(&world.units, at, HYPNO_NUM_PEOPLE as usize, pred, fresh).into_iter().map(|unit| WorldEvent::Hypnotize { unit, tribe: owner }).collect()
    }
}

impl GameMap {
    /// The unit `unit` joins `tribe` like `change_owner`, but keeps its life and carried wood
    /// (`Unit::change_owner_keeping_body`), and restarts its `HYPNO_STEPS` countdown
    /// (`status::Statuses.hypnotized`). Its `hypnotized_from` keeps the tribe it had *before its first capture*: a
    /// unit already hypnotised that gets recast on (by its current captor or anyone else) changes hands and its
    /// timer restarts, but what it reverts to never changes mid-chain.
    pub(crate) fn hypnotize(&mut self, unit: crate::unit::UnitId, tribe: u8) {
        let Some(u) = self.unit_mut(unit).filter(|u| u.is_alive()) else { return };
        let original = u.statuses.hypnotized_from.unwrap_or(u.owner);
        u.change_owner_keeping_body(tribe);
        if let Some(u) = self.unit_mut(unit) {
            u.statuses.hypnotized = crate::status::Timed::steps(crate::status::HYPNO_STEPS);
            u.statuses.hypnotized_from = Some(original);
        }
    }

    /// Every unit whose hypnotism just ran out this turn (`status::Statuses.hypnotized_from` still set, but
    /// `hypnotized` no longer on) reverts to its original tribe, keeping its life and carried wood same as the
    /// capture did, unless that tribe is wiped out by now (`tribe_wiped_out`), in which case the change just stays
    /// as it is (`hypnotized_from` cleared either way, so this never matches the same unit twice).
    pub(crate) fn revert_expired_hypnosis(&mut self) {
        let expired: Vec<(crate::unit::UnitId, u8)> = self
            .units
            .iter()
            .filter(|u| u.statuses.hypnotized_from.is_some() && !u.statuses.hypnotized.is_on())
            .map(|u| (u.id, u.statuses.hypnotized_from.unwrap()))
            .collect();
        for (unit, original) in expired {
            if self.tribe_wiped_out(original) {
                if let Some(u) = self.unit_mut(unit) {
                    u.statuses.hypnotized_from = None;
                }
            } else if let Some(u) = self.unit_mut(unit) {
                u.change_owner_keeping_body(original);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::{Timed, SHIELD_STEPS};
    use crate::terrain::Heightmap;
    use crate::unit::UnitId;

    const AT: (u16, u16) = (10 * 512 + 256, 20 * 512 + 256);

    /// Dry land everywhere (unlike `Heightmap::new`'s default sea level): these tests run hundreds of ticks, long
    /// enough for a unit left standing in water to be stranded and drown, which isn't what they're testing.
    fn bare() -> GameMap {
        let mut terrain = Heightmap::new(128);
        for z in 0..128 {
            for x in 0..128 {
                terrain.set(x, z, 100);
            }
        }
        GameMap::bare("hypnotism-test", terrain, Vec::new())
    }

    #[test]
    fn hypnotises_the_nearest_six_of_other_tribes_not_wildmen_shamans_or_the_shielded() {
        let mut map = bare();
        let near: Vec<UnitId> = (0..HYPNO_NUM_PEOPLE).map(|k| map.spawn(1, UnitKind::Brave, (AT.0.wrapping_add(k as u16 * 10), AT.1)).id).collect();
        let far = map.spawn(1, UnitKind::Brave, (AT.0.wrapping_add(4000), AT.1)).id;
        let own = map.spawn(0, UnitKind::Brave, (AT.0.wrapping_add(5), AT.1)).id;
        let wildman = map.spawn(255, UnitKind::Wildman, (AT.0.wrapping_add(5), AT.1)).id;
        let shaman = map.spawn(1, UnitKind::Shaman, (AT.0.wrapping_add(5), AT.1)).id;
        let shielded = map.spawn(1, UnitKind::Brave, (AT.0.wrapping_add(5), AT.1)).id;
        map.unit_mut(shielded).unwrap().statuses.shield = Timed::steps(SHIELD_STEPS);
        map.add_effect(effect(0, AT));
        map.tick();
        for &id in &near {
            let u = map.unit(id).unwrap();
            assert_eq!(u.owner, 0, "hypnotised");
            assert!(u.statuses.hypnotized.is_on() && u.statuses.hypnotized_from == Some(1));
        }
        assert_eq!(map.unit(own).unwrap().statuses.hypnotized_from, None, "already the caster's own: never touched");
        for &id in &[far, wildman, shaman, shielded] {
            assert_ne!(map.unit(id).unwrap().owner, 0, "spared: {id:?}");
        }
    }

    #[test]
    fn never_reaches_past_its_3x3_cells_even_with_room_to_spare() {
        let mut map = bare();
        // Only one other candidate on the whole map, well outside the cast's 3 × 3 cells: with room for 5 more,
        // an unbounded nearest-first pick would still reach it, but the selector's own area must stop it first.
        let far = map.spawn(1, UnitKind::Brave, (AT.0.wrapping_add(2000), AT.1)).id;
        map.add_effect(effect(0, AT));
        map.tick();
        assert_eq!(map.unit(far).unwrap().owner, 1, "outside the 3 × 3 cells: spared");
    }

    #[test]
    fn a_recast_reaches_fresh_enemies_before_recapturing_an_already_hypnotised_one() {
        let mut map = bare();
        // Already captured by tribe 2, closer to the cast than everyone else.
        let captured = map.spawn(1, UnitKind::Spy, (AT.0.wrapping_add(5), AT.1)).id;
        map.hypnotize(captured, 2);
        // 6 fresh enemies, a little farther out: still reached first.
        let fresh: Vec<UnitId> = (0..HYPNO_NUM_PEOPLE).map(|k| map.spawn(1, UnitKind::Spy, (AT.0.wrapping_add(50 + k as u16 * 10), AT.1)).id).collect();
        map.add_effect(effect(0, AT));
        map.tick();
        assert!(fresh.iter().all(|&id| map.unit(id).unwrap().owner == 0), "the 6 fresh ones, not the closer captured one");
        assert_eq!(map.unit(captured).unwrap().owner, 2, "left with its captor: there was room for everyone fresh");
    }

    #[test]
    fn reverts_to_the_original_tribe_once_hypnotism_runs_out() {
        let mut map = bare();
        // Spies (`sight: 0`) never look for a fight on their own, so a newly captured one and the decoys left
        // behind in its old tribe never end up skirmishing while this test just watches the revert timer.
        let enemy = map.spawn(1, UnitKind::Spy, AT).id;
        // Enough closer decoys (all still in the 3 × 3 block) keep some of tribe 1 out of the nearest 6 and so
        // still alive, letting this test the revert itself, not the permanent case (that's the other test, below).
        for k in 1..=10u16 {
            map.spawn(1, UnitKind::Spy, (AT.0.wrapping_add(k * 50), AT.1));
        }
        map.add_effect(effect(0, AT));
        map.tick();
        assert_eq!(map.unit(enemy).unwrap().owner, 0);
        let reverted = (0..500).find(|_| {
            map.tick();
            map.unit(enemy).unwrap().owner == 1
        });
        assert!(reverted.is_some(), "back to its own tribe within HYPNO_STEPS, phase aside");
        assert_eq!(map.unit(enemy).unwrap().statuses.hypnotized_from, None);
    }

    #[test]
    fn recapturing_an_already_hypnotised_unit_changes_hands_but_keeps_its_true_original_tribe() {
        let mut map = bare();
        let enemy = map.spawn(1, UnitKind::Spy, AT).id;
        // Tribe 1 keeps another unit alive elsewhere so it is never wiped out and this stays a plain revert.
        map.spawn(1, UnitKind::Spy, (AT.0.wrapping_add(20_000), AT.1));
        map.hypnotize(enemy, 0);
        assert_eq!((map.unit(enemy).unwrap().owner, map.unit(enemy).unwrap().statuses.hypnotized_from), (0, Some(1)));
        // Tribe 2 recaptures it from tribe 0 before its timer runs out.
        map.hypnotize(enemy, 2);
        assert_eq!(
            (map.unit(enemy).unwrap().owner, map.unit(enemy).unwrap().statuses.hypnotized_from),
            (2, Some(1)),
            "controlled by the latest caster, but still remembers tribe 1, not tribe 0"
        );
        let reverted = (0..500).find(|_| {
            map.tick();
            map.unit(enemy).unwrap().owner == 1
        });
        assert!(reverted.is_some(), "reverts to tribe 1 in the end, never to tribe 0 who held it in between");
    }

    #[test]
    fn keeps_life_and_wood_across_a_capture_and_its_later_revert() {
        // No `map.tick()` here: an idle unit holding wood puts it down again after a while regardless of its
        // owner (`GameMap::drop_wood`) - unrelated to this fix, and it would hide what's being tested. Calling
        // `hypnotize`/`revert_expired_hypnosis` directly instead checks exactly what they do to the body.
        let mut map = bare();
        let enemy = map.spawn(1, UnitKind::Brave, AT).id;
        map.spawn(1, UnitKind::Brave, (AT.0.wrapping_add(20_000), AT.1)); // keeps tribe 1 alive, so the revert happens
        let u = map.unit_mut(enemy).unwrap();
        (u.health, u.carrying) = (crate::health::Health::new(1, u.health.max()), 7);
        map.hypnotize(enemy, 0);
        let u = map.unit(enemy).unwrap();
        assert_eq!((u.owner, u.health.current(), u.carrying), (0, 1, 7), "captured, but keeping its body as it was");
        map.unit_mut(enemy).unwrap().statuses.hypnotized = Timed::default();
        map.revert_expired_hypnosis();
        let u = map.unit(enemy).unwrap();
        assert_eq!((u.owner, u.health.current(), u.carrying), (1, 1, 7), "reverted, body untouched again");
    }

    #[test]
    fn stays_permanent_once_hypnotising_its_last_unit_already_wipes_out_the_original_tribe() {
        let mut map = bare();
        // Tribe 1's only unit: once hypnotised away, tribe 1 already has nobody left (`tribe_wiped_out` reads
        // current ownership, docs/specs/outcome.md), so the revert below must not happen.
        let enemy = map.spawn(1, UnitKind::Brave, AT).id;
        map.add_effect(effect(0, AT));
        map.tick();
        assert_eq!(map.unit(enemy).unwrap().owner, 0);
        // A phase's worth of slack (`status::STEP`) past the worst case, so the timer has certainly run out.
        map.run(Ticks::new(crate::status::STEP.period() * (crate::status::HYPNO_STEPS as u32 + 1)));
        assert!(!map.unit(enemy).unwrap().statuses.hypnotized.is_on(), "the timer has run out");
        assert_eq!(map.unit(enemy).unwrap().owner, 0, "tribe 1 is gone: stays with its captor");
        assert_eq!(map.unit(enemy).unwrap().statuses.hypnotized_from, None, "the marker is cleared either way");
    }
}
