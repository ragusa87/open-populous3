//! Convert (pop3-rev-analysis.md "Per spell" 17), an effect (`effect::EffectKind::Convert`): aimed at a point like
//! a blast, it lasts `LIFETIME` turns regardless of success (confirmed, docs/plans/spells.md: not an indefinite
//! wait for a first conversion). Every `CHECK_EVERY` turns it looks for the nearest wildman within `RANGE_CELLS`
//! of the cast point and makes it a brave of the caster (`WorldEvent::ChangeOwner`, the same event preacher
//! conversion and the reincarnation site's free welcome already use), if the tribe has room
//! (`headcount::MAX_POPULATION`, the same cap as hut breeding and preacher conversion).

use crate::effect::{Effect, EffectKind};
use crate::event::WorldEvent;
use crate::headcount::MAX_POPULATION;
use crate::map::GameMap;
use crate::time::{Countdown, Ticks};
use crate::unit::{torus_delta, Unit, UnitId, UnitKind};
use pop3_format::WORLD_UNITS_PER_CELL;
use crate::tribes::Owner;

/// A conversion chance every 8 turns: its own internal countdown, not a global-phase `Every` (unlike Swamp, no
/// other Convert needs to stay in step with this one).
pub const CHECK_EVERY: Ticks = Ticks::new(8);
/// The whole effect's lifetime (confirmed, docs/plans/spells.md): expires here regardless of success.
pub const LIFETIME: Ticks = Ticks::new(18);
/// Only a wildman within this many cells of the cast point is eligible.
pub const RANGE_CELLS: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Convert {
    /// Turns left to the next conversion chance.
    next: Countdown,
}

/// Cast by tribe `owner` at `at` (world units).
pub fn effect(owner: impl Into<Owner>, at: (u16, u16)) -> Effect {
    Effect::new(owner, at, Countdown::new(LIFETIME), EffectKind::Convert(Convert { next: Countdown::new(CHECK_EVERY) }))
}

impl Convert {
    /// One turn at `at`, cast by `owner`: every `CHECK_EVERY` turns, the nearest wildman in range becomes a brave
    /// of `owner`, if its tribe has room.
    pub fn turn(&mut self, at: (u16, u16), owner: Owner, world: &GameMap) -> Vec<WorldEvent> {
        if !self.next.tick() {
            return Vec::new();
        }
        self.next = Countdown::new(CHECK_EVERY);
        let Some(tribe) = owner.tribe() else { return Vec::new() };
        if world.housing(tribe).population >= MAX_POPULATION {
            return Vec::new();
        }
        nearest_wildman(at, &world.units).map(|unit| vec![WorldEvent::ChangeOwner { unit, tribe }]).unwrap_or_default()
    }
}

/// The nearest living wildman to `at`, within `RANGE_CELLS`, if any (`nearest::nearest_n`, the shared selector).
fn nearest_wildman(at: (u16, u16), units: &[Unit]) -> Option<UnitId> {
    let range = RANGE_CELLS * WORLD_UNITS_PER_CELL;
    let in_range = |u: &Unit| {
        let (dx, dz) = (torus_delta(at.0, u.x) as i64, torus_delta(at.1, u.z) as i64);
        (dx * dx + dz * dz) as u64 <= (range as u64) * (range as u64)
    };
    let pred = |u: &Unit| u.kind == UnitKind::Wildman && u.is_alive() && in_range(u);
    crate::nearest::nearest_n(units, at, 1, pred).into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tribes::TribeId;
    use crate::id::Id;
    use crate::terrain::Heightmap;

    const AT: (u16, u16) = (10 * WORLD_UNITS_PER_CELL as u16 + 256, 20 * WORLD_UNITS_PER_CELL as u16 + 256);

    fn bare() -> GameMap {
        GameMap::bare("convert-test", Heightmap::new(128), Vec::new())
    }

    #[test]
    fn converts_the_nearest_wildman_in_range_every_8_turns() {
        let mut map = bare();
        map.add_effect(effect(TribeId::Blue, AT));
        let near = map.spawn(Owner::Wild, UnitKind::Wildman, (AT.0.wrapping_add(100), AT.1)).id;
        let far = map.spawn(Owner::Wild, UnitKind::Wildman, (AT.0.wrapping_add(2 * WORLD_UNITS_PER_CELL as u16), AT.1)).id;
        map.run(Ticks::new(7));
        assert_eq!(map.unit(near).unwrap().owner, Owner::Wild, "not yet its turn");
        map.tick();
        assert_eq!(map.unit(near).unwrap().owner, Owner::Tribe(TribeId::Blue), "the nearest in range, converted");
        assert_eq!(map.unit(near).unwrap().kind, UnitKind::Brave);
        assert_eq!(map.unit(far).unwrap().owner, Owner::Wild, "out of range: spared");
    }

    #[test]
    fn expires_after_its_lifetime_even_without_a_single_conversion() {
        let mut map = bare();
        let id = map.add_effect(effect(TribeId::Blue, AT));
        map.run(Ticks::new(LIFETIME.get()));
        assert!(!map.effects.contains(id), "gone regardless of success");
    }

    #[test]
    fn never_converts_once_the_tribe_is_full() {
        let mut map = bare();
        map.add_effect(effect(TribeId::Blue, AT));
        for n in 0..MAX_POPULATION {
            map.spawn(TribeId::Blue, UnitKind::Brave, (0, n));
        }
        let wildman = map.spawn(Owner::Wild, UnitKind::Wildman, (AT.0.wrapping_add(10), AT.1)).id;
        map.run(Ticks::new(8));
        assert_eq!(map.unit(wildman).unwrap().owner, Owner::Wild, "no room");
    }

    #[test]
    fn nearest_wildman_ignores_the_dead_and_other_kinds() {
        let mut units = vec![Unit::new(Id::from_raw(1), Owner::Wild, UnitKind::Brave, AT), Unit::new(Id::from_raw(2), Owner::Wild, UnitKind::Wildman, AT)];
        units[1].action = crate::unit::Action::Dead { left: Countdown::new(Ticks::new(3)) };
        assert_eq!(nearest_wildman(AT, &units), None, "a brave and a dead wildman: neither counts");
        let alive = Unit::new(Id::from_raw(3), Owner::Wild, UnitKind::Wildman, AT);
        units.push(alive);
        assert_eq!(nearest_wildman(AT, &units), Some(Id::from_raw(3)));
    }
}
