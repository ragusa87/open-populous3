//! Up to 8 patrol points a unit (or several selected together) cycles through forever once more than
//! one is set (units.md "Patrol orders"): not a coordinator (coordinators.md, model 10, "Guard
//! Control") — each point is an ordinary move or attack order, nothing patrol-specific happens at the
//! point itself. A point is a pure value, shared by every unit that targets it: freed only once
//! nothing alive still does (`GameMap::prune_patrol_points`, `unit.rs`'s `queued_patrol`), never
//! reference-counted.

use crate::hidden_point::HiddenPointId;
use crate::id::Id;
use crate::map::GameMap;
use crate::tribes::TribeId;
use crate::unit::{Order, Target, UnitId};
use std::collections::{BTreeSet, VecDeque};

/// Up to 8 points (units.md "Patrol orders").
pub const PATROL_CAPACITY: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatrolAction {
    /// Walk to an invisible marker (a bare position needs its own id, `hidden_point.rs`).
    GoTo(HiddenPointId),
    /// Fight whoever is there when the circuit reaches this point (a person or a building); a failed
    /// attempt keeps retrying every tick for as long as the circuit sits on it.
    Attack(Target),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PatrolPoint {
    pub action: PatrolAction,
}

pub type PatrolPointId = Id<PatrolPoint>;

/// A unit's own patrol points, next to reach last: `advance` moves the point just reached to the back
/// (the whole "cycle forever" behaviour), a single point reads as a stationary guard post.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PatrolQueue(VecDeque<PatrolPointId>);

impl PatrolQueue {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.0.len() >= PATROL_CAPACITY
    }

    pub fn iter(&self) -> impl Iterator<Item = PatrolPointId> + '_ {
        self.0.iter().copied()
    }

    /// Appends `point`; false once already at `PATROL_CAPACITY`.
    pub fn push_back(&mut self, point: PatrolPointId) -> bool {
        if self.is_full() {
            return false;
        }
        self.0.push_back(point);
        true
    }

    /// The point just reached moves to the back; the new front (None if empty).
    pub fn advance(&mut self) -> Option<PatrolPointId> {
        if self.0.is_empty() {
            return None;
        }
        self.0.rotate_left(1);
        self.0.front().copied()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

impl GameMap {
    /// A fresh shared patrol point with `action`; its id.
    pub(crate) fn new_patrol_point(&mut self, action: PatrolAction) -> PatrolPointId {
        let id = self.new_id_of();
        self.patrol_points.insert(id, PatrolPoint { action });
        id
    }

    /// The order a patrol point resolves to, if it (and what it targets) still exists.
    fn patrol_order(&self, point: PatrolPointId) -> Option<Order> {
        match self.patrol_points.get(point)?.action {
            PatrolAction::GoTo(hp) => self.hidden_points.get(hp).map(|p| Order::MoveTo { x: p.x, z: p.z }),
            PatrolAction::Attack(target) => Some(Order::Attack { target }),
        }
    }

    /// `SetPatrol`/`SetPatrolTarget` (the first point of a new sequence): clears each of the player's
    /// own, not locked, not aboard `units`' order and patrol, shares one new point on all of them,
    /// starts it at once (a direct order begins immediately).
    pub(crate) fn set_patrol(&mut self, player: TribeId, units: &[UnitId], action: PatrolAction) {
        let point = self.new_patrol_point(action);
        for &id in units {
            let Some(i) = self.unit_index(id).filter(|&i| self.units[i].owner == player && !self.locked(i) && self.units[i].aboard.is_none()) else { continue };
            self.units[i].clear_queue();
            self.units[i].clear_patrol();
            if self.units[i].add_patrol_point(point)
                && let Some(order) = self.patrol_order(point)
            {
                self.start_order(i, order);
            }
        }
    }

    /// `AddPatrolPoint`/`AddPatrolTargetPoint` (every point after the first): a fresh shared point
    /// appended to each eligible unit's existing patrol (a no-op per unit already at `PATROL_CAPACITY`).
    pub(crate) fn add_patrol_point_to(&mut self, player: TribeId, units: &[UnitId], action: PatrolAction) {
        let point = self.new_patrol_point(action);
        for &id in units {
            if let Some(i) = self.unit_index(id).filter(|&i| self.units[i].owner == player && !self.locked(i) && self.units[i].aboard.is_none()) {
                self.units[i].add_patrol_point(point);
            }
        }
    }

    /// `ReleasePatrol` (`N`): every one of the player's own `units` drops its whole patrol.
    pub(crate) fn release_patrol(&mut self, player: TribeId, units: &[UnitId]) {
        for &id in units {
            if let Some(i) = self.unit_index(id).filter(|&i| self.units[i].owner == player) {
                self.units[i].clear_patrol();
            }
        }
    }

    /// Every free unit with a patrol moves on to its next point: the one just reached goes to the back,
    /// the new front started — unless it is a `GoTo` already standing exactly there (the stationary
    /// guard post), which is left alone rather than replanning a walk that goes nowhere.
    pub(crate) fn advance_patrols(&mut self) {
        for i in 0..self.units.len() {
            if !self.units[i].is_free() || self.units[i].queued_patrol().is_empty() {
                continue;
            }
            let Some(point) = self.units[i].patrol_advance() else { continue };
            let Some(order) = self.patrol_order(point) else { continue };
            if let Order::MoveTo { x, z } = order
                && (x, z) == (self.units[i].x, self.units[i].z)
            {
                continue;
            }
            self.start_order(i, order);
        }
    }

    /// Patrol points no longer referenced by any alive unit's own patrol are pruned (`GameMap::tick`,
    /// next to `release_orphans`, before `prune_hidden_points`: its result feeds that one).
    pub(crate) fn prune_patrol_points(&mut self) {
        let referenced: BTreeSet<PatrolPointId> = self.units.iter().filter(|u| u.is_alive()).flat_map(|u| u.queued_patrol().iter()).collect();
        self.patrol_points.retain(|id, _| referenced.contains(&id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::Id;

    fn id(n: u32) -> PatrolPointId {
        Id::from_raw(n)
    }

    #[test]
    fn advances_rotate_and_a_single_point_keeps_returning_itself() {
        let mut q = PatrolQueue::default();
        assert_eq!(q.advance(), None, "empty");
        assert!(q.push_back(id(1)));
        assert_eq!(q.advance(), Some(id(1)), "the only point, over and over");
        assert_eq!(q.advance(), Some(id(1)));
        assert!(q.push_back(id(2)));
        assert_eq!(q.iter().collect::<Vec<_>>(), [id(1), id(2)]);
        assert_eq!(q.advance(), Some(id(2)), "1 reached, cycles to 2");
        assert_eq!(q.advance(), Some(id(1)), "2 reached, back to 1");
    }

    #[test]
    fn refuses_past_capacity() {
        let mut q = PatrolQueue::default();
        for n in 0..PATROL_CAPACITY as u32 {
            assert!(q.push_back(id(n)), "{n}");
        }
        assert!(q.is_full());
        assert!(!q.push_back(id(99)));
    }

    #[test]
    fn clear_empties_it() {
        let mut q = PatrolQueue::default();
        q.push_back(id(1));
        q.clear();
        assert!(q.is_empty());
        assert_eq!(q.advance(), None);
    }
}
