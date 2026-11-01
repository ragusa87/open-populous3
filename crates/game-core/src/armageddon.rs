//! Armageddon (docs/plans/spells.md "Armageddon"; `TODO.md` for what's left): a v1 placeholder only — casting it
//! pauses the economy and combat-seeking (`map.rs::tick`), cancels any fight in progress, and settles the match
//! immediately by comparing each side's combined headcount, lightning-killing every losing tribe's shaman without
//! reincarnation (`WorldEvent::RemoveSite`/`DamageKind::Lightning`, the "fighters hit zero" moment collapsed into
//! the instant the match resolves, since v1 never lets anyone actually fight it out first). The rest of the full
//! mechanic (clearing the map, gathering every tribe into a real arena by headcount, a per-tribe shield while its
//! fighters live, a continuous camera takeover) is deliberately not implemented here.

use crate::coordinator::{Coordinator, Step};
use crate::effect::DamageKind;
use crate::event::WorldEvent;
use crate::id::Id;
use crate::map::{GameMap, Lcg};
use crate::outcome::Reason;
use crate::time::Tick;
use crate::tribes::TribeId;
use crate::unit::{Action, Unit, UnitId, UnitKind};

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
        let mut events = vec![WorldEvent::Declare { winner, reason: Reason::Armageddon }];
        if let Some(winner) = winner {
            events.extend(losers(world, winner));
        }
        Step { finished: true, events }
    }
}

/// Every tribe not on `winner`'s side hits zero fighters the instant the match resolves (v1 never lets anyone
/// actually fight it out - "Armageddon" above): its reincarnation is disabled for good (`WorldEvent::RemoveSite`)
/// and its shaman, if still alive, is struck by lightning (`DamageKind::Lightning`, the ordinary path: blocked by
/// a shield same as any other lightning strike, pop3-rev-analysis.md "Per spell" 3) - a permanent exit from the
/// match, not just standing down like `declare_outcome` already does for everyone.
fn losers(world: &GameMap, winner: TribeId) -> Vec<WorldEvent> {
    world
        .tribes
        .all()
        .filter(|&tribe| !world.tribes.same_team(tribe, winner))
        .flat_map(|tribe| {
            let shaman = world.units.iter().find(|u| u.owner == tribe && u.kind == UnitKind::Shaman && u.is_alive());
            std::iter::once(WorldEvent::RemoveSite { tribe })
                .chain(shaman.map(|u| WorldEvent::Damage { unit: u.id, amount: u16::MAX, kind: DamageKind::Lightning, from: None }))
        })
        .collect()
}

/// Placeholder decision rule for v1: the side (alliance group, `Tribes::teams`) with the highest combined headcount
/// wins; an overall tie (including no tribes at all) is a draw. Replaced by the real rule (whichever side's
/// shaman(s) are the last standing after an actual fight) once the full mechanic lands.
fn best_side(world: &GameMap) -> Option<TribeId> {
    let mut totals: Vec<(TribeId, u32)> = world
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
    /// headcount, lightning-killing every losing tribe's shaman without reincarnation (`losers`). Not implemented
    /// yet (docs/plans/spells.md "Armageddon", TODO.md): clearing the map, gathering everyone into a real arena,
    /// the per-tribe shield while its own fighters live. `at` is unused by this first pass beyond being carried
    /// for the camera — the full version needs it as the arena's centre too.
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
    use crate::tribes::Owner;
    use crate::command::Command;
    use crate::spell::Spell;
    use crate::time::Ticks;
    use crate::tribes::{TribeId, Tribes};
    use crate::unit::UnitKind;

    fn two_tribes() -> GameMap {
        GameMap::generate(1)
    }

    #[test]
    fn casting_it_cancels_fights_and_starts_exactly_one_match() {
        let mut map = two_tribes();
        let a = map.spawn(TribeId::Blue, UnitKind::Brave, (10_000, 10_000)).id;
        let b = map.spawn(TribeId::Red, UnitKind::Brave, (10_000, 10_000)).id;
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
        assert!(!map.can_cast(TribeId::Blue, &Spell::Armageddon { at: (2, 2) }));
    }

    #[test]
    fn every_command_is_rejected_while_a_match_is_active() {
        let mut map = two_tribes();
        let before = map.units.clone();
        map.start_armageddon((1, 1));
        map.apply(&Command::Order { player: TribeId::Blue, order: crate::unit::Order::Stop });
        map.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::Blast { at: (1, 1) } });
        assert_eq!(map.units, before);
    }

    #[test]
    fn combat_seeking_is_suspended_for_the_whole_tick_the_match_resolves_in() {
        let mut map = two_tribes();
        let a = map.spawn(TribeId::Blue, UnitKind::Brave, (10_000, 10_000)).id;
        map.spawn(TribeId::Red, UnitKind::Warrior, (10_000, 10_064)); // close enough to spot and attack, were it not suspended
        map.start_armageddon((1, 1));
        map.tick(); // resolves the match this same tick, but spot_enemies/tend_attacks were still suspended for it
        assert_eq!(map.unit(a).unwrap().action, Action::Idle);
    }

    #[test]
    fn best_side_picks_the_higher_combined_headcount_side() {
        let mut map = two_tribes();
        map.spawn(TribeId::Blue, UnitKind::Brave, (0, 0));
        map.spawn(TribeId::Blue, UnitKind::Brave, (0, 0));
        map.spawn(TribeId::Red, UnitKind::Brave, (0, 0));
        assert_eq!(best_side(&map), Some(TribeId::Blue));
    }

    #[test]
    fn an_allied_pair_outweighs_a_lone_third_tribe() {
        let mut map = two_tribes();
        // Tribe 2 alone has more fighters than either ally individually, but less than 0 and 1 combined.
        for _ in 0..3 {
            map.spawn(TribeId::Yellow, UnitKind::Brave, (0, 0));
        }
        for _ in 0..2 {
            map.spawn(TribeId::Blue, UnitKind::Brave, (0, 0));
            map.spawn(TribeId::Red, UnitKind::Brave, (0, 0));
        }
        map.tribes = Tribes::from_owners([TribeId::Blue, TribeId::Red, TribeId::Yellow]).with_allies([0b011, 0b011, 0b100, 0]);
        assert_eq!(best_side(&map), Some(TribeId::Blue), "0 and 1 together (4 fighters) beat lone tribe 2 (3 fighters)");
    }

    #[test]
    fn a_flat_tie_is_a_draw() {
        let mut map = two_tribes();
        map.spawn(TribeId::Blue, UnitKind::Brave, (0, 0));
        map.spawn(TribeId::Red, UnitKind::Brave, (0, 0));
        assert_eq!(best_side(&map), None);
    }

    #[test]
    fn casting_resolves_within_the_turn_and_declares_the_outcome() {
        let mut map = two_tribes();
        map.spawn(TribeId::Blue, UnitKind::Brave, (0, 0));
        map.start_armageddon((1, 1));
        map.tick();
        assert_eq!(map.outcome.unwrap().reason, Reason::Armageddon);
        assert_eq!(map.outcome.unwrap().winner, Some(TribeId::Blue));
        assert!(map.coordinators.armageddons.is_empty());
    }

    #[test]
    fn the_losing_tribes_shaman_is_struck_down_without_reincarnation_the_winner_untouched() {
        let mut map = two_tribes();
        map.spawn(TribeId::Blue, UnitKind::Brave, (0, 0)); // tribe 0 outnumbers tribe 1: it wins
        map.start_armageddon((1, 1));
        map.tick();
        assert_eq!(map.outcome.unwrap().winner, Some(TribeId::Blue));
        let loser = map.units.iter().find(|u| u.owner == Owner::Tribe(TribeId::Red) && u.kind == UnitKind::Shaman).unwrap();
        assert!(matches!(loser.action, Action::Dying { .. }), "struck down: {:?}", loser.action);
        assert!(!map.sites.iter().find(|s| s.owner == TribeId::Red).unwrap().enabled, "no coming back for tribe 1");
        let winner = map.units.iter().find(|u| u.owner == Owner::Tribe(TribeId::Blue) && u.kind == UnitKind::Shaman).unwrap();
        assert!(winner.is_alive() && winner.action == Action::Idle, "the winner's shaman is untouched");
        assert!(map.sites.iter().find(|s| s.owner == TribeId::Blue).unwrap().enabled, "and still reincarnates if it ever needs to");
    }

    #[test]
    fn a_shielded_losing_shaman_survives_the_strike_same_as_any_other_lightning() {
        let mut map = two_tribes();
        map.spawn(TribeId::Blue, UnitKind::Brave, (0, 0));
        let i = map.units.iter().position(|u| u.owner == Owner::Tribe(TribeId::Red) && u.kind == UnitKind::Shaman).unwrap();
        map.units[i].statuses.shield = crate::status::Timed::steps(crate::status::SHIELD_STEPS);
        map.start_armageddon((1, 1));
        map.tick();
        let loser = map.units.iter().find(|u| u.owner == Owner::Tribe(TribeId::Red) && u.kind == UnitKind::Shaman).unwrap();
        assert!(loser.is_alive() && loser.action == Action::Idle, "shielded: spared the strike");
        assert!(!map.sites.iter().find(|s| s.owner == TribeId::Red).unwrap().enabled, "still loses its reincarnation either way");
    }

    #[test]
    fn a_draw_strikes_nobody_down() {
        let mut map = two_tribes();
        map.start_armageddon((1, 1));
        map.tick();
        assert_eq!(map.outcome.unwrap().winner, None);
        assert!(map.units.iter().all(|u| u.is_alive() && u.action == Action::Idle));
        assert!(map.sites.iter().all(|s| s.enabled));
    }

    #[test]
    fn the_same_seed_gives_the_same_result() {
        let run = || {
            let mut map = GameMap::generate(1).seeded(9);
            map.spawn(TribeId::Blue, UnitKind::Brave, (0, 0));
            map.spawn(TribeId::Red, UnitKind::Brave, (0, 0));
            map.spawn(TribeId::Red, UnitKind::Brave, (0, 0));
            map.start_armageddon((1, 1));
            map.run(Ticks::new(3));
            map.outcome
        };
        assert_eq!(run(), run());
    }
}
