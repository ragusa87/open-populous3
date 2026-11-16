//! The splash where someone goes into the sea (pop3-rev-analysis.md "Drowning splash", effect type 0x41): its own
//! effect, spawned once at the point of entry and independent of the person afterwards, only seen. It lasts
//! `TICKS` (16 turns, the drowning's own length) on the water and is gone at once on land. Also spawned by a
//! drowned person's spirit rising out of the water.

use crate::effect::{Effect, EffectKind};
use crate::time::{Countdown, Ticks};
use crate::tribes::Owner;

pub const TICKS: Ticks = Ticks::new(16);

/// A splash of tribe `owner` at `at` (world units).
pub fn effect(owner: impl Into<Owner>, at: (u16, u16)) -> Effect {
    Effect::new(owner, at, Countdown::new(TICKS), EffectKind::Splash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::GameMap;
    use crate::terrain::Heightmap;
    use crate::tribes::TribeId;
    use crate::unit::{Action, UnitKind};
    use pop3_format::WORLD_UNITS_PER_CELL;

    /// Land, but for a sea of `width` cells from x = 60.
    fn coast(width: i32) -> GameMap {
        let mut terrain = Heightmap::new(128);
        for z in 0..128 {
            for x in 0..128 {
                terrain.set(x, z, if (60..=60 + width).contains(&x) { 0 } else { 300 });
            }
        }
        GameMap::bare("splash-test", terrain, Vec::new())
    }

    const SEA: (u16, u16) = (61 * WORLD_UNITS_PER_CELL as u16 + 256, 64 * WORLD_UNITS_PER_CELL as u16 + 256);

    fn splashes(map: &GameMap) -> Vec<(u16, u16)> {
        map.effects.entries().filter(|(_, e)| e.kind == EffectKind::Splash).map(|(_, e)| (e.x, e.z)).collect()
    }

    #[test]
    fn going_under_spawns_one_splash_there_lasting_the_drowning() {
        let mut map = coast(4);
        map.spawn(TribeId::Blue, UnitKind::Brave, SEA);
        map.tick();
        assert_eq!(splashes(&map), vec![SEA], "where it went in");
        map.run(Ticks::new(TICKS.get() - 1));
        assert_eq!(splashes(&map), vec![SEA], "once, not repeated while it sinks");
        map.tick();
        assert!(splashes(&map).is_empty(), "gone after {TICKS:?}, as the drowning ends");
    }

    #[test]
    fn it_outlives_someone_who_climbs_back_out() {
        let mut map = coast(4);
        let u = map.spawn(TribeId::Blue, UnitKind::Brave, SEA).id;
        map.tick();
        map.unit_mut(u).unwrap().x = 50 * WORLD_UNITS_PER_CELL as u16;
        map.tick();
        assert!(!matches!(map.unit(u).unwrap().action, Action::Drowning { .. }), "back on land");
        assert_eq!(splashes(&map).len(), 1, "still splashing on its own");
    }

    #[test]
    fn on_land_it_is_gone_at_once() {
        let mut map = coast(4);
        map.add_effect(effect(TribeId::Blue, (30 * WORLD_UNITS_PER_CELL as u16, 30 * WORLD_UNITS_PER_CELL as u16)));
        map.tick();
        assert!(splashes(&map).is_empty());
    }

    #[test]
    fn a_drowned_spirit_rises_out_of_the_water_with_a_splash() {
        let mut map = coast(4);
        map.spawn(TribeId::Blue, UnitKind::Brave, SEA);
        let mut seen = 0;
        for _ in 0..80 {
            let before = splashes(&map).len();
            map.tick();
            seen += splashes(&map).len().saturating_sub(before);
        }
        assert_eq!(seen, 2, "going under, then the spirit rising");
    }
}
