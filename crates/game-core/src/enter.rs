//! Going inside (docs/specs/huts-and-training.md "Done"): followers ordered into one of their tribe's
//! built buildings with room (`BuildingKind::capacity`: huts rest people, a drum tower holds one)
//! walk to its door, in, and stay there (`Unit::inside`) until another order takes them out by the
//! door. Builders use the same way in (`crate::work`). Vehicles will hold people the same way.

use crate::building::{BuildingId, BuildingKind, Stage};
use crate::command::Command;
use crate::map::{torus_cells, torus_dist2, GameMap};
use crate::time::Ticks;
use crate::tribes::TribeId;
use crate::unit::{Action, Inside, Order, Unit, UnitId, UnitKind};
use crate::work::work_point;

/// A click this close (world units) to a built building's footprint is on it.
pub const ENTER_MARGIN: i32 = 128;

/// How long an idle brave waits with nothing at all to do before heading home on its own
/// (huts-and-training.md rule 3, "Auto-housing") - `[ours]`, a guess, not itself traced.
pub const HOUSE_DELAY: Ticks = Ticks::secs(10);
/// How far it looks for a hut with room (huts-and-training.md rule 3) - `[ours]`.
pub const HOUSE_SEARCH_RADIUS_CELLS: i32 = 8;
/// How far an idle brave looks for a site to help build or dismantle (`tend_auto_work`) - `[ours]`,
/// kept tighter than `HOUSE_SEARCH_RADIUS_CELLS`: a site is usually staffed by braves already standing
/// right next to it, not drifting in from across the map.
pub const SITE_SEARCH_RADIUS_CELLS: i32 = 3;
/// A site this crowded already has enough hands (`tend_auto_work`), whatever its own `max_braves` -
/// `[ours]`: a handful of passers-by is a plausible volunteer crew, a dozen is not, and a site this
/// busy churns through its few height/wood-delivery points fast enough that extra idle bystanders just
/// end up contending over which one of them gets credit for the last one (reported live: at 16 workers
/// sharing 8 flatten points, the worker standing right on the very last one kept losing it to a
/// wood-fetch errand - `work_on`'s own fetch-before-flatten order - every time it went free for even one
/// tick, because something else was still (wrongly) "claiming" the point in the meantime).
const MAX_AUTO_WORKERS: usize = 4;

impl GameMap {
    /// The player's built building with room for people (`BuildingKind::capacity`) whose footprint,
    /// grown by `ENTER_MARGIN`, holds `at`: a click there sends followers in.
    pub fn shelter_at(&self, player: TribeId, at: (u16, u16)) -> Option<usize> {
        self.buildings.iter().position(|b| b.owner == player && b.stage() == Stage::Built && b.kind.capacity() > 0 && b.covers(at, ENTER_MARGIN))
    }

    /// Orders sending the player's followers among `units` into the building `house` (index), in id
    /// order; the shaman walks to its door.
    pub fn enter_orders(&self, player: TribeId, units: &[UnitId], house: usize) -> Vec<Command> {
        let Some(b) = self.buildings.get(house).filter(|b| b.owner == player) else { return Vec::new() };
        let mine = |u: &&Unit| u.owner == player && u.is_alive() && units.contains(&u.id);
        let mut going: Vec<UnitId> = self.units.iter().filter(mine).filter(|u| can_enter(u.kind)).map(|u| u.id).collect();
        going.sort_unstable();
        let others: Vec<UnitId> = units.iter().copied().filter(|id| !going.contains(id)).collect();
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
        if crate::vehicle::VehicleKind::made_by(b.kind).is_some() {
            let site = (b.x, b.z);
            return self.join_crew(i, site);
        }
        if u.inside.is_some_and(|ins| ins.building == b.id) {
            self.units[i].start(Order::Stop);
            return;
        }
        let (building, door) = (b.id, b.door());
        if crate::training::trains(b.kind).is_some() {
            return self.join_queue(i, building);
        }
        self.units[i].go_enter(building, door);
    }

    /// Unit `i` is at the door of the building `building`: in it goes if there is room, else it stays
    /// at the door, idle.
    pub(crate) fn come_in(&mut self, i: usize, building: BuildingId) {
        let Some(b) = self.building(building).cloned() else { return };
        if b.stage() != Stage::Built {
            return;
        }
        if self.people_inside(building) >= b.kind.capacity() as usize {
            if crate::training::trains(b.kind).is_some() {
                self.join_queue(i, building);
            }
            return;
        }
        let id = self.units[i].id;
        self.units[i].enter(Inside { building, door: b.door() }, work_point(&b, id));
    }

    /// Living units in the building `building`, standing or walking in (not those walking out).
    pub fn people_inside(&self, building: BuildingId) -> usize {
        self.units_inside(building).count()
    }

    /// Shift + left click on one of `player`'s buildings (shortcuts.md "Buildings": "one follower comes
    /// out", the same gesture as "Shift + left click on a vehicle"): its own lowest-id occupant walks out
    /// by the door to a free spot around it, released (`GameMap::send_out`) - a no-op without one of
    /// theirs inside. `[ours]`: which one leaves isn't traced, only that exactly one does.
    pub fn eject(&mut self, player: TribeId, building: BuildingId) {
        let Some(b) = self.building_index(building).filter(|&b| self.buildings[b].owner == player) else { return };
        let door = self.buildings[b].door();
        let Some(id) = self.units_inside(building).filter(|u| u.owner == player).map(|u| u.id).min() else { return };
        self.send_out(building, door, |u| u.id == id);
    }

    /// The living units in the building `building`, standing or walking in (not those walking out).
    pub fn units_inside(&self, building: BuildingId) -> impl Iterator<Item = &Unit> {
        self.units.iter().filter(move |u| u.is_alive() && u.inside.is_some_and(|i| i.building == building) && !matches!(u.action, Action::Walking { .. } | Action::Stranded { .. }))
    }

    /// Counts each building's people inside (`Building::inside`: a busy hut smokes).
    pub(crate) fn count_inside(&mut self) {
        for k in 0..self.buildings.len() {
            let id = self.buildings[k].id;
            self.buildings[k].inside = self.people_inside(id).min(u8::MAX as usize) as u8;
        }
    }

    /// Auto-housing (huts-and-training.md rule 3): a brave idle with nothing at all to do for
    /// `HOUSE_DELAY` walks into the nearest of its own tribe's built huts with room, within
    /// `HOUSE_SEARCH_RADIUS_CELLS`; none close enough with room, and it just keeps waiting (checked again
    /// next tick). Other kinds only ever enter when ordered (`can_enter`).
    pub(crate) fn tend_auto_housing(&mut self) {
        for i in 0..self.units.len() {
            let u = &self.units[i];
            let ready = u.kind == UnitKind::Brave && u.is_alive() && u.inside.is_none() && u.idle_ticks() as u32 >= HOUSE_DELAY.get();
            if !ready {
                continue;
            }
            let Some(owner) = u.owner.tribe() else { continue };
            let at = (u.x, u.z);
            let home = self
                .buildings
                .iter()
                .enumerate()
                .filter(|(_, b)| b.owner == owner && matches!(b.kind, BuildingKind::Hut { .. }) && b.stage() == Stage::Built && torus_cells(at, (b.x, b.z)) <= HOUSE_SEARCH_RADIUS_CELLS)
                .filter(|(_, b)| self.people_inside(b.id) < b.kind.capacity() as usize)
                .min_by_key(|(_, b)| torus_dist2(at, (b.x, b.z)))
                .map(|(_, b)| (b.id, b.door()));
            if let Some((building, door)) = home {
                self.units[i].go_enter(building, door);
            }
        }
    }

    /// A genuinely idle brave also looks to help out, the same `HOUSE_DELAY`/`HOUSE_SEARCH_RADIUS_CELLS`
    /// as auto-housing: the nearest of its own tribe's sites still to build (any stage short of
    /// `Stage::Built` - a fresh plan, under construction, or being dismantled) with room for another
    /// worker, assigned exactly as `Order::Build` would (`GameMap::assign`). Run after
    /// `tend_auto_housing` in the same tick, so a brave it just housed is `busy` (`to_house`) and not
    /// immediately pulled back out to a site here instead.
    ///
    /// **Open question, not settled by this session**: pop3-rev-analysis.md "Idle behaviour" reads the
    /// exe's own closest match (`0x4D9B50`, "an idle unit periodically relocates to any work site with
    /// free capacity") as gated to AI-controlled tribes only (`tribe_struct+0xC1F == 1`), never running
    /// for the player's own tribe - a player's own recollection of a human-controlled idle brave
    /// autonomously joining a construction site contradicts that reading. A coding session cannot
    /// itself re-check the gate against the exe (clean-room: that needs a dedicated spec session);
    /// implemented here regardless, for the player's own tribe, `[ours]`, pending that recheck.
    pub(crate) fn tend_auto_work(&mut self) {
        for i in 0..self.units.len() {
            let u = &self.units[i];
            let ready = u.kind == UnitKind::Brave && u.is_alive() && !u.busy() && u.idle_ticks() as u32 >= HOUSE_DELAY.get();
            if !ready {
                continue;
            }
            let Some(owner) = u.owner.tribe() else { continue };
            let at = (u.x, u.z);
            let site = self
                .buildings
                .iter()
                .filter(|b| b.owner == owner && b.stage() != Stage::Built && torus_cells(at, (b.x, b.z)) <= SITE_SEARCH_RADIUS_CELLS)
                .filter(|b| self.workers(b.id).count() < (b.kind.max_braves() as usize).min(MAX_AUTO_WORKERS))
                .min_by_key(|b| torus_dist2(at, (b.x, b.z)))
                .map(|b| (b.x, b.z));
            if let Some(corner) = site {
                self.assign(i, corner);
            }
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
    use crate::tribes::Owner;
    use crate::building::{Building, BuildingKind};

    fn corner(dx: i32, dz: i32) -> (u16, u16) {
        (((64 + dx) * 512) as u16, ((64 + dz) * 512) as u16)
    }

    /// A small, flat, self-contained map (no shaman, no sandbox fixtures) for `tend_auto_work`: it only
    /// needs one building and one brave, placed exactly where the test wants them, free of any
    /// incidental interaction with a shared sandbox's own showcase sites or free braves.
    fn bare() -> GameMap {
        let mut terrain = crate::terrain::Heightmap::new(64);
        for z in 0..64 {
            for x in 0..64 {
                terrain.set(x, z, 64);
            }
        }
        GameMap::bare("auto-work-test", terrain, Vec::new())
    }

    fn braves(map: &GameMap, n: usize) -> Vec<UnitId> {
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
        let hut = map.buildings.iter().position(|b| b.owner == Owner::Tribe(TribeId::Blue) && b.kind == BuildingKind::Hut { size: 1 } && b.stage() == Stage::Built && b.inside == 0).unwrap();
        let id = map.buildings[hut].id;
        let units = braves(&map, 5);
        for c in map.enter_orders(TribeId::Blue, &units, hut) {
            map.apply(&c);
        }
        run_until(&mut map, 600, |m| m.units.iter().filter(|u| units.contains(&u.id)).all(|u| u.action == Action::Idle)).expect("arrived");
        assert_eq!(map.people_inside(id), 3, "a small hut holds 3");
        assert_eq!(map.buildings[hut].inside, 3);
        let walled = map.buildings[hut].walled_cells(128);
        let outside = map.units.iter().filter(|u| units.contains(&u.id) && u.inside.is_none()).count();
        assert_eq!(outside, 2, "the others wait at the door");
        let resting = map.units.iter().find(|u| u.inside.is_some_and(|i| i.building == id)).unwrap().id;
        assert!(walled.contains(&map.unit(resting).unwrap().cell()));
        map.apply(&Command::OrderUnit { player: TribeId::Blue, unit: resting, order: Order::MoveTo { x: corner(20, 10).0, z: corner(20, 10).1 } });
        map.tick();
        assert_eq!(map.buildings[hut].inside, 2, "leaving");
    }

    #[test]
    fn eject_sends_out_the_lowest_id_occupant_and_leaves_the_rest() {
        let mut map = GameMap::sandbox_buildings();
        let hut = map.buildings.iter().position(|b| b.owner == Owner::Tribe(TribeId::Blue) && b.kind == BuildingKind::Hut { size: 1 } && b.stage() == Stage::Built && b.inside == 0).unwrap();
        let id = map.buildings[hut].id;
        let units = braves(&map, 3);
        for c in map.enter_orders(TribeId::Blue, &units, hut) {
            map.apply(&c);
        }
        run_until(&mut map, 600, |m| m.people_inside(id) == 3).expect("all 3 in");
        let lowest = *units.iter().min().unwrap();
        map.apply(&Command::Eject { player: TribeId::Blue, building: id });
        assert!(matches!(map.unit(lowest).unwrap().action, Action::Walking { .. }), "sent out by the door at once");
        run_until(&mut map, 600, |m| m.unit(lowest).unwrap().inside.is_none()).expect("out the door");
        assert_eq!(map.people_inside(id), 2, "walked all the way out");
        run_until(&mut map, 600, |m| m.unit(lowest).unwrap().action == Action::Idle).expect("settles outside");
        for &u in &units {
            if u != lowest {
                assert_eq!(map.unit(u).unwrap().inside, Some(Inside { building: id, door: map.buildings[hut].door() }), "{u:?} stays");
            }
        }
    }

    #[test]
    fn eject_is_a_no_op_without_one_of_the_players_own_inside() {
        let mut map = GameMap::sandbox_buildings();
        let hut = map.buildings.iter().position(|b| b.owner == Owner::Tribe(TribeId::Blue) && b.kind == BuildingKind::Hut { size: 1 } && b.stage() == Stage::Built && b.inside == 0).unwrap();
        let id = map.buildings[hut].id;
        map.apply(&Command::Eject { player: TribeId::Blue, building: id });
        assert_eq!(map.people_inside(id), 0, "empty: nothing to eject");
        map.apply(&Command::Eject { player: TribeId::Red, building: id });
        assert_eq!(map.people_inside(id), 0, "not their building either way");
    }

    #[test]
    fn an_idle_brave_with_nothing_to_do_houses_itself_after_a_delay() {
        let mut map = GameMap::sandbox_buildings();
        let hut = map
            .buildings
            .iter()
            .position(|b| b.owner == Owner::Tribe(TribeId::Blue) && b.kind == BuildingKind::Hut { size: 1 } && b.stage() == Stage::Built && (b.inside as usize) < b.kind.capacity() as usize)
            .unwrap();
        let (id, door) = (map.buildings[hut].id, map.buildings[hut].door());
        let brave = map.spawn(TribeId::Blue, UnitKind::Brave, door).id;
        for _ in 0..(HOUSE_DELAY.get() - 1) {
            map.tick();
        }
        assert!(map.unit(brave).unwrap().inside.is_none(), "not yet: still waiting out the delay");
        run_until(&mut map, 600, |m| m.unit(brave).unwrap().inside == Some(Inside { building: id, door })).expect("housed itself, with no order from the player");
    }

    #[test]
    fn a_brave_still_assigned_to_a_task_never_auto_houses() {
        let mut map = GameMap::sandbox_buildings();
        let hut = map
            .buildings
            .iter()
            .position(|b| b.owner == Owner::Tribe(TribeId::Blue) && b.kind == BuildingKind::Hut { size: 1 } && b.stage() == Stage::Built && (b.inside as usize) < b.kind.capacity() as usize)
            .unwrap();
        let door = map.buildings[hut].door();
        let brave = map.spawn(TribeId::Blue, UnitKind::Brave, door).id;
        let site = map.buildings.iter().find(|b| b.owner == Owner::Tribe(TribeId::Blue) && matches!(b.stage(), Stage::UnderConstruction { .. })).unwrap().id;
        // Standing idle right now (between two steps of its own task), but still assigned: `busy()` must
        // still hold off auto-housing, not just the moment-to-moment `Action::Idle` check.
        let k = map.unit_index(brave).unwrap();
        map.units[k].work = Some(site);
        for _ in 0..(HOUSE_DELAY.get() as usize * 4) {
            map.tick();
        }
        assert!(map.unit(brave).unwrap().inside.is_none(), "still assigned to a task: never auto-houses");
    }

    #[test]
    fn no_hut_close_enough_or_with_room_just_leaves_it_waiting() {
        let mut map = GameMap::sandbox_buildings();
        let far = corner(1000, 1000);
        let brave = map.spawn(TribeId::Blue, UnitKind::Brave, far).id;
        for _ in 0..(HOUSE_DELAY.get() as usize * 4) {
            map.tick();
        }
        assert!(map.unit(brave).unwrap().inside.is_none(), "nothing within HOUSE_SEARCH_RADIUS_CELLS: stays put");
    }

    #[test]
    fn an_idle_brave_near_a_fresh_plan_joins_it_and_flattens_it() {
        let mut map = bare();
        let site = Building::site(BuildingKind::Hut { size: 1 }, TribeId::Blue, 20 * 512, 20 * 512, 0);
        let id = map.add_building(site);
        let brave = map.spawn(TribeId::Blue, UnitKind::Brave, (22 * 512, 20 * 512)).id;
        for _ in 0..(HOUSE_DELAY.get() - 1) {
            map.tick();
        }
        assert_eq!(map.unit(brave).unwrap().work, None, "not yet: still waiting out the delay");
        run_until(&mut map, 600, |m| m.unit(brave).unwrap().work == Some(id)).expect("joined the site on its own");
        run_until(&mut map, 3000, |m| m.buildings.iter().find(|b| b.id == id).is_some_and(|b| b.flat)).expect("flattened it");
    }

    #[test]
    fn an_idle_brave_near_a_building_being_dismantled_joins_that_work() {
        let mut map = bare();
        let mut dismantling = Building::new(BuildingKind::Hut { size: 1 }, TribeId::Blue, 20 * 512, 20 * 512, 0);
        dismantling.dismantling = true;
        let id = map.add_building(dismantling);
        let brave = map.spawn(TribeId::Blue, UnitKind::Brave, (22 * 512, 20 * 512)).id;
        run_until(&mut map, HOUSE_DELAY.get() as usize + 600, |m| m.unit(brave).unwrap().work == Some(id)).expect("joined the dismantling work on its own");
        run_until(&mut map, 3000, |m| m.building(id).is_none()).expect("took it apart, piece by piece");
    }

    #[test]
    fn tend_auto_work_never_crowds_more_than_max_auto_workers_onto_one_site() {
        let mut map = bare();
        let site = Building::site(BuildingKind::Hut { size: 3 }, TribeId::Blue, 20 * 512, 20 * 512, 0);
        let id = map.add_building(site);
        // Several idle braves, all within SITE_SEARCH_RADIUS_CELLS, more than MAX_AUTO_WORKERS.
        for k in 0..8 {
            map.spawn(TribeId::Blue, UnitKind::Brave, ((21 + k) * 512, 20 * 512));
        }
        run_until(&mut map, HOUSE_DELAY.get() as usize + 200, |m| m.workers(id).count() > 0).expect("some joined");
        for _ in 0..200 {
            map.tick();
            assert!(map.workers(id).count() <= MAX_AUTO_WORKERS, "never more than MAX_AUTO_WORKERS at once: {}", map.workers(id).count());
        }
    }

    #[test]
    fn a_drum_tower_holds_one() {
        let mut map = GameMap::sandbox_buildings();
        let tower = map.buildings.iter().position(|b| b.owner == Owner::Tribe(TribeId::Blue) && b.kind == BuildingKind::DrumTower && b.stage() == Stage::Built && b.inside == 0).unwrap();
        let units = braves(&map, 2);
        assert_eq!(map.shelter_at(TribeId::Blue, map.buildings[tower].centre()), Some(tower));
        for c in map.enter_orders(TribeId::Blue, &units, tower) {
            map.apply(&c);
        }
        run_until(&mut map, 800, |m| m.units.iter().filter(|u| units.contains(&u.id)).all(|u| u.action == Action::Idle)).expect("arrived");
        assert_eq!(map.buildings[tower].inside, 1);
    }
}
