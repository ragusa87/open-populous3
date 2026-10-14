//! The match's result: win, lose, or a draw (docs/architecture.md), and why. A generic, "hookable into scripts"
//! entry point (`GameMap::declare_outcome`) rather than one hardcoded to elimination — the automatic check below is
//! just its first caller; Armageddon (`armageddon.rs`) is its second, and a future level-script interpreter
//! (`TODO.md`) will be a third.

use crate::map::GameMap;
use crate::status::{Timed, SHIELD_STEPS};
use crate::time::Tick;

/// Why the match ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Every other side already had zero living units of any kind.
    Eliminated,
    /// Armageddon's match settled it (`armageddon.rs`).
    Armageddon,
    /// Reserved for a future level-script interpreter (`TODO.md`); nothing declares this yet.
    Scripted,
}

/// The match's one and only result: the tribe left standing (`None`: a draw), why, and when.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub winner: Option<u8>,
    pub reason: Reason,
    pub at: Tick,
}

impl GameMap {
    /// Declares the match's result, once. `false` (no-op) if one was already declared — nothing can un-declare or
    /// relabel a result afterwards (the "hookable into scripts" entry point).
    pub fn declare_outcome(&mut self, winner: Option<u8>, reason: Reason) -> bool {
        if self.outcome.is_some() {
            return false;
        }
        self.outcome = Some(Outcome { winner, reason, at: self.now });
        true
    }

    /// No living unit of any kind for `tribe`, right now (an instantaneous snapshot by current owner, not an
    /// original-tribe/permanent-death mechanic: a unit hypnotized away from an enemy counts for us; one hypnotized
    /// away from us doesn't). `pub(crate)`, not private: Hypnotism's own already-recorded design (`docs/plans/
    /// spells.md` "unless that tribe is dead") will reuse this exact check once implemented.
    pub(crate) fn tribe_wiped_out(&self, tribe: u8) -> bool {
        !self.units.iter().any(|u| u.owner == tribe && u.is_alive())
    }

    /// A no-op once `outcome` is set, and on single-tribe maps (every sandbox/editor map), where "elimination"
    /// isn't a meaningful event — without this guard every such map would declare an instant win on its first tick.
    pub(crate) fn check_elimination(&mut self) {
        if self.outcome.is_some() || self.tribes.count() < 2 {
            return;
        }
        let sides = self.tribes.alive_sides(|t| self.tribe_wiped_out(t));
        if sides.len() <= 1 {
            self.declare_outcome(sides.first().copied(), Reason::Eliminated);
        }
    }

    /// Once the match has ended, nothing can hurt anyone any more: every living unit's Magical Shield is kept
    /// topped up, every tick, for as long as the game keeps running past that point — a no-op before `outcome` is
    /// set. "Permanent" by being continuously refreshed, the same trick a real Armageddon shield will need
    /// (`docs/plans/spells.md` "Armageddon"), not a new status kind.
    pub(crate) fn shield_everyone_once_ended(&mut self) {
        if self.outcome.is_none() {
            return;
        }
        for u in self.units.iter_mut().filter(|u| u.is_alive()) {
            u.statuses.shield = Timed::steps(SHIELD_STEPS);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::map::GameMap;
    use crate::tribes::Tribes;
    use crate::unit::UnitKind;

    /// `GameMap::generate` already gives two real tribes (0 and 1), each with a living shaman and `tribes` set
    /// from their sites.
    fn two_tribes() -> GameMap {
        GameMap::generate(1)
    }

    #[test]
    fn tribe_wiped_out_is_true_the_instant_its_only_unit_starts_dying() {
        let mut map = two_tribes();
        let shaman = map.shaman_of(0).unwrap().id;
        map.unit_mut(shaman).unwrap().take_damage(u16::MAX);
        assert!(map.tribe_wiped_out(0));
        assert!(!map.tribe_wiped_out(1));
    }

    #[test]
    fn declare_outcome_is_a_one_shot_latch() {
        let mut map = two_tribes();
        assert!(map.declare_outcome(Some(0), super::Reason::Eliminated));
        assert!(!map.declare_outcome(Some(1), super::Reason::Armageddon));
        assert_eq!(map.outcome.unwrap().winner, Some(0));
    }

    #[test]
    fn check_elimination_never_fires_on_a_single_tribe_map() {
        let mut map = GameMap::sandbox_units();
        map.run(crate::time::Ticks::new(5));
        assert!(map.outcome.is_none());
    }

    #[test]
    fn a_three_tribe_map_waits_for_every_other_tribe_before_declaring() {
        let mut map = two_tribes();
        map.spawn(2, UnitKind::Brave, (0, 0));
        map.tribes = Tribes::from_owners([0, 1, 2]);
        map.unit_mut(map.shaman_of(1).unwrap().id).unwrap().take_damage(u16::MAX);
        map.tick();
        assert!(map.outcome.is_none(), "tribe 2 still alive");
        let brave2 = map.units.iter().find(|u| u.owner == 2).unwrap().id;
        map.unit_mut(brave2).unwrap().take_damage(u16::MAX);
        map.tick();
        assert_eq!(map.outcome.unwrap().winner, Some(0));
    }

    #[test]
    fn a_simultaneous_double_wipe_declares_a_draw() {
        let mut map = two_tribes();
        for owner in [0u8, 1] {
            let id = map.shaman_of(owner).unwrap().id;
            map.unit_mut(id).unwrap().take_damage(u16::MAX);
        }
        map.tick();
        assert_eq!(map.outcome.unwrap().winner, None);
    }

    #[test]
    fn further_ticks_never_change_an_already_declared_outcome() {
        let mut map = two_tribes();
        map.declare_outcome(Some(0), super::Reason::Eliminated);
        map.run(crate::time::Ticks::new(5));
        assert_eq!(map.outcome.unwrap().winner, Some(0));
    }

    #[test]
    fn nobody_is_shielded_before_the_match_ends() {
        let map = two_tribes();
        assert!(map.units.iter().all(|u| !u.statuses.shield.is_on()));
    }

    #[test]
    fn every_living_unit_is_shielded_once_the_match_ends_and_stays_shielded() {
        let mut map = two_tribes();
        map.declare_outcome(Some(0), super::Reason::Eliminated);
        map.run(crate::time::Ticks::new(1));
        assert!(map.units.iter().filter(|u| u.is_alive()).all(|u| u.statuses.shield.is_on()));
        map.run(crate::time::Ticks::secs(130)); // longer than SHIELD_STEPS' own duration
        assert!(map.units.iter().filter(|u| u.is_alive()).all(|u| u.statuses.shield.is_on()), "continuously refreshed, not a one-shot");
    }

    #[test]
    fn every_command_is_rejected_once_the_match_has_ended_even_without_an_active_armageddon() {
        let mut map = two_tribes();
        let before = map.units.clone();
        map.declare_outcome(Some(0), super::Reason::Eliminated);
        map.apply(&crate::command::Command::Order { player: 0, order: crate::unit::Order::Stop });
        assert_eq!(map.units, before);
    }
}
