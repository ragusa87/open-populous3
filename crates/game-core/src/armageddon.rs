//! Armageddon (docs/plans/spells.md "Armageddon"; `TODO.md` for what's left): a v1 placeholder only — casting it
//! pauses the economy and combat-seeking (`map.rs::tick`), cancels any fight in progress, and settles the match
//! immediately by comparing each side's combined headcount. The full mechanic (clearing the map, gathering every
//! tribe into a real arena by headcount, per-tribe shields, a lightning-killed loser, a continuous camera takeover)
//! is deliberately not implemented here.

use crate::coordinator::{Coordinator, Step};
use crate::event::WorldEvent;
use crate::id::Id;
use crate::map::{GameMap, Lcg};
use crate::outcome::Reason;
use crate::time::Tick;
use crate::unit::{Action, Unit, UnitId};

/// Armageddon's match. Just the cast target today (for the client's camera, `GameMap::armageddon`) — headcount is
/// already final the instant fighting stops (nothing in this first pass makes anyone fight afterwards), so it
/// resolves the moment it ticks. The full version's `Phase` (Gathering, then Fighting, over many turns) and its
/// other fields land here once implemented; the `Coordinator` shape (a stable id, ticked every turn, torn down when
/// `finished`) doesn't need to change either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArmageddonMatch {
    pub at: (u16, u16),
}

impl Coordinator for ArmageddonMatch {
    /// Global by construction: `start_armageddon` makes every unit idle, so there's no narrower membership to
    /// track — the match's own condition ("how much headcount does each side have") is read straight off `world`.
    fn link_of(_u: &Unit) -> Option<Id<Self>> {
        None
    }

    fn tick(&mut self, _id: Id<Self>, _members: &[UnitId], _now: Tick, world: &GameMap, _rng: &mut Lcg) -> Step {
        let winner = best_side(world);
        Step { finished: true, events: vec![WorldEvent::Declare { winner, reason: Reason::Armageddon }] }
    }
}

/// Placeholder decision rule for v1: the side (alliance group, `Tribes::teams`) with the highest combined headcount
/// wins; an overall tie (including no tribes at all) is a draw. Replaced by the real rule (whichever side's
/// shaman(s) are the last standing after an actual fight) once the full mechanic lands.
fn best_side(world: &GameMap) -> Option<u8> {
    let mut totals: Vec<(u8, u32)> = world
        .tribes
        .teams()
        .into_iter()
        .map(|team| (team[0], team.iter().map(|&t| world.headcount(t).total() as u32).sum()))
        .collect();
    totals.sort_unstable_by_key(|&(rep, _)| rep);
    let best = totals.iter().map(|&(_, total)| total).max()?;
    let mut leaders = totals.iter().filter(|&&(_, total)| total == best);
    let first = leaders.next()?;
    leaders.next().is_none().then_some(first.0)
}

impl GameMap {
    /// The "Armageddon flag": `Some(at)` — the cast target, doubling as the client camera's destination — while a
    /// match is active, `None` otherwise. The one place anything (inside this module or out) should read
    /// `self.coordinators.armageddons`.
    pub fn armageddon(&self) -> Option<(u16, u16)> {
        self.coordinators.armageddons.iter().next().map(|(_, m)| m.at)
    }

    /// Armageddon takes effect at `at`: cancels any fight in progress (every unit mid-combat returns to Idle — the
    /// "pause the simulation" this first pass delivers) and starts `ArmageddonMatch`, which settles the match by
    /// headcount. Not implemented yet (docs/plans/spells.md "Armageddon", TODO.md): clearing the map, gathering
    /// everyone into a real arena, the per-tribe shield and lightning-struck loser. `at` is unused by this first
    /// pass beyond being carried for the camera — the full version needs it as the arena's centre too.
    pub(crate) fn start_armageddon(&mut self, at: (u16, u16)) {
        for u in &mut self.units {
            if matches!(u.action, Action::Attacking { .. } | Action::Melee { .. } | Action::Challenge { .. }) {
                u.action = Action::Idle;
            }
        }
        let id = self.new_id_of();
        self.coordinators.armageddons.insert(id, ArmageddonMatch { at });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::spell::Spell;
    use crate::time::Ticks;
    use crate::tribes::Tribes;
    use crate::unit::UnitKind;

    fn two_tribes() -> GameMap {
        GameMap::generate(1)
    }

    #[test]
    fn casting_it_cancels_fights_and_starts_exactly_one_match() {
        let mut map = two_tribes();
        let a = map.spawn(0, UnitKind::Brave, (10_000, 10_000)).id;
        let b = map.spawn(1, UnitKind::Brave, (10_000, 10_000)).id;
        map.unit_mut(a).unwrap().action = Action::Attacking { target: crate::unit::Target::Unit(b), to: (10_000, 10_000) };
        map.start_armageddon((5_000, 5_000));
        assert_eq!(map.unit(a).unwrap().action, Action::Idle);
        assert_eq!(map.coordinators.armageddons.len(), 1);
        assert_eq!(map.armageddon(), Some((5_000, 5_000)));
    }

    #[test]
    fn a_second_cast_while_one_is_active_is_refused() {
        let mut map = two_tribes();
        map.start_armageddon((1, 1));
        assert!(!map.can_cast(0, &Spell::Armageddon { at: (2, 2) }));
    }

    #[test]
    fn every_command_is_rejected_while_a_match_is_active() {
        let mut map = two_tribes();
        let before = map.units.clone();
        map.start_armageddon((1, 1));
        map.apply(&Command::Order { player: 0, order: crate::unit::Order::Stop });
        map.apply(&Command::Cast { player: 0, spell: Spell::Blast { at: (1, 1) } });
        assert_eq!(map.units, before);
    }

    #[test]
    fn combat_seeking_is_suspended_for_the_whole_tick_the_match_resolves_in() {
        let mut map = two_tribes();
        let a = map.spawn(0, UnitKind::Brave, (10_000, 10_000)).id;
        map.spawn(1, UnitKind::Warrior, (10_000, 10_064)); // close enough to spot and attack, were it not suspended
        map.start_armageddon((1, 1));
        map.tick(); // resolves the match this same tick, but spot_enemies/tend_attacks were still suspended for it
        assert_eq!(map.unit(a).unwrap().action, Action::Idle);
    }

    #[test]
    fn best_side_picks_the_higher_combined_headcount_side() {
        let mut map = two_tribes();
        map.spawn(0, UnitKind::Brave, (0, 0));
        map.spawn(0, UnitKind::Brave, (0, 0));
        map.spawn(1, UnitKind::Brave, (0, 0));
        assert_eq!(best_side(&map), Some(0));
    }

    #[test]
    fn an_allied_pair_outweighs_a_lone_third_tribe() {
        let mut map = two_tribes();
        // Tribe 2 alone has more fighters than either ally individually, but less than 0 and 1 combined.
        for _ in 0..3 {
            map.spawn(2, UnitKind::Brave, (0, 0));
        }
        for _ in 0..2 {
            map.spawn(0, UnitKind::Brave, (0, 0));
            map.spawn(1, UnitKind::Brave, (0, 0));
        }
        map.tribes = Tribes::from_owners([0, 1, 2]).with_allies([0b011, 0b011, 0b100, 0]);
        assert_eq!(best_side(&map), Some(0), "0 and 1 together (4 fighters) beat lone tribe 2 (3 fighters)");
    }

    #[test]
    fn a_flat_tie_is_a_draw() {
        let mut map = two_tribes();
        map.spawn(0, UnitKind::Brave, (0, 0));
        map.spawn(1, UnitKind::Brave, (0, 0));
        assert_eq!(best_side(&map), None);
    }

    #[test]
    fn casting_resolves_within_the_turn_and_declares_the_outcome() {
        let mut map = two_tribes();
        map.spawn(0, UnitKind::Brave, (0, 0));
        map.start_armageddon((1, 1));
        map.tick();
        assert_eq!(map.outcome.unwrap().reason, Reason::Armageddon);
        assert_eq!(map.outcome.unwrap().winner, Some(0));
        assert!(map.coordinators.armageddons.is_empty());
    }

    #[test]
    fn the_same_seed_gives_the_same_result() {
        let run = || {
            let mut map = GameMap::generate(1).seeded(9);
            map.spawn(0, UnitKind::Brave, (0, 0));
            map.spawn(1, UnitKind::Brave, (0, 0));
            map.spawn(1, UnitKind::Brave, (0, 0));
            map.start_armageddon((1, 1));
            map.run(Ticks::new(3));
            map.outcome
        };
        assert_eq!(run(), run());
    }
}
