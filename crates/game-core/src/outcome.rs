//! The match's result: win, lose, or a draw (docs/architecture.md), and why. A generic, "hookable into scripts"
//! entry point (`GameMap::declare_outcome`) rather than one hardcoded to elimination — the automatic check below is
//! just its first caller; Armageddon (`armageddon.rs`) is its second, and a future level-script interpreter
//! (`TODO.md`) will be a third.

use crate::map::GameMap;
use crate::status::{Timed, SHIELD_STEPS};
use crate::time::Tick;
use crate::unit::Action;

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
    /// relabel a result afterwards (the "hookable into scripts" entry point). Every unit still acting stands down to
    /// Idle right away (a death already in train — `Dying`, `Dead`, `Spirit` — finishes on its own; the shaman still
    /// doesn't get a last-stand reincarnation out of it, `GameMap::tick`'s own freeze below).
    pub fn declare_outcome(&mut self, winner: Option<u8>, reason: Reason) -> bool {
        if self.outcome.is_some() {
            return false;
        }
        self.outcome = Some(Outcome { winner, reason, at: self.now });
        for u in self.units.iter_mut() {
            if !matches!(u.action, Action::Dying { .. } | Action::Dead { .. } | Action::Spirit { .. }) {
                u.action = Action::Idle;
            }
        }
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

    /// Whether `tribe` is on the winning side once the match has ended: the winner itself, or one of its allies
    /// (`Tribes::same_team`). `false` before `outcome` is set and after a draw (`winner: None`) — a draw is a win
    /// for nobody. The one place a client-side "victory lap" effect (the happy jump, the celebrating pose) should
    /// read to decide who gets to celebrate.
    pub fn on_winning_side(&self, tribe: u8) -> bool {
        self.outcome.and_then(|o| o.winner).is_some_and(|winner| self.tribes.same_team(tribe, winner))
    }

    /// Once the match has ended, nothing can hurt the winning side's units any more: every one of theirs still alive
    /// gets its Magical Shield kept topped up, every tick, for as long as the game keeps running past that point —
    /// a no-op before `outcome` is set, and on a draw, nobody is shielded. "Permanent" by being continuously
    /// refreshed, the same trick a real Armageddon shield will need (`docs/plans/spells.md` "Armageddon"), not a
    /// new status kind.
    pub(crate) fn shield_everyone_once_ended(&mut self) {
        let Some(winner) = self.outcome.and_then(|o| o.winner) else { return };
        let tribes = &self.tribes;
        for u in self.units.iter_mut().filter(|u| u.is_alive() && tribes.same_team(u.owner, winner)) {
            u.statuses.shield = Timed::steps(SHIELD_STEPS);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::map::GameMap;
    use crate::tribes::Tribes;
    use crate::unit::{Action, UnitKind};

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
    fn the_winning_sides_living_units_are_shielded_once_the_match_ends_and_stay_shielded() {
        let mut map = two_tribes();
        map.declare_outcome(Some(0), super::Reason::Eliminated);
        map.run(crate::time::Ticks::new(1));
        assert!(map.units.iter().filter(|u| u.owner == 0 && u.is_alive()).all(|u| u.statuses.shield.is_on()));
        assert!(map.units.iter().filter(|u| u.owner == 1 && u.is_alive()).all(|u| !u.statuses.shield.is_on()), "the losing side gets no victory shield");
        map.run(crate::time::Ticks::secs(130)); // longer than SHIELD_STEPS' own duration
        assert!(map.units.iter().filter(|u| u.owner == 0 && u.is_alive()).all(|u| u.statuses.shield.is_on()), "continuously refreshed, not a one-shot");
    }

    #[test]
    fn nobody_is_shielded_on_a_draw() {
        let mut map = two_tribes();
        map.declare_outcome(None, super::Reason::Eliminated);
        map.run(crate::time::Ticks::new(1));
        assert!(map.units.iter().all(|u| !u.statuses.shield.is_on()), "a draw is a win for nobody");
    }

    #[test]
    fn an_allys_units_are_shielded_too() {
        let mut map = two_tribes();
        map.tribes = Tribes::from_owners([0, 1]).with_allies([0b11, 0b11, 0, 0]);
        map.declare_outcome(Some(0), super::Reason::Eliminated);
        map.run(crate::time::Ticks::new(1));
        assert!(map.units.iter().filter(|u| u.is_alive()).all(|u| u.statuses.shield.is_on()), "tribe 1 is allied with the winner");
    }

    #[test]
    fn declaring_the_outcome_stands_every_acting_unit_down_to_idle() {
        let mut map = two_tribes();
        let brave = map.spawn(0, UnitKind::Brave, (10_000, 10_000)).id;
        map.unit_mut(brave).unwrap().action = Action::Walking { to: (20_000, 20_000) };
        map.declare_outcome(Some(0), super::Reason::Eliminated);
        assert_eq!(map.unit(brave).unwrap().action, Action::Idle);
    }

    #[test]
    fn a_lone_shaman_never_reincarnates_once_the_match_has_ended() {
        let mut map = two_tribes();
        let shaman = map.shaman_of(1).unwrap().id;
        map.unit_mut(shaman).unwrap().take_damage(u16::MAX);
        map.tick();
        assert_eq!(map.outcome.unwrap().winner, Some(0), "tribe 1 had no other unit left");
        map.run(crate::time::Ticks::secs(10)); // well past her dying + respawn countdowns
        assert!(matches!(map.unit(shaman).unwrap().action, Action::Dead { .. }), "no last-stand comeback once the match is over");
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
