//! Sending a selection into the player's buildings of one kind, or round their camp fires (the Build tab's shift
//! click, ui-and-editor.md "Build tab"): each unit that may go there gets a place with room, the units spread evenly
//! over the places and the nearest paired first; those left without room stay where they are.

use crate::building::{BuildingKind, Stage};
use crate::command::Command;
use crate::map::{torus_dist2, GameMap};
use crate::unit::{Order, Unit, UnitId, UnitKind};
use crate::vehicle::{VehicleKind, CREW};

/// Where a selection is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    /// The player's built buildings of this kind (a hut of any size).
    Kind(BuildingKind),
    /// The player's camp fires.
    Campfire,
}

/// A place units can be sent to: where it is, and how many more it takes (None: as many as come).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub at: (u16, u16),
    pub room: Option<usize>,
}

/// Which place (index in `places`) each of `units` (id, position) goes to: the units spread evenly over the places
/// with room (at most `n / places` rounded up each, first), the nearest unit and place paired first, then those left
/// to the nearest place still with room; units that fit nowhere are left out. Ties by unit id, then place order.
pub fn assign(units: &[(UnitId, (u16, u16))], places: &[Place]) -> Vec<(UnitId, usize)> {
    let open: Vec<usize> = (0..places.len()).filter(|&p| places[p].room != Some(0)).collect();
    if open.is_empty() || units.is_empty() {
        return Vec::new();
    }
    let share = units.len().div_ceil(open.len());
    let mut pairs: Vec<(i64, UnitId, usize)> = units.iter().flat_map(|&(id, at)| open.iter().map(move |&p| (torus_dist2(at, places[p].at), id, p))).collect();
    pairs.sort_unstable();
    let mut load = vec![0usize; places.len()];
    let mut out: Vec<(UnitId, usize)> = Vec::new();
    for even in [true, false] {
        for &(_, id, p) in &pairs {
            let cap = places[p].room.unwrap_or(usize::MAX).min(if even { share } else { usize::MAX });
            if load[p] < cap && !out.iter().any(|&(u, _)| u == id) {
                load[p] += 1;
                out.push((id, p));
            }
        }
    }
    out.sort_unstable();
    out
}

/// Whether `a` and `b` are the same kind for the Build tab: a hut of any size is a hut.
fn same_kind(a: BuildingKind, b: BuildingKind) -> bool {
    matches!((a, b), (BuildingKind::Hut { .. }, BuildingKind::Hut { .. })) || a == b
}

/// Who may be sent into a building of `kind`: braves only into a vehicle hut (its crew), followers into the others
/// (not the shaman nor wildmen).
fn may_go_in(u: &Unit, kind: BuildingKind) -> bool {
    match VehicleKind::made_by(kind) {
        Some(_) => u.kind == UnitKind::Brave,
        None => !matches!(u.kind, UnitKind::Shaman | UnitKind::Wildman),
    }
}

impl GameMap {
    /// Orders sending the player's `units` to `to` (`assign`): into their built buildings of that kind, as far as
    /// there is room (a hut's or a tower's free room, a vehicle hut's free crew places; a training unit takes
    /// everyone, the others queue at its door), or round their camp fires. Units already in one of them stay.
    pub fn send_into(&self, player: u8, units: &[UnitId], to: Destination) -> Vec<Command> {
        let mine = |u: &&Unit| u.owner == player && u.is_alive() && u.aboard.is_none() && units.contains(&u.id);
        match to {
            Destination::Campfire => {
                let fires: Vec<&crate::campfire::Campfire> = self.campfires.iter().filter(|f| f.owner == player).collect();
                let places: Vec<Place> = fires.iter().map(|f| Place { at: f.centre(), room: None }).collect();
                let going: Vec<(UnitId, (u16, u16))> = self.units.iter().filter(mine).filter(|u| u.campfire().is_none()).map(|u| (u.id, (u.x, u.z))).collect();
                let assigned = assign(&going, &places);
                (0..fires.len())
                    .flat_map(|p| {
                        let ids: Vec<UnitId> = assigned.iter().filter(|a| a.1 == p).map(|a| a.0).collect();
                        if ids.is_empty() { Vec::new() } else { self.gather(player, &ids, fires[p].id) }
                    })
                    .collect()
            }
            Destination::Kind(kind) => {
                let buildings: Vec<&crate::building::Building> = self.buildings.iter().filter(|b| b.owner == player && b.stage() == Stage::Built && same_kind(b.kind, kind)).collect();
                let room = |b: &crate::building::Building| match (crate::training::trains(b.kind), VehicleKind::made_by(b.kind)) {
                    (Some(_), _) => None,
                    (_, Some(_)) => Some((CREW as usize).saturating_sub(self.workers(b.id).count())),
                    _ => Some((b.kind.capacity() as usize).saturating_sub(self.people_inside(b.id))),
                };
                let places: Vec<Place> = buildings.iter().map(|b| Place { at: b.centre(), room: room(b) }).collect();
                let already = |u: &Unit| buildings.iter().any(|b| u.inside.is_some_and(|i| i.building == b.id) || u.work == Some(b.id));
                let going: Vec<(UnitId, (u16, u16))> = self.units.iter().filter(mine).filter(|u| may_go_in(u, kind) && !already(u)).map(|u| (u.id, (u.x, u.z))).collect();
                assign(&going, &places)
                    .into_iter()
                    .map(|(unit, p)| Command::OrderUnit { player, unit, order: Order::Enter { site: (buildings[p].x, buildings[p].z) } })
                    .collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::Building;
    use crate::id::Id;

    fn units(at: &[(u16, u16)]) -> Vec<(UnitId, (u16, u16))> {
        at.iter().enumerate().map(|(k, &p)| (Id::from_raw(k as u32 + 1), p)).collect()
    }

    fn count(assigned: &[(UnitId, usize)], place: usize) -> usize {
        assigned.iter().filter(|a| a.1 == place).count()
    }

    #[test]
    fn as_many_as_there_is_room_the_rest_stay() {
        let five = units(&[(100, 0), (200, 0), (300, 0), (400, 0), (500, 0)]);
        let hut = [Place { at: (0, 0), room: Some(4) }];
        let assigned = assign(&five, &hut);
        assert_eq!(assigned.len(), 4);
        assert!(!assigned.iter().any(|a| a.0 == Id::from_raw(5)), "the farthest stays");
        assert!(assign(&five, &[Place { at: (0, 0), room: Some(0) }]).is_empty(), "a full hut");
    }

    #[test]
    fn many_spread_evenly_over_the_places_nearest_first() {
        let thirty: Vec<(u16, u16)> = (0..30).map(|k| (1000 + k * 10, 1000)).collect();
        let huts = [Place { at: (0, 0), room: None }, Place { at: (60_000, 0), room: None }, Place { at: (30_000, 30_000), room: None }];
        let assigned = assign(&units(&thirty), &huts);
        assert_eq!(assigned.len(), 30);
        assert_eq!([count(&assigned, 0), count(&assigned, 1), count(&assigned, 2)], [10, 10, 10], "an equal share each");
        let near = units(&[(100, 0), (59_900, 0)]);
        let assigned = assign(&near, &huts[..2]);
        assert_eq!(assigned, vec![(Id::from_raw(1), 0), (Id::from_raw(2), 1)], "each to its nearest");
    }

    #[test]
    fn a_small_place_fills_and_the_others_take_the_rest() {
        let six = units(&[(10, 0), (20, 0), (30, 0), (40, 0), (50, 0), (60, 0)]);
        let assigned = assign(&six, &[Place { at: (0, 0), room: Some(1) }, Place { at: (5000, 0), room: Some(10) }]);
        assert_eq!((count(&assigned, 0), count(&assigned, 1)), (1, 5));
    }

    #[test]
    fn braves_crew_vehicle_huts_followers_go_into_huts_and_training_units() {
        let mut map = GameMap::sandbox_buildings();
        let player = 0;
        let followers: Vec<UnitId> = map.units.iter().filter(|u| u.owner == player && u.in_the_open() && u.campfire().is_none()).map(|u| u.id).collect();
        let warrior = map.spawn(player, UnitKind::Warrior, (64 * 512, 60 * 512)).id;
        let selection: Vec<UnitId> = followers.iter().copied().chain([warrior]).collect();
        let crew = map.send_into(player, &selection, Destination::Kind(BuildingKind::BoatHut));
        assert!(!crew.is_empty());
        assert!(crew.iter().all(|c| matches!(c, Command::OrderUnit { unit, .. } if map.unit(*unit).unwrap().kind == UnitKind::Brave)), "braves only");
        let trainees = map.send_into(player, &selection, Destination::Kind(BuildingKind::WarriorTraining));
        assert_eq!(trainees.len(), selection.iter().filter(|&&id| map.unit(id).unwrap().kind != UnitKind::Shaman).count(), "a training unit takes everyone: they queue");
        let id = map.add_building(Building::new(BuildingKind::Hut { size: 3 }, player, 70 * 512, 60 * 512, 0));
        let huts = map.send_into(player, &selection, Destination::Kind(BuildingKind::Hut { size: 1 }));
        assert!(huts.iter().any(|c| matches!(c, Command::OrderUnit { order: Order::Enter { site }, .. } if *site == (map.building(id).unwrap().x, map.building(id).unwrap().z))), "a hut of any size");
        assert!(map.send_into(1, &selection, Destination::Kind(BuildingKind::BoatHut)).is_empty(), "only one's own people");
    }

    #[test]
    fn people_go_round_the_camp_fires() {
        let map = GameMap::sandbox_buildings();
        let braves: Vec<UnitId> = map.units.iter().filter(|u| u.kind == UnitKind::Brave && u.campfire().is_none() && u.in_the_open()).take(2).map(|u| u.id).collect();
        let orders = map.send_into(0, &braves, Destination::Campfire);
        assert_eq!(orders.len(), 2);
        assert!(orders.iter().all(|c| matches!(c, Command::OrderUnit { order: Order::Campfire { .. }, .. })));
    }
}
