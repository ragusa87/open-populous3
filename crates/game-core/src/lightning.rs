//! Lightning (pop3-rev-analysis.md "Per spell" 3, "Components" Lightning 7/0x1E), an effect
//! (`effect::EffectKind::Lightning`): on its first turn it kills outright up to `KILLS` people in the target cell,
//! whatever their health and tribe (own followers and enemy shamans too) but the caster's own shaman and the
//! shielded (`effect::hurts`), sets the building there on fire (`crate::fire`: the people inside are not struck,
//! they come out 8 turns later, panicking), burns the trees of the cell or else lights a fire (`ground_fire`), and
//! spawns a simple blast; the rest of its turns the bolt is only seen.

use crate::effect::{DamageKind, Effect, EffectKind};
use crate::event::WorldEvent;
use crate::map::GameMap;
use crate::time::{Countdown, Ticks};
use crate::unit::{cell_of, Unit, UnitId};

/// `LIGHTNING_NUM_KILLS` (file and exe 6) + 1: the most people one strike kills.
pub const KILLS: usize = 7;
/// How long the bolt is seen (a guess: the original's component length is not traced).
pub const STRIKE_TICKS: Ticks = Ticks::new(12);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Strike {
    /// Whether it has struck (its first turn).
    struck: bool,
}

/// A strike cast by tribe `owner` at `at` (world units).
pub fn effect(owner: u8, at: (u16, u16)) -> Effect {
    Effect::new(owner, at, Countdown::new(STRIKE_TICKS), EffectKind::Lightning(Strike::default()))
}

impl Strike {
    /// One turn of the strike at `at` cast by `owner` on `world`: on the first, the kills, the fires and the blast.
    pub fn turn(&mut self, at: (u16, u16), owner: u8, world: &GameMap) -> Vec<WorldEvent> {
        if std::mem::replace(&mut self.struck, true) {
            return Vec::new();
        }
        let cell = (cell_of(at.0), cell_of(at.1));
        let building = world.buildings.iter().find(|b| b.covers(at, 0)).map(|b| b.id);
        let mut events: Vec<WorldEvent> = struck(at, owner, &world.units)
            .into_iter()
            .map(|unit| WorldEvent::Damage { unit, amount: u16::MAX, kind: DamageKind::Lightning, from: Some(owner) })
            .collect();
        events.extend(building.map(|building| WorldEvent::SetFire { building, by: owner }));
        events.push(match world.trees.iter().any(|t| t.cell() == cell && t.wood() > 0) {
            true => WorldEvent::BurnTrees { cell },
            false => WorldEvent::SpawnEffect(crate::ground_fire::effect(owner, at)),
        });
        events.push(WorldEvent::SpawnEffect(crate::blast::effect(owner, at)));
        events
    }
}

/// The people a strike at `at` by `owner` kills (ids): the living ones in its cell, not inside a building (the original
/// walks the cell's unit list, which a person inside is not in, pop3-rev-analysis.md spell 3), that it can hurt
/// (`effect::hurts`: not the caster's own shaman, not the shielded), in id order, at most `KILLS`.
pub fn struck(at: (u16, u16), owner: u8, units: &[Unit]) -> Vec<UnitId> {
    let cell = (cell_of(at.0), cell_of(at.1));
    units
        .iter()
        .filter(|u| u.in_the_open() && u.cell() == cell && crate::effect::hurts(u, DamageKind::Lightning, Some(owner)))
        .take(KILLS)
        .map(|u| u.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::Id;
    use crate::status::{Timed, SHIELD_STEPS};
    use crate::unit::UnitKind;

    const AT: (u16, u16) = (10 * 512 + 256, 20 * 512 + 256);

    fn person(id: u32, owner: u8, kind: UnitKind, at: (u16, u16)) -> Unit {
        Unit::new(Id::from_raw(id), owner, kind, at)
    }

    #[test]
    fn kills_at_most_seven_in_its_cell_of_any_tribe() {
        let mut units: Vec<Unit> = (1..=9).map(|id| person(id, (id % 2) as u8, UnitKind::Brave, (AT.0 + id as u16 * 20, AT.1))).collect();
        units.push(person(10, 1, UnitKind::Brave, (AT.0 + 512, AT.1)));
        assert_eq!(struck(AT, 0, &units), [1, 2, 3, 4, 5, 6, 7].map(Id::from_raw), "own braves too, 7 at most, in id order");
        assert!(!struck(AT, 0, &units).contains(&Id::from_raw(10)), "the next cell is spared");
    }

    #[test]
    fn spares_the_casters_shaman_and_the_shielded_not_an_enemy_shaman() {
        let mut shielded = person(3, 1, UnitKind::Warrior, AT);
        shielded.statuses.shield = Timed::steps(SHIELD_STEPS);
        let units = [person(1, 0, UnitKind::Shaman, AT), person(2, 1, UnitKind::Shaman, AT), shielded];
        assert_eq!(struck(AT, 0, &units), [Id::from_raw(2)]);
    }

    #[test]
    fn spares_the_people_inside_a_building() {
        let mut inside = person(1, 1, UnitKind::Brave, AT);
        inside.inside = Some(crate::unit::Inside { building: Id::from_raw(50), door: AT });
        let units = [inside, person(2, 1, UnitKind::Brave, AT)];
        assert_eq!(struck(AT, 0, &units), [Id::from_raw(2)]);
    }

    fn world(units: Vec<Unit>, buildings: Vec<crate::building::Building>, trees: Vec<crate::tree::Tree>) -> GameMap {
        let mut map = GameMap::bare("strike", crate::terrain::Heightmap::new(128), Vec::new());
        (map.units, map.buildings, map.trees) = (units, buildings, trees);
        map
    }

    fn kinds(events: &[WorldEvent]) -> Vec<String> {
        events.iter().map(|e| match e {
            WorldEvent::SpawnEffect(e) => format!("{:?}", std::mem::discriminant(&e.kind)),
            e => format!("{e:?}"),
        }).collect()
    }

    #[test]
    fn sets_the_building_it_hits_on_fire_and_lights_a_fire_on_bare_ground() {
        let hut = crate::building::Building { id: Id::from_raw(50), ..crate::building::Building::new(crate::building::BuildingKind::Hut { size: 1 }, 0, AT.0 - 256, AT.1 - 256, 0) };
        let events = Strike::default().turn(AT, 0, &world(vec![], vec![hut], vec![]));
        assert_eq!(events[0], WorldEvent::SetFire { building: Id::from_raw(50), by: 0 });
        let fire = crate::ground_fire::effect(0, AT);
        assert_eq!(events[1], WorldEvent::SpawnEffect(fire), "no tree: a fire");
    }

    #[test]
    fn burns_the_tree_of_its_cell_instead_of_lighting_a_fire() {
        let tree = crate::tree::Tree::new((10, 20), 0, 2);
        let events = Strike::default().turn(AT, 0, &world(vec![], vec![], vec![tree.clone()]));
        assert_eq!(events[0], WorldEvent::BurnTrees { cell: (10, 20) });
        let bare = crate::tree::Tree::new((10, 20), 0, 0);
        assert_eq!(kinds(&Strike::default().turn(AT, 0, &world(vec![], vec![], vec![bare]))), kinds(&[WorldEvent::SpawnEffect(crate::ground_fire::effect(0, AT)), WorldEvent::SpawnEffect(crate::blast::effect(0, AT))]), "nothing to burn");
    }

    #[test]
    fn strikes_once_then_only_shows() {
        let map = world(vec![person(1, 1, UnitKind::Brave, AT)], vec![], vec![]);
        let mut s = Strike::default();
        let first = s.turn(AT, 0, &map);
        assert!(matches!(first[0], WorldEvent::Damage { unit, amount: u16::MAX, kind: DamageKind::Lightning, from: Some(0) } if unit == Id::from_raw(1)));
        assert!(matches!(first.last(), Some(WorldEvent::SpawnEffect(e)) if matches!(e.kind, EffectKind::SimpleBlast(_))), "and a simple blast");
        assert!(s.turn(AT, 0, &map).is_empty());
    }
}
