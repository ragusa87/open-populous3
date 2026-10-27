//! Formations (docs/specs/coordinators.md "The original's internal models" 1; pop3-rev-analysis.md "Formations"):
//! the spacing several of the player's units keep while walking to one shared move order's target, instead of each
//! aiming straight at it. Created by `Command::Formation`, the one command (like `SetPatrol`) that carries a whole
//! group instead of resolving it into one command per unit beforehand, so the group is still known when the
//! formation is minted.
//!
//! Unlike a fight or a challenge (coordinator.rs), a formation does not own what its members are doing: joining or
//! leaving one never changes a unit's action, only where it is currently told to walk (`WorldEvent::Placed`). So it
//! keeps its own member list instead of the generic coordinator `Action` link, and is its own typed list on
//! `GameMap` (architecture.md "The tick") rather than a `Coordinator` kind.
//!
//! The original (`add_unit_to_formation`/0x5018C0, `process_formation_unit`/0x501000) holds up to 12 members at a
//! fixed slot offset around a controller that jitters its own heading periodically, seeded from the order's own
//! target, dropping a member that strays too far off its slot with a bad heading and an unreachable path. Not
//! traced closely enough to give: the slot table itself, the jitter's rhythm, how fast (or whether) the controller
//! advances towards its target, or that drop condition's own numbers - all guesses or gaps here, tagged below. Ours
//! drops a member once ordinary pathing itself gives up on its slot (`Action::Stranded`, ordinary walking's own
//! ground-truth for "unreachable"), rather than a separate distance check: nudging an already-correct path a little
//! off-centre towards a slot never makes it unreachable on its own, so no extra "too far, drop it" rule is needed -
//! the original's own ≥ 568-unit number is left unmodelled rather than guessed at.

use crate::angle;
use crate::event::WorldEvent;
use crate::map::{GameMap, Lcg};
use crate::time::{Every, Tick};
use crate::unit::{Action, UnitId};
use pop3_format::WORLD_UNITS_PER_CELL;

/// The most members one formation holds ("Formations" [exe]).
pub const MAX_MEMBERS: usize = 12;
/// The controller's own heading turns on this rhythm (not in the trace: a guess).
pub const JITTER_EVERY: Every = Every::new(32);
/// The jitter itself: a draw below this much of `angle::TURN` each time (not in the trace: a guess).
pub const JITTER_TURN: u32 = angle::TURN / 16;
/// Columns of the slot grid (guess: the original's own table, "(dx, dz) × 16-unit slot offset", isn't in the
/// trace). `MAX_MEMBERS` divides evenly by this into whole rows.
const SLOT_COLS: i32 = 4;
/// World units between neighbouring slots: the same pitch standing units already keep off each other
/// (`slots::PER_CELL`), not the original's own untraced "16-unit" pitch, which would stand members on top of one
/// another.
pub const SLOT_SPACING: i32 = (WORLD_UNITS_PER_CELL / crate::slots::PER_CELL as u32) as i32;

/// A group of the player's units walking to one point together, spaced into a slot grid around it that slowly
/// turns (docs/specs/coordinators.md "The original's internal models" 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formation {
    pub owner: u8,
    /// The order's own target, where the formation is seeded and stays for its life: the original's controller
    /// may itself advance towards it (untraced), ours does not - a guess, kept simple.
    pub at: (u16, u16),
    /// The controller's own heading, of `angle::TURN`, jittering on `JITTER_EVERY`.
    pub heading: u32,
    /// Each member with the slot it was given when it joined (`slot`), fixed for as long as it stays: a member
    /// dropping out must never shift its neighbours into a different slot, or they would abandon whatever
    /// progress they had made towards the old one and walk towards the new one instead - repeatedly, as each
    /// further departure reshuffled the rest again.
    pub members: Vec<(UnitId, usize)>,
}

impl Formation {
    pub(crate) fn new(owner: u8, at: (u16, u16), members: Vec<UnitId>) -> Self {
        let members = members.into_iter().enumerate().map(|(k, id)| (id, k)).collect();
        Formation { owner, at, heading: 0, members }
    }

    /// Where the `k`-th member stands in the formation: a fixed slot around `at`, turned by the controller's own
    /// heading. The grid (`SLOT_COLS` columns) is not the original's own layout (not in the trace): a guess, picked
    /// only to keep up to `MAX_MEMBERS` apart and roughly centred on `at`. Public for the client's debug overlay
    /// (F4), which draws every member's own slot.
    pub fn slot(&self, k: usize) -> (u16, u16) {
        let k = k as i32;
        let rows = MAX_MEMBERS as i32 / SLOT_COLS;
        let (row, col) = (k / SLOT_COLS, k % SLOT_COLS);
        let dx = (2 * col - (SLOT_COLS - 1)) * SLOT_SPACING;
        let dz = (2 * row - (rows - 1)) * SLOT_SPACING;
        let (rx, rz) = angle::rotate((dx, dz), self.heading);
        (self.at.0.wrapping_add(rx as u16), self.at.1.wrapping_add(rz as u16))
    }

    /// Its turn: members no longer walking (arrived, reassigned, dead, or stranded: their slot turned out
    /// unreachable) drop out; the rest are nudged towards their own slot (`WorldEvent::Placed`, only where it
    /// differs from their current target); the controller's own heading jitters on its rhythm. `finished`: every
    /// member is gone.
    pub(crate) fn tick(&mut self, now: Tick, world: &GameMap, rng: &mut Lcg) -> (bool, Vec<WorldEvent>) {
        let mut events = Vec::new();
        let before = std::mem::take(&mut self.members);
        self.members = before
            .into_iter()
            .filter_map(|(id, k)| {
                let Action::Walking { to } = world.unit(id)?.action else { return None };
                let slot = self.slot(k);
                if to != slot {
                    events.push(WorldEvent::Placed { unit: id, at: slot });
                }
                Some((id, k))
            })
            .collect();
        if JITTER_EVERY.fires(now, 0) {
            self.heading = self.heading.wrapping_add(rng.next_u32() % JITTER_TURN);
        }
        (self.members.is_empty(), events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::torus_delta;

    fn walking(map: &mut GameMap, ids: &[UnitId], to: (u16, u16)) {
        for &id in ids {
            map.unit_mut(id).unwrap().start(crate::unit::Order::MoveTo { x: to.0, z: to.1 });
        }
    }

    fn braves(map: &mut GameMap, n: usize) -> Vec<UnitId> {
        let s = map.shaman_of(0).unwrap();
        let at = (s.x.wrapping_add(2048), s.z);
        (0..n).map(|k| map.spawn(0, crate::unit::UnitKind::Brave, (at.0, at.1.wrapping_add(300 * k as u16))).id).collect()
    }

    #[test]
    fn members_spread_across_distinct_slots() {
        let mut map = GameMap::sandbox_units();
        let units = braves(&mut map, 4);
        let to = (map.terrain.size() as u16 * 256, map.terrain.size() as u16 * 256);
        walking(&mut map, &units, to);
        let f = Formation::new(0, to, units.clone());
        let slots: std::collections::BTreeSet<_> = (0..units.len()).map(|k| f.slot(k)).collect();
        assert_eq!(slots.len(), units.len(), "one slot each: {slots:?}");
        for s in slots {
            assert!(torus_delta(s.0, to.0).unsigned_abs() < 2000 && torus_delta(s.1, to.1).unsigned_abs() < 2000, "close to the target");
        }
    }

    #[test]
    fn a_member_no_longer_walking_drops_out_without_ending_the_formation() {
        let mut map = GameMap::sandbox_units();
        let units = braves(&mut map, 2);
        let to = (map.terrain.size() as u16 * 256, map.terrain.size() as u16 * 256);
        walking(&mut map, &units, to);
        let mut f = Formation::new(0, to, units.clone());
        map.unit_mut(units[0]).unwrap().action = Action::Idle;
        let (finished, _) = f.tick(map.now, &map, &mut Lcg(0));
        assert_eq!(f.members, vec![(units[1], 1)], "the idle one is gone, the walker stays");
        assert!(!finished);
    }

    #[test]
    fn a_member_dropping_out_does_not_reshuffle_the_rest_into_new_slots() {
        let mut map = GameMap::sandbox_units();
        let units = braves(&mut map, 3);
        let to = (map.terrain.size() as u16 * 256, map.terrain.size() as u16 * 256);
        let mut f = Formation::new(0, to, units.clone());
        // Everyone is already walking to its own slot, like after a first tick nudged it there.
        for k in 0..units.len() {
            walking(&mut map, &units[k..=k], f.slot(k));
        }
        map.unit_mut(units[0]).unwrap().action = Action::Idle;
        let (_, events) = f.tick(map.now, &map, &mut Lcg(0));
        assert!(events.is_empty(), "units 1 and 2 were already on their own slots; dropping unit 0 must not move them: {events:?}");
    }

    #[test]
    fn empty_once_every_member_is_gone() {
        let mut map = GameMap::sandbox_units();
        let units = braves(&mut map, 2);
        for &id in &units {
            map.unit_mut(id).unwrap().action = Action::Idle;
        }
        let mut f = Formation::new(0, (0, 0), units);
        let (finished, events) = f.tick(map.now, &map, &mut Lcg(0));
        assert!(finished && events.is_empty());
    }

    #[test]
    fn nudges_a_member_whose_target_drifted_off_its_slot() {
        let mut map = GameMap::sandbox_units();
        let units = braves(&mut map, 1);
        let to = (map.terrain.size() as u16 * 256, map.terrain.size() as u16 * 256);
        walking(&mut map, &units, (to.0.wrapping_add(50), to.1));
        let mut f = Formation::new(0, to, units.clone());
        let expected = f.slot(0);
        let (_, events) = f.tick(map.now, &map, &mut Lcg(0));
        assert_eq!(events, vec![WorldEvent::Placed { unit: units[0], at: expected }]);
    }

    #[test]
    fn no_event_once_already_on_its_slot() {
        let mut map = GameMap::sandbox_units();
        let units = braves(&mut map, 1);
        let to = (map.terrain.size() as u16 * 256, map.terrain.size() as u16 * 256);
        let mut f = Formation::new(0, to, units.clone());
        walking(&mut map, &units, f.slot(0));
        let (_, events) = f.tick(map.now, &map, &mut Lcg(0));
        assert!(events.is_empty());
    }

    #[test]
    fn a_stranded_member_drops_out_its_slot_turned_out_unreachable() {
        let mut map = GameMap::sandbox_units();
        let units = braves(&mut map, 1);
        let to = (map.terrain.size() as u16 * 256, map.terrain.size() as u16 * 256);
        map.unit_mut(units[0]).unwrap().action = Action::Stranded { to };
        let mut f = Formation::new(0, to, units);
        let (finished, events) = f.tick(map.now, &map, &mut Lcg(0));
        assert!(finished && events.is_empty(), "stranded, not walking any more: dropped");
    }
}
