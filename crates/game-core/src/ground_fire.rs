//! A short fire on the ground (pop3-rev-analysis.md "Components": the fire 7/0x0A lightning lights where it burns no
//! tree), an effect only seen: it does nothing to the world, the client draws it for its length.

use crate::event::WorldEvent;
use crate::effect::{Effect, EffectKind};
use crate::time::{Countdown, Ticks};
use crate::tree::Tree;
use crate::unit::cell_of;
use crate::tribes::Owner;

/// How long it burns: brief, like the blast's own explosion burst it lands alongside (seen directly in-game; the
/// original's own burn-length fields were never decoded by trace, pop3-rev-analysis.md "Fireball impact
/// (Firestorm, Blast)", so the exact turn count stays a guess).
pub const TICKS: Ticks = Ticks::new(9);

/// A fire lit by tribe `owner` at `at` (world units).
pub fn effect(owner: impl Into<Owner>, at: (u16, u16)) -> Effect {
    Effect::new(owner, at, Countdown::new(TICKS), EffectKind::GroundFire)
}

/// The "burn cell" mechanic (pop3-rev-analysis.md "Components" "Burn cell", 7/0x05; shared by Lightning's own
/// strike and Blast's own cast chain, both of which spawn this same effect): once, on the cell under `at`, a tree
/// with wood left there catches fire, otherwise a short ground fire is lit.
pub fn burn_cell(owner: impl Into<Owner>, at: (u16, u16), trees: &[Tree]) -> WorldEvent {
    let cell = (cell_of(at.0), cell_of(at.1));
    match trees.iter().any(|t| t.cell() == cell && t.wood() > 0) {
        true => WorldEvent::BurnTrees { cell },
        false => WorldEvent::SpawnEffect(effect(owner, at)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tribes::TribeId;

    #[test]
    fn burns_a_tree_there_or_else_lights_a_ground_fire() {
        let at = (1024, 1024);
        let wooded = Tree::new((2, 2), 0, 3);
        assert_eq!(burn_cell(TribeId::Blue, at, &[wooded]), WorldEvent::BurnTrees { cell: (2, 2) });
        let bare = Tree::new((2, 2), 0, 0);
        assert_eq!(burn_cell(TribeId::Blue, at, &[bare]), WorldEvent::SpawnEffect(effect(TribeId::Blue, at)), "no wood left: not a tree to burn");
        assert_eq!(burn_cell(TribeId::Blue, at, &[]), WorldEvent::SpawnEffect(effect(TribeId::Blue, at)));
    }
}
