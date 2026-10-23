//! Erode (pop3-rev-analysis.md "Erosion passes"), an effect (`effect::EffectKind::Erode`): `PASSES` turns of a
//! falloff brush (the same shape `terrain::Heightmap::raise` already uses), each one jittered a few cells from the
//! cast point, lowering the ground there towards sea level over `DURATION` turns. docs/plans/spells.md "Erosion
//! (the real hydraulic version)": the original's own per-cell steepest-descent slope pick is confirmed not needed
//! ("approximate algorithm confirmed fine... just a slower/deeper/longer-duration version of the falloff pattern
//! already there") — this reuses `raise` verbatim rather than a new hydraulic simulation.

use crate::effect::{Effect, EffectKind, TerrainChange};
use crate::event::WorldEvent;
use crate::map::Lcg;
use crate::spell::Cell;
use crate::terrain::{DirtyRect, Heightmap, SEA_LEVEL};
use crate::time::{Countdown, Ticks};
use pop3_format::WORLD_UNITS_PER_CELL;

/// Turns the effect lasts (the original's own length, traced).
pub const DURATION: u32 = 64;
/// Actual erosion passes: the last tick just ends it (traced, pop3-rev-analysis.md "Erosion passes").
pub const PASSES: u32 = DURATION - 1;
/// The falloff brush's reach each pass (a guess: the old one-shot brush's own radius).
pub const RADIUS_CELLS: i32 = 4;
/// Jitter `-4..+3` cells each axis (two draws, `& 7 - 4`, matching the trace's own two-PRNG-draw jitter shape, but
/// in whole cells rather than the original's half-cells: our own rounding, a guess).
pub const JITTER_CELLS: i32 = 4;
/// Height lost at a pass's own centre (a guess: the original's write-back magnitude is still untraced).
pub const AMOUNT: i32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Erode {
    pub at: Cell,
    /// Passes done so far.
    turn: u32,
}

/// An erosion cast by tribe `owner` on the height point `at` of `terrain`.
pub fn effect(owner: u8, at: Cell, terrain: &Heightmap) -> Effect {
    let erode = Erode { at, turn: 0 };
    let pos = |c: i32| (c.rem_euclid(terrain.size() as i32) as u32 * WORLD_UNITS_PER_CELL) as u16;
    Effect::new(owner, (pos(at.0), pos(at.1)), Countdown::new(Ticks::new(DURATION)), EffectKind::Erode(erode))
}

impl Erode {
    /// One turn: a jittered pass now, unless this is the last tick (traced: it only ends the effect).
    pub fn turn(&mut self, size: i32, rng: &mut Lcg) -> Vec<WorldEvent> {
        let mut events = Vec::new();
        if self.turn < PASSES {
            let jitter = |rng: &mut Lcg| (rng.next_u32() & 7) as i32 - JITTER_CELLS;
            let at = ((self.at.0 + jitter(rng)).rem_euclid(size), (self.at.1 + jitter(rng)).rem_euclid(size));
            events.push(WorldEvent::Terrain(TerrainChange::Erode { at }));
        }
        self.turn += 1;
        events
    }

    /// The cells still within reach of a future pass that have not yet eroded down to sea level.
    pub fn reshaping(&self, terrain: &Heightmap) -> Vec<Cell> {
        if self.turn >= PASSES {
            return Vec::new();
        }
        let r = JITTER_CELLS + RADIUS_CELLS;
        (-r..=r)
            .flat_map(|dz| (-r..=r).map(move |dx| (self.at.0 + dx, self.at.1 + dz)))
            .filter(|&(x, z)| terrain.get(x, z) != SEA_LEVEL)
            .collect()
    }
}

/// One jittered pass of a falloff brush at `at`; the region it touched.
pub fn apply(terrain: &mut Heightmap, at: Cell) -> DirtyRect {
    terrain.raise(at, RADIUS_CELLS, -AMOUNT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::GameMap;

    fn build(terrain: &mut Heightmap, at: Cell, turns: u32, rng: &mut Lcg) -> (Erode, u32) {
        let Effect { kind: EffectKind::Erode(mut erode), .. } = effect(0, at, terrain) else { unreachable!() };
        let mut passes = 0;
        for _ in 0..turns {
            for e in erode.turn(terrain.size() as i32, rng) {
                if let WorldEvent::Terrain(TerrainChange::Erode { at }) = e {
                    apply(terrain, at);
                    passes += 1;
                }
            }
        }
        (erode, passes)
    }

    #[test]
    fn exactly_63_passes_over_64_turns_the_last_tick_does_nothing() {
        let mut terrain = Heightmap::new(32);
        for z in 0..32 {
            for x in 0..32 {
                terrain.set(x, z, 500);
            }
        }
        let mut rng = Lcg(1);
        let (mut erode, passes) = build(&mut terrain, (16, 16), DURATION, &mut rng);
        assert_eq!(passes, PASSES);
        assert_eq!(erode.turn(32, &mut rng), Vec::new(), "done: no more passes");
    }

    #[test]
    fn never_goes_below_sea_level() {
        let mut terrain = Heightmap::new(32);
        for z in 0..32 {
            for x in 0..32 {
                terrain.set(x, z, 20);
            }
        }
        let mut rng = Lcg(42);
        build(&mut terrain, (16, 16), DURATION, &mut rng);
        assert_eq!(terrain.get(16, 16), SEA_LEVEL, "the centre settles at sea level, never below (u16 can't anyway)");
    }

    #[test]
    fn the_same_seed_erodes_the_same_shape() {
        let run = || {
            let mut terrain = Heightmap::new(32);
            for z in 0..32 {
                for x in 0..32 {
                    terrain.set(x, z, 500);
                }
            }
            let mut rng = Lcg(7);
            build(&mut terrain, (16, 16), DURATION, &mut rng);
            terrain.heights().to_vec()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn reshaping_sparkles_while_passes_remain_and_drops_sea_level_cells() {
        let mut terrain = Heightmap::new(32);
        for z in 0..32 {
            for x in 0..32 {
                terrain.set(x, z, 500);
            }
        }
        terrain.set(16, 16, SEA_LEVEL);
        let Effect { kind: EffectKind::Erode(mut erode), .. } = effect(0, (16, 16), &terrain) else { unreachable!() };
        let points = erode.reshaping(&terrain);
        assert!(!points.is_empty() && !points.contains(&(16, 16)), "already at sea level: dropped");
        erode.turn = PASSES;
        assert!(erode.reshaping(&terrain).is_empty(), "no more passes left: nothing still reshaping");
    }

    #[test]
    fn casting_it_through_the_spell_book_adds_the_controller_effect() {
        use crate::command::Command;
        use crate::spell::Spell;
        use crate::spell_book::{Availability, SpellKind};

        let mut map = GameMap::sandbox_walk();
        map.spell_books[0].set(SpellKind::Erosion, Availability::Unlimited);
        let at = (map.units[0].x, map.units[0].z);
        let cell = (crate::unit::cell_of(at.0), crate::unit::cell_of(at.1));
        map.apply(&Command::Cast { player: 0, spell: Spell::Erode { at: cell } });
        map.run(crate::unit::CAST_RELEASE);
        assert!(map.effects.entries().any(|(_, e)| matches!(e.kind, EffectKind::Erode(_))));
    }
}
