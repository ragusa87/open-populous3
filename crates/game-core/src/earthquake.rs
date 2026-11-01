//! Earthquake (pop3-rev-analysis.md "Per spell" 14), a timed controller effect (`effect::EffectKind::Earthquake`),
//! `DURATION_TURNS` (120, 10 s, file-confirmed) of shake. At `SINK_AT` (60, confirmed) turns in, once: the ground
//! within `SINK_RADIUS_CELLS` (8) sinks by up to `SINK_AMOUNT` (600), reusing the existing `raise` brush
//! (`WorldEvent::Terrain(TerrainChange::Raise)`, `by: -SINK_AMOUNT`) rather than the trace's own distinct
//! `600 × (4096 − d) / 4096` falloff — an approximation, the same call Erode already makes with the same brush — and
//! a lava fissure opens at the centre (`lava::effect`, the "lava square" patch this module names it for, see its
//! own doc: it decays and self-destructs on its own, no lifetime of our own to guess any more) over
//! `FISSURE_RADIUS_CELLS` (2, a guess: no size given). The "shake" itself is drawn client-side only
//! (`ground_shake.rs` [client], `ground_shake`, below): a trembling patch of ground, shared with Volcano's own
//! pre-eruption rumble the same way `lava` is shared by their aftermaths — its magnitude and reach are our own
//! guess, untraced.
//!
//! At `SHAKE_TURNS` (24, 56 and 88, confirmed), each building within `SHAKE_HALF_WIDTH_CELLS` (8, "16 × 16 cells")
//! of the cast point draws a `SHAKE_CHANCE` in `SHAKE_CHANCE_OUT_OF` (20/128) roll (pop3-rev-analysis.md "Per
//! spell" 14 and its own more detailed function-level row): a hit costs it one piece of wood outright (`[exe]`:
//! not the ordinary wear-threshold model "Attacking a building" uses, a direct loss every time, same primitive as
//! fire's own burn tick, `Building::lose_a_piece`), a large building's occupants thrown out regardless
//! (`wreck::large`, same rule a wear hit already follows) and, only if that was its last piece, a partial
//! explosion (confirmed: "then a partial explosion, yes if the wood runs out" - not on every hit). `WorldEvent::Shake`
//! carries this through the effect's own read-only `tick` (`event.rs`, `GameMap::apply_event`).
//!
//! Left: the trace's "possibly two lava flows", not confirmed enough to add a second patch.

use crate::effect::{Effect, EffectKind, TerrainChange};
use crate::event::WorldEvent;
use crate::map::{GameMap, Lcg};
use crate::time::{Countdown, Ticks};
use crate::unit::{cell_of, torus_delta};
use crate::tribes::Owner;

pub const DURATION_TURNS: u32 = 120;
pub const SINK_AT: u32 = 60;
pub const SINK_RADIUS_CELLS: i32 = 8;
pub const SINK_AMOUNT: i32 = 600;
pub const FISSURE_RADIUS_CELLS: f32 = 2.0;
/// Fraction of `DURATION_TURNS` the shake (`ground_shake`) takes to ramp up to, and back down from, full
/// intensity (a guess, untraced: a quick attack/release around an otherwise steady tremor).
const SHAKE_ATTACK: f32 = 0.06;
const SHAKE_RELEASE: f32 = 0.08;
/// The 3 turns each building in range rolls for a shake [exe, confirmed].
pub const SHAKE_TURNS: [u32; 3] = [24, 56, 88];
/// A building's chance of being shaken, out of `SHAKE_CHANCE_OUT_OF` [exe, confirmed].
pub const SHAKE_CHANCE: u32 = 20;
pub const SHAKE_CHANCE_OUT_OF: u32 = 128;
/// Half the side of the square of cells a shake reaches ("16 × 16 cells" [exe]).
pub const SHAKE_HALF_WIDTH_CELLS: i32 = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Earthquake {
    /// Turns done.
    turn: u32,
}

/// Cast by tribe `owner` at `at` (world units).
pub fn effect(owner: impl Into<Owner>, at: (u16, u16)) -> Effect {
    Effect::new(owner, at, Countdown::new(Ticks::new(DURATION_TURNS)), EffectKind::Earthquake(Earthquake::default()))
}

impl Earthquake {
    /// One turn at `at`, cast by `owner`: the ground sinks and the fissure opens once, at `SINK_AT`; at each of
    /// `SHAKE_TURNS`, every building in range draws its own shake chance.
    pub fn turn(&mut self, at: (u16, u16), owner: Owner, world: &GameMap, rng: &mut Lcg) -> Vec<WorldEvent> {
        let mut events = Vec::new();
        if self.turn == SINK_AT {
            let cell = (cell_of(at.0), cell_of(at.1));
            events.push(WorldEvent::Terrain(TerrainChange::Raise { at: cell, radius: SINK_RADIUS_CELLS, by: -SINK_AMOUNT }));
            let size = world.terrain.size() as i32;
            let fissure = crate::lava::disc(at, FISSURE_RADIUS_CELLS, size);
            events.push(WorldEvent::SpawnEffect(crate::lava::effect(owner, at, fissure)));
        }
        if SHAKE_TURNS.contains(&self.turn) {
            events.extend(self.shake(at, world, rng));
        }
        self.turn += 1;
        events
    }

    /// Every building within `SHAKE_HALF_WIDTH_CELLS` of `at` draws its `SHAKE_CHANCE` in
    /// `SHAKE_CHANCE_OUT_OF` roll, in id order (deterministic for lockstep).
    fn shake(&self, at: (u16, u16), world: &GameMap, rng: &mut Lcg) -> Vec<WorldEvent> {
        let reach = SHAKE_HALF_WIDTH_CELLS * pop3_format::WORLD_UNITS_PER_CELL as i32;
        let in_range = world.buildings.iter().filter(|b| {
            let (x, z) = b.centre();
            torus_delta(at.0, x).abs() <= reach && torus_delta(at.1, z).abs() <= reach
        });
        let ids: Vec<_> = in_range.map(|b| b.id).collect();
        ids.into_iter().filter(|_| rng.next_u32() % SHAKE_CHANCE_OUT_OF < SHAKE_CHANCE).map(|building| WorldEvent::Shake { building }).collect()
    }

    /// The tremor (`effect::GroundShake`) running for the whole effect: a quick ramp up, a steady shake, a quick
    /// ramp down (a guess, untraced: the trace only says "shake").
    pub fn ground_shake(&self) -> Option<crate::effect::GroundShake> {
        let p = self.turn as f32 / DURATION_TURNS as f32;
        let intensity = if p < SHAKE_ATTACK {
            smoothstep(0.0, SHAKE_ATTACK, p)
        } else if p > 1.0 - SHAKE_RELEASE {
            smoothstep(1.0, 1.0 - SHAKE_RELEASE, p)
        } else {
            1.0
        };
        Some(crate::effect::GroundShake { radius_cells: SINK_RADIUS_CELLS as f32, intensity })
    }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tribes::TribeId;
    use crate::building::{Building, BuildingKind};
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
    fn ground_shake_ramps_up_then_holds_then_ramps_down() {
        let mut quake = Earthquake::default();
        assert_eq!(quake.ground_shake().unwrap().intensity, 0.0, "stands still the very first turn");
        quake.turn = DURATION_TURNS / 2;
        assert_eq!(quake.ground_shake().unwrap().intensity, 1.0, "full tremor at the midpoint");
        quake.turn = DURATION_TURNS - 1;
        assert!(quake.ground_shake().unwrap().intensity < 1.0, "easing off right before it ends");
    }

    #[test]
    fn shakes_without_touching_the_terrain_until_turn_60() {
        let mut map = bare();
        map.add_effect(effect(TribeId::Blue, AT));
        map.run(Ticks::new(SINK_AT));
        assert_eq!(map.terrain.get(64, 64), 700, "still just shaking");
    }

    #[test]
    fn sinks_the_centre_and_opens_a_lava_fissure_at_turn_60() {
        let mut map = bare();
        map.add_effect(effect(TribeId::Blue, AT));
        map.run(Ticks::new(SINK_AT + 1));
        assert!(map.terrain.get(64, 64) < 700, "the centre sank");
        assert!(map.effects.entries().any(|(_, e)| matches!(e.kind, EffectKind::Lava(_))), "a fissure opened");
    }

    #[test]
    fn sinks_only_once_even_if_it_kept_running() {
        let mut map = bare();
        map.add_effect(effect(TribeId::Blue, AT));
        map.run(Ticks::new(SINK_AT + 1));
        let sunk = map.terrain.get(64, 64);
        map.run(Ticks::new(DURATION_TURNS - SINK_AT - 1));
        assert_eq!(map.terrain.get(64, 64), sunk, "no second sink before the effect ends");
    }

    #[test]
    fn shake_only_reaches_buildings_within_the_16x16_square() {
        let mut map = bare();
        let near = map.add_building(Building::new(BuildingKind::Hut { size: 1 }, TribeId::Blue, AT.0, AT.1, 0));
        let edge = map.add_building(Building::new(BuildingKind::Hut { size: 1 }, TribeId::Blue, AT.0.wrapping_add(7 * pop3_format::WORLD_UNITS_PER_CELL as u16), AT.1, 0));
        let far = map.add_building(Building::new(BuildingKind::Hut { size: 1 }, TribeId::Blue, AT.0.wrapping_add(20 * pop3_format::WORLD_UNITS_PER_CELL as u16), AT.1, 0));
        let quake = Earthquake::default();
        let mut ever_shaken = std::collections::HashSet::new();
        for seed in 0..500u32 {
            let mut rng = Lcg(seed);
            for e in quake.shake(AT, &map, &mut rng) {
                if let WorldEvent::Shake { building } = e {
                    ever_shaken.insert(building);
                }
            }
        }
        assert!(ever_shaken.contains(&near), "well within range, hit over enough rolls");
        assert!(ever_shaken.contains(&edge), "7 cells out, just inside the half-width");
        assert!(!ever_shaken.contains(&far), "20 cells away: outside the 16 x 16 square");
    }

    /// A unit standing inside `building`, at its door.
    fn put_someone_inside(map: &mut GameMap, building: crate::building::BuildingId) -> crate::unit::UnitId {
        let door = map.building(building).unwrap().door();
        let inside = map.spawn(TribeId::Red, crate::unit::UnitKind::Brave, door).id;
        map.unit_mut(inside).unwrap().inside = Some(crate::unit::Inside { building, door });
        let i = map.building_index(building).unwrap();
        map.buildings[i].inside += 1;
        inside
    }

    #[test]
    fn a_shake_costs_one_piece_outright_and_a_large_buildings_people_leave_even_short_of_destroying_it() {
        let mut map = bare();
        let hut = map.add_building(Building::new(BuildingKind::Hut { size: 1 }, TribeId::Blue, AT.0, AT.1, 0));
        let i = map.building_index(hut).unwrap();
        map.buildings[i].used = 2;
        let someone = put_someone_inside(&mut map, hut);
        let (_, events) = map.apply_event(WorldEvent::Shake { building: hut });
        assert_eq!(map.building(hut).unwrap().used, 1, "one piece lost outright, not a wear threshold");
        assert!(!events.contains(&WorldEvent::Panic { unit: someone }), "a small hut's people stay put short of destroying it");

        let temple = map.add_building(Building::new(BuildingKind::Temple, TribeId::Blue, AT.0, AT.1.wrapping_add(2048), 0));
        let i = map.building_index(temple).unwrap();
        map.buildings[i].used = 2;
        let someone = put_someone_inside(&mut map, temple);
        let (_, events) = map.apply_event(WorldEvent::Shake { building: temple });
        assert_eq!(map.building(temple).unwrap().used, 1);
        assert!(events.contains(&WorldEvent::Panic { unit: someone }), "large buildings lose their people on any shake");
    }

    #[test]
    fn a_shake_on_the_last_piece_destroys_it() {
        let mut map = bare();
        let hut = map.add_building(Building::new(BuildingKind::Hut { size: 1 }, TribeId::Blue, AT.0, AT.1, 0));
        let i = map.building_index(hut).unwrap();
        map.buildings[i].used = 1;
        let someone = put_someone_inside(&mut map, hut);
        let (dirty, events) = map.apply_event(WorldEvent::Shake { building: hut });
        assert!(dirty.is_some(), "burnt down: the building's cells changed");
        assert!(events.contains(&WorldEvent::Panic { unit: someone }), "its people out too");
        assert!(map.building(hut).is_none(), "gone");
    }

    #[test]
    fn casting_it_through_the_spell_book_adds_the_controller_effect() {
        let mut map = GameMap::sandbox_walk();
        map.spell_books[0].set(SpellKind::Earthquake, Availability::Unlimited);
        let at = (map.units[0].x, map.units[0].z);
        map.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::Earthquake { at } });
        map.run(crate::unit::CAST_RELEASE);
        assert!(map.effects.entries().any(|(_, e)| matches!(e.kind, EffectKind::Earthquake(_))));
    }
}
