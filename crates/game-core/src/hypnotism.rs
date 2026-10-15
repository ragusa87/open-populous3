//! Hypnotism (pop3-rev-analysis.md "Per spell" 7), an effect (`effect::EffectKind::Hypnotism`): aimed at a point
//! like a blast. On its one turn, the `HYPNO_NUM_PEOPLE` nearest people of other tribes around it — not a
//! wildman, not a shaman, not a shielded one (`nearest::nearest_n`, the shared selector, pop3-rev-analysis.md
//! 0x515E30) — join the caster's tribe for `status::HYPNO_STEPS` steps (`WorldEvent::Hypnotize`, `GameMap::
//! hypnotize`). Once that runs out, each one reverts to its own original tribe (`GameMap::
//! revert_expired_hypnosis`), unless that tribe is wiped out by then, when the change stays permanent (confirmed,
//! docs/specs/spells.md: "if their tribe is gone, they stay").

use crate::effect::{Effect, EffectKind};
use crate::event::WorldEvent;
use crate::map::GameMap;
use crate::nearest::nearest_n;
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
    /// Its one turn at `at`, cast by `owner`: the nearest `HYPNO_NUM_PEOPLE` eligible people join `owner`'s tribe.
    pub fn turn(&mut self, at: (u16, u16), owner: u8, world: &GameMap) -> Vec<WorldEvent> {
        let pred = |u: &Unit| u.owner != owner && u.kind != UnitKind::Wildman && u.kind != UnitKind::Shaman && u.is_alive() && !u.statuses.shield.is_on();
        nearest_n(&world.units, at, HYPNO_NUM_PEOPLE as usize, pred).into_iter().map(|unit| WorldEvent::Hypnotize { unit, tribe: owner }).collect()
    }
}

impl GameMap {
    /// The unit `unit` joins `tribe` like `change_owner`, but remembers its own original tribe
    /// (`status::Statuses.hypnotized_from`) and starts its `HYPNO_STEPS` countdown (`status::Statuses.hypnotized`).
    pub(crate) fn hypnotize(&mut self, unit: crate::unit::UnitId, tribe: u8) {
        let Some(u) = self.unit_mut(unit).filter(|u| u.is_alive()) else { return };
        let original = u.owner;
        u.change_owner(tribe);
        if let Some(u) = self.unit_mut(unit) {
            u.statuses.hypnotized = crate::status::Timed::steps(crate::status::HYPNO_STEPS);
            u.statuses.hypnotized_from = Some(original);
        }
    }

    /// Every unit whose hypnotism just ran out this turn (`status::Statuses.hypnotized_from` still set, but
    /// `hypnotized` no longer on) reverts to its original tribe, unless that tribe is wiped out by now
    /// (`tribe_wiped_out`), in which case the change just stays as it is (`hypnotized_from` cleared either way, so
    /// this never matches the same unit twice).
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
            } else {
                self.change_owner(unit, original);
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
    fn reverts_to_the_original_tribe_once_hypnotism_runs_out() {
        let mut map = bare();
        // Spies (`sight: 0`) never look for a fight on their own, so a newly captured one and the decoys left
        // behind in its old tribe never end up skirmishing while this test just watches the revert timer.
        let enemy = map.spawn(1, UnitKind::Spy, AT).id;
        // `nearest_n` has no maximum range, so a lone tribe-1 unit far away would be swept up by the same cast and
        // leave tribe 1 wiped out too (that's the other test, below): enough closer decoys keep some of tribe 1
        // out of the nearest 6 and so still alive, letting this test the revert itself, not the permanent case.
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
