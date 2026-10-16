//! Earthquake (pop3-rev-analysis.md "Per spell" 14), a timed controller effect (`effect::EffectKind::Earthquake`),
//! `DURATION_TURNS` (120, 10 s, file-confirmed) of shake. At `SINK_AT` (60, confirmed) turns in, once: the ground
//! within `SINK_RADIUS_CELLS` (8) sinks by up to `SINK_AMOUNT` (600), reusing the existing `raise` brush
//! (`WorldEvent::Terrain(TerrainChange::Raise)`, `by: -SINK_AMOUNT`) rather than the trace's own distinct
//! `600 × (4096 − d) / 4096` falloff — an approximation, the same call Erode already makes with the same brush — and
//! a lava fissure opens at the centre (`lava::effect`, the fixed patch this module names it for, see its own doc)
//! over `FISSURE_RADIUS_CELLS` (2, a guess: no size given) for `FISSURE_LIFETIME` ("short-lived", a guess, no
//! number given either).
//!
//! Not yet: the three building-shake checks at turns 24, 56 and 88 (20/128 chance per building in 16 × 16 cells,
//! occupants out, a partial explosion) — they need `Raid` (`event.rs`), itself still a stub; and the trace's
//! "possibly two lava flows", not confirmed enough to add a second patch.

use crate::effect::{Effect, EffectKind, TerrainChange};
use crate::event::WorldEvent;
use crate::map::GameMap;
use crate::time::{Countdown, Ticks};
use crate::unit::cell_of;

pub const DURATION_TURNS: u32 = 120;
pub const SINK_AT: u32 = 60;
pub const SINK_RADIUS_CELLS: i32 = 8;
pub const SINK_AMOUNT: i32 = 600;
pub const FISSURE_RADIUS_CELLS: f32 = 2.0;
/// "Short-lived" (a guess, no number given): a little longer than Volcano's own fireball shots live.
pub const FISSURE_LIFETIME: Ticks = Ticks::secs(5);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Earthquake {
    /// Turns done.
    turn: u32,
}

/// Cast by tribe `owner` at `at` (world units).
pub fn effect(owner: u8, at: (u16, u16)) -> Effect {
    Effect::new(owner, at, Countdown::new(Ticks::new(DURATION_TURNS)), EffectKind::Earthquake(Earthquake::default()))
}

impl Earthquake {
    /// One turn at `at`, cast by `owner`: the ground sinks and the fissure opens once, at `SINK_AT`.
    pub fn turn(&mut self, at: (u16, u16), owner: u8, world: &GameMap) -> Vec<WorldEvent> {
        let mut events = Vec::new();
        if self.turn == SINK_AT {
            let cell = (cell_of(at.0), cell_of(at.1));
            events.push(WorldEvent::Terrain(TerrainChange::Raise { at: cell, radius: SINK_RADIUS_CELLS, by: -SINK_AMOUNT }));
            let size = world.terrain.size() as i32;
            let fissure = crate::lava::disc(at, FISSURE_RADIUS_CELLS, size);
            events.push(WorldEvent::SpawnEffect(crate::lava::effect(owner, at, fissure, Countdown::new(FISSURE_LIFETIME))));
        }
        self.turn += 1;
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::spell::Spell;
    use crate::spell_book::{Availability, SpellKind};
    use crate::terrain::Heightmap;

    const AT: (u16, u16) = (64 * pop3_format::WORLD_UNITS_PER_CELL as u16 + 256, 64 * pop3_format::WORLD_UNITS_PER_CELL as u16 + 256);

    fn bare() -> GameMap {
        let mut terrain = Heightmap::new(128);
        for z in 0..128 {
            for x in 0..128 {
                terrain.set(x, z, 700);
            }
        }
        GameMap::bare("earthquake-test", terrain, Vec::new())
    }

    #[test]
    fn shakes_without_touching_the_terrain_until_turn_60() {
        let mut map = bare();
        map.add_effect(effect(0, AT));
        map.run(Ticks::new(SINK_AT));
        assert_eq!(map.terrain.get(64, 64), 700, "still just shaking");
    }

    #[test]
    fn sinks_the_centre_and_opens_a_lava_fissure_at_turn_60() {
        let mut map = bare();
        map.add_effect(effect(0, AT));
        map.run(Ticks::new(SINK_AT + 1));
        assert!(map.terrain.get(64, 64) < 700, "the centre sank");
        assert!(map.effects.entries().any(|(_, e)| matches!(e.kind, EffectKind::Lava(_))), "a fissure opened");
    }

    #[test]
    fn sinks_only_once_even_if_it_kept_running() {
        let mut map = bare();
        map.add_effect(effect(0, AT));
        map.run(Ticks::new(SINK_AT + 1));
        let sunk = map.terrain.get(64, 64);
        map.run(Ticks::new(DURATION_TURNS - SINK_AT - 1));
        assert_eq!(map.terrain.get(64, 64), sunk, "no second sink before the effect ends");
    }

    #[test]
    fn casting_it_through_the_spell_book_adds_the_controller_effect() {
        let mut map = GameMap::sandbox_walk();
        map.spell_books[0].set(SpellKind::Earthquake, Availability::Unlimited);
        let at = (map.units[0].x, map.units[0].z);
        map.apply(&Command::Cast { player: 0, spell: Spell::Earthquake { at } });
        map.run(crate::unit::CAST_RELEASE);
        assert!(map.effects.entries().any(|(_, e)| matches!(e.kind, EffectKind::Earthquake(_))));
    }
}
