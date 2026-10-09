//! Going inside (docs/specs/huts-and-training.md "Done"): followers ordered into one of their tribe's
//! built buildings with room (`BuildingKind::capacity`: huts rest people, a drum tower holds one)
//! walk to its door, in, and stay there (`Unit::inside`) until another order takes them out by the
//! door. Builders use the same way in (`crate::work`). Vehicles will hold people the same way.

use crate::building::Stage;
use crate::command::Command;
use crate::map::GameMap;
use crate::unit::{Action, Inside, Order, Unit, UnitKind};
use crate::work::work_point;

/// A click this close (world units) to a built building's footprint is on it.
pub const ENTER_MARGIN: i32 = 128;

impl GameMap {
    /// The player's built building with room for people (`BuildingKind::capacity`) whose footprint,
    /// grown by `ENTER_MARGIN`, holds `at`: a click there sends followers in.
    pub fn shelter_at(&self, player: u8, at: (u16, u16)) -> Option<usize> {
        self.buildings.iter().position(|b| b.owner == player && b.stage() == Stage::Built && b.kind.capacity() > 0 && b.covers(at, ENTER_MARGIN))
    }

    /// Orders sending the player's followers among `units` into the building `house` (index), in id
    /// order; the shaman walks to its door.
    pub fn enter_orders(&self, player: u8, units: &[u32], house: usize) -> Vec<Command> {
        let Some(b) = self.buildings.get(house).filter(|b| b.owner == player) else { return Vec::new() };
        let mine = |u: &&Unit| u.owner == player && u.is_alive() && units.contains(&u.id);
        let mut going: Vec<u32> = self.units.iter().filter(mine).filter(|u| can_enter(u.kind)).map(|u| u.id).collect();
        going.sort_unstable();
        let others: Vec<u32> = units.iter().copied().filter(|id| !going.contains(id)).collect();
        let enter = going.into_iter().map(|unit| Command::OrderUnit { player, unit, order: Order::Enter { site: (b.x, b.z) } });
        enter.chain(if others.is_empty() { Vec::new() } else { self.dispatch(player, &others, b.door()) }).collect()
    }

    /// Unit `i` takes `Order::Enter`: a follower of the owner walks to the built building's door
    /// (`come_in` once there).
    pub(crate) fn go_in(&mut self, i: usize, site: (u16, u16)) {
        let Some(b) = self.building_at_corner(site).map(|b| &self.buildings[b]) else { return };
        let u = &self.units[i];
        if !can_enter(u.kind) || u.owner != b.owner || b.stage() != Stage::Built || b.kind.capacity() == 0 {
            return;
        }
        if u.inside.is_some_and(|ins| ins.site == site) {
            self.units[i].start(Order::Stop);
            return;
        }
        let door = b.door();
        self.units[i].go_enter(site, door);
    }

    /// Unit `i` is at the door of the building at `site`: in it goes if there is room, else it stays at
    /// the door, idle.
    pub(crate) fn come_in(&mut self, i: usize, site: (u16, u16)) {
        let Some(b) = self.building_at_corner(site).map(|b| self.buildings[b].clone()) else { return };
        if b.stage() != Stage::Built || self.people_inside(site) >= b.kind.capacity() as usize {
            return;
        }
        let id = self.units[i].id;
        self.units[i].enter(Inside { site, door: b.door() }, work_point(&b, id));
    }

    /// Living units in the building at `site`, standing or walking in (not those walking out).
    pub fn people_inside(&self, site: (u16, u16)) -> usize {
        self.units.iter().filter(|u| u.is_alive() && u.inside.is_some_and(|i| i.site == site) && !matches!(u.action, Action::Walking { .. } | Action::Stranded { .. })).count()
    }

    /// Counts each building's people inside (`Building::inside`: a busy hut smokes).
    pub(crate) fn count_inside(&mut self) {
        for k in 0..self.buildings.len() {
            let site = (self.buildings[k].x, self.buildings[k].z);
            self.buildings[k].inside = self.people_inside(site).min(u8::MAX as usize) as u8;
        }
    }

}

/// Who goes inside buildings: followers, not the shaman nor wildmen.
fn can_enter(kind: UnitKind) -> bool {
    !matches!(kind, UnitKind::Shaman | UnitKind::Wildman)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::BuildingKind;

    fn corner(dx: i32, dz: i32) -> (u16, u16) {
        (((64 + dx) * 512) as u16, ((64 + dz) * 512) as u16)
    }

    fn braves(map: &GameMap, n: usize) -> Vec<u32> {
        map.units.iter().filter(|u| u.kind == UnitKind::Brave && u.campfire().is_none() && u.inside.is_none()).take(n).map(|u| u.id).collect()
    }

    fn run_until(map: &mut GameMap, ticks: usize, done: impl Fn(&GameMap) -> bool) -> Option<usize> {
        (0..ticks).find(|_| {
            map.tick();
            done(map)
        })
    }

    #[test]
    fn followers_rest_in_a_hut_up_to_its_room() {
        let mut map = GameMap::sandbox_buildings();
        let hut = map.buildings.iter().position(|b| b.owner == 0 && b.kind == BuildingKind::Hut { size: 1 } && b.stage() == Stage::Built && b.inside == 0).unwrap();
        let site = (map.buildings[hut].x, map.buildings[hut].z);
        let units = braves(&map, 5);
        for c in map.enter_orders(0, &units, hut) {
            map.apply(&c);
        }
        run_until(&mut map, 600, |m| m.units.iter().filter(|u| units.contains(&u.id)).all(|u| u.action == Action::Idle)).expect("arrived");
        assert_eq!(map.people_inside(site), 3, "a small hut holds 3");
        assert_eq!(map.buildings[hut].inside, 3);
        let walled = map.buildings[hut].walled_cells(128);
        let outside = map.units.iter().filter(|u| units.contains(&u.id) && u.inside.is_none()).count();
        assert_eq!(outside, 2, "the others wait at the door");
        let resting = map.units.iter().find(|u| u.inside.is_some_and(|i| i.site == site)).unwrap().id;
        assert!(walled.contains(&map.units.iter().find(|u| u.id == resting).unwrap().cell()));
        map.apply(&Command::OrderUnit { player: 0, unit: resting, order: Order::MoveTo { x: corner(20, 10).0, z: corner(20, 10).1 } });
        map.tick();
        assert_eq!(map.buildings[hut].inside, 2, "leaving");
    }

    #[test]
    fn a_drum_tower_holds_one() {
        let mut map = GameMap::sandbox_buildings();
        let tower = map.buildings.iter().position(|b| b.owner == 0 && b.kind == BuildingKind::DrumTower && b.stage() == Stage::Built && b.inside == 0).unwrap();
        let units = braves(&map, 2);
        assert_eq!(map.shelter_at(0, map.buildings[tower].centre()), Some(tower));
        for c in map.enter_orders(0, &units, tower) {
            map.apply(&c);
        }
        run_until(&mut map, 800, |m| m.units.iter().filter(|u| units.contains(&u.id)).all(|u| u.action == Action::Idle)).expect("arrived");
        assert_eq!(map.buildings[tower].inside, 1);
    }
}
