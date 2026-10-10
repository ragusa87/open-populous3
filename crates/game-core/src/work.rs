//! Braves building (docs/specs/buildings.md "Building it"): placing a plan, assigning braves to it
//! (`Order::Build`), and what each assigned brave does when it is free, every tick:
//! - carrying wood: takes it to the pile by the door;
//! - plan not flat yet: one brave fetches the first piece, the others jump on the footprint's height
//!   points, each at least once and until it is at the site's level, nearest first, one brave per
//!   point; once all are done the
//!   ring around is blended (`Building::flatten`) and it is under construction;
//! - under construction (walled, `GameMap::update_walls`): takes a piece from the pile, walks in by the
//!   door and builds it in from inside, else fetches wood while less is delivered and on the way than
//!   needed, else walks in and hammers until there is wood;
//! - built: released.
//!
//! Integers only, units in id order.

use crate::building::{Building, BuildingKind, Stage, DOOR_GAP};
use crate::wood::WoodPiece;
use crate::command::Command;
use crate::map::{torus_dist2, GameMap};
use crate::placement::can_place;
use crate::slots;
use crate::terrain::DirtyRect;
use crate::time::Countdown;
use crate::unit::{Action, Inside, Order, Unit, UnitEvent, UnitKind, BUILD_TICKS, JUMP_TICKS};

/// Height a brave's jump moves a height point towards the site's level.
pub const JUMP_STEP: u16 = 32;
/// A click this close (world units) to a site's footprint is on it.
pub const AROUND: i32 = 400;
/// A brave this close (world units) to the door puts his wood on the pile.
pub const DELIVER_RADIUS: i64 = 512;
/// A brave this close (world units) to a height point jumps on it.
pub const JUMP_RADIUS: i64 = 256;

impl GameMap {
    /// The building whose stored corner is `site`.
    pub fn building_at_corner(&self, site: (u16, u16)) -> Option<usize> {
        self.buildings.iter().position(|b| (b.x, b.z) == site)
    }

    /// The player's building still to be built (plan or under construction) whose footprint, grown by
    /// `AROUND`, holds world point `at`.
    pub fn site_at(&self, player: u8, at: (u16, u16)) -> Option<usize> {
        self.buildings.iter().position(|b| b.owner == player && b.stage() != Stage::Built && b.covers(at, AROUND))
    }

    /// Places `player`'s plan of `kind` with its corner at `at` if it can stand there; its index.
    pub fn place_building(&mut self, player: u8, kind: BuildingKind, at: (u16, u16), facing: u8) -> Option<usize> {
        let b = Building::placed(kind, player, at.0, at.1, facing % 8, &self.terrain);
        if kind.wood_cost() == 0 || !can_place(self, &b) {
            return None;
        }
        self.buildings.push(b);
        Some(self.buildings.len() - 1)
    }

    /// Removes `player`'s plan (not flat yet) whose footprint holds `at`: its braves stop, the wood
    /// brought is lost.
    pub fn cancel_building(&mut self, player: u8, at: (u16, u16)) {
        let Some(i) = self.buildings.iter().position(|b| b.owner == player && b.stage() == Stage::Blueprint && b.covers(at, 0)) else { return };
        let site = (self.buildings[i].x, self.buildings[i].z);
        self.buildings.remove(i);
        for u in self.units.iter_mut().filter(|u| u.work == Some(site)) {
            u.work = None;
            u.start(Order::Stop);
        }
    }

    /// Switches dismantling of `player`'s building at `site` (stored corner), flat and taking wood. On,
    /// everyone inside walks out and its braves stop building it (taking it apart comes later); off, it
    /// is built again: braves bring its missing wood back.
    pub fn set_dismantling(&mut self, player: u8, site: (u16, u16), on: bool) {
        let Some(b) = self.building_at_corner(site).filter(|&b| self.buildings[b].owner == player && self.buildings[b].flat && self.buildings[b].kind.wood_cost() > 0) else { return };
        if self.buildings[b].dismantling == on {
            return;
        }
        self.buildings[b].dismantling = on;
        if on {
            let door = self.buildings[b].door();
            self.empty_building(site, door);
        }
    }

    /// Orders for the player's `units` clicked on the building `site` (index): braves work on it (in
    /// id order, the first ones up to its maximum are assigned), the others walk next to its door.
    pub fn build_orders(&self, player: u8, units: &[u32], site: usize) -> Vec<Command> {
        let Some(b) = self.buildings.get(site).filter(|b| b.owner == player) else { return Vec::new() };
        self.orders_to_build(b, units)
    }

    /// The orders `build_orders` will give once `place` (a `Command::PlaceBuilding`) placed its plan, to send
    /// with it on the same tick; none if it cannot be placed now.
    pub fn place_orders(&self, place: &Command, units: &[u32]) -> Vec<Command> {
        let Command::PlaceBuilding { player, kind, at, facing } = *place else { return Vec::new() };
        let b = Building::placed(kind, player, at.0, at.1, facing % 8, &self.terrain);
        if kind.wood_cost() == 0 || !can_place(self, &b) {
            return Vec::new();
        }
        self.orders_to_build(&b, units)
    }

    fn orders_to_build(&self, b: &Building, units: &[u32]) -> Vec<Command> {
        let player = b.owner;
        let mine = |u: &&Unit| u.owner == player && u.is_alive() && units.contains(&u.id);
        let mut braves: Vec<u32> = self.units.iter().filter(mine).filter(|u| u.kind == UnitKind::Brave).map(|u| u.id).collect();
        braves.sort_unstable();
        let others: Vec<u32> = units.iter().copied().filter(|id| !braves.contains(id)).collect();
        let build = braves.into_iter().map(|unit| Command::OrderUnit { player, unit, order: Order::Build { site: (b.x, b.z) } });
        build.chain(if others.is_empty() { Vec::new() } else { self.dispatch(player, &others, b.door()) }).collect()
    }

    /// Living braves assigned to the building at `site`.
    pub fn workers(&self, site: (u16, u16)) -> impl Iterator<Item = &Unit> {
        self.units.iter().filter(move |u| u.is_alive() && u.work == Some(site))
    }

    /// Unit `i` takes `Order::Build`: a brave of the owner is assigned if the building still needs
    /// work and has room for him; otherwise he walks next to its door.
    pub(crate) fn assign(&mut self, i: usize, site: (u16, u16)) {
        let Some(b) = self.building_at_corner(site).map(|b| &self.buildings[b]) else { return };
        let u = &self.units[i];
        if u.kind != UnitKind::Brave || u.owner != b.owner || b.stage() == Stage::Built {
            return;
        }
        let room = self.workers(site).count() < b.kind.max_braves() as usize;
        let door = b.door();
        if room {
            self.units[i].start(Order::Stop);
            self.units[i].work = Some(site);
        } else {
            self.units[i].start(Order::MoveTo { x: door.0, z: door.1 });
        }
    }

    /// Unit `i` leaves the building it works on (another order): a piece being built goes back on
    /// the pile.
    pub(crate) fn unassign(&mut self, i: usize) {
        let Some(site) = self.units[i].work.take() else { return };
        if matches!(self.units[i].action, Action::Building { .. })
            && let Some(b) = self.building_at_corner(site)
        {
            self.buildings[b].stock += 1;
        }
    }

    /// Assigned braves of `site` other than `except` bringing wood: carrying, fetching, cutting, or
    /// building a piece taken off the pile.
    fn wood_on_the_way(&self, site: (u16, u16), except: u32) -> usize {
        self.workers(site)
            .filter(|u| u.id != except)
            .filter(|u| u.carrying > 0 || u.fetching().is_some() || u.cutting().is_some() || matches!(u.action, Action::Building { .. }))
            .count()
    }

    /// A jump landed or a piece is built in: the site of unit `i` changes. The ground a jump moved.
    pub(crate) fn work_event(&mut self, i: usize, event: UnitEvent) -> Option<DirtyRect> {
        let b = self.building_at_corner(self.units[i].work?)?;
        match event {
            UnitEvent::Jumped { at } => {
                if !self.buildings[b].jumped.contains(&at) {
                    self.buildings[b].jumped.push(at);
                }
                let (h, level) = (self.terrain.get(at.0, at.1), self.buildings[b].level);
                let h = if h < level { (h + JUMP_STEP).min(level) } else { h.saturating_sub(JUMP_STEP).max(level) };
                self.terrain.set(at.0, at.1, h);
                Some(DirtyRect { min: (at.0 - 1, at.1 - 1), max: at })
            }
            UnitEvent::Built => {
                let b = &mut self.buildings[b];
                b.used = (b.used + 1).min(b.kind.wood_cost());
                if b.stage() == Stage::Built {
                    let (site, door) = ((b.x, b.z), b.door());
                    self.empty_building(site, door);
                }
                None
            }
            _ => None,
        }
    }

    /// Everyone inside the building at `site` is released and walks out by its `door` to a free spot
    /// around it, then stands idle.
    fn empty_building(&mut self, site: (u16, u16), door: (u16, u16)) {
        let out: Vec<(u32, (u16, u16))> = self.units.iter().filter(|u| u.is_alive() && u.inside.is_some_and(|i| i.site == site)).map(|u| (u.id, (u.x, u.z))).collect();
        let ids: Vec<u32> = out.iter().map(|o| o.0).collect();
        let spots = slots::dispatch(&self.ground(), &out, door, &self.taken_spots(&ids));
        for u in self.units.iter_mut().filter(|u| ids.contains(&u.id)) {
            let to = spots.iter().find(|s| s.0 == u.id).map_or(door, |s| s.1);
            u.work = None;
            u.clear_queue();
            u.start(Order::MoveTo { x: to.0, z: to.1 });
        }
    }

    /// Every free assigned brave (idle, nothing chained) goes on with his building; returns the ground
    /// levelled around sites that became flat.
    pub(crate) fn work(&mut self) -> Vec<DirtyRect> {
        let mut dirty = Vec::new();
        for i in 0..self.units.len() {
            let u = &self.units[i];
            let Some(site) = u.work else { continue };
            if !u.is_alive() || !u.is_free() || !u.queued().is_empty() {
                continue;
            }
            match self.building_at_corner(site).filter(|&b| self.buildings[b].stage() != Stage::Built) {
                Some(b) if self.buildings[b].dismantling => {}
                Some(b) => dirty.extend(self.work_on(i, b)),
                None => self.units[i].work = None,
            }
        }
        dirty
    }

    /// Free brave `i` goes on with building `b`.
    fn work_on(&mut self, i: usize, b: usize) -> Option<DirtyRect> {
        let (me, id, carrying) = ((self.units[i].x, self.units[i].z), self.units[i].id, self.units[i].carrying);
        let building = self.buildings[b].clone();
        let site = (building.x, building.z);
        if carrying > 0 {
            let door = building.door();
            if torus_dist2(me, door) <= DELIVER_RADIUS * DELIVER_RADIUS {
                self.units[i].carrying = 0;
                self.buildings[b].stock += 1;
            } else {
                self.units[i].start(Order::MoveTo { x: door.0, z: door.1 });
            }
            return None;
        }
        let wanted = building.wood_wanted() as usize > self.wood_on_the_way(site, id);
        if !building.flat {
            if wanted {
                self.fetch_wood(i);
                if !self.units[i].is_free() {
                    return None;
                }
            }
            return self.flatten_step(i, b);
        }
        let inside = self.units[i].inside.map(|i| i.site) == Some(site);
        let door = building.door();
        if building.stock > 0 && inside {
            self.buildings[b].stock -= 1;
            self.units[i].action = Action::Building { left: Countdown::new(BUILD_TICKS) };
            self.units[i].facing = facing_to(me, building.centre());
        } else if building.stock > 0 && torus_dist2(me, door) <= DELIVER_RADIUS * DELIVER_RADIUS {
            self.units[i].enter(Inside { site, door }, work_point(&building, id));
        } else if building.stock > 0 {
            self.units[i].start(Order::MoveTo { x: door.0, z: door.1 });
        } else if wanted {
            self.fetch_wood(i);
        } else if inside {
            self.units[i].action = Action::Hammering;
        } else if torus_dist2(me, door) <= DELIVER_RADIUS * DELIVER_RADIUS {
            self.units[i].enter(Inside { site, door }, work_point(&building, id));
        } else {
            self.units[i].start(Order::MoveTo { x: door.0, z: door.1 });
        }
        None
    }

    /// Whether the plan's height point `p` is done: at the plan's level and jumped on at least once.
    fn ground_done(&self, b: &Building, p: (i32, i32)) -> bool {
        self.terrain.get(p.0, p.1) == b.level && b.jumped.contains(&p)
    }

    /// Brave `i` jumps on the nearest height point of plan `b` still off its level or never jumped on that no other
    /// brave is on or going to, walking there first; with none left the plan is flat.
    fn flatten_step(&mut self, i: usize, b: usize) -> Option<DirtyRect> {
        let building = self.buildings[b].clone();
        let site = (building.x, building.z);
        let (me, id) = ((self.units[i].x, self.units[i].z), self.units[i].id);
        let size = self.terrain.size() as i32;
        let world = |(x, z): (i32, i32)| ((x.rem_euclid(size) * 512) as u16, (z.rem_euclid(size) * 512) as u16);
        let uneven: Vec<(i32, i32)> = building.ground_points().into_iter().filter(|&p| !self.ground_done(&building, p)).collect();
        if uneven.is_empty() {
            self.buildings[b].flat = true;
            self.buildings[b].jumped.clear();
            let dirty = self.buildings[b].flatten(&mut self.terrain);
            self.update_walls();
            self.clear_wood_under(&self.buildings[b].clone());
            return Some(dirty);
        }
        let claimed = |p: (i32, i32)| {
            self.workers(site).any(|u| {
                u.id != id
                    && match u.action {
                        Action::Flattening { at, .. } => at == p,
                        Action::Walking { to } => to == world(p),
                        _ => false,
                    }
            })
        };
        let point = uneven.into_iter().filter(|&p| !claimed(p)).min_by_key(|&p| torus_dist2(me, world(p)))?;
        let at = world(point);
        if torus_dist2(me, at) <= JUMP_RADIUS * JUMP_RADIUS {
            self.units[i].action = Action::Flattening { at: point, left: Countdown::new(JUMP_TICKS) };
        } else {
            self.units[i].start(Order::MoveTo { x: at.0, z: at.1 });
        }
        None
    }

    /// Wood lying in `b`'s walls is moved out in front of its door, a little past its pile, where braves
    /// can still take it.
    fn clear_wood_under(&mut self, b: &Building) {
        let f = b.kind.footprint();
        let mut k: i32 = 0;
        for w in 0..self.wood.len() {
            if self.walls.at(cell((self.wood[w].x, self.wood[w].z))) == Some((b.x, b.z)) {
                let (col, row) = (k % 4, k / 4);
                let (x, z) = b.local_point((f.offset.0 + (col * 2 - 3) * 80, f.offset.1 - f.half.1 - DOOR_GAP - 350 - row * 90));
                self.wood[w] = WoodPiece::new(x, z);
                k += 1;
            }
        }
    }

    /// Whether a chained `Order::Build` still has a building to work on.
    pub(crate) fn still_building(&self, site: (u16, u16)) -> bool {
        self.building_at_corner(site).is_some_and(|b| self.buildings[b].stage() != Stage::Built)
    }
}

fn cell((x, z): (u16, u16)) -> (i32, i32) {
    ((x as u32 / 512) as i32, (z as u32 / 512) as i32)
}

/// Where brave `id` builds inside `b`: around its centre, a third of a cell apart.
pub(crate) fn work_point(b: &Building, id: u32) -> (u16, u16) {
    let k = (id % 9) as i32;
    b.local_point((b.kind.footprint().offset.0 + (k % 3 - 1) * 170, b.kind.footprint().offset.1 + (k / 3 - 1) * 170))
}

/// Heading from `from` to `to`.
fn facing_to(from: (u16, u16), to: (u16, u16)) -> u8 {
    let (dx, dz) = (crate::unit::torus_delta(from.0, to.0), crate::unit::torus_delta(from.1, to.1));
    crate::unit::octant(dx, dz)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::GameMap;

    const C: i32 = 64;

    /// The buildings sandbox's free ground east of the site: 8 braves, a pile of wood, trees.
    fn sandbox() -> GameMap {
        GameMap::sandbox_buildings()
    }

    fn corner(dx: i32, dz: i32) -> (u16, u16) {
        (((C + dx) * 512) as u16, ((C + dz) * 512) as u16)
    }

    fn braves(map: &GameMap, n: usize) -> Vec<u32> {
        map.units.iter().filter(|u| u.kind == UnitKind::Brave && u.campfire().is_none()).take(n).map(|u| u.id).collect()
    }

    fn place(map: &mut GameMap, kind: BuildingKind, at: (u16, u16), units: &[u32]) -> usize {
        map.apply(&Command::PlaceBuilding { player: 0, kind, at, facing: 0 });
        let site = map.building_at_corner(at).expect("placed");
        for c in map.build_orders(0, units, site) {
            map.apply(&c);
        }
        site
    }

    fn run_until(map: &mut GameMap, ticks: usize, done: impl Fn(&GameMap) -> bool) -> Option<usize> {
        (0..ticks).find(|_| {
            map.tick();
            done(map)
        })
    }

    #[test]
    fn orders_sent_with_a_plan_are_those_given_once_it_is_placed() {
        let mut map = sandbox();
        let mut units = braves(&map, 3);
        units.push(map.units.iter().find(|u| u.kind == UnitKind::Shaman).unwrap().id);
        let place = Command::PlaceBuilding { player: 0, kind: BuildingKind::Hut { size: 1 }, at: corner(24, 0), facing: 0 };
        let before = map.place_orders(&place, &units);
        assert_eq!(before.len(), 4, "the braves build, the shaman goes to the door");
        map.apply(&place);
        let site = map.building_at_corner(corner(24, 0)).unwrap();
        assert_eq!(before, map.build_orders(0, &units, site));
        assert!(map.place_orders(&place, &units).is_empty(), "the place is taken now");
    }

    #[test]
    fn a_plan_is_placed_only_where_it_can_stand() {
        let mut map = sandbox();
        let n = map.buildings.len();
        assert!(map.place_building(0, BuildingKind::Hut { size: 1 }, corner(24, 0), 0).is_some());
        assert_eq!(map.buildings[n].stage(), Stage::Blueprint);
        assert!(map.place_building(0, BuildingKind::Hut { size: 1 }, corner(24, 0), 0).is_none(), "taken");
        assert!(map.place_building(0, BuildingKind::Vault, corner(30, 10), 0).is_none(), "never built");
        assert!(map.place_building(0, BuildingKind::Hut { size: 1 }, corner(60, 0), 0).is_none(), "sea");
    }

    #[test]
    fn braves_flatten_fetch_and_build_a_hut() {
        let mut map = sandbox();
        let c = C + 24;
        for (k, (x, z)) in [(c, C - 1), (c + 1, C), (c - 1, C + 1)].into_iter().enumerate() {
            map.terrain.set(x, z, 64 + 40 * k as u16);
        }
        let units = braves(&map, 3);
        let site = place(&mut map, BuildingKind::Hut { size: 1 }, corner(24, 0), &units);
        let at = (map.buildings[site].x, map.buildings[site].z);
        assert_eq!(map.workers(at).count(), 3);
        let flat = run_until(&mut map, 2000, |m| m.buildings[site].flat).expect("flattened");
        let b = &map.buildings[site];
        assert!(b.ground_points().iter().all(|&(x, z)| map.terrain.get(x, z) == b.level));
        let built = run_until(&mut map, 6000, |m| m.buildings[site].stage() == Stage::Built).expect("built");
        assert!(flat < built);
        assert_eq!((map.buildings[site].used, map.buildings[site].stock), (3, 0));
        map.tick();
        assert_eq!(map.workers(at).count(), 0, "released");
    }

    #[test]
    fn only_as_many_braves_fetch_as_pieces_are_missing() {
        let mut map = sandbox();
        let units = braves(&map, 5);
        let site = place(&mut map, BuildingKind::Hut { size: 1 }, corner(24, 0), &units);
        let at = (map.buildings[site].x, map.buildings[site].z);
        for _ in 0..300 {
            map.tick();
            let b = &map.buildings[site];
            let out = map.workers(at).filter(|u| u.carrying > 0 || u.fetching().is_some() || u.cutting().is_some()).count();
            let limit = if b.flat { 3 } else { 1 };
            assert!(out + b.delivered() as usize <= limit, "{out} out, {} delivered", b.delivered());
        }
    }

    #[test]
    fn extra_braves_beyond_the_maximum_are_not_assigned() {
        let mut map = sandbox();
        for k in 0..6u16 {
            let id = map.units.len() as u32 + 1;
            map.units.push(Unit::new(id, 0, UnitKind::Brave, (corner(20, 6).0 + 170 * k, corner(20, 6).1)));
        }
        let units: Vec<u32> = map.units.iter().filter(|u| u.kind == UnitKind::Brave).map(|u| u.id).collect();
        assert!(units.len() > 6);
        let site = place(&mut map, BuildingKind::Hut { size: 1 }, corner(24, 0), &units);
        let at = (map.buildings[site].x, map.buildings[site].z);
        let assigned: Vec<u32> = map.workers(at).map(|u| u.id).collect();
        let mut first = units.clone();
        first.sort_unstable();
        assert_eq!(assigned, first[..6], "the first by id");
    }

    #[test]
    fn another_order_unassigns_and_puts_the_piece_back() {
        let mut map = sandbox();
        let units = braves(&map, 1);
        let site = place(&mut map, BuildingKind::Hut { size: 1 }, corner(24, 0), &units);
        let b = &mut map.buildings[site];
        (b.flat, b.stock) = (true, 1);
        let at = (b.x, b.z);
        run_until(&mut map, 400, |m| matches!(m.units.iter().find(|u| u.id == units[0]).unwrap().action, Action::Building { .. })).expect("builds");
        assert_eq!(map.buildings[site].stock, 0);
        map.apply(&Command::OrderUnit { player: 0, unit: units[0], order: Order::Stop });
        assert_eq!((map.buildings[site].stock, map.workers(at).count()), (1, 0));
    }

    #[test]
    fn a_plan_is_cancelled_its_wood_lost_not_once_flat() {
        let mut map = sandbox();
        let units = braves(&map, 2);
        let site = place(&mut map, BuildingKind::DrumTower, corner(24, 0), &units);
        map.buildings[site].stock = 1;
        let centre = map.buildings[site].centre();
        let n = map.buildings.len();
        map.apply(&Command::CancelBuilding { player: 1, at: centre });
        assert_eq!(map.buildings.len(), n, "not theirs");
        map.buildings[site].flat = true;
        map.apply(&Command::CancelBuilding { player: 0, at: centre });
        assert_eq!(map.buildings.len(), n, "flat: only dismantled");
        map.buildings[site].flat = false;
        let wood = map.wood.len();
        map.apply(&Command::CancelBuilding { player: 0, at: centre });
        assert_eq!((map.buildings.len(), map.wood.len()), (n - 1, wood));
        assert!(map.units.iter().all(|u| u.work.is_none()));
    }

    #[test]
    fn dismantling_empties_a_built_building_until_switched_back() {
        let mut map = sandbox();
        let hut = map.buildings.iter().position(|b| b.owner == 0 && b.kind == BuildingKind::Hut { size: 1 } && b.stage() == Stage::Built).unwrap();
        let site = (map.buildings[hut].x, map.buildings[hut].z);
        let resting = braves(&map, 1)[0];
        let k = map.units.iter().position(|u| u.id == resting).unwrap();
        map.units[k].inside = Some(Inside { site, door: map.buildings[hut].door() });
        map.apply(&Command::Dismantle { player: 1, site, on: true });
        assert!(!map.buildings[hut].dismantling, "not theirs");
        map.apply(&Command::Dismantle { player: 0, site, on: true });
        assert_eq!(map.buildings[hut].stage(), Stage::Dismantling { used: 3, of: 3 });
        assert!(matches!(map.units[k].action, Action::Walking { .. }), "walks out");
        map.apply(&Command::Dismantle { player: 0, site, on: false });
        assert_eq!(map.buildings[hut].stage(), Stage::Built);
    }

    #[test]
    fn a_site_dismantled_is_left_alone_until_switched_back() {
        let mut map = sandbox();
        let units = braves(&map, 1);
        let plan = place(&mut map, BuildingKind::Hut { size: 1 }, corner(24, 0), &units);
        let b = &mut map.buildings[plan];
        (b.flat, b.stock) = (true, 1);
        let at = (b.x, b.z);
        map.apply(&Command::Dismantle { player: 0, site: at, on: true });
        for _ in 0..300 {
            map.tick();
        }
        assert_eq!((map.buildings[plan].used, map.buildings[plan].stock), (0, 1), "nobody builds it");
        map.apply(&Command::Dismantle { player: 0, site: at, on: false });
        run_until(&mut map, 600, |m| m.buildings[plan].used > 0).expect("built again");
    }

    #[test]
    fn wood_brought_lands_on_the_pile() {
        let mut map = sandbox();
        map.wood.clear();
        let units = braves(&map, 1);
        let site = place(&mut map, BuildingKind::Hut { size: 1 }, corner(24, 0), &units);
        map.wood.push(WoodPiece::new(corner(22, 3).0, corner(22, 3).1));
        run_until(&mut map, 600, |m| m.buildings[site].stock == 1).expect("delivered");
        assert!(map.wood.is_empty());
    }

    #[test]
    fn walkers_go_around_a_building_never_through_a_plan_is_walked_on() {
        let mut map = sandbox();
        let units = braves(&map, 1);
        let site = map.place_building(0, BuildingKind::Temple, corner(24, 0), 0).unwrap();
        map.tick();
        assert!(map.walls.at((C + 24, C)).is_none(), "a plan has no walls");
        map.buildings[site].flat = true;
        map.tick();
        let walled: Vec<(i32, i32)> = map.buildings[site].walled_cells(128);
        assert!(walled.iter().all(|&c| map.walls.at(c).is_some()));
        let u = map.units.iter().position(|u| u.id == units[0]).unwrap();
        (map.units[u].x, map.units[u].z) = (corner(19, 0).0 + 256, corner(19, 0).1 + 256);
        map.units[u].inside = None;
        let to = (corner(29, 0).0 + 256, corner(29, 0).1 + 256);
        map.apply(&Command::OrderUnit { player: 0, unit: units[0], order: Order::MoveTo { x: to.0, z: to.1 } });
        for _ in 0..400 {
            map.tick();
            assert!(!walled.contains(&map.units[u].cell()), "crossed the temple at {:?}", map.units[u].cell());
        }
        assert_eq!((map.units[u].x, map.units[u].z), to);
    }

    #[test]
    fn builders_go_in_and_out_by_the_door() {
        let mut map = sandbox();
        let units = braves(&map, 4);
        let site = place(&mut map, BuildingKind::Hut { size: 1 }, corner(24, 0), &units);
        let (walled, door) = (map.buildings[site].walled_cells(128), map.buildings[site].door());
        let near_door = |p: (u16, u16)| torus_dist2(p, door) <= DELIVER_RADIUS * DELIVER_RADIUS;
        let pos = |m: &GameMap| m.units.iter().map(|u| (u.cell(), (u.x, u.z), u.inside)).collect::<Vec<_>>();
        let mut before = pos(&map);
        let mut entered = 0;
        for _ in 0..600 {
            // Only steps across walls standing before the tick count: walls going up around a brave on
            // the footprint put him inside without a step.
            let up = map.walls.at(walled[0]).is_some();
            map.tick();
            let now = pos(&map);
            for (a, b) in before.iter().zip(&now) {
                if up && !walled.contains(&a.0) && walled.contains(&b.0) {
                    assert!(near_door(a.1), "walked in away from the door");
                    assert!(b.2.is_some(), "inside");
                    entered += 1;
                }
                if up && walled.contains(&a.0) && !walled.contains(&b.0) {
                    assert!(near_door(b.1), "walked out away from the door");
                }
            }
            before = now;
        }
        assert_eq!(map.buildings[site].stage(), Stage::Built);
        assert!(entered >= 1, "built from inside");
    }

    #[test]
    fn braves_jump_the_sandbox_mound_flat() {
        let mut map = sandbox();
        let units = braves(&map, 4);
        let site = place(&mut map, BuildingKind::Hut { size: 1 }, corner(26, -6), &units);
        assert!(!map.buildings[site].flat, "the mound is uneven");
        let jumps = (0..2000).filter(|_| {
            map.tick();
            map.units.iter().any(|u| matches!(u.action, Action::Flattening { .. }))
        });
        assert!(jumps.count() > 0);
        assert!(map.buildings[site].flat);
    }

    #[test]
    fn once_built_everyone_inside_walks_out_and_idles() {
        let mut map = sandbox();
        let units = braves(&map, 4);
        let site = place(&mut map, BuildingKind::Hut { size: 1 }, corner(24, 0), &units);
        let at = (map.buildings[site].x, map.buildings[site].z);
        run_until(&mut map, 2000, |m| m.buildings[site].stage() == Stage::Built).expect("built");
        assert!(map.units.iter().filter(|u| u.inside.is_some_and(|i| i.site == at)).all(|u| matches!(u.action, Action::Walking { .. })), "on their way out");
        run_until(&mut map, 300, |m| m.units.iter().filter(|u| units.contains(&u.id)).all(|u| u.inside.is_none() && !matches!(u.action, Action::Walking { .. }))).expect("all out");
        let walled = map.buildings[site].walled_cells(128);
        for id in &units {
            let u = map.units.iter().find(|u| u.id == *id).unwrap();
            assert!(!walled.contains(&u.cell()) && u.work.is_none());
            assert!(matches!(u.action, Action::Idle | Action::Holding { .. }), "{:?}", u.action);
        }
    }

    #[test]
    fn trees_under_a_building_do_not_grow_back() {
        let mut map = sandbox();
        let at = corner(24, 0);
        map.trees.push(crate::tree::Tree::new((C + 24, C), 0, 0));
        let site = map.place_building(0, BuildingKind::Hut { size: 1 }, at, 0).unwrap();
        assert!(map.buildings[site].covers((map.trees.last().unwrap().x, map.trees.last().unwrap().z), 0));
        map.run(crate::tree::GROW_TICKS * 2);
        assert_eq!(map.trees.last().unwrap().size, 0);
    }

    #[test]
    fn six_braves_on_a_hut_never_stand_idle_once_flat_the_waiting_ones_hammer_inside() {
        let mut map = sandbox();
        for k in 0..2u16 {
            let id = map.units.len() as u32 + 1;
            map.units.push(Unit::new(id, 0, UnitKind::Brave, (corner(20, 6).0 + 170 * k, corner(20, 6).1)));
        }
        let mut units = braves(&map, 5);
        units.push(map.units.last().unwrap().id);
        let site = place(&mut map, BuildingKind::Hut { size: 1 }, corner(24, 0), &units);
        let at = (map.buildings[site].x, map.buildings[site].z);
        let mut idle = vec![0; units.len()];
        let mut hammered = false;
        while map.buildings[site].stage() != Stage::Built {
            map.tick();
            for (k, id) in units.iter().enumerate() {
                let u = map.units.iter().find(|u| u.id == *id).unwrap();
                hammered |= u.action == Action::Hammering && u.inside.is_some_and(|i| i.site == at);
                let building = map.buildings[site].flat;
                idle[k] = if building && u.action == Action::Idle { idle[k] + 1 } else { 0 };
                assert!(idle[k] <= 2, "brave {id} stands idle");
            }
        }
        assert!(hammered);
    }

    #[test]
    fn braves_taken_off_a_site_for_another_reach_it() {
        let mut map = sandbox();
        for k in 0..2u16 {
            let id = map.units.len() as u32 + 1;
            map.units.push(Unit::new(id, 0, UnitKind::Brave, (corner(20, 6).0 + 170 * k, corner(20, 6).1)));
        }
        let mut units = braves(&map, 5);
        units.push(map.units.last().unwrap().id);
        map.wood.clear();
        let first = place(&mut map, BuildingKind::Hut { size: 1 }, corner(24, 0), &units);
        let centre = map.buildings[first].centre();
        for (dx, dz) in [(0, 0), (300, 200), (-250, -300)] {
            map.wood.push(WoodPiece::new((centre.0 as i32 + dx) as u16, (centre.1 as i32 + dz) as u16));
        }
        for t in 0..400 {
            map.tick();
            if t > 100 && map.units.iter().any(|u| units.contains(&u.id) && u.inside.is_some() && u.action == Action::Hammering) {
                break;
            }
        }
        assert!(map.buildings[first].flat);
        assert!(map.wood.iter().all(|w| !map.behind_walls((w.x, w.z))), "the wood under it was moved out by the door");
        let second = place(&mut map, BuildingKind::Hut { size: 1 }, corner(24, -8), &units);
        for t in 0..1500 {
            map.tick();
            for u in map.units.iter().filter(|u| units.contains(&u.id)) {
                assert!(!matches!(u.action, Action::Stranded { .. }), "tick {t}: brave {} stranded at {:?} inside {:?}", u.id, (u.x, u.z), u.inside);
            }
            if map.buildings[second].stage() == Stage::Built {
                return;
            }
        }
        panic!("second hut not built");
    }

    #[test]
    fn braves_moved_between_any_two_kinds_are_never_stranded() {
        for (k, &first_kind) in crate::build_book::BUILDABLE.iter().enumerate() {
            let second_kind = crate::build_book::BUILDABLE[(k + 2) % crate::build_book::BUILDABLE.len()];
            if first_kind == BuildingKind::BoatHut || second_kind == BuildingKind::BoatHut {
                continue;
            }
            let mut map = sandbox();
            let units = braves(&map, 5);
            let first = place(&mut map, first_kind, corner(26, 2), &units);
            for _ in 0..250 {
                map.tick();
            }
            assert!(map.buildings[first].flat, "{first_kind:?} flat");
            let second = place(&mut map, second_kind, corner(26, -8), &units);
            for t in 0..2500 {
                map.tick();
                for u in map.units.iter().filter(|u| units.contains(&u.id)) {
                    assert!(!matches!(u.action, Action::Stranded { .. }), "{first_kind:?} -> {second_kind:?}, tick {t}: brave {} stranded at {:?} inside {:?}", u.id, u.cell(), u.inside);
                }
                if map.buildings[second].flat {
                    break;
                }
            }
            assert!(map.buildings[second].flat, "{first_kind:?} -> {second_kind:?}: second flattened");
        }
    }

    #[test]
    fn warrior_hut_to_temple_anywhere_any_time() {
        for wait in [5, 120, 400] {
            for (dx, dz) in [(0, -6), (0, 6), (6, 0), (-6, 0), (0, -5), (5, 0), (0, 5), (-5, 0), (9, 9)] {
                let mut map = sandbox();
                let units = braves(&map, 5);
                place(&mut map, BuildingKind::WarriorTraining, corner(26, 2), &units);
                for _ in 0..wait {
                    map.tick();
                }
                map.apply(&Command::PlaceBuilding { player: 0, kind: BuildingKind::Temple, at: corner(26 + dx, 2 + dz), facing: 0 });
                let Some(second) = map.building_at_corner(corner(26 + dx, 2 + dz)) else { continue };
                for c in map.build_orders(0, &units, second) {
                    map.apply(&c);
                }
                for t in 0..1000 {
                    map.tick();
                    for u in map.units.iter().filter(|u| units.contains(&u.id)) {
                        assert!(!matches!(u.action, Action::Stranded { .. }), "wait {wait}, temple at {dx},{dz}, tick {t}: brave {} stranded at {:?} ({:?}) inside {:?} to {:?}", u.id, u.cell(), (u.x, u.z), u.inside, u.action);
                    }
                }
            }
        }
    }
}
