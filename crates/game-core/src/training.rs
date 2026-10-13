//! Training (pop3-rev-analysis.md "Training buildings"): a follower inside a built training unit of its tribe sets
//! the building's cost (`train_cost`); the mana distribution fills its store (`crate::production`), and once the
//! store reaches the cost the trainee is replaced by a new person of the building's kind at the door. Ordered out
//! before, it leaves unchanged and the store is given back (`Production::refund`).

use crate::building::{BuildingKind, Stage};
use crate::map::GameMap;
use crate::slots;
use crate::time::Every;
use crate::unit::{Order, UnitKind};

/// The cost is recomputed on this rhythm while training ("Cost").
pub const RECOST_EVERY: Every = Every::new(16);

/// Percent of the cost by the number of that kind the tribe already has, up to each bound (`TRAIN_MANA_BAND_*`);
/// above the last, `TOP_BAND`.
pub const BANDS: [(usize, u32); 5] = [(3, 100), (7, 125), (11, 150), (15, 175), (20, 200)];
pub const TOP_BAND: u32 = 250;

/// The kind a building trains, None for the others.
pub fn trains(kind: BuildingKind) -> Option<UnitKind> {
    match kind {
        BuildingKind::Temple => Some(UnitKind::Preacher),
        BuildingKind::SpyTraining => Some(UnitKind::Spy),
        BuildingKind::WarriorTraining => Some(UnitKind::Warrior),
        BuildingKind::FirewarriorTraining => Some(UnitKind::Firewarrior),
        _ => None,
    }
}

/// Mana to train one `kind` (`HUMAN_TRAIN_MANA_*`; the computer players' 1000 once tribes have a controller).
pub fn train_mana(kind: UnitKind) -> u32 {
    match kind {
        UnitKind::Warrior | UnitKind::Preacher => 3500,
        UnitKind::Spy | UnitKind::Firewarrior => 4000,
        _ => 0,
    }
}

/// Mana to train one `kind` for a tribe that has `count` of them already.
pub fn train_cost(kind: UnitKind, count: usize) -> u16 {
    let band = BANDS.iter().find(|&&(bound, _)| count <= bound).map_or(TOP_BAND, |&(_, p)| p);
    (train_mana(kind) * band / 100).min(u16::MAX as u32) as u16
}

impl GameMap {
    /// Each built training unit: its cost set from its trainee, or cleared without one; with the store full, the
    /// trainee is replaced by a new person of its kind at the door, who walks to the nearest free spot.
    pub(crate) fn tend_training(&mut self) {
        let now = self.now;
        for k in 0..self.buildings.len() {
            let b = &self.buildings[k];
            let Some(output) = trains(b.kind).filter(|_| b.stage() == Stage::Built) else { continue };
            let (id, owner, door) = (b.id, b.owner, b.door());
            let trainee = self.units_inside(id).next().map(|u| u.id);
            let count = self.units.iter().filter(|u| u.owner == owner && u.kind == output && u.is_alive()).count();
            let p = self.buildings[k].production.get_or_insert_default();
            let Some(trainee) = trainee else {
                p.cost = 0;
                continue;
            };
            if !p.is_active() || RECOST_EVERY.fires(now, 0) {
                p.cost = train_cost(output, count);
            }
            if p.stored < p.cost {
                continue;
            }
            p.stored = 0;
            p.cost = 0;
            let i = self.unit_index(trainee).unwrap();
            let ghost = self.units.remove(i).statuses.ghost;
            let taken = self.taken_spots(&[]);
            let spot = slots::free_spots_near(&self.ground(), slots::spot_of(door), &taken, 1).first().copied();
            let made = self.spawn(owner, output, door);
            made.statuses.ghost = ghost;
            if let Some(spot) = spot {
                let (x, z) = slots::spot_centre(spot);
                made.start(Order::MoveTo { x, z });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::{Action, UnitId};

    #[test]
    fn the_cost_grows_with_the_band() {
        assert_eq!(train_cost(UnitKind::Warrior, 0), 3500);
        assert_eq!(train_cost(UnitKind::Warrior, 3), 3500);
        assert_eq!(train_cost(UnitKind::Warrior, 4), 4375);
        assert_eq!(train_cost(UnitKind::Firewarrior, 12), 7000);
        assert_eq!(train_cost(UnitKind::Spy, 20), 8000);
        assert_eq!(train_cost(UnitKind::Preacher, 21), 8750);
    }

    #[test]
    fn only_training_units_train() {
        assert_eq!(trains(BuildingKind::FirewarriorTraining), Some(UnitKind::Firewarrior));
        assert_eq!(trains(BuildingKind::Hut { size: 1 }), None);
    }

    fn training_hut(map: &GameMap) -> usize {
        map.buildings.iter().position(|b| b.owner == 0 && b.kind == BuildingKind::WarriorTraining && b.stage() == Stage::Built && b.inside == 0).unwrap()
    }

    fn send_brave(map: &mut GameMap, hut: usize) -> UnitId {
        let brave = map.units.iter().find(|u| u.owner == 0 && u.kind == UnitKind::Brave && u.inside.is_none() && u.campfire().is_none()).unwrap().id;
        for c in map.enter_orders(0, &[brave], hut) {
            map.apply(&c);
        }
        let id = map.buildings[hut].id;
        (0..800).find(|_| {
            map.tick();
            map.units_inside(id).count() == 1
        });
        assert_eq!(map.units_inside(id).next().map(|u| u.id), Some(brave), "inside");
        brave
    }

    #[test]
    fn a_brave_inside_comes_out_a_warrior_once_the_store_is_full() {
        let mut map = GameMap::sandbox_buildings();
        let hut = training_hut(&map);
        let warriors = map.units.iter().filter(|u| u.owner == 0 && u.kind == UnitKind::Warrior).count();
        let brave = send_brave(&mut map, hut);
        map.tick();
        let p = map.buildings[hut].production.unwrap();
        assert_eq!(p.cost, train_cost(UnitKind::Warrior, warriors));
        map.buildings[hut].production.as_mut().unwrap().stored = p.cost - 1;
        (0..32).find(|_| {
            map.tick();
            map.unit(brave).is_none()
        })
        .expect("trained after the next income");
        assert_eq!(map.buildings[hut].production.map(|p| (p.stored, p.cost)), Some((0, 0)));
        let made = map.units.last().unwrap();
        assert_eq!((made.owner, made.kind, made.inside), (0, UnitKind::Warrior, None));
        assert!(made.health.current() == made.health.max(), "full life");
        let id = made.id;
        (0..200).find(|_| {
            map.tick();
            map.unit(id).unwrap().action == Action::Idle
        })
        .expect("walks off the door");
    }

    #[test]
    fn a_trainee_ordered_out_leaves_unchanged_and_the_store_goes_back() {
        let mut map = GameMap::sandbox_buildings();
        let hut = training_hut(&map);
        let brave = send_brave(&mut map, hut);
        map.tick();
        map.buildings[hut].production.as_mut().unwrap().stored = 250;
        let (x, z) = map.buildings[hut].door();
        map.apply(&crate::command::Command::OrderUnit { player: 0, unit: brave, order: Order::MoveTo { x: x.wrapping_add(2048), z } });
        (0..300).find(|_| {
            map.tick();
            map.buildings[hut].production.unwrap().stored == 0
        })
        .expect("given back");
        assert_eq!(map.unit(brave).unwrap().kind, UnitKind::Brave);
        assert_eq!(map.buildings[hut].production.unwrap().cost, 0);
    }
}
