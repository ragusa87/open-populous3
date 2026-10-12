//! Changes to the world that an object makes to something else (architecture.md "The tick"): effects and
//! coordinators return them, the map applies each with its one rule (`GameMap::apply_event`), in order.

use crate::effect::{DamageKind, Effect, TerrainChange};
use crate::motion::Velocity;
use std::collections::VecDeque;

/// One change, applied by the map. A unit or building gone meanwhile (its id missing) is skipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldEvent {
    /// Hurts through `GameMap::damage`, the one hurt path; `from` is the attacker's tribe, if any.
    Damage { unit: u32, amount: u16, kind: DamageKind, from: Option<u8> },
    Push { unit: u32, vel: Velocity },
    Panic { unit: u32 },
    Raid { building: u32 },
    SetFire { building: u32 },
    /// Sets the trees of the cell on fire (`Tree::ignite`).
    BurnTrees { cell: (i32, i32) },
    /// A new effect, ticking from the next turn; its id is given then.
    SpawnEffect(Effect),
    Terrain(TerrainChange),
    /// The unit leaves its coordinator (`Unit::release`); nothing if it is in none.
    Release { unit: u32 },
}

/// Applies `batch` in order with `apply`; what `apply` returns comes after the whole batch, never in its middle.
pub fn run_in_order(batch: Vec<WorldEvent>, mut apply: impl FnMut(WorldEvent) -> Vec<WorldEvent>) {
    let mut queue = VecDeque::from(batch);
    while let Some(event) = queue.pop_front() {
        queue.extend(apply(event));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_emission_order_follow_ups_after_the_batch() {
        let panic = |unit| WorldEvent::Panic { unit };
        let mut applied = Vec::new();
        run_in_order(vec![panic(1), panic(2), panic(3)], |e| {
            applied.push(e.clone());
            match e {
                WorldEvent::Panic { unit: 1 } => vec![panic(10), panic(11)],
                WorldEvent::Panic { unit: 10 } => vec![panic(20)],
                _ => Vec::new(),
            }
        });
        assert_eq!(applied, [1, 2, 3, 10, 11, 20].map(panic));
    }
}
