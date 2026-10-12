//! Burning buildings (pop3-rev-analysis.md "Shared person and building states", Burning building), set on fire by
//! `WorldEvent::SetFire` (lightning): the people inside are thrown out by the door and panic, the building loses one
//! piece of wood at once and burns for `BURN_TICKS`. With wood left it stands as under construction, which braves
//! repair with wood once the fire is out; with none left it is destroyed.

use crate::building::{Building, Stage};
use crate::event::WorldEvent;
use crate::map::GameMap;
use crate::time::{Countdown, Tick, Ticks};
use crate::unit::Order;

/// 127 turns (10.6 s), the original's burning building.
pub const BURN_TICKS: Ticks = Ticks::new(127);

/// What setting a building on fire did to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Burnt {
    /// Not burnt: never built with wood (vault, prison...), only a plan, or already burning (ours, a guess).
    Spared,
    /// Burning, one piece of wood less, the rest standing.
    Damaged,
    /// Burning, its last piece of wood lost.
    Destroyed,
}

impl Building {
    /// Sets it on fire: one structural hit of 100 wood (one piece), and it burns for `BURN_TICKS`.
    pub fn ignite(&mut self) -> Burnt {
        if self.kind.wood_cost() == 0 || self.stage() == Stage::Blueprint || self.burning.is_some() {
            return Burnt::Spared;
        }
        self.burning = Some(Countdown::new(BURN_TICKS));
        self.used = self.used.min(self.kind.wood_cost()).saturating_sub(1);
        if self.used == 0 { Burnt::Destroyed } else { Burnt::Damaged }
    }

    /// The tick its fire was lit, the map being at `now`; None when it does not burn.
    pub fn lit_at(&self, now: Tick) -> Option<Tick> {
        self.burning.map(|c| Tick::from_wire(now.to_wire().wrapping_sub(c.done().get())))
    }

    /// One tick of its fire, out when it has burnt `BURN_TICKS`.
    pub fn burn(&mut self) {
        if self.burning.as_mut().is_some_and(Countdown::tick) {
            self.burning = None;
        }
    }
}

impl GameMap {
    /// Sets the building `building` on fire (`Building::ignite`): everyone inside walks out by the door and panics,
    /// a building with no wood left is removed. The panics, to apply next.
    pub(crate) fn set_fire(&mut self, building: u32) -> Vec<WorldEvent> {
        let Some(i) = self.building_index(building) else { return Vec::new() };
        let burnt = self.buildings[i].ignite();
        if burnt == Burnt::Spared {
            return Vec::new();
        }
        let out: Vec<u32> = self.units.iter().filter(|u| u.is_alive() && u.inside.is_some_and(|i| i.building == building)).map(|u| u.id).collect();
        let door = self.buildings[i].door();
        self.empty_building(building, door);
        if burnt == Burnt::Destroyed {
            self.remove_building(i);
        }
        out.into_iter().map(|unit| WorldEvent::Panic { unit }).collect()
    }

    /// Removes the building at `i`: its braves stop working on it.
    pub(crate) fn remove_building(&mut self, i: usize) {
        let id = self.buildings.remove(i).id;
        for u in self.units.iter_mut().filter(|u| u.work == Some(id)) {
            u.work = None;
            u.start(Order::Stop);
        }
    }

    /// Each burning building's fire burns on.
    pub(crate) fn tend_fires(&mut self) {
        self.buildings.iter_mut().for_each(Building::burn);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::BuildingKind;
    use crate::unit::Action;

    #[test]
    fn a_fire_takes_one_piece_and_burns_out() {
        let mut hut = Building::new(BuildingKind::Hut { size: 1 }, 0, 0, 0, 0);
        assert_eq!(hut.ignite(), Burnt::Damaged);
        assert_eq!(hut.stage(), Stage::UnderConstruction { used: 2, of: 3 }, "repaired like a construction");
        assert_eq!(hut.ignite(), Burnt::Spared, "already burning");
        (1..BURN_TICKS.get()).for_each(|_| hut.burn());
        assert!(hut.burning.is_some());
        assert_eq!(hut.lit_at(Tick::from_wire(200)), Some(Tick::from_wire(200 - (BURN_TICKS.get() - 1))));
        hut.burn();
        assert_eq!((hut.burning, hut.lit_at(Tick::from_wire(200))), (None, None));
        assert_eq!(hut.ignite(), Burnt::Damaged);
        hut.burning = None;
        assert_eq!(hut.ignite(), Burnt::Destroyed, "no wood left");
    }

    #[test]
    fn plans_and_buildings_without_wood_never_burn() {
        assert_eq!(Building::new(BuildingKind::Vault, 0, 0, 0, 0).ignite(), Burnt::Spared);
        assert_eq!(Building::site(BuildingKind::Temple, 0, 0, 0, 0).ignite(), Burnt::Spared);
        let frame = Building { used: 0, ..Building::new(BuildingKind::Temple, 0, 0, 0, 0) };
        assert_eq!(frame.clone().ignite(), Burnt::Destroyed, "a bare frame burns down");
    }

    fn sandbox_hut(map: &GameMap) -> usize {
        map.buildings.iter().position(|b| b.owner == 0 && b.kind == BuildingKind::Hut { size: 1 } && b.stage() == Stage::Built && b.inside == 0).unwrap()
    }

    fn rest_inside(map: &mut GameMap, hut: usize, n: usize) -> Vec<u32> {
        let units: Vec<u32> = map.units.iter().filter(|u| u.owner == 0 && u.kind == crate::unit::UnitKind::Brave && u.inside.is_none() && u.campfire().is_none()).take(n).map(|u| u.id).collect();
        for c in map.enter_orders(0, &units, hut) {
            map.apply(&c);
        }
        let id = map.buildings[hut].id;
        (0..800).find(|_| {
            map.tick();
            map.people_inside(id) == n
        });
        assert_eq!(map.people_inside(id), n);
        units
    }

    #[test]
    fn its_people_are_thrown_out_braves_repair_it_once_the_fire_is_out() {
        let mut map = GameMap::sandbox_buildings();
        let hut = sandbox_hut(&map);
        let id = map.buildings[hut].id;
        let people = rest_inside(&mut map, hut, 2);
        let panics = map.set_fire(id);
        assert_eq!(panics, people.iter().map(|&unit| WorldEvent::Panic { unit }).collect::<Vec<_>>());
        map.tick();
        assert_eq!(map.people_inside(id), 0, "walking out");
        assert_eq!(map.buildings[hut].stage(), Stage::UnderConstruction { used: 2, of: 3 });
        for c in map.build_orders(0, &people, hut) {
            map.apply(&c);
        }
        for _ in 0..BURN_TICKS.get() - 2 {
            map.tick();
            assert!(!map.units.iter().any(|u| people.contains(&u.id) && matches!(u.action, Action::Building { .. })), "not while it burns");
        }
        let repaired = (0..4000).find(|_| {
            map.tick();
            map.buildings[hut].stage() == Stage::Built
        });
        assert!(repaired.is_some(), "braves bring the lost piece back");
    }

    #[test]
    fn a_building_burnt_to_nothing_is_removed() {
        let mut map = GameMap::sandbox_buildings();
        let hut = sandbox_hut(&map);
        let id = map.buildings[hut].id;
        map.buildings[hut].used = 1;
        map.set_fire(id);
        assert!(map.building(id).is_none());
    }
}
