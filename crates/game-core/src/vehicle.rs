//! Vehicles (docs/specs/vehicles.md, pop3-rev-analysis.md "Vehicles"): boats sail the sea and its coast, balloons
//! fly anywhere, each carrying a few people (`Vehicle::passengers`, `Unit::aboard`). Its health is also its abandon
//! timer: one less every turn, full again every turn while it moves or carries anyone, so an empty, still vehicle
//! falls apart after `LIFE` turns. Destroyed, a boat on the water sinks and one on land explodes; a balloon rises,
//! then explodes. A boat whose cell turns to land is stranded: emptied, it keels over and explodes later.

use crate::angle::{atan2, Angle};
use crate::building::BuildingKind;
use crate::health::Health;
use crate::id::Id;
use crate::map::{GameMap, Lcg};
use crate::motion::{Motion, Velocity};
use crate::path::{self, touches_sea, Ground, Mobility, Walled};
use crate::terrain::Heightmap;
use crate::time::{Countdown, Every, Tick, Ticks};
use crate::unit::{isqrt, torus_delta, UnitId};
use pop3_format::WORLD_UNITS_PER_CELL;

pub type VehicleId = Id<Vehicle>;

/// `VEHICLE_LIFE_BOAT` / `_BALLOON` (constant.dat; the exe's default is 8000): its health, and the turns an empty,
/// still vehicle lasts.
pub const LIFE: u16 = 5000;
/// A stranded boat's health: this, plus a draw below `STRANDED_SPREAD` ("Stranded boat").
pub const STRANDED_LIFE: u16 = 160;
pub const STRANDED_SPREAD: u32 = 128;
/// Turns a vehicle waits after someone boards before it moves off ("Boarding and driving").
pub const BOARD_WAIT: Ticks = Ticks::new(7);
/// A balloon floats this high over the ground (vehicle table +0xF, "Drawing").
pub const HOVER: u16 = 200;
/// A sinking boat goes down and tilts this much a turn, its tilt stopping at `SINK_TILT_MAX`, and is gone at
/// `SUNK_DEPTH` ("Life, damage and destruction").
pub const SINK_STEP: u16 = 8;
pub const SINK_TILT: i32 = 8;
pub const SINK_TILT_MAX: Angle = Angle::new(398);
pub const SUNK_DEPTH: u16 = 192;
/// A destroyed balloon climbs this much a turn and bursts at `BURST_ALTITUDE` ("Life, damage and destruction").
pub const RISE_STEP: u16 = 80;
pub const BURST_ALTITUDE: u16 = 1024;
/// Boats check whether their cell turned to land on this rhythm ("Stranded boat").
pub const STRAND_EVERY: Every = Every::new(16);
/// A stranded boat keels over this much a turn, until on its side.
pub const KEEL_STEP: i32 = 8;
pub const KEEL_MAX: Angle = Angle::new(512);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VehicleKind {
    Boat,
    /// The airship.
    Balloon,
}

impl VehicleKind {
    pub const ALL: [VehicleKind; 2] = [VehicleKind::Boat, VehicleKind::Balloon];

    /// People it carries, a hard limit (vehicle table +0x8).
    pub fn seats(self) -> u8 {
        match self {
            VehicleKind::Boat => 5,
            VehicleKind::Balloon => 2,
        }
    }

    /// Pieces of wood its hut spends on one (`WOOD_VEHICLE_BOAT1` / `_AIRSHIP_1`, constant.dat 400 / 300, a piece
    /// being 100).
    pub fn wood_cost(self) -> u8 {
        match self {
            VehicleKind::Boat => 4,
            VehicleKind::Balloon => 3,
        }
    }

    pub fn mobility(self) -> Mobility {
        match self {
            VehicleKind::Boat => Mobility::Sail,
            VehicleKind::Balloon => Mobility::Fly,
        }
    }

    /// World units per turn (a guess: a little faster than people walk).
    pub fn speed(self) -> u16 {
        match self {
            VehicleKind::Boat => 96,
            VehicleKind::Balloon => 80,
        }
    }

    /// The vehicle a hut of `kind` makes, None for other buildings.
    pub fn made_by(kind: BuildingKind) -> Option<VehicleKind> {
        match kind {
            BuildingKind::BoatHut => Some(VehicleKind::Boat),
            BuildingKind::AirshipHut => Some(VehicleKind::Balloon),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            VehicleKind::Boat => "Boat",
            VehicleKind::Balloon => "Balloon",
        }
    }
}

/// The side a stranded boat falls on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Port,
    Starboard,
}

impl Side {
    fn sign(self) -> i32 {
        match self {
            Side::Port => -1,
            Side::Starboard => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Parked,
    /// On its way to `to`.
    Moving { to: (u16, u16) },
    /// A destroyed boat going down, `depth` under where it floated, its bow up by `pitch`.
    Sinking { depth: u16, pitch: Angle },
    /// A destroyed balloon going up, `climbed` over its hover height.
    Rising { climbed: u16 },
    /// A boat on land, keeled over to `side` by `roll`.
    Stranded { side: Side, roll: Angle },
}

/// What a vehicle's turn does to others: the map applies it (`GameMap::tend_vehicles`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleEvent {
    /// Everyone aboard is put out where it is.
    PutOut,
    /// It blows up: everyone aboard put out, a blast there, the vehicle gone.
    Exploded,
    /// Under the water: gone.
    Sunk,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vehicle {
    pub kind: VehicleKind,
    /// The tribe of the last one who boarded it, or of the hut that made it.
    pub owner: u8,
    pub x: u16,
    pub z: u16,
    /// Where it points, from +x towards +z.
    pub heading: Angle,
    /// Hull and abandon timer at once (see the module).
    pub health: Health,
    pub state: State,
    /// Who is aboard, in boarding order: the first drives.
    pub passengers: Vec<UnitId>,
    /// Turns before it moves off after a boarding.
    pub wait: Countdown,
    /// Waypoints left to `State::Moving::to`, next one last.
    route: Vec<(u16, u16)>,
    /// Ground revision the route was planned on; None: plan on its next turn.
    planned_on: Option<(u32, u32)>,
}

impl Vehicle {
    /// A new, empty vehicle parked at `at`.
    pub fn new(kind: VehicleKind, owner: u8, (x, z): (u16, u16), heading: Angle) -> Self {
        Vehicle { kind, owner, x, z, heading, health: Health::full(LIFE), state: State::Parked, passengers: Vec::new(), wait: Countdown::new(Ticks::ZERO), route: Vec::new(), planned_on: None }
    }

    pub fn cell(&self) -> (i32, i32) {
        ((self.x as u32 / WORLD_UNITS_PER_CELL) as i32, (self.z as u32 / WORLD_UNITS_PER_CELL) as i32)
    }

    /// Sinking or rising: past saving, nobody gets on.
    pub fn is_destroyed(&self) -> bool {
        matches!(self.state, State::Sinking { .. } | State::Rising { .. })
    }

    /// Seats nobody sits in.
    pub fn free_seats(&self) -> u8 {
        self.kind.seats().saturating_sub(self.passengers.len() as u8)
    }

    /// The height its people are at: the ground (the sea's surface) under a boat, a sinking one's depth left out (drawn
    /// from its state); `HOVER` over the ground for a balloon, and what a destroyed one has climbed.
    pub fn altitude(&self, terrain: &Heightmap) -> u16 {
        let ground = terrain.height_at(self.x as u32, self.z as u32, WORLD_UNITS_PER_CELL).max(0) as u16;
        match (self.kind, self.state) {
            (VehicleKind::Boat, _) => ground,
            (VehicleKind::Balloon, State::Rising { climbed }) => ground.saturating_add(HOVER).saturating_add(climbed),
            (VehicleKind::Balloon, _) => ground.saturating_add(HOVER),
        }
    }

    /// Sets off to `to` (world units), planning its way on its next turn; nothing once destroyed or stranded.
    pub fn go(&mut self, to: (u16, u16)) {
        if self.is_destroyed() || matches!(self.state, State::Stranded { .. }) {
            return;
        }
        self.state = State::Moving { to };
        (self.route, self.planned_on) = (Vec::new(), None);
    }

    /// `unit` of tribe `owner` takes a seat: the vehicle is that tribe's now and waits `BOARD_WAIT` before moving.
    pub fn seat(&mut self, unit: UnitId, owner: u8) {
        self.passengers.push(unit);
        self.owner = owner;
        self.wait = Countdown::new(BOARD_WAIT);
    }

    /// One turn: a destroyed one sinks or rises; a boat on land strands; health goes down, full again while it moves
    /// or carries anyone; then it moves on.
    pub fn tick(&mut self, now: Tick, ground: &impl Ground, rng: &mut Lcg) -> Option<VehicleEvent> {
        let terrain = ground.terrain();
        match self.state {
            State::Sinking { depth, pitch } => {
                let depth = depth.saturating_add(SINK_STEP);
                self.state = State::Sinking { depth, pitch: pitch.turned(SINK_TILT).min(SINK_TILT_MAX) };
                return (depth >= SUNK_DEPTH).then_some(VehicleEvent::Sunk);
            }
            State::Rising { climbed } => {
                self.state = State::Rising { climbed: climbed.saturating_add(RISE_STEP) };
                return (self.altitude(terrain) >= BURST_ALTITUDE).then_some(VehicleEvent::Exploded);
            }
            _ => {}
        }
        let on_land = !touches_sea(terrain, self.cell());
        if let State::Stranded { side, roll } = self.state {
            self.state = match on_land {
                true => State::Stranded { side, roll: keel(roll, side) },
                false => State::Parked,
            };
        } else if self.kind == VehicleKind::Boat && on_land && STRAND_EVERY.fires(now, 0) {
            let side = if rng.next_u32() & 1 == 0 { Side::Port } else { Side::Starboard };
            self.health = Health::new(STRANDED_LIFE + (rng.next_u32() % STRANDED_SPREAD) as u16, LIFE);
            self.state = State::Stranded { side, roll: Angle::ZERO };
            self.route.clear();
            return Some(VehicleEvent::PutOut);
        }
        if self.health.damage(1) {
            return Some(self.destroyed(terrain));
        }
        if matches!(self.state, State::Moving { .. }) || !self.passengers.is_empty() {
            self.health.heal(LIFE);
        }
        if let State::Moving { to } = self.state
            && self.wait.tick()
        {
            self.travel(to, ground);
        }
        None
    }

    /// Out of health: a boat on the water sinks, one on land explodes; a balloon rises. Who is aboard is put out.
    fn destroyed(&mut self, terrain: &Heightmap) -> VehicleEvent {
        self.route.clear();
        match self.kind {
            VehicleKind::Boat if terrain.is_water_at(self.x, self.z) => self.state = State::Sinking { depth: 0, pitch: Angle::ZERO },
            VehicleKind::Boat => return VehicleEvent::Exploded,
            VehicleKind::Balloon => self.state = State::Rising { climbed: 0 },
        }
        VehicleEvent::PutOut
    }

    /// One step of the way to `to`, planned again when the ground changed; parked once there, or when no way leads
    /// there.
    fn travel(&mut self, to: (u16, u16), ground: &impl Ground) {
        let mobility = self.kind.mobility();
        if self.planned_on != Some(ground.revision()) {
            match path::route(ground, mobility, (self.x, self.z), to, true) {
                Some(route) => self.route = route.into_iter().rev().collect(),
                None => {
                    self.state = State::Parked;
                    self.route.clear();
                    return;
                }
            }
            self.planned_on = Some(ground.revision());
        }
        let Some(&next) = self.route.last() else {
            self.state = State::Parked;
            return;
        };
        let (dx, dz) = (torus_delta(self.x, next.0), torus_delta(self.z, next.1));
        if (dx, dz) != (0, 0) {
            self.heading = atan2(dz, dx);
        }
        let dist = isqrt((dx * dx + dz * dz) as u32) as i32;
        let speed = self.kind.speed() as i32;
        let (sx, sz) = if dist <= speed { (dx, dz) } else { (dx * speed / dist, dz * speed / dist) };
        let (nx, nz) = (self.x.wrapping_add(sx as u16), self.z.wrapping_add(sz as u16));
        let cell = ((nx as u32 / WORLD_UNITS_PER_CELL) as i32, (nz as u32 / WORLD_UNITS_PER_CELL) as i32);
        if cell != self.cell() && !ground.passable(mobility, cell) {
            self.planned_on = None;
            return;
        }
        (self.x, self.z) = (nx, nz);
        if (nx, nz) == next {
            self.route.pop();
            if self.route.is_empty() {
                self.state = State::Parked;
            }
        }
    }
}

/// `roll` one `KEEL_STEP` further over to `side`, at most `KEEL_MAX`.
fn keel(roll: Angle, side: Side) -> Angle {
    let max = KEEL_MAX.signed();
    Angle::ZERO.turned((roll.signed() + KEEL_STEP * side.sign()).clamp(-max, max))
}

impl GameMap {
    /// Adds `v` with a fresh id; its id.
    pub fn add_vehicle(&mut self, v: Vehicle) -> VehicleId {
        let id = self.new_id_of();
        self.vehicles.insert(id, v);
        id
    }

    pub fn vehicle(&self, id: VehicleId) -> Option<&Vehicle> {
        self.vehicles.get(id)
    }

    /// Every vehicle's turn, then what it did (`VehicleEvent`), then its passengers moved along with it.
    pub(crate) fn tend_vehicles(&mut self) {
        self.sync_passengers();
        let (now, mut rng) = (self.now, self.rng);
        let ground = Walled { terrain: &self.terrain, walls: &self.walls, inside: None };
        let events: Vec<(VehicleId, VehicleEvent)> = self.vehicles.iter_mut().filter_map(|(id, v)| v.tick(now, &ground, &mut rng).map(|e| (id, e))).collect();
        self.rng = rng;
        for (id, event) in events {
            match event {
                VehicleEvent::PutOut => self.put_out(id),
                VehicleEvent::Sunk => self.vehicles.retain(|v, _| v != id),
                VehicleEvent::Exploded => {
                    self.put_out(id);
                    if let Some(v) = self.vehicles.get(id) {
                        let blast = crate::blast::effect(v.owner, (v.x, v.z));
                        self.add_effect(blast);
                    }
                    self.vehicles.retain(|v, _| v != id);
                }
            }
        }
        self.carry_passengers();
    }

    /// Passengers no longer aboard (dead, gone, teleported off) leave their seat; units aboard a vehicle that does not
    /// hold them are aboard nothing.
    fn sync_passengers(&mut self) {
        let units = &self.units;
        let aboard = |id: VehicleId, p: UnitId| units.binary_search_by_key(&p, |u| u.id).ok().is_some_and(|i| units[i].is_alive() && units[i].aboard == Some(id));
        for (id, v) in self.vehicles.iter_mut() {
            v.passengers.retain(|&p| aboard(id, p));
        }
        let vehicles = &self.vehicles;
        for u in self.units.iter_mut().filter(|u| u.aboard.is_some()) {
            let seated = u.aboard.and_then(|id| vehicles.get(id)).is_some_and(|v| v.passengers.contains(&u.id));
            if !seated {
                u.aboard = None;
            }
        }
    }

    /// Everyone aboard `id` gets out where it is: falling from a balloon, in the water off a boat at sea (where they
    /// drown, `Unit::tick`).
    pub(crate) fn put_out(&mut self, id: VehicleId) {
        let Some(v) = self.vehicles.get_mut(id) else { return };
        let out = std::mem::take(&mut v.passengers);
        let (at, altitude, ground) = ((v.x, v.z), v.altitude(&self.terrain) as i32, self.terrain.height_at(v.x as u32, v.z as u32, WORLD_UNITS_PER_CELL));
        for unit in out {
            if let Some(u) = self.unit_mut(unit) {
                u.aboard = None;
                (u.x, u.z) = at;
                if altitude > ground {
                    u.motion = Motion::flying(Velocity::ZERO, altitude);
                }
            }
        }
    }

    /// Passengers stand where their vehicle is.
    fn carry_passengers(&mut self) {
        for (_, v) in self.vehicles.iter() {
            for &p in &v.passengers {
                if let Ok(i) = self.units.binary_search_by_key(&p, |u| u.id) {
                    (self.units[i].x, self.units[i].z) = (v.x, v.z);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::{Action, UnitKind};

    const C: i32 = 64;

    /// Land at height 100 west of x = `C`, sea east of it, with an island at (`C` + 20, `C`).
    fn coast() -> GameMap {
        let mut t = Heightmap::new(128);
        for z in 0..128 {
            for x in 0..C {
                t.set(x, z, 100);
            }
        }
        for z in C - 3..=C + 3 {
            for x in C + 18..=C + 22 {
                t.set(x, z, 100);
            }
        }
        GameMap::bare("coast", t, Vec::new())
    }

    fn at(cell: (i32, i32)) -> (u16, u16) {
        crate::slots::cell_centre(cell)
    }

    fn boat(map: &mut GameMap, cell: (i32, i32)) -> VehicleId {
        map.add_vehicle(Vehicle::new(VehicleKind::Boat, 0, at(cell), Angle::ZERO))
    }

    fn aboard(map: &mut GameMap, v: VehicleId, n: usize) -> Vec<UnitId> {
        let at = { let v = map.vehicle(v).unwrap(); (v.x, v.z) };
        (0..n)
            .map(|_| {
                let u = map.spawn(0, UnitKind::Brave, at);
                u.aboard = Some(v);
                let id = u.id;
                map.vehicles.get_mut(v).unwrap().seat(id, 0);
                id
            })
            .collect()
    }

    fn run(map: &mut GameMap, ticks: u32) {
        map.run(Ticks::new(ticks));
    }

    #[test]
    fn an_empty_still_vehicle_falls_apart_after_its_life_a_manned_one_never() {
        let mut map = coast();
        let empty = boat(&mut map, (C + 5, C));
        let manned = boat(&mut map, (C + 8, C));
        aboard(&mut map, manned, 1);
        run(&mut map, LIFE as u32 - 1);
        assert_eq!(map.vehicle(empty).unwrap().health.current(), 1);
        assert_eq!(map.vehicle(manned).unwrap().health.current(), LIFE, "refilled every turn");
        run(&mut map, 1);
        assert!(matches!(map.vehicle(empty).unwrap().state, State::Sinking { .. }), "on the water it sinks");
        run(&mut map, (SUNK_DEPTH / SINK_STEP) as u32);
        assert!(map.vehicle(empty).is_none(), "gone under");
        assert!(map.vehicle(manned).is_some());
    }

    #[test]
    fn a_sinking_boat_goes_down_bow_up() {
        let mut map = coast();
        let v = boat(&mut map, (C + 5, C));
        map.vehicles.get_mut(v).unwrap().health = Health::new(1, LIFE);
        run(&mut map, 1 + 20);
        let State::Sinking { depth, pitch } = map.vehicle(v).unwrap().state else { panic!("sinking") };
        assert_eq!((depth, pitch), (20 * SINK_STEP, Angle::new(20 * SINK_TILT as u32)));
    }

    #[test]
    fn its_people_are_put_out_when_it_sinks_and_drown() {
        let mut map = coast();
        let v = boat(&mut map, (C + 5, C));
        let people = aboard(&mut map, v, 2);
        map.vehicles.get_mut(v).unwrap().health = Health::new(1, LIFE);
        run(&mut map, 1);
        assert!(matches!(map.vehicle(v).unwrap().state, State::Sinking { .. }), "manned, but out of health");
        assert!(map.vehicle(v).unwrap().passengers.is_empty());
        run(&mut map, 1);
        assert!(people.iter().all(|&p| map.unit(p).unwrap().aboard.is_none() && map.unit(p).unwrap().action == Action::Drowning));
    }

    #[test]
    fn a_boat_destroyed_on_land_explodes() {
        let mut map = coast();
        let v = boat(&mut map, (C - 1, C));
        map.vehicles.get_mut(v).unwrap().health = Health::new(1, LIFE);
        run(&mut map, 1);
        assert!(map.vehicle(v).is_none());
        assert_eq!(map.effects.len(), 1, "a blast where it stood");
    }

    #[test]
    fn a_destroyed_balloon_rises_drops_its_people_and_bursts() {
        let mut map = coast();
        let v = map.add_vehicle(Vehicle::new(VehicleKind::Balloon, 0, at((C - 10, C)), Angle::ZERO));
        let people = aboard(&mut map, v, 2);
        map.vehicles.get_mut(v).unwrap().health = Health::new(1, LIFE);
        run(&mut map, 1);
        assert!(people.iter().all(|&p| map.unit(p).unwrap().motion.airborne()), "they fall");
        assert!(matches!(map.vehicle(v).unwrap().state, State::Rising { .. }));
        let turns = (BURST_ALTITUDE - 100 - HOVER).div_ceil(RISE_STEP) as u32;
        run(&mut map, turns - 1);
        assert!(map.vehicle(v).is_some());
        run(&mut map, 1);
        assert!(map.vehicle(v).is_none(), "burst at {BURST_ALTITUDE}");
        assert_eq!(map.effects.len(), 1);
    }

    #[test]
    fn a_boat_on_raised_land_is_emptied_keels_over_and_rights_when_the_water_comes_back() {
        let mut map = coast();
        let v = boat(&mut map, (C + 5, C));
        let people = aboard(&mut map, v, 2);
        for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            map.terrain.set(C + 5 + dx, C + dz, 50);
        }
        run(&mut map, STRAND_EVERY.period());
        let state = map.vehicle(v).unwrap().state;
        assert!(matches!(state, State::Stranded { .. }), "{state:?}");
        assert!(map.vehicle(v).unwrap().passengers.is_empty());
        assert!(people.iter().all(|&p| map.unit(p).unwrap().aboard.is_none()));
        assert!((STRANDED_LIFE..STRANDED_LIFE + STRANDED_SPREAD as u16).contains(&map.vehicle(v).unwrap().health.current()));
        run(&mut map, 100);
        let State::Stranded { roll, .. } = map.vehicle(v).unwrap().state else { panic!("stranded") };
        assert_eq!(roll.signed().abs(), KEEL_MAX.signed(), "on its side");
        map.terrain.set(C + 5, C, 0);
        run(&mut map, 1);
        assert_eq!(map.vehicle(v).unwrap().state, State::Parked);
    }

    #[test]
    fn a_stranded_boat_explodes_once_its_short_life_is_out() {
        let mut map = coast();
        let v = boat(&mut map, (C + 5, C));
        for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            map.terrain.set(C + 5 + dx, C + dz, 50);
        }
        let gone = (0..STRAND_EVERY.period() + STRANDED_LIFE as u32 + STRANDED_SPREAD).find(|_| {
            map.tick();
            map.vehicle(v).is_none()
        });
        assert!(gone.is_some_and(|t| t + 1 >= STRAND_EVERY.period() + STRANDED_LIFE as u32));
        assert_eq!(map.effects.len(), 1, "it blew up");
    }

    #[test]
    fn a_boat_sails_round_the_island_to_the_far_coast_carrying_its_people() {
        let mut map = coast();
        let v = boat(&mut map, (C + 10, C));
        let people = aboard(&mut map, v, 2);
        let to = at((C + 22, C));
        map.vehicles.get_mut(v).unwrap().go(to);
        (0..400).find(|_| {
            map.tick();
            map.vehicle(v).unwrap().state == State::Parked
        })
        .expect("arrived");
        let b = map.vehicle(v).unwrap();
        assert_eq!((b.x, b.z), to);
        assert!(touches_sea(&map.terrain, b.cell()) && !crate::unit::is_sea(&map.terrain, b.cell()), "the coast");
        assert!(people.iter().all(|&p| map.unit(p).is_some_and(|u| (u.x, u.z) == to && u.action == Action::Idle)));
    }

    #[test]
    fn it_waits_after_a_boarding_and_heads_where_it_goes() {
        let mut map = coast();
        let v = boat(&mut map, (C + 5, C + 10));
        aboard(&mut map, v, 1);
        map.vehicles.get_mut(v).unwrap().go(at((C + 5, C + 30)));
        let start = at((C + 5, C + 10));
        let pos = |m: &GameMap| m.vehicle(v).map(|v| (v.x, v.z)).unwrap();
        run(&mut map, BOARD_WAIT.get() - 1);
        assert_eq!(pos(&map), start, "still waiting");
        run(&mut map, 2);
        assert_ne!(pos(&map), start);
        assert_eq!(map.vehicle(v).unwrap().heading, Angle::new(512), "towards +z");
    }

    #[test]
    fn a_boat_cannot_be_sent_inland() {
        let mut map = coast();
        let v = boat(&mut map, (C + 5, C));
        map.vehicles.get_mut(v).unwrap().go(at((C - 20, C)));
        run(&mut map, 1);
        assert_eq!(map.vehicle(v).unwrap().state, State::Parked);
    }

    #[test]
    fn keeling_stops_on_its_side() {
        assert_eq!(keel(Angle::ZERO, Side::Port).signed(), -KEEL_STEP);
        assert_eq!(keel(Angle::new(508), Side::Starboard), KEEL_MAX);
        assert_eq!(keel(KEEL_MAX, Side::Starboard), KEEL_MAX);
        assert_eq!(keel(Angle::ZERO.turned(-508), Side::Port).signed(), -512);
    }
}
