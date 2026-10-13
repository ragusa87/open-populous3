//! Vehicles (docs/specs/vehicles.md, pop3-rev-analysis.md "Vehicles"): boats sail the sea and its coast, balloons
//! fly anywhere, each carrying a few people (`Vehicle::passengers`, `Unit::aboard`). Its health is also its abandon
//! timer: one less every turn, full again every turn while it moves or carries anyone, so an empty, still vehicle
//! falls apart after `LIFE` turns. Destroyed, a boat on the water sinks and one on land explodes; a balloon rises,
//! then explodes. A boat whose cell turns to land is stranded: emptied, it keels over and explodes later.

use crate::angle::{atan2, Angle};
use crate::building::BuildingKind;
use crate::command::Command;
use crate::health::Health;
use crate::id::Id;
use crate::map::{GameMap, Lcg};
use crate::motion::{Motion, Velocity};
use crate::path::{self, Ground, Mobility, Walled};
use crate::unit::is_sea;
use crate::terrain::Heightmap;
use crate::time::{Countdown, Every, Tick, Ticks};
use crate::unit::{isqrt, torus_delta, Action, Order, UnitId, UnitKind};
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
/// A moving balloon floats `HOVER` over the highest ground under it and at these distances ahead (world units), and
/// climbs or sinks towards that at most `CLIMB_STEP` a turn ("Terrain changes": it samples the ground ahead and
/// moves at its profile's vertical rate; the distances and the rate are guesses). It never comes nearer the ground
/// under it than `CLEARANCE` [ours].
pub const LOOK_AHEAD: [i32; 4] = [256, 512, 1024, 1536];
pub const CLIMB_STEP: u16 = 32;
pub const CLEARANCE: u16 = 100;
/// A balloon floats this high over a building, the pyramid of knowledge among them (vehicle table +0xD, "Terrain
/// changes"), and over a tree or a totem [ours, a guess], instead of `HOVER`.
pub const HOVER_OVER_BUILDING: u16 = 560;
pub const HOVER_OVER_TREE: u16 = 450;
pub const HOVER_OVER_TOTEM: u16 = 450;
/// A sinking boat goes down and tilts this much a turn, its tilt stopping at `SINK_TILT_MAX`, and is gone at
/// `SUNK_DEPTH` ("Life, damage and destruction").
pub const SINK_STEP: u16 = 8;
pub const SINK_TILT: i32 = 8;
pub const SINK_TILT_MAX: Angle = Angle::new(398);
pub const SUNK_DEPTH: u16 = 192;
/// A destroyed balloon climbs this much a turn and bursts at `BURST_ALTITUDE` ("Life, damage and destruction").
pub const RISE_STEP: u16 = 80;
pub const BURST_ALTITUDE: u16 = 1024;
/// A person's jump into its vehicle, or down from it, in turns: the vehicle's wait after a boarding [ours].
pub const BOARD_JUMP: Ticks = BOARD_WAIT;
/// A person gets on within this of the vehicle, per axis ("within 1-2 cells", "Boarding and driving").
pub const BOARD_REACH: u16 = 1024;
/// People get off onto walkable ground within this of the vehicle, per axis (as `BOARD_REACH`).
pub const UNLOAD_REACH: u16 = 1024;
/// An empty vehicle away from a resting place (`resting`) drifts to the nearest one, checked on this rhythm, at
/// this speed ("Parking and the shore": 64 turns, speed 8).
pub const DRIFT_EVERY: Every = Every::new(64);
pub const DRIFT_SPEED: u16 = 8;
/// Braves working in a vehicle hut at once [ours, a guess].
pub const CREW: u8 = 4;
/// Work for one vehicle, once its wood is there: each crew member inside adds 1 a turn (a guess: 100 s alone, 25 s
/// with a full crew).
pub const LAUNCH_WORK: u16 = 1200;
/// Up to this many cells from its jetty a boat hut looks for open sea to put its boat on.
pub const LAUNCH_SEARCH: i32 = 4;
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

/// A vehicle hut's work on its next vehicle: `done` of `of`, the vehicle out when they meet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Launch {
    done: u16,
    of: u16,
}

impl Default for Launch {
    fn default() -> Self {
        Launch { done: 0, of: LAUNCH_WORK }
    }
}

impl Launch {
    /// One turn with `crew` working inside; true once the work is done.
    pub fn advance(&mut self, crew: u8) -> bool {
        self.done = self.done.saturating_add(crew as u16).min(self.of);
        self.is_done()
    }

    pub fn is_done(self) -> bool {
        self.done >= self.of
    }

    /// Work done and work needed, for a progress bar.
    pub fn progress(self) -> (u16, u16) {
        (self.done, self.of)
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
    /// Empty, slowly on its way to a resting place at `to` (`DRIFT_SPEED`).
    Drifting { to: (u16, u16) },
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
    /// A balloon's height (terrain height units), following the ground under and ahead of it (`float_target`); None
    /// for a boat, and for a balloon until its first turn (then `HOVER` over the ground).
    pub float: Option<u16>,
    /// Waypoints left to `State::Moving::to`, next one last.
    route: Vec<(u16, u16)>,
    /// Ground revision the route was planned on; None: plan on its next turn.
    planned_on: Option<(u32, u32)>,
}

impl Vehicle {
    /// A new, empty vehicle parked at `at`.
    pub fn new(kind: VehicleKind, owner: u8, (x, z): (u16, u16), heading: Angle) -> Self {
        Vehicle { kind, owner, x, z, heading, health: Health::full(LIFE), state: State::Parked, passengers: Vec::new(), wait: Countdown::new(Ticks::ZERO), float: None, route: Vec::new(), planned_on: None }
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
    /// from its state); a balloon's `float` (`HOVER` over the ground before its first turn), never nearer the ground
    /// than `CLEARANCE`, and what a destroyed one has climbed.
    pub fn altitude(&self, terrain: &Heightmap) -> u16 {
        let ground = ground_at(terrain, (self.x, self.z));
        let floating = self.float.unwrap_or(ground.saturating_add(HOVER)).max(ground.saturating_add(CLEARANCE));
        match (self.kind, self.state) {
            (VehicleKind::Boat, _) => ground,
            (VehicleKind::Balloon, State::Rising { climbed }) => floating.saturating_add(climbed),
            (VehicleKind::Balloon, _) => floating,
        }
    }

    /// The height a balloon floats towards: over the ground under it and, while it moves, at `LOOK_AHEAD` along its
    /// heading, the highest of the ground plus what it floats over there (`hover`: `HOVER`, or more over a building,
    /// a tree, a totem).
    pub fn float_target(&self, terrain: &Heightmap, hover: impl Fn((i32, i32)) -> u16) -> u16 {
        let moving = matches!(self.state, State::Moving { .. } | State::Drifting { .. });
        let ahead = LOOK_AHEAD.iter().filter(|_| moving).map(|&d| {
            let (dx, dz) = crate::angle::rotate((d, 0), self.heading.steps());
            (self.x.wrapping_add(dx as u16), self.z.wrapping_add(dz as u16))
        });
        let cell = |p: (u16, u16)| ((p.0 as u32 / WORLD_UNITS_PER_CELL) as i32, (p.1 as u32 / WORLD_UNITS_PER_CELL) as i32);
        std::iter::once((self.x, self.z)).chain(ahead).map(|p| ground_at(terrain, p).saturating_add(hover(cell(p)))).max().unwrap_or(HOVER)
    }

    /// Sets off to `to` (world units), planning its way on its next turn; nothing once destroyed or stranded.
    pub fn go(&mut self, to: (u16, u16)) {
        if self.is_destroyed() || matches!(self.state, State::Stranded { .. }) {
            return;
        }
        self.state = State::Moving { to };
        (self.route, self.planned_on) = (Vec::new(), None);
    }

    /// Drifts to `to` (`DRIFT_SPEED`), unless destroyed or stranded.
    pub fn drift(&mut self, to: (u16, u16)) {
        if self.is_destroyed() || matches!(self.state, State::Stranded { .. }) {
            return;
        }
        self.state = State::Drifting { to };
        (self.route, self.planned_on) = (Vec::new(), None);
    }

    /// Stops where it is.
    pub fn stop(&mut self) {
        if let State::Moving { .. } | State::Drifting { .. } = self.state {
            self.state = State::Parked;
            self.route.clear();
        }
    }

    /// Whether a person of tribe `owner` may get on now: a free seat, not wrecked nor stranded, and empty or already
    /// that tribe's (an empty enemy vehicle becomes the boarder's).
    pub fn welcomes(&self, owner: u8) -> bool {
        !self.is_destroyed() && !matches!(self.state, State::Stranded { .. }) && self.free_seats() > 0 && (self.passengers.is_empty() || self.owner == owner)
    }

    /// `unit` of tribe `owner` takes a seat: the vehicle is that tribe's now and waits `BOARD_WAIT` before moving.
    pub fn seat(&mut self, unit: UnitId, owner: u8) {
        self.passengers.push(unit);
        self.owner = owner;
        self.wait = Countdown::new(BOARD_WAIT);
    }

    /// One turn: a destroyed one sinks or rises; a boat on land strands; health goes down, full again while it moves
    /// or carries anyone; then it moves on.
    pub fn tick(&mut self, now: Tick, ground: &impl Ground, hover: impl Fn((i32, i32)) -> u16, rng: &mut Lcg) -> Option<VehicleEvent> {
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
        let on_land = !is_sea(terrain, self.cell());
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
        if matches!(self.state, State::Moving { .. } | State::Drifting { .. }) || !self.passengers.is_empty() {
            self.health.heal(LIFE);
        }
        match self.state {
            State::Moving { to } if self.wait.tick() => self.travel(to, self.kind.speed(), ground),
            State::Drifting { to } => self.travel(to, DRIFT_SPEED, ground),
            _ => {}
        }
        if self.kind == VehicleKind::Balloon {
            let (now, target) = (self.altitude(terrain), self.float_target(terrain, hover));
            self.float = Some(if target > now { now.saturating_add(CLIMB_STEP).min(target) } else { now.saturating_sub(CLIMB_STEP).max(target) });
        }
        None
    }

    /// Out of health: a boat on the water sinks, one on land explodes; a balloon rises. Who is aboard is put out.
    fn destroyed(&mut self, terrain: &Heightmap) -> VehicleEvent {
        self.route.clear();
        match self.kind {
            VehicleKind::Boat if is_sea(terrain, self.cell()) => self.state = State::Sinking { depth: 0, pitch: Angle::ZERO },
            VehicleKind::Boat => return VehicleEvent::Exploded,
            VehicleKind::Balloon => self.state = State::Rising { climbed: 0 },
        }
        VehicleEvent::PutOut
    }

    /// One step of the way to `to`, planned again when the ground changed; parked once there, or when no way leads
    /// there.
    fn travel(&mut self, to: (u16, u16), speed: u16, ground: &impl Ground) {
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
        let speed = speed as i32;
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

/// Where vehicle hut `b` puts its new vehicle, and its heading: out past its jetty side (local +z), facing away
/// from the hut. A boat on the open sea nearest that point within `LAUNCH_SEARCH` cells (None without); a balloon
/// just past that side.
pub fn launch_point(b: &crate::building::Building, kind: VehicleKind, terrain: &Heightmap) -> Option<((u16, u16), Angle)> {
    let f = b.kind.footprint();
    let out = b.local_point((f.offset.0, f.offset.1 + f.half.1 + WORLD_UNITS_PER_CELL as i32 / 2));
    let centre = b.centre();
    let heading = atan2(torus_delta(centre.1, out.1), torus_delta(centre.0, out.0));
    if kind == VehicleKind::Balloon {
        return Some((out, heading));
    }
    let (cx, cz) = ((out.0 as u32 / WORLD_UNITS_PER_CELL) as i32, (out.1 as u32 / WORLD_UNITS_PER_CELL) as i32);
    let sea = (-LAUNCH_SEARCH..=LAUNCH_SEARCH)
        .flat_map(|dz| (-LAUNCH_SEARCH..=LAUNCH_SEARCH).map(move |dx| (cx + dx, cz + dz)))
        .filter(|&c| crate::unit::is_sea(terrain, c))
        .min_by_key(|&(x, z)| ((x - cx).pow(2) + (z - cz).pow(2), z, x))?;
    let size = terrain.size() as i32;
    Some((crate::slots::cell_centre((sea.0.rem_euclid(size), sea.1.rem_euclid(size))), heading))
}

/// Where an empty vehicle may rest [ours, after "Parking and the shore"]: a boat on a water cell next to land (by
/// the shore, where people board it), a balloon anywhere but over the water.
pub fn resting(kind: VehicleKind, terrain: &Heightmap, cell: (i32, i32)) -> bool {
    let water = is_sea(terrain, cell);
    match kind {
        VehicleKind::Boat => water && by_the_shore(terrain, cell),
        VehicleKind::Balloon => !water,
    }
}

/// Whether one of the 8 cells around `cell` can be walked on: a boat there is at the shore.
pub fn by_the_shore(terrain: &Heightmap, (x, z): (i32, i32)) -> bool {
    (-1..=1).flat_map(|dz| (-1..=1).map(move |dx| (dx, dz))).filter(|&d| d != (0, 0)).any(|(dx, dz)| Mobility::Walk.passable(terrain, (x + dx, z + dz)))
}

/// How high a balloon floats over `cell` (`Vehicle::float_target`): over a building's walls, a visible tree or a totem
/// standing there, higher than over bare ground.
pub fn hover_over(walls: &path::Walls, trees: &[crate::tree::Tree], totems: &[crate::totem::Totem], cell: (i32, i32)) -> u16 {
    let totem_cell = |t: &crate::totem::Totem| ((t.x as u32 / WORLD_UNITS_PER_CELL) as i32, (t.z as u32 / WORLD_UNITS_PER_CELL) as i32);
    if walls.at(cell).is_some() {
        HOVER_OVER_BUILDING
    } else if trees.iter().any(|t| t.is_visible() && t.cell() == cell) {
        HOVER_OVER_TREE
    } else if totems.iter().any(|t| !t.is_gone() && totem_cell(t) == cell) {
        HOVER_OVER_TOTEM
    } else {
        HOVER
    }
}

/// The ground's height at world point `p`, the sea at 0.
fn ground_at(terrain: &Heightmap, p: (u16, u16)) -> u16 {
    terrain.height_at(p.0 as u32, p.1 as u32, WORLD_UNITS_PER_CELL).max(0) as u16
}

/// Whether `a` is within `reach` of `b` on both axes, the short way around the torus.
fn within(a: (u16, u16), b: (u16, u16), reach: u16) -> bool {
    torus_delta(a.0, b.0).unsigned_abs() <= reach as u32 && torus_delta(a.1, b.1).unsigned_abs() <= reach as u32
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

    /// Every vehicle's turn, then what it did (`VehicleEvent`), then its passengers moved along with it. On
    /// `DRIFT_EVERY`, empty vehicles parked away from a resting place first drift to the nearest free one.
    pub(crate) fn tend_vehicles(&mut self) {
        self.sync_passengers();
        if DRIFT_EVERY.fires(self.now, 0) {
            self.drift_to_rest();
        }
        let (now, mut rng) = (self.now, self.rng);
        let ground = Walled { terrain: &self.terrain, walls: &self.walls, inside: None };
        let (trees, totems) = (&self.trees, &self.totems);
        let hover = |cell: (i32, i32)| hover_over(&self.walls, trees, totems, cell);
        let events: Vec<(VehicleId, VehicleEvent)> = self.vehicles.iter_mut().filter_map(|(id, v)| v.tick(now, &ground, hover, &mut rng).map(|e| (id, e))).collect();
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

    /// Unit `i` sent into the vehicle hut whose stored corner is `site`: a brave joins its crew (`work`) while it has
    /// room (`CREW`), anyone else walks to its door.
    pub(crate) fn join_crew(&mut self, i: usize, site: (u16, u16)) {
        let Some(b) = self.building_at_corner(site).map(|b| &self.buildings[b]) else { return };
        let (id, door) = (b.id, b.door());
        let room = self.workers(id).filter(|u| u.id != self.units[i].id).count() < CREW as usize;
        if self.units[i].kind == UnitKind::Brave && room {
            self.units[i].start(Order::Stop);
            self.units[i].work = Some(id);
        } else {
            self.units[i].start(Order::MoveTo { x: door.0, z: door.1 });
        }
    }

    /// Each built vehicle hut with its vehicle's wood on the pile works on it, a turn per crew member inside
    /// (`Launch`); once done, the wood is spent and the vehicle comes out (`launch_point`) with the first of the crew
    /// inside aboard. No room for a boat on the water: it waits, done.
    pub(crate) fn tend_vehicle_huts(&mut self) {
        for k in 0..self.buildings.len() {
            let b = &self.buildings[k];
            let Some(kind) = VehicleKind::made_by(b.kind).filter(|_| b.stage() == crate::building::Stage::Built) else { continue };
            let id = b.id;
            let crew: Vec<UnitId> = self.units_inside(id).filter(|u| u.work == Some(id)).map(|u| u.id).collect();
            if b.stock < kind.wood_cost() || crew.is_empty() {
                continue;
            }
            if !self.buildings[k].launch.get_or_insert_default().advance(crew.len().min(CREW as usize) as u8) {
                continue;
            }
            let Some((at, heading)) = launch_point(&self.buildings[k], kind, &self.terrain) else { continue };
            let b = &mut self.buildings[k];
            (b.launch, b.stock) = (None, b.stock - kind.wood_cost());
            let owner = b.owner;
            let vehicle = self.add_vehicle(Vehicle::new(kind, owner, at, heading));
            let driver = crew[0];
            if let Some(u) = self.unit_mut(driver) {
                u.work = None;
                u.start(Order::Stop);
                (u.inside, u.aboard, u.x, u.z) = (None, Some(vehicle), at.0, at.1);
            }
            self.vehicles.get_mut(vehicle).unwrap().seat(driver, owner);
        }
    }

    /// Building `i`'s work on its next vehicle, if it is a vehicle hut working on one.
    pub fn launch_progress(&self, i: usize) -> Option<Launch> {
        self.buildings.get(i)?.launch
    }

    /// Unit `i` takes `Order::Board`: anyone but wildmen walks to the ground nearest the vehicle (`AtVehicle` once
    /// there, `board`), if it may get on.
    pub(crate) fn go_board(&mut self, i: usize, vehicle: VehicleId) {
        let u = &self.units[i];
        let Some(v) = self.vehicle(vehicle).filter(|v| v.welcomes(u.owner)) else { return };
        if u.kind == UnitKind::Wildman || u.aboard.is_some() {
            return;
        }
        let cell = path::nearest_reachable(&self.ground(), Mobility::Walk, u.cell(), v.cell());
        let stand = path::closest_point_in_cell(cell, (v.x, v.z));
        self.units[i].go_board(vehicle, stand);
    }

    /// Unit `i` is next to the vehicle `vehicle`: it gets on if within `BOARD_REACH` and still welcome, else stays
    /// where it is.
    pub(crate) fn board(&mut self, i: usize, vehicle: VehicleId) {
        let u = &self.units[i];
        let Some(v) = self.vehicle(vehicle).filter(|v| v.welcomes(u.owner) && within((u.x, u.z), (v.x, v.z), BOARD_REACH)) else { return };
        let (at, id, owner) = ((v.x, v.z), u.id, u.owner);
        let u = &mut self.units[i];
        let from = (u.x, u.z);
        u.start(Order::Stop);
        (u.inside, u.aboard, (u.x, u.z)) = (None, Some(vehicle), at);
        u.action = Action::Boarding { from, left: Countdown::new(BOARD_JUMP) };
        self.vehicles.get_mut(vehicle).unwrap().seat(id, owner);
    }

    /// Sends the vehicle `id` to `to`, or to the closest point it can reach (a boat to the shore nearest a point
    /// inland); another vehicle there or going there: to the nearest free cell (one vehicle a cell).
    pub fn move_vehicle(&mut self, id: VehicleId, to: (u16, u16)) {
        let Some(v) = self.vehicle(id) else { return };
        let goal = ((to.0 as u32 / WORLD_UNITS_PER_CELL) as i32, (to.1 as u32 / WORLD_UNITS_PER_CELL) as i32);
        let ground = self.ground();
        let cell = path::nearest_reachable(&ground, v.kind.mobility(), v.cell(), goal);
        let taken = self.berths_taken(id);
        let free = path::nearest_matching(&ground, v.kind.mobility(), cell, |c| !taken.contains(&c)).unwrap_or(cell);
        let to = if free == cell { path::closest_point_in_cell(cell, to) } else { crate::slots::cell_centre(free) };
        self.vehicles.get_mut(id).unwrap().go(to);
    }

    /// An order to unit `i`, aboard a vehicle: a move takes its vehicle there, a stop stops it; it takes no other
    /// order before getting off.
    pub(crate) fn order_aboard(&mut self, i: usize, order: Order) {
        let Some(vehicle) = self.units[i].aboard else { return };
        match order {
            Order::MoveTo { x, z } => self.move_vehicle(vehicle, (x, z)),
            Order::Stop => {
                if let Some(v) = self.vehicles.get_mut(vehicle) {
                    v.stop();
                }
            }
            _ => {}
        }
    }

    /// Orders sending the player's `units` to `to`: those aboard move their vehicle there (one order per vehicle),
    /// the others walk there (`dispatch`).
    pub fn move_orders(&self, player: u8, units: &[UnitId], to: (u16, u16)) -> Vec<Command> {
        let mut vehicles: Vec<VehicleId> = Vec::new();
        let mut walkers = Vec::new();
        for &id in units {
            match self.unit(id).filter(|u| u.owner == player && u.is_alive()).map(|u| u.aboard) {
                Some(Some(v)) if !vehicles.contains(&v) => vehicles.push(v),
                Some(Some(_)) => {}
                _ => walkers.push(id),
            }
        }
        let moves = vehicles.into_iter().map(|vehicle| Command::MoveVehicle { player, vehicle, to });
        moves.chain(if walkers.is_empty() { Vec::new() } else { self.dispatch(player, &walkers, to) }).collect()
    }

    /// Where the people aboard `id` get off, in seat order: free spots on the walkable ground nearest the vehicle,
    /// within `UNLOAD_REACH`. Empty when there is none (a boat away from the shore): nobody can get off.
    pub fn unload_spots(&self, id: VehicleId) -> Vec<(u16, u16)> {
        let Some(v) = self.vehicle(id).filter(|v| !v.passengers.is_empty()) else { return Vec::new() };
        let ground = self.ground();
        let reach = (UNLOAD_REACH as u32 / WORLD_UNITS_PER_CELL) as i32;
        let (cx, cz) = v.cell();
        let near = (-reach..=reach)
            .flat_map(|dz| (-reach..=reach).map(move |dx| (dx, dz)))
            .filter(|&(dx, dz)| ground.passable(Mobility::Walk, (cx + dx, cz + dz)))
            .min_by_key(|&(dx, dz)| (dx * dx + dz * dz, dz, dx));
        let Some((dx, dz)) = near else { return Vec::new() };
        let taken = self.taken_spots(&v.passengers);
        let from = crate::slots::spot_of(crate::slots::cell_centre((cx + dx, cz + dz)));
        crate::slots::free_spots_near(&ground, from, &taken, v.passengers.len())
            .into_iter()
            .map(crate::slots::spot_centre)
            .filter(|&p| within(p, (v.x, v.z), UNLOAD_REACH))
            .collect()
    }

    /// Whether anyone aboard `id` can get off now.
    pub fn can_unload(&self, id: VehicleId) -> bool {
        !self.unload_spots(id).is_empty()
    }

    /// Everyone aboard `id` gets off onto its own spot (`unload_spots`), jumping down to it (`Action::Alighting`);
    /// those left without one stay aboard.
    pub fn unload(&mut self, id: VehicleId) {
        let spots = self.unload_spots(id);
        let Some(height) = self.vehicle(id).map(|v| v.altitude(&self.terrain)) else { return };
        let Some(v) = self.vehicles.get_mut(id) else { return };
        let from = (v.x, v.z);
        let off: Vec<UnitId> = v.passengers.iter().copied().take(spots.len()).collect();
        v.passengers.retain(|p| !off.contains(p));
        for (unit, at) in off.into_iter().zip(spots) {
            if let Some(u) = self.unit_mut(unit) {
                u.aboard = None;
                (u.x, u.z) = at;
                u.action = Action::Alighting { from, height, left: Countdown::new(BOARD_JUMP) };
            }
        }
    }

    /// The vehicle `id` loses `amount` health, unless it is of tribe `from`; at 0 it is destroyed on its next turn.
    /// Only an empty, still one stays hurt: moving or manned, it is whole again every turn.
    pub(crate) fn damage_vehicle(&mut self, id: VehicleId, amount: u16, from: u8) {
        if let Some(v) = self.vehicles.get_mut(id).filter(|v| v.owner != from) {
            v.health.damage(amount);
        }
    }

    /// Empty parked vehicles away from a resting place (`resting`) drift to the nearest free one they reach.
    fn drift_to_rest(&mut self) {
        let adrift: Vec<VehicleId> = self.vehicles.iter().filter(|(_, v)| v.state == State::Parked && v.passengers.is_empty() && !resting(v.kind, &self.terrain, v.cell())).map(|(id, _)| id).collect();
        for id in adrift {
            let v = &self.vehicles.get(id).unwrap();
            let taken = self.berths_taken(id);
            let ground = self.ground();
            let berth = path::nearest_matching(&ground, v.kind.mobility(), v.cell(), |c| resting(v.kind, &self.terrain, c) && !taken.contains(&c));
            if let Some(cell) = berth {
                self.vehicles.get_mut(id).unwrap().drift(crate::slots::cell_centre(cell));
            }
        }
    }

    /// The cells other vehicles than `except` stand in or are going to: one vehicle a cell.
    fn berths_taken(&self, except: VehicleId) -> Vec<(i32, i32)> {
        let cell = |p: (u16, u16)| ((p.0 as u32 / WORLD_UNITS_PER_CELL) as i32, (p.1 as u32 / WORLD_UNITS_PER_CELL) as i32);
        self.vehicles
            .iter()
            .filter(|&(id, _)| id != except)
            .map(|(_, v)| match v.state {
                State::Moving { to } | State::Drifting { to } => cell(to),
                _ => v.cell(),
            })
            .collect()
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
        let empty = boat(&mut map, (C, C));
        let manned = boat(&mut map, (C + 8, C));
        aboard(&mut map, manned, 1);
        run(&mut map, LIFE as u32 - 1);
        assert_eq!(map.vehicle(empty).unwrap().health.current(), 1);
        assert_eq!(map.vehicle(manned).unwrap().health.current(), LIFE, "refilled every turn");
        run(&mut map, 1);
        assert!(matches!(map.vehicle(empty).unwrap().state, State::Sinking { .. }), "on the coast's water it sinks");
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
        let v = boat(&mut map, (C - 3, C));
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
        for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            map.terrain.set(C + 5 + dx, C + dz, 0);
        }
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
        let to = at((C + 23, C));
        map.vehicles.get_mut(v).unwrap().go(to);
        (0..400).find(|_| {
            map.tick();
            map.vehicle(v).unwrap().state == State::Parked
        })
        .expect("arrived");
        let b = map.vehicle(v).unwrap();
        assert_eq!((b.x, b.z), to);
        assert!(is_sea(&map.terrain, b.cell()) && by_the_shore(&map.terrain, b.cell()), "on the water by the island");
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

    fn braves(map: &mut GameMap, owner: u8, n: i32) -> Vec<UnitId> {
        (0..n).map(|k| map.spawn(owner, UnitKind::Brave, at((C - 4, C - 3 + k))).id).collect()
    }

    fn order(map: &mut GameMap, player: u8, units: &[UnitId], order: crate::unit::Order) {
        for &unit in units {
            map.apply(&Command::OrderUnit { player, unit, order });
        }
    }

    fn until(map: &mut GameMap, ticks: u32, done: impl Fn(&GameMap) -> bool) -> bool {
        (0..ticks).any(|_| {
            map.tick();
            done(map)
        })
    }

    #[test]
    fn people_board_up_to_its_seats_the_others_stay_ashore() {
        let mut map = coast();
        let v = boat(&mut map, (C, C));
        let people = braves(&mut map, 0, 6);
        order(&mut map, 0, &people, Order::Board { vehicle: v });
        let all_still = |m: &GameMap| people.iter().all(|&p| m.unit(p).unwrap().action == Action::Idle);
        assert!(until(&mut map, 300, all_still));
        let b = map.vehicle(v).unwrap();
        assert_eq!(b.passengers.len(), 5, "a hard limit");
        let seated: Vec<UnitId> = people.iter().copied().filter(|&p| map.unit(p).unwrap().aboard == Some(v)).collect();
        assert_eq!(seated.len(), 5, "in the order they came");
        assert!(seated.iter().all(|&p| b.passengers.contains(&p) && (map.unit(p).unwrap().x, map.unit(p).unwrap().z) == (b.x, b.z)));
        assert_eq!(map.headcount(0).get(crate::headcount::State::InBoat, UnitKind::Brave), 5);
    }

    #[test]
    fn an_empty_enemy_boat_becomes_the_boarders_a_manned_one_takes_nobody_else() {
        let mut map = coast();
        let v = boat(&mut map, (C, C));
        map.vehicles.get_mut(v).unwrap().owner = 1;
        let mine = braves(&mut map, 0, 1);
        order(&mut map, 0, &mine, Order::Board { vehicle: v });
        assert!(until(&mut map, 300, |m| m.unit(mine[0]).unwrap().aboard.is_some()));
        assert!(matches!(map.unit(mine[0]).unwrap().action, Action::Boarding { .. }), "jumping in");
        assert_eq!(map.vehicle(v).unwrap().owner, 0);
        let theirs = braves(&mut map, 1, 1);
        order(&mut map, 1, &theirs, Order::Board { vehicle: v });
        until(&mut map, 300, |_| false);
        assert_eq!(map.unit(theirs[0]).unwrap().aboard, None);
        assert_eq!(map.vehicle(v).unwrap().passengers, mine);
    }

    #[test]
    fn a_boat_takes_its_people_to_the_island_and_they_get_off_there() {
        let mut map = coast();
        let v = boat(&mut map, (C + 10, C));
        let people = aboard(&mut map, v, 3);
        assert!(!map.can_unload(v), "no shore within reach");
        let island = at((C + 20, C));
        for c in map.move_orders(0, &people, island) {
            map.apply(&c);
        }
        assert!(until(&mut map, 600, |m| m.vehicle(v).unwrap().state == State::Parked));
        assert!(map.can_unload(v));
        map.apply(&Command::Unload { player: 0, vehicle: v });
        assert!(map.vehicle(v).unwrap().passengers.is_empty());
        let b = map.vehicle(v).unwrap();
        for &p in &people {
            let u = map.unit(p).unwrap();
            assert_eq!(u.aboard, None);
            assert!(map.ground().passable(Mobility::Walk, u.cell()), "on the island");
            assert!(within((u.x, u.z), (b.x, b.z), UNLOAD_REACH));
        }
        assert!(people.iter().all(|&p| matches!(map.unit(p).unwrap().action, Action::Alighting { .. })), "jumping down");
        run(&mut map, BOARD_JUMP.get());
        assert!(people.iter().all(|&p| map.unit(p).unwrap().action == Action::Idle), "standing, not drowning");
    }

    #[test]
    fn a_move_order_to_a_passenger_moves_its_vehicle_other_orders_wait_till_ashore() {
        let mut map = coast();
        let v = boat(&mut map, (C + 10, C));
        let people = aboard(&mut map, v, 1);
        map.apply(&Command::OrderUnit { player: 0, unit: people[0], order: Order::MoveTo { x: at((C + 10, C + 20)).0, z: at((C + 10, C + 20)).1 } });
        assert!(matches!(map.vehicle(v).unwrap().state, State::Moving { .. }));
        map.apply(&Command::OrderUnit { player: 0, unit: people[0], order: Order::FetchWood });
        assert_eq!(map.unit(people[0]).unwrap().aboard, Some(v));
        map.apply(&Command::MoveVehicle { player: 1, vehicle: v, to: at((C + 30, C)) });
        assert_eq!(map.vehicle(v).unwrap().state, State::Moving { to: at((C + 10, C + 20)) }, "not theirs");
    }

    #[test]
    fn people_in_a_balloon_stand_at_its_height_out_of_a_blasts_reach() {
        let mut map = coast();
        let v = map.add_vehicle(Vehicle::new(VehicleKind::Balloon, 0, at((C - 10, C)), Angle::ZERO));
        let people = aboard(&mut map, v, 2);
        let u = map.unit(people[0]).unwrap();
        assert_eq!(map.altitude(u), 100 + HOVER);
        map.add_effect(crate::blast::effect(1, at((C - 10, C))));
        run(&mut map, 3);
        assert!(people.iter().all(|&p| map.unit(p).unwrap().health.is_full() && map.unit(p).unwrap().aboard == Some(v)));
    }

    /// A built hut of `kind` on the coast, its jetty side to the sea (+x), with `wood` pieces lying inland.
    fn shipyard(kind: BuildingKind, wood: u16) -> (GameMap, usize) {
        let mut map = coast();
        map.add_building(crate::building::Building::new(kind, 0, ((C - 2) * 512) as u16, (C * 512) as u16, 2));
        for k in 0..wood {
            map.wood.push(crate::wood::WoodPiece::new(at((C - 7, C - 2)).0 + 90 * k, at((C - 7, C - 2)).1));
        }
        map.update_walls();
        (map, 0)
    }

    fn crew(map: &mut GameMap, hut: usize, n: i32) -> Vec<UnitId> {
        let people = braves(map, 0, n);
        for c in map.enter_orders(0, &people, hut) {
            map.apply(&c);
        }
        people
    }

    #[test]
    fn a_crew_brings_the_wood_then_a_boat_comes_out_on_the_sea_with_one_of_them_aboard() {
        let (mut map, hut) = shipyard(BuildingKind::BoatHut, 6);
        let people = crew(&mut map, hut, 3);
        let id = map.buildings[hut].id;
        assert_eq!(map.workers(id).count(), 3);
        assert!(until(&mut map, 6000, |m| !m.vehicles.is_empty()), "a boat");
        let (_, boat) = map.vehicles.entries().next().unwrap();
        assert_eq!((boat.kind, boat.owner), (VehicleKind::Boat, 0));
        assert!(crate::unit::is_sea(&map.terrain, boat.cell()), "on the open sea");
        assert_eq!(boat.passengers.len(), 1);
        let driver = boat.passengers[0];
        assert!(people.contains(&driver));
        assert_eq!(map.unit(driver).unwrap().work, None, "out of the crew");
        assert_eq!(map.workers(id).count(), 2, "the others go on");
        assert_eq!(map.buildings[hut].stock + map.wood.len() as u8, 2, "4 pieces spent");
        assert_eq!(map.buildings[hut].launch, None);
    }

    #[test]
    fn no_crew_inside_no_work_and_a_full_crew_works_four_times_as_fast() {
        let (mut map, hut) = shipyard(BuildingKind::BoatHut, 0);
        map.buildings[hut].stock = 4;
        run(&mut map, 50);
        assert_eq!(map.launch_progress(hut), None, "nobody inside");
        crew(&mut map, hut, 4);
        assert!(until(&mut map, 300, |m| m.units_inside(m.buildings[hut].id).count() == 4));
        let before = map.launch_progress(hut).map_or(0, |l| l.progress().0);
        run(&mut map, 10);
        assert_eq!(map.launch_progress(hut).unwrap().progress().0, before + 40);
    }

    #[test]
    fn an_airship_hut_makes_a_balloon_by_the_hut() {
        let (mut map, hut) = shipyard(BuildingKind::AirshipHut, 0);
        map.buildings[hut].stock = VehicleKind::Balloon.wood_cost();
        map.buildings[hut].launch = Some(Launch { done: LAUNCH_WORK - 1, of: LAUNCH_WORK });
        let people = crew(&mut map, hut, 1);
        assert!(until(&mut map, 300, |m| !m.vehicles.is_empty()));
        let (vid, balloon) = map.vehicles.entries().next().map(|(id, v)| (*id, v.clone())).unwrap();
        assert_eq!(balloon.kind, VehicleKind::Balloon);
        assert_eq!(map.unit(people[0]).unwrap().aboard, Some(vid));
        assert_eq!(map.buildings[hut].stock, 0);
    }

    #[test]
    fn only_braves_crew_and_only_four() {
        let (mut map, hut) = shipyard(BuildingKind::BoatHut, 0);
        let people = crew(&mut map, hut, 5);
        let warrior = map.spawn(0, UnitKind::Warrior, at((C - 6, C + 3))).id;
        for c in map.enter_orders(0, &[warrior], hut) {
            map.apply(&c);
        }
        let id = map.buildings[hut].id;
        assert_eq!(map.workers(id).map(|u| u.id).collect::<Vec<_>>(), people[..4]);
        assert_eq!(map.unit(warrior).unwrap().work, None);
    }

    #[test]
    fn a_blast_sinks_a_worn_empty_enemy_boat_a_manned_or_own_one_holds() {
        let mut map = coast();
        let (empty, manned, own) = (boat(&mut map, (C + 6, C)), boat(&mut map, (C + 6, C + 1)), boat(&mut map, (C + 7, C)));
        aboard(&mut map, manned, 1);
        map.vehicles.get_mut(own).unwrap().owner = 1;
        for v in [empty, manned, own] {
            map.vehicles.get_mut(v).unwrap().health = Health::new(140, LIFE);
        }
        map.add_effect(crate::blast::effect(1, at((C + 6, C))));
        run(&mut map, 3);
        assert!(matches!(map.vehicle(empty).unwrap().state, State::Sinking { .. }), "3 × 50 wore it out");
        assert_eq!(map.vehicle(manned).unwrap().state, State::Parked, "whole again every turn");
        assert_eq!(map.vehicle(own).unwrap().health.current(), 140 - 3, "its own tribe's blast: only time");
    }

    #[test]
    fn a_shaman_in_a_balloon_is_flown_into_range_then_casts() {
        let mut map = coast();
        map.sites = vec![crate::site::ReincarnationSite::at_cell(0, (C - 30, C))];
        map.spell_books = vec![crate::spell_book::SpellBook::sandbox(); 4];
        let v = map.add_vehicle(Vehicle::new(VehicleKind::Balloon, 0, at((C - 20, C)), Angle::ZERO));
        let shaman = map.spawn(0, UnitKind::Shaman, at((C - 20, C))).id;
        map.unit_mut(shaman).unwrap().aboard = Some(v);
        map.vehicles.get_mut(v).unwrap().seat(shaman, 0);
        let target = at((C + 20, C + 2));
        map.apply(&Command::Cast { player: 0, spell: crate::spell::Spell::Blast { at: target } });
        assert!(matches!(map.vehicle(v).unwrap().state, State::Moving { .. }), "the balloon sets off");
        assert!(until(&mut map, 600, |m| !m.effects.is_empty()), "cast once in range");
        let b = map.vehicle(v).unwrap();
        assert!(torus_delta(b.x, target.0).abs() > 1024, "from afar, not over the target");
        assert_eq!(map.unit(shaman).unwrap().aboard, Some(v));
    }

    #[test]
    fn an_empty_boat_at_sea_drifts_to_the_nearest_free_coast_and_stays() {
        let mut map = coast();
        let v = boat(&mut map, (C + 4, C));
        let berth = boat(&mut map, (C, C));
        assert!(resting(VehicleKind::Boat, &map.terrain, (C, C)) && !resting(VehicleKind::Boat, &map.terrain, (C + 4, C)));
        assert!(!resting(VehicleKind::Boat, &map.terrain, (C - 1, C)), "the coast is land for a boat");
        run(&mut map, DRIFT_EVERY.period());
        let State::Drifting { to } = map.vehicle(v).unwrap().state else { panic!("drifting") };
        let cell = ((to.0 / 512) as i32, (to.1 / 512) as i32);
        assert!(resting(VehicleKind::Boat, &map.terrain, cell) && cell != (C, C), "a free water cell by the shore: {cell:?}");
        assert_eq!(map.vehicle(berth).unwrap().state, State::Parked, "already resting");
        assert!(until(&mut map, 600, |m| m.vehicle(v).unwrap().state == State::Parked));
        assert_eq!(map.vehicle(v).unwrap().cell(), cell);
        assert_eq!(map.vehicle(v).unwrap().health.current(), LIFE, "whole while it drifted");
    }

    #[test]
    fn a_manned_boat_never_drifts_and_two_never_share_a_cell() {
        let mut map = coast();
        let (a, b) = (boat(&mut map, (C + 4, C)), boat(&mut map, (C + 6, C)));
        aboard(&mut map, a, 1);
        aboard(&mut map, b, 1);
        let to = at((C + 10, C + 10));
        map.move_vehicle(a, to);
        map.move_vehicle(b, to);
        assert!(until(&mut map, 300, |m| m.vehicle(a).unwrap().state == State::Parked && m.vehicle(b).unwrap().state == State::Parked));
        assert_eq!(map.vehicle(a).unwrap().cell(), (C + 10, C + 10));
        assert_ne!(map.vehicle(b).unwrap().cell(), (C + 10, C + 10));
        run(&mut map, DRIFT_EVERY.period());
        assert_eq!(map.vehicle(a).unwrap().state, State::Parked, "at sea but manned");
    }

    #[test]
    fn no_land_bridge_from_a_vehicle() {
        let mut map = coast();
        map.spell_books = vec![crate::spell_book::SpellBook::sandbox(); 4];
        let v = boat(&mut map, (C + 2, C));
        let shaman = map.spawn(0, UnitKind::Shaman, at((C - 2, C))).id;
        let bridge = crate::spell::Spell::LandBridge { from: (C - 2, C), to: (C - 10, C) };
        assert!(map.can_cast(0, &bridge));
        map.unit_mut(shaman).unwrap().aboard = Some(v);
        map.vehicles.get_mut(v).unwrap().seat(shaman, 0);
        assert!(!map.can_cast(0, &bridge), "aboard");
        map.shaman_omni = true;
        assert!(map.can_cast(0, &bridge), "a god bridges from the view");
    }

    #[test]
    fn a_balloon_climbs_before_a_hill_keeps_clear_of_it_and_comes_down_after() {
        let mut map = coast();
        map.terrain.raise((C - 20, C), 2, 400);
        let v = map.add_vehicle(Vehicle::new(VehicleKind::Balloon, 0, at((C - 30, C)), Angle::ZERO));
        aboard(&mut map, v, 1);
        map.vehicles.get_mut(v).unwrap().go(at((C - 10, C)));
        let (mut highest, mut before_hill) = (0, 0);
        let parked = until(&mut map, 600, |m| m.vehicle(v).unwrap().state == State::Parked);
        assert!(parked);
        map.vehicles.get_mut(v).unwrap().go(at((C - 30, C)));
        for _ in 0..600 {
            map.tick();
            let b = map.vehicle(v).unwrap();
            let ground = ground_at(&map.terrain, (b.x, b.z));
            let altitude = b.altitude(&map.terrain);
            assert!(altitude >= ground + CLEARANCE, "clear of the ground");
            highest = highest.max(altitude);
            if b.cell().0 == C - 16 {
                before_hill = before_hill.max(altitude);
            }
            if b.state == State::Parked {
                break;
            }
        }
        let peak = map.terrain.get(C - 20, C);
        assert!(highest >= peak + HOVER / 2, "over the hill: {highest}");
        assert!(before_hill > 100 + HOVER, "climbing before it");
        run(&mut map, 40);
        assert_eq!(map.vehicle(v).unwrap().altitude(&map.terrain), 100 + HOVER, "down again");
    }

    #[test]
    fn a_balloon_floats_over_a_building_and_a_tree_without_bumping_into_them() {
        let mut map = coast();
        map.add_building(crate::building::Building::new(BuildingKind::Hut { size: 1 }, 0, ((C - 20) * 512) as u16, (C * 512) as u16, 0));
        map.trees.push(crate::tree::Tree::new((C - 27, C), 0, crate::tree::MAX_SIZE));
        map.update_walls();
        let v = map.add_vehicle(Vehicle::new(VehicleKind::Balloon, 0, at((C - 40, C)), Angle::ZERO));
        aboard(&mut map, v, 1);
        map.vehicles.get_mut(v).unwrap().go(at((C - 10, C)));
        let (mut over_hut, mut over_tree) = (false, false);
        for _ in 0..600 {
            map.tick();
            let b = map.vehicle(v).unwrap();
            let (cell, altitude) = (b.cell(), b.altitude(&map.terrain));
            let ground = ground_at(&map.terrain, (b.x, b.z));
            if map.walls.at(cell).is_some() {
                assert!(altitude >= ground + HOVER_OVER_BUILDING, "over the hut at {altitude}");
                over_hut = true;
            }
            if cell == (C - 27, C) {
                assert!(altitude >= ground + HOVER_OVER_TREE, "over the tree at {altitude}");
                over_tree = true;
            }
            if b.state == State::Parked {
                break;
            }
        }
        assert!(over_hut && over_tree);
        run(&mut map, 40);
        assert_eq!(map.vehicle(v).unwrap().altitude(&map.terrain), 100 + HOVER, "down again past them");
    }

    #[test]
    fn keeling_stops_on_its_side() {
        assert_eq!(keel(Angle::ZERO, Side::Port).signed(), -KEEL_STEP);
        assert_eq!(keel(Angle::new(508), Side::Starboard), KEEL_MAX);
        assert_eq!(keel(KEEL_MAX, Side::Starboard), KEEL_MAX);
        assert_eq!(keel(Angle::ZERO.turned(-508), Side::Port).signed(), -512);
    }
}
