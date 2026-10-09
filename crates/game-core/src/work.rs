//! Braves building (docs/specs/buildings.md "Building it"): placing a plan, assigning braves to it
//! (`Order::Build`), and what each assigned brave does when it is free, every tick:
//! - carrying wood: takes it to the pile by the door;
//! - plan not flat yet: one brave fetches the first piece, the others jump on the footprint's height
//!   points not yet at the site's level, nearest first, one brave per point; once all are level the
//!   ring around is blended (`Building::flatten`) and it is under construction;
//! - under construction (walled, `GameMap::update_walls`): takes a piece from the pile, walks in by the
//!   door and builds it in from inside, else fetches wood while less is delivered and on the way than
//!   needed, else waits (inside, or around it);
//! - built: released.
//!
//! Integers only, units in id order.

use crate::building::{Building, BuildingKind, Stage};
use crate::command::Command;
use crate::map::{torus_dist2, GameMap};
use crate::placement::can_place;
use crate::slots;
use crate::terrain::DirtyRect;
use crate::unit::{Action, Inside, Order, Unit, UnitEvent, UnitKind, BUILD_TICKS, JUMP_TICKS};

/// Height a brave's jump moves a height point towards the site's level.
pub const JUMP_STEP: u16 = 32;
/// Braves wait and build within this far (world units) out of the footprint.
pub const AROUND: i32 = 400;
/// A brave this close (world units) to the door puts his wood on the pile.
pub const DELIVER_RADIUS: i64 = 512;
/// A brave this close (world units) to a height point jumps on it.
pub const JUMP_RADIUS: i64 = 256;
/// Standing spots looked at around a building for a brave to work from.
const SPOTS_AROUND: usize = 600;

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

    /// Orders for the player's `units` clicked on the building `site` (index): braves work on it (in
    /// id order, the first ones up to its maximum are assigned), the others walk next to its door.
    pub fn build_orders(&self, player: u8, units: &[u32], site: usize) -> Vec<Command> {
        let Some(b) = self.buildings.get(site).filter(|b| b.owner == player) else { return Vec::new() };
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
                let (h, level) = (self.terrain.get(at.0, at.1), self.buildings[b].level);
                let h = if h < level { (h + JUMP_STEP).min(level) } else { h.saturating_sub(JUMP_STEP).max(level) };
                self.terrain.set(at.0, at.1, h);
                Some(DirtyRect { min: (at.0 - 1, at.1 - 1), max: at })
            }
            UnitEvent::Built => {
                let b = &mut self.buildings[b];
                b.used = (b.used + 1).min(b.kind.wood_cost());
                None
            }
            _ => None,
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
        let around = inside || !building.covers(me, 0) && building.covers(me, AROUND);
        let door = building.door();
        if building.stock > 0 && inside {
            self.buildings[b].stock -= 1;
            self.units[i].action = Action::Building { left: BUILD_TICKS };
            self.units[i].facing = facing_to(me, building.centre());
        } else if building.stock > 0 && torus_dist2(me, door) <= DELIVER_RADIUS * DELIVER_RADIUS {
            self.units[i].enter(Inside { site, door }, work_point(&building, id));
        } else if building.stock > 0 {
            self.units[i].start(Order::MoveTo { x: door.0, z: door.1 });
        } else if wanted {
            self.fetch_wood(i);
        } else if !around && let Some(spot) = self.spot_around(b, me, id) {
            self.units[i].start(Order::MoveTo { x: spot.0, z: spot.1 });
        }
        None
    }

    /// Brave `i` jumps on the nearest height point of plan `b` still off its level that no other
    /// brave is on or going to, walking there first; with none left the plan is flat.
    fn flatten_step(&mut self, i: usize, b: usize) -> Option<DirtyRect> {
        let building = self.buildings[b].clone();
        let site = (building.x, building.z);
        let (me, id) = ((self.units[i].x, self.units[i].z), self.units[i].id);
        let size = self.terrain.size() as i32;
        let world = |(x, z): (i32, i32)| ((x.rem_euclid(size) * 512) as u16, (z.rem_euclid(size) * 512) as u16);
        let uneven: Vec<(i32, i32)> = building.ground_points().into_iter().filter(|&(x, z)| self.terrain.get(x, z) != building.level).collect();
        if uneven.is_empty() {
            let b = &mut self.buildings[b];
            b.flat = true;
            let dirty = b.flatten(&mut self.terrain);
            self.update_walls();
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
            self.units[i].action = Action::Flattening { at: point, left: JUMP_TICKS };
        } else {
            self.units[i].start(Order::MoveTo { x: at.0, z: at.1 });
        }
        None
    }

    /// The free standing spot out of building `b`'s footprint and within `AROUND` of it nearest to
    /// `me`.
    fn spot_around(&self, b: usize, me: (u16, u16), id: u32) -> Option<(u16, u16)> {
        let building = &self.buildings[b];
        let taken = self.taken_spots(&[id]);
        slots::free_spots_near(&self.ground(), slots::spot_of(building.door()), &taken, SPOTS_AROUND)
            .into_iter()
            .map(slots::spot_centre)
            .filter(|&p| !building.covers(p, 0) && building.covers(p, AROUND))
            .min_by_key(|&p| torus_dist2(me, p))
    }

    /// Whether a chained `Order::Build` still has a building to work on.
    pub(crate) fn still_building(&self, site: (u16, u16)) -> bool {
        self.building_at_corner(site).is_some_and(|b| self.buildings[b].stage() != Stage::Built)
    }
}

/// Where brave `id` builds inside `b`: around its centre, a third of a cell apart.
fn work_point(b: &Building, id: u32) -> (u16, u16) {
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
    use crate::wood::WoodPiece;

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
            map.tick();
            let now = pos(&map);
            for (a, b) in before.iter().zip(&now) {
                if !walled.contains(&a.0) && walled.contains(&b.0) {
                    assert!(near_door(a.1), "walked in away from the door");
                    assert!(b.2.is_some(), "inside");
                    entered += 1;
                }
                if walled.contains(&a.0) && !walled.contains(&b.0) {
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
}
