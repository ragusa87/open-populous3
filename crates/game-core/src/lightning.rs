//! Lightning (pop3-rev-analysis.md "Per spell" 3, "Components" Lightning 7/0x1E), an effect
//! (`effect::EffectKind::Lightning`): on its first turn it kills outright up to `KILLS` people in the target cell,
//! whatever their health and tribe (own followers and enemy shamans too) but the caster's own shaman and the
//! shielded (`effect::hurts`), the people inside the building there counted with them, sets that building on fire
//! (`crate::fire`) and spawns a simple blast; the rest of its turns the bolt is only seen.

use crate::effect::{DamageKind, Effect, EffectKind};
use crate::event::WorldEvent;
use crate::building::Building;
use crate::time::{Countdown, Ticks};
use crate::unit::{cell_of, Unit};

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
    /// One turn of the strike at `at` cast by `owner`: on the first, the kills, the fire and the blast.
    pub fn turn(&mut self, at: (u16, u16), owner: u8, units: &[Unit], buildings: &[Building]) -> Vec<WorldEvent> {
        if std::mem::replace(&mut self.struck, true) {
            return Vec::new();
        }
        let building = buildings.iter().find(|b| b.covers(at, 0)).map(|b| b.id);
        let mut events: Vec<WorldEvent> = struck(at, owner, units, building)
            .into_iter()
            .map(|unit| WorldEvent::Damage { unit, amount: u16::MAX, kind: DamageKind::Lightning, from: Some(owner) })
            .collect();
        events.extend(building.map(|building| WorldEvent::SetFire { building }));
        events.push(WorldEvent::SpawnEffect(crate::blast::effect(owner, at)));
        events
    }
}

/// The people a strike at `at` by `owner` kills (ids): the living ones in its cell outside buildings and those inside
/// the building it hits (`building`), that it can hurt (`effect::hurts`: not the caster's own shaman, not the
/// shielded), in id order, at most `KILLS`.
pub fn struck(at: (u16, u16), owner: u8, units: &[Unit], building: Option<u32>) -> Vec<u32> {
    let cell = (cell_of(at.0), cell_of(at.1));
    let there = |u: &Unit| match u.inside {
        None => u.cell() == cell,
        Some(inside) => Some(inside.building) == building,
    };
    units
        .iter()
        .filter(|u| there(u) && crate::effect::hurts(u, DamageKind::Lightning, Some(owner)))
        .take(KILLS)
        .map(|u| u.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::{Timed, SHIELD_STEPS};
    use crate::unit::UnitKind;

    const AT: (u16, u16) = (10 * 512 + 256, 20 * 512 + 256);

    fn person(id: u32, owner: u8, kind: UnitKind, at: (u16, u16)) -> Unit {
        Unit::new(id, owner, kind, at)
    }

    #[test]
    fn kills_at_most_seven_in_its_cell_of_any_tribe() {
        let mut units: Vec<Unit> = (1..=9).map(|id| person(id, (id % 2) as u8, UnitKind::Brave, (AT.0 + id as u16 * 20, AT.1))).collect();
        units.push(person(10, 1, UnitKind::Brave, (AT.0 + 512, AT.1)));
        assert_eq!(struck(AT, 0, &units, None), [1, 2, 3, 4, 5, 6, 7], "own braves too, 7 at most, in id order");
        assert!(!struck(AT, 0, &units, None).contains(&10), "the next cell is spared");
    }

    #[test]
    fn spares_the_casters_shaman_and_the_shielded_not_an_enemy_shaman() {
        let mut shielded = person(3, 1, UnitKind::Warrior, AT);
        shielded.statuses.shield = Timed::steps(SHIELD_STEPS);
        let units = [person(1, 0, UnitKind::Shaman, AT), person(2, 1, UnitKind::Shaman, AT), shielded];
        assert_eq!(struck(AT, 0, &units, None), [2]);
    }

    #[test]
    fn kills_the_people_inside_the_building_it_hits() {
        let mut inside = person(1, 1, UnitKind::Brave, (AT.0 + 2048, AT.1));
        inside.inside = Some(crate::unit::Inside { building: 50, door: AT });
        let mut elsewhere = inside.clone();
        elsewhere.id = 2;
        elsewhere.inside = Some(crate::unit::Inside { building: 51, door: AT });
        let units = [inside, elsewhere, person(3, 1, UnitKind::Brave, AT)];
        assert_eq!(struck(AT, 0, &units, Some(50)), [1, 3]);
        assert_eq!(struck(AT, 0, &units, None), [3], "no building hit: only those outside");
    }

    #[test]
    fn sets_the_building_it_hits_on_fire() {
        let hut = Building { id: 50, ..Building::new(crate::building::BuildingKind::Hut { size: 1 }, 0, AT.0 - 256, AT.1 - 256, 0) };
        let events = Strike::default().turn(AT, 0, &[], &[hut]);
        assert_eq!(events[0], WorldEvent::SetFire { building: 50 });
    }

    #[test]
    fn strikes_once_then_only_shows() {
        let units = [person(1, 1, UnitKind::Brave, AT)];
        let mut s = Strike::default();
        let first = s.turn(AT, 0, &units, &[]);
        assert!(matches!(first[0], WorldEvent::Damage { unit: 1, amount: u16::MAX, kind: DamageKind::Lightning, from: Some(0) }));
        assert!(matches!(first.last(), Some(WorldEvent::SpawnEffect(_))), "and a simple blast");
        assert!(s.turn(AT, 0, &units, &[]).is_empty());
    }
}
