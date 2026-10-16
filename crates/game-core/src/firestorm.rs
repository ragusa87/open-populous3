//! Firestorm (pop3-rev-analysis.md "Per spell" 8), a pure scheduler effect (`effect::EffectKind::Firestorm`,
//! `DURATION_TURNS` 220 turns, 18.3 s, file-confirmed): every `FIREBALL_EVERY` (4) turns, one `panicking_blast`
//! fireball lands at a random point of the `AREA_CELLS` 6 × 6-cell square around the target (`volcano`'s own
//! `random_point_in_area`, the same area shape and helper as Volcano's second half — "about 55" fireballs in all,
//! matching 220 / 4 exactly). Each impact, beyond the fireball's own simple-blast-that-panics: "a short fire" (our
//! own reading of the trace, a guess — no component number is cited here, unlike Lightning's — taken literally as a
//! `ground_fire` effect at the landing point rather than Lightning's tree-burns-else-fire branch) and, if a
//! building covers the point, it catches fire (`WorldEvent::SetFire`, the ordinary fire/evacuation path). Every
//! fireball and its fire outlive the controller once it ends, same as Volcano's own.

use crate::effect::{Effect, EffectKind};
use crate::event::WorldEvent;
use crate::map::{GameMap, Lcg};
use crate::time::{Countdown, Ticks};
use crate::unit::cell_of;

pub const DURATION_TURNS: u32 = 220;
pub const FIREBALL_EVERY: Ticks = Ticks::new(4);
/// Half the side of the 6 × 6 target square, same shape as Volcano's `DESTROY_AREA_CELLS`.
pub const AREA_CELLS: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Firestorm {
    /// Turns left to the next fireball.
    next: Countdown,
}

/// Cast by tribe `owner` at `at` (world units).
pub fn effect(owner: u8, at: (u16, u16)) -> Effect {
    let firestorm = Firestorm { next: Countdown::new(FIREBALL_EVERY) };
    Effect::new(owner, at, Countdown::new(Ticks::new(DURATION_TURNS)), EffectKind::Firestorm(firestorm))
}

impl Firestorm {
    /// One turn at `at`, cast by `owner`: every `FIREBALL_EVERY` turns, a fireball lands somewhere in the area.
    pub fn turn(&mut self, at: (u16, u16), owner: u8, world: &GameMap, rng: &mut Lcg) -> Vec<WorldEvent> {
        if !self.next.tick() {
            return Vec::new();
        }
        self.next = Countdown::new(FIREBALL_EVERY);
        let cell = (cell_of(at.0), cell_of(at.1));
        let size = world.terrain.size() as i32;
        let landing = crate::volcano::random_point_in_area(cell, AREA_CELLS, size, rng);
        impact(owner, landing, world)
    }
}

/// One fireball's own impact at `landing`: its simple-blast-that-panics, its short fire, and the building there (if
/// any) catching fire too.
fn impact(owner: u8, landing: (u16, u16), world: &GameMap) -> Vec<WorldEvent> {
    let mut events = vec![WorldEvent::SpawnEffect(crate::panicking_blast::effect(owner, landing)), WorldEvent::SpawnEffect(crate::ground_fire::effect(owner, landing))];
    if let Some(b) = world.buildings.iter().find(|b| b.covers(landing, 0)) {
        events.push(WorldEvent::SetFire { building: b.id });
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::{Building, BuildingKind};
    use crate::command::Command;
    use crate::spell::Spell;
    use crate::spell_book::{Availability, SpellKind};
    use crate::terrain::Heightmap;

    const AT: (u16, u16) = (64 * pop3_format::WORLD_UNITS_PER_CELL as u16 + 256, 64 * pop3_format::WORLD_UNITS_PER_CELL as u16 + 256);

    fn bare() -> GameMap {
        GameMap::bare("firestorm-test", Heightmap::new(128), Vec::new())
    }

    #[test]
    fn drops_a_fireball_every_4_turns_and_nothing_between() {
        let mut map = bare();
        map.add_effect(effect(0, AT));
        map.run(Ticks::new(3));
        assert!(map.effects.entries().filter(|(_, e)| matches!(e.kind, EffectKind::PanickingBlast(_))).count() == 0, "not yet");
        map.tick();
        assert_eq!(map.effects.entries().filter(|(_, e)| matches!(e.kind, EffectKind::PanickingBlast(_))).count(), 1);
    }

    #[test]
    fn drops_about_55_fireballs_over_its_whole_lifetime() {
        let map = bare();
        let mut firestorm = Firestorm { next: Countdown::new(FIREBALL_EVERY) };
        let mut rng = map.rng;
        let mut spawned = 0;
        for _ in 0..DURATION_TURNS {
            let events = firestorm.turn(AT, 0, &map, &mut rng);
            spawned += events.iter().filter(|e| matches!(e, WorldEvent::SpawnEffect(eff) if matches!(eff.kind, EffectKind::PanickingBlast(_)))).count();
        }
        assert_eq!(spawned, (DURATION_TURNS / FIREBALL_EVERY.get()) as usize);
    }

    #[test]
    fn sets_a_building_on_fire_when_a_fireball_lands_on_it() {
        let mut map = bare();
        let hut = map.add_building(Building::new(BuildingKind::Hut { size: 1 }, 1, AT.0, AT.1, 0));
        let events = impact(0, AT, &map);
        assert!(events.contains(&WorldEvent::SetFire { building: hut }));
        assert!(events.iter().any(|e| matches!(e, WorldEvent::SpawnEffect(eff) if matches!(eff.kind, EffectKind::PanickingBlast(_)))));
        assert!(events.iter().any(|e| matches!(e, WorldEvent::SpawnEffect(eff) if matches!(eff.kind, EffectKind::GroundFire))));
    }

    #[test]
    fn no_building_there_means_no_set_fire() {
        let map = bare();
        let events = impact(0, AT, &map);
        assert!(!events.iter().any(|e| matches!(e, WorldEvent::SetFire { .. })));
    }

    #[test]
    fn casting_it_through_the_spell_book_adds_the_controller_effect() {
        let mut map = GameMap::sandbox_walk();
        map.spell_books[0].set(SpellKind::Firestorm, Availability::Unlimited);
        let at = (map.units[0].x, map.units[0].z);
        map.apply(&Command::Cast { player: 0, spell: Spell::Firestorm { at } });
        map.run(crate::unit::CAST_RELEASE);
        assert!(map.effects.entries().any(|(_, e)| matches!(e.kind, EffectKind::Firestorm(_))));
    }
}
