//! Changes to the world that an object makes to something else (architecture.md "The tick"): effects and
//! coordinators return them, the map applies each with its one rule (`GameMap::apply_event`), in order.

use crate::building::BuildingId;
use crate::effect::{DamageKind, Effect, TerrainChange};
use crate::motion::Velocity;
use crate::unit::UnitId;
use std::collections::VecDeque;

/// One change, applied by the map. A unit or building gone meanwhile (its id missing) is skipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldEvent {
    /// Hurts through `GameMap::damage`, the one hurt path; `from` is the attacker's tribe, if any.
    Damage { unit: UnitId, amount: u16, kind: DamageKind, from: Option<u8> },
    /// Throws the unit (`Unit::fling`), unless its `kind` of push spares it (`effect::pushes`).
    Push { unit: UnitId, vel: Velocity, kind: PushKind },
    Panic { unit: UnitId },
    Raid { building: BuildingId },
    SetFire { building: BuildingId },
    /// Sets the trees of the cell on fire (`Tree::ignite`).
    BurnTrees { cell: (i32, i32) },
    /// A new effect, ticking from the next turn; its id is given then.
    SpawnEffect(Effect),
    Terrain(TerrainChange),
    /// A coordinator tells one of its units where to go (a charge's target, a melee's place).
    Placed { unit: UnitId, at: (u16, u16) },
    /// The attacker of a challenge starts its shove (`GameMap::shove`).
    Shove { attacker: UnitId },
    /// A blow in a melee (`GameMap::struck`); `knock`: the one struck is thrown back so, once it has reeled.
    Struck { attacker: UnitId, target: UnitId, blow: crate::combat::Blow, knock: Option<Velocity> },
    /// A challenge won: the two fight in a new melee (`GameMap::start_melee`).
    StartMelee { attacker: UnitId, target: UnitId },
    /// The unit leaves its coordinator (`Unit::release`); nothing if it is in none.
    Release { unit: UnitId },
    /// A firewarrior hit by `thrower`'s bolt fires back: at once on its feet, else once it has landed.
    FireBack { unit: UnitId, thrower: UnitId },
}

/// What throws a unit, for its immunities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushKind {
    Spell,
    /// A fighter's shove or knock: the shield does not stop it, and it stays in its fight (combat.md).
    Melee,
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
    use crate::id::Id;

    #[test]
    fn in_emission_order_follow_ups_after_the_batch() {
        let panic = |unit| WorldEvent::Panic { unit: Id::from_raw(unit) };
        let mut applied = Vec::new();
        run_in_order(vec![panic(1), panic(2), panic(3)], |e| {
            applied.push(e.clone());
            match e {
                WorldEvent::Panic { unit } if unit == Id::from_raw(1) => vec![panic(10), panic(11)],
                WorldEvent::Panic { unit } if unit == Id::from_raw(10) => vec![panic(20)],
                _ => Vec::new(),
            }
        });
        assert_eq!(applied, [1, 2, 3, 10, 11, 20].map(panic));
    }
}
