//! An invisible, stationary point on the ground: a bare position that needs its own id so a patrol's
//! move-type point can target "something with an id" like every other order target (units.md "Patrol
//! orders"), instead of the order machinery special-casing a bare position. Reusable later for AI
//! script markers.

use crate::id::Id;
use crate::patrol::PatrolAction;
use std::collections::BTreeSet;

pub type HiddenPointId = Id<HiddenPoint>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HiddenPoint {
    /// World units.
    pub x: u16,
    pub z: u16,
}

impl crate::map::GameMap {
    /// Adds a hidden point at `(x, z)` with a fresh id; its id.
    pub(crate) fn add_hidden_point(&mut self, x: u16, z: u16) -> HiddenPointId {
        let id = self.new_id_of();
        self.hidden_points.insert(id, HiddenPoint { x, z });
        id
    }

    /// Hidden points no longer referenced by any surviving patrol point's own `GoTo` are pruned
    /// (`GameMap::tick`, right after `prune_patrol_points`, whose result this depends on).
    pub(crate) fn prune_hidden_points(&mut self) {
        let referenced: BTreeSet<HiddenPointId> = self
            .patrol_points
            .iter()
            .filter_map(|(_, p)| match p.action {
                PatrolAction::GoTo(hp) => Some(hp),
                PatrolAction::Attack(_) => None,
            })
            .collect();
        self.hidden_points.retain(|id, _| referenced.contains(&id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::GameMap;

    #[test]
    fn adds_a_point_with_a_fresh_id() {
        let mut m = GameMap::sandbox_units();
        let a = m.add_hidden_point(10, 20);
        let b = m.add_hidden_point(30, 40);
        assert_ne!(a, b);
        assert_eq!(m.hidden_points.get(a), Some(&HiddenPoint { x: 10, z: 20 }));
        assert_eq!(m.hidden_points.get(b), Some(&HiddenPoint { x: 30, z: 40 }));
    }
}
