//! Swamp (pop3-rev-analysis.md "Per spell" 11), an effect (`effect::EffectKind::Swamp`): no terrain change, lasts
//! `LIFETIME` turns. Every `KILL_EVERY` turns it kills outright everyone standing on its 3 × 3 cells, any tribe,
//! even the caster's own, shielded or not (`effect::hurts` never gates `DamageKind::Swamp`, TODO.md "Mana"), and
//! inside a building there too — unlike lightning, it does not spare the people a building's walls hide.
//!
//! Several can stand on the same cell, any player's: unlike every other effect, a swamp is not self-contained, so
//! the kill itself is not its own turn but `GameMap::tend_swamps`, which sees every swamp at once and settles each
//! contested cell on the most recently cast one there (ids only grow, so the highest id is the newest). That one's
//! own tally is the one that grows; once it reaches `SWAMP_NUM_PEOPLE` it ends there and then, short of its full
//! `LIFETIME`. The 30-per-tribe, oldest-goes bookkeeping the original also keeps (pop3-rev-analysis.md) is not
//! implemented (TODO.md).

use crate::effect::{Effect, EffectKind};
use crate::time::{Countdown, Every, Ticks};
use crate::unit::cell_of;

/// `SWAMP_NUM_PEOPLE` (exe 10): a swamp's own kills, after which it ends early.
pub const SWAMP_NUM_PEOPLE: u32 = 10;
/// A kill check every 4 turns (file and exe), the same turns for every swamp at once (phase 0).
pub const KILL_EVERY: Every = Every::new(4);
/// 32 000 turns (44 min), its full lifetime absent the kill cap.
pub const LIFETIME: Ticks = Ticks::new(32_000);
/// Half the side of its 3 × 3 cells.
pub const HALF: i32 = 1;

/// One swamp's own state: who it has killed so far.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Swamp {
    pub processed: u32,
}

/// A swamp cast by tribe `owner` at `at` (world units).
pub fn effect(owner: u8, at: (u16, u16)) -> Effect {
    Effect::new(owner, at, Countdown::new(LIFETIME), EffectKind::Swamp(Swamp::default()))
}

/// The cells (wrapped, a `size`-cell-wide torus) a swamp at `at` covers: its own cell and the ring around it.
pub fn cells((x, z): (u16, u16), size: i32) -> Vec<(i32, i32)> {
    let (cx, cz) = (cell_of(x), cell_of(z));
    (-HALF..=HALF).flat_map(|dz| (-HALF..=HALF).map(move |dx| ((cx + dx).rem_euclid(size), (cz + dz).rem_euclid(size)))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::GameMap;
    use crate::status::{Timed, SHIELD_STEPS};
    use crate::terrain::Heightmap;
    use crate::unit::UnitKind;

    #[test]
    fn covers_its_cell_and_the_ring_around_it_wrapped_at_the_edge() {
        let at = cells((0, 0), 128);
        assert_eq!(at.len(), 9);
        assert!(at.contains(&(0, 0)) && at.contains(&(1, 1)) && at.contains(&(127, 127)), "wraps past the map's edge");
    }

    const AT: (u16, u16) = (10 * 512 + 256, 20 * 512 + 256);

    fn bare() -> GameMap {
        GameMap::bare("swamp-test", Heightmap::new(128), Vec::new())
    }

    #[test]
    fn kills_everyone_on_its_cells_every_four_turns_any_tribe_shielded_or_inside_a_building() {
        let mut map = bare();
        map.add_effect(effect(0, AT));
        let own = map.spawn(0, UnitKind::Brave, AT).id;
        let foe = map.spawn(1, UnitKind::Brave, AT).id;
        let shielded = map.spawn(1, UnitKind::Brave, AT).id;
        map.unit_mut(shielded).unwrap().statuses.shield = Timed::steps(SHIELD_STEPS);
        let inside = map.spawn(1, UnitKind::Brave, AT).id;
        map.unit_mut(inside).unwrap().inside = Some(crate::unit::Inside { building: crate::id::Id::from_raw(50), door: AT });
        let all = [own, foe, shielded, inside];
        map.run(Ticks::new(3));
        assert!(all.iter().all(|&u| map.unit(u).unwrap().is_alive()), "not yet its turn");
        map.tick();
        assert!(all.iter().all(|&u| !map.unit(u).unwrap().is_alive()), "its own tribe, a shielded enemy, and one hidden inside a building: none spared");
    }

    #[test]
    fn the_later_cast_swamp_claims_an_overlapping_cell_and_its_tally_grows() {
        let mut map = bare();
        let first = map.add_effect(effect(0, AT));
        let second = map.add_effect(effect(1, AT));
        map.spawn(2, UnitKind::Brave, AT);
        map.run(Ticks::new(4));
        let processed = |e: &GameMap, id| match e.effects.get(id).map(|e| &e.kind) {
            Some(EffectKind::Swamp(s)) => s.processed,
            _ => panic!("gone or not a swamp"),
        };
        assert_eq!((processed(&map, first), processed(&map, second)), (0, 1), "the later one claims the overlap, not the earlier");
    }

    #[test]
    fn ends_once_its_own_tally_reaches_swamp_num_people_short_of_its_full_lifetime() {
        let mut map = bare();
        let id = map.add_effect(effect(0, AT));
        for n in 0..SWAMP_NUM_PEOPLE {
            map.spawn(1, UnitKind::Brave, (AT.0.wrapping_add(n as u16), AT.1));
        }
        map.run(Ticks::new(4));
        assert!(!map.effects.contains(id), "ended early, long before its 32 000-turn lifetime");
    }
}
