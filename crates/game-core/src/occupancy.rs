//! What a thing on the map holds, as slots (docs/specs/tooltips.md "Slot rows"): the people in it or
//! working on it (unit ids, a click selects one) and its wood (a count). The rules of who counts live
//! here, next to the simulation; the tooltips only draw them.

use crate::building::{Building, BuildingKind, Stage};
use crate::map::GameMap;

/// Something whose slots can be shown, by its index in its `GameMap` list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holder {
    Building(usize),
    Totem(usize),
    Tree(usize),
}

/// People slots: `capacity` places, `filled` by these units (ids, in fill order, at most `capacity`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct People {
    pub capacity: u16,
    pub filled: Vec<u32>,
}

/// Wood slots: `capacity` pieces, `filled` of them there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wood {
    pub capacity: u16,
    pub filled: u16,
}

/// People a built building of `kind` holds: a hut's room, one in a drum tower, one trainee in a
/// training hut, the shaman in a pyramid; 0 for the others.
pub fn room(kind: BuildingKind) -> u16 {
    match kind {
        BuildingKind::Hut { .. } | BuildingKind::DrumTower => kind.capacity() as u16,
        BuildingKind::Temple | BuildingKind::SpyTraining | BuildingKind::WarriorTraining | BuildingKind::FirewarriorTraining | BuildingKind::Vault => 1,
        _ => 0,
    }
}

impl GameMap {
    /// The people slots of `holder`, None when it holds nobody: a plan or site, its braves at work
    /// (`max_braves`); a built building with room, the people inside; a spent vault, none; a totem,
    /// its `prayers`, filled by the first of its queue (those counted).
    pub fn people_slots(&self, holder: Holder) -> Option<People> {
        match holder {
            Holder::Building(i) => {
                let b = self.buildings.get(i)?;
                let site = (b.x, b.z);
                let (capacity, mut filled): (u16, Vec<u32>) = if b.stage() == Stage::Built {
                    let room = if b.vault.is_some_and(|v| v.is_spent()) { 0 } else { room(b.kind) };
                    let praying = self.units.iter().filter(|u| u.is_alive() && u.action == crate::unit::Action::Worshipping { site });
                    (room, self.units_inside(site).chain(praying).map(|u| u.id).collect())
                } else if b.kind.wood_cost() > 0 {
                    (b.kind.max_braves() as u16, self.workers(site).map(|u| u.id).collect())
                } else {
                    (0, Vec::new())
                };
                filled.sort_unstable();
                filled.truncate(capacity as usize);
                (capacity > 0).then_some(People { capacity, filled })
            }
            Holder::Totem(i) => self.totems.get(i).map(|t| People { capacity: t.prayers, filled: t.queue.iter().take(t.prayers as usize).copied().collect() }),
            Holder::Tree(_) => None,
        }
    }

    /// The wood slots of `holder`, None when it has no wood to show: a plan, site or dismantled
    /// building, its cost and the wood provided (pile + built in); a built one, the wood in it; a tree,
    /// its current wood only (no capacity known); none for a tree with no wood left.
    pub fn wood_slots(&self, holder: Holder) -> Option<Wood> {
        match holder {
            Holder::Building(i) => self.buildings.get(i).and_then(building_wood),
            Holder::Tree(i) => self.trees.get(i).filter(|t| t.size > 0).map(|t| Wood { capacity: t.size as u16, filled: t.size as u16 }),
            Holder::Totem(_) => None,
        }
    }
}

fn building_wood(b: &Building) -> Option<Wood> {
    let cost = b.kind.wood_cost() as u16;
    if cost == 0 {
        return None;
    }
    Some(match b.stage() {
        Stage::Built => Wood { capacity: b.used as u16, filled: b.used as u16 },
        _ => Wood { capacity: cost, filled: (b.delivered() as u16).min(cost) },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::Building;
    use crate::totem::{Totem, TotemKind};
    use crate::unit::Order;

    fn sandbox() -> GameMap {
        GameMap::sandbox_buildings()
    }

    fn find(map: &GameMap, kind: BuildingKind, stage: impl Fn(Stage) -> bool) -> usize {
        map.buildings.iter().position(|b| b.owner == 0 && b.kind == kind && stage(b.stage()) && b.inside == 0).unwrap()
    }

    #[test]
    fn a_built_hut_holds_its_room_filled_by_the_people_inside() {
        let mut map = sandbox();
        let hut = find(&map, BuildingKind::Hut { size: 1 }, |s| s == Stage::Built);
        assert_eq!(map.people_slots(Holder::Building(hut)), Some(People { capacity: 3, filled: Vec::new() }));
        let braves: Vec<u32> = map.units.iter().filter(|u| u.kind == crate::unit::UnitKind::Brave && u.inside.is_none() && u.work.is_none()).take(2).map(|u| u.id).collect();
        for c in map.enter_orders(0, &braves, hut) {
            map.apply(&c);
        }
        (0..600).find(|_| {
            map.tick();
            map.people_slots(Holder::Building(hut)).is_some_and(|p| p.filled.len() == 2)
        });
        let mut sorted = braves.clone();
        sorted.sort_unstable();
        assert_eq!(map.people_slots(Holder::Building(hut)).unwrap().filled, sorted, "in id order");
    }

    #[test]
    fn a_site_holds_its_builders_and_its_wood() {
        let map = sandbox();
        let i = map.buildings.iter().position(|b| matches!(b.stage(), Stage::UnderConstruction { .. }) && b.owner == 0).unwrap();
        let b = &map.buildings[i];
        let people = map.people_slots(Holder::Building(i)).unwrap();
        assert_eq!(people.capacity, b.kind.max_braves() as u16);
        assert_eq!(map.wood_slots(Holder::Building(i)), Some(Wood { capacity: b.kind.wood_cost() as u16, filled: b.delivered().min(b.kind.wood_cost()) as u16 }));
    }

    #[test]
    fn built_wood_is_all_there_and_buildings_without_wood_show_none() {
        let built = Building::new(BuildingKind::DrumTower, 0, 0, 0, 0);
        let cost = BuildingKind::DrumTower.wood_cost() as u16;
        assert_eq!(building_wood(&built), Some(Wood { capacity: cost, filled: cost }));
        assert_eq!(building_wood(&Building::new(BuildingKind::Vault, 255, 0, 0, 0)), None, "never built");
    }

    #[test]
    fn a_vault_holds_one_shaman_until_spent_and_a_totem_its_prayers() {
        let mut map = GameMap::sandbox_worship();
        let v = map.buildings.iter().position(|b| b.kind == BuildingKind::Vault).unwrap();
        assert_eq!(map.people_slots(Holder::Building(v)).map(|p| p.capacity), Some(1));
        map.buildings[v].vault.as_mut().unwrap().phase = crate::vault::VaultPhase::Spent;
        assert_eq!(map.people_slots(Holder::Building(v)), None, "spent: nobody");
        map.totems = vec![Totem { prayers: 8, ..Totem::new(TotemKind::StoneHead, (0, 0)) }];
        assert_eq!(map.people_slots(Holder::Totem(0)), Some(People { capacity: 8, filled: Vec::new() }));
        assert_eq!(map.wood_slots(Holder::Totem(0)), None);
    }

    #[test]
    fn a_tree_shows_its_current_wood_only() {
        let mut map = sandbox();
        let (mut full, mut bare) = (map.trees[0].clone(), map.trees[0].clone());
        (full.size, bare.size) = (3, 0);
        map.trees = vec![full, bare];
        assert_eq!(map.wood_slots(Holder::Tree(0)), Some(Wood { capacity: 3, filled: 3 }));
        assert_eq!(map.wood_slots(Holder::Tree(1)), None, "no wood: no tooltip");
        assert_eq!(map.people_slots(Holder::Tree(0)), None);
    }

    #[test]
    fn filled_never_exceeds_capacity() {
        let mut map = sandbox();
        let hut = find(&map, BuildingKind::Hut { size: 1 }, |s| s == Stage::Built);
        let site = (map.buildings[hut].x, map.buildings[hut].z);
        for u in map.units.iter_mut().filter(|u| u.kind == crate::unit::UnitKind::Brave).take(5) {
            u.inside = Some(crate::unit::Inside { site, door: site });
            u.start(Order::Stop);
        }
        assert_eq!(map.people_slots(Holder::Building(hut)).unwrap().filled.len(), 3);
    }
}
