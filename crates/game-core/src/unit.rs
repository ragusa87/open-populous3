//! Units (shaman, later braves, warriors...). Integer state, advanced one tick at a time.
//! Positions are fixed-point (1 cell = 512 units, like the original), wrapping at 65536: plain
//! `u16` wrapping arithmetic walks around the torus.

use crate::campfire;
use crate::path::{self, Mobility};
use crate::site::ReincarnationSite;
use crate::terrain::Heightmap;
use pop3_format::WORLD_UNITS_PER_CELL;

/// Simulation ticks per second (the client runs them at a fixed rate).
pub const TICKS_PER_SECOND: u32 = 10;
pub const SHAMAN_MAX_HEALTH: u16 = 100;
/// World units per tick on flat ground (1.25 cells per second).
pub const SHAMAN_SPEED: i32 = 64;
/// Walking speed factor change in 1/256 per 100 of slope (height per cell). Uphill: the sandbox
/// ramp (30 per cell) is ~22% slower, the steep hill (100 per cell) quarter speed. Downhill: the
/// steep hill is 1.5x faster.
pub const SLOPE_SLOWDOWN_UP: i32 = 192;
pub const SLOPE_SPEEDUP_DOWN: i32 = 128;
/// Walking speed factor bounds in 1/256 (steep climbs crawl, steep descents are capped).
pub const SLOPE_FACTOR_RANGE: (i32, i32) = (32, 384);
/// Health lost per tick while in the sea.
pub const DROWN_DAMAGE: u16 = 4;
/// Ticks between two health points regained on land.
pub const REGEN_EVERY: u8 = 5;
/// Ticks between two health points lost while stranded (100 health points last 30 s).
pub const STRANDED_HURT_EVERY: u8 = 3;
/// Length of the cast jump, the fall when dying, and the wait before reincarnation.
pub const CAST_TICKS: u16 = 12;
/// After a teleport she floats down onto the ground for this long.
pub const LANDING_TICKS: u16 = 6;
pub const DYING_TICKS: u16 = 8;
pub const RESPAWN_TICKS: u16 = 30;
/// Chopping one piece of wood off a tree (6 s, to check against the original).
pub const CHOP_TICKS: u16 = 60;
/// With nothing more to do, a brave holds his piece of wood this long before putting it down (3 s).
pub const HOLD_TICKS: u16 = 30;
/// Going round a camp fire: this fraction (numerator, denominator) of the walking speed.
pub const AROUND_FIRE_PACE: (i32, i32) = (1, 2);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnitKind {
    Shaman,
    Brave,
    Warrior,
    Preacher,
    Spy,
    Firewarrior,
    /// Neutral wild people (no tribe, owner 255 in the levels): the Convert spell makes braves of them.
    Wildman,
}

impl UnitKind {
    pub const ALL: [UnitKind; 7] =
        [UnitKind::Shaman, UnitKind::Brave, UnitKind::Warrior, UnitKind::Preacher, UnitKind::Spy, UnitKind::Firewarrior, UnitKind::Wildman];
    /// The kinds a tribe trains (not the shaman, not wildmen).
    pub const FOLLOWERS: [UnitKind; 5] = [UnitKind::Brave, UnitKind::Warrior, UnitKind::Preacher, UnitKind::Spy, UnitKind::Firewarrior];

    /// The kind of a level's person thing (model 1-7), None for others.
    pub fn from_person_model(model: u8) -> Option<Self> {
        Some(match model {
            1 => UnitKind::Wildman,
            2 => UnitKind::Brave,
            3 => UnitKind::Warrior,
            4 => UnitKind::Preacher,
            5 => UnitKind::Spy,
            6 => UnitKind::Firewarrior,
            7 => UnitKind::Shaman,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            UnitKind::Shaman => "Shaman",
            UnitKind::Brave => "Brave",
            UnitKind::Warrior => "Warrior",
            UnitKind::Preacher => "Preacher",
            UnitKind::Spy => "Spy",
            UnitKind::Firewarrior => "Firewarrior",
            UnitKind::Wildman => "Wildman",
        }
    }

    /// Placeholder balance until combat: braves are the frailest, warriors the toughest.
    pub fn max_health(self) -> u16 {
        match self {
            UnitKind::Shaman => SHAMAN_MAX_HEALTH,
            UnitKind::Brave | UnitKind::Spy | UnitKind::Wildman => 60,
            UnitKind::Preacher => 70,
            UnitKind::Firewarrior => 80,
            UnitKind::Warrior => 120,
        }
    }

    /// World units per tick on flat ground: spies are the quickest, warriors and preachers the slowest.
    pub fn speed(self) -> i32 {
        match self {
            UnitKind::Shaman | UnitKind::Brave => SHAMAN_SPEED,
            UnitKind::Spy => 72,
            UnitKind::Firewarrior => 60,
            UnitKind::Warrior | UnitKind::Preacher | UnitKind::Wildman => 56,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Idle,
    /// Following a path to `to` (replanned whenever the terrain changes).
    Walking { to: (u16, u16) },
    /// `to` cannot be reached (never the shaman, she stays idle): arms up, loses health until a
    /// path opens or the unit dies.
    Stranded { to: (u16, u16) },
    Praying,
    /// Jumping with the spell in her hands, `left` ticks to go.
    Casting { left: u16 },
    /// Just teleported: floating down onto the ground, `left` ticks to go.
    Landing { left: u16 },
    /// The ground under her is sea: loses health until dead or the land comes back.
    Drowning,
    Dying { left: u16 },
    /// Waiting to reincarnate at her site.
    Dead { left: u16 },
    /// Going round the camp fire at `fire` (its centre), last at its ring point `point`
    /// (`campfire::ring_point`), at `AROUND_FIRE_PACE` of her walking speed.
    AroundFire { fire: (u16, u16), point: u8 },
    /// A brave cutting one piece of wood off the tree at `tree` (its position), `left` ticks to go.
    Chopping { tree: (u16, u16), left: u16 },
    /// Standing with a piece of wood and nothing more to do: puts it down when `left` runs out.
    Holding { left: u16 },
}

impl Action {
    pub fn name(&self) -> &'static str {
        match self {
            Action::Idle => "Idle",
            Action::Walking { .. } => "Walking",
            Action::Stranded { .. } => "Stranded",
            Action::Praying => "Praying",
            Action::Casting { .. } => "Casting",
            Action::Landing { .. } => "Landing",
            Action::Drowning => "Drowning",
            Action::Dying { .. } => "Dying",
            Action::Dead { .. } => "Dead",
            Action::AroundFire { .. } => "Around the fire",
            Action::Chopping { .. } => "Cutting wood",
            Action::Holding { .. } => "Holding wood",
        }
    }

    /// Ticks since a timed action started (for one-shot animations), None for open-ended ones.
    pub fn elapsed(&self) -> Option<u16> {
        match *self {
            Action::Casting { left } => Some(CAST_TICKS - left.min(CAST_TICKS)),
            Action::Landing { left } => Some(LANDING_TICKS - left.min(LANDING_TICKS)),
            Action::Dying { left } => Some(DYING_TICKS - left.min(DYING_TICKS)),
            Action::Dead { left } => Some(RESPAWN_TICKS - left.min(RESPAWN_TICKS)),
            _ => None,
        }
    }

    fn can_take_orders(&self) -> bool {
        !matches!(self, Action::Drowning | Action::Dying { .. } | Action::Dead { .. })
    }
}

/// What a player tells their shaman to do (carried by `Command::Order`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    MoveTo { x: u16, z: u16 },
    /// Walk to point `point` of the ring of the camp fire at `fire` (its centre), then go round it.
    Campfire { fire: (u16, u16), point: u8 },
    Pray,
    Cast,
    Stop,
    /// Braves: cut one piece of wood off the tree at `tree` (its position), or the nearest tree with
    /// wood to spare if that one has none (`GameMap::start_order`).
    CutTree { tree: (u16, u16) },
    /// Braves: get one piece of wood from the nearest wood, a piece on the floor or a tree.
    FetchWood,
    /// Braves: pick up the piece of wood nearest to `at` (a pile clicked on), within `PICK_RADIUS`.
    PickUp { at: (u16, u16) },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitEvent {
    Died,
    /// Back at her site: the caller levels its ground.
    Reincarnated,
    /// Done holding a piece of wood: the caller puts it down where she stands.
    PutDown,
    /// Done chopping the tree at `tree`: the caller cuts it and gives the piece.
    Chopped { tree: (u16, u16) },
    /// Arrived on the piece of wood at `at`: the caller picks it up.
    PickedUp { at: (u16, u16) },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    pub id: u32,
    pub owner: u8,
    pub kind: UnitKind,
    pub x: u16,
    pub z: u16,
    /// Heading in eighths of a turn: 0 = +z, 2 = +x, 4 = -z, 6 = -x.
    pub facing: u8,
    pub health: u16,
    pub action: Action,
    /// Pieces of wood carried (braves, 0 or 1).
    pub carrying: u8,
    /// Ticks counted towards the next health point gained (or lost while stranded).
    regen: u8,
    /// Waypoints left on the way to `Walking::to`, next one last.
    route: Vec<(u16, u16)>,
    /// Terrain revision the route was planned on; None = plan on the next tick.
    planned_on: Option<u32>,
    /// Where the current cast jump takes her when it ends (Teleport).
    teleport_to: Option<(u16, u16)>,
    /// The camp fire (centre) and ring point she is walking to, to go round it once there.
    to_fire: Option<((u16, u16), u8)>,
    /// The tree (position) she is walking to, to cut it once there.
    to_tree: Option<(u16, u16)>,
    /// The piece of wood (position) she is walking to, to pick it up once there.
    to_wood: Option<(u16, u16)>,
    /// Chained orders, next first: started one by one each time the unit is idle (`GameMap::tick`).
    queue: Vec<Order>,
}

impl Unit {
    pub fn shaman(id: u32, site: &ReincarnationSite) -> Self {
        Unit::new(id, site.owner, UnitKind::Shaman, site.spawn_point())
    }

    /// A unit standing idle at `(x, z)` (world units), at full health.
    pub fn new(id: u32, owner: u8, kind: UnitKind, (x, z): (u16, u16)) -> Self {
        Unit {
            id,
            owner,
            kind,
            x,
            z,
            facing: 0,
            health: kind.max_health(),
            action: Action::Idle,
            carrying: 0,
            regen: 0,
            route: Vec::new(),
            planned_on: None,
            teleport_to: None,
            to_fire: None,
            to_tree: None,
            to_wood: None,
            queue: Vec::new(),
        }
    }

    pub fn max_health(&self) -> u16 {
        self.kind.max_health()
    }

    /// Every person walks (vehicles will sail or fly).
    pub fn mobility(&self) -> Mobility {
        Mobility::Walk
    }

    pub fn is_alive(&self) -> bool {
        !matches!(self.action, Action::Dying { .. } | Action::Dead { .. })
    }

    /// Cell holding the unit.
    pub fn cell(&self) -> (i32, i32) {
        (cell_of(self.x), cell_of(self.z))
    }

    /// Teleport: the cast jump, then she is at `to` (if she can still stand there by then).
    /// Ignored while drowning, dying or dead; another order before the jump ends cancels it.
    pub fn cast_teleport(&mut self, to: (u16, u16)) {
        if self.action.can_take_orders() {
            self.order(Order::Cast);
            self.teleport_to = Some(to);
        }
    }

    /// Where the current cast jump takes her, if anywhere.
    pub fn teleport_target(&self) -> Option<(u16, u16)> {
        self.teleport_to
    }

    /// The camp fire (centre) she is going round or walking to, if any.
    pub fn campfire(&self) -> Option<(u16, u16)> {
        match self.action {
            Action::AroundFire { fire, .. } => Some(fire),
            Action::Walking { .. } | Action::Stranded { .. } => self.to_fire.map(|(fire, _)| fire),
            _ => None,
        }
    }

    /// A direct order: replaces the chained ones. Ignored while drowning, dying or dead.
    pub fn order(&mut self, order: Order) {
        if !self.action.can_take_orders() {
            return;
        }
        self.queue.clear();
        self.start(order);
    }

    /// The tree (position) she is cutting or walking to cut, if any.
    pub fn cutting(&self) -> Option<(u16, u16)> {
        match self.action {
            Action::Chopping { tree, .. } => Some(tree),
            Action::Walking { .. } | Action::Stranded { .. } => self.to_tree,
            _ => None,
        }
    }

    /// The piece of wood (position) she is walking to pick up, if any.
    pub fn fetching(&self) -> Option<(u16, u16)> {
        match self.action {
            Action::Walking { .. } | Action::Stranded { .. } => self.to_wood,
            _ => None,
        }
    }

    /// Walks to `stand`, then cuts the tree at `tree`.
    pub fn go_cut(&mut self, tree: (u16, u16), stand: (u16, u16)) {
        self.start(Order::MoveTo { x: stand.0, z: stand.1 });
        if self.action.can_take_orders() {
            self.to_tree = Some(tree);
        }
    }

    /// Walks onto the piece of wood at `at`, then picks it up.
    pub fn go_pick(&mut self, at: (u16, u16)) {
        self.start(Order::MoveTo { x: at.0, z: at.1 });
        if self.action.can_take_orders() {
            self.to_wood = Some(at);
        }
    }

    /// Idle, or only holding a piece of wood before putting it down: free for the next order.
    pub fn is_free(&self) -> bool {
        matches!(self.action, Action::Idle | Action::Holding { .. })
    }

    /// Forgets the chained orders (a direct order replaces them).
    pub fn clear_queue(&mut self) {
        if self.action.can_take_orders() {
            self.queue.clear();
        }
    }

    /// Chained orders still to come, next first.
    pub fn queued(&self) -> &[Order] {
        &self.queue
    }

    /// Chains `order` after the current action and the orders already chained.
    pub fn enqueue(&mut self, order: Order) {
        if self.action.can_take_orders() {
            self.queue.push(order);
        }
    }

    /// Takes the next chained order, if any.
    pub fn pop_queued(&mut self) -> Option<Order> {
        (!self.queue.is_empty()).then(|| self.queue.remove(0))
    }

    /// Starts `order` now, keeping the chained ones. Ignored while drowning, dying or dead.
    pub fn start(&mut self, order: Order) {
        if !self.action.can_take_orders() {
            return;
        }
        (self.route, self.planned_on, self.teleport_to, self.to_fire) = (Vec::new(), None, None, None);
        (self.to_tree, self.to_wood) = (None, None);
        self.action = match order {
            Order::MoveTo { x, z } => Action::Walking { to: (x, z) },
            Order::Campfire { fire, point } => {
                self.to_fire = Some((fire, point));
                Action::Walking { to: campfire::ring_point(fire, point) }
            }
            Order::Pray => Action::Praying,
            Order::Cast => Action::Casting { left: CAST_TICKS },
            Order::Stop | Order::FetchWood | Order::PickUp { .. } => Action::Idle,
            Order::CutTree { tree } => {
                self.to_tree = Some(tree);
                Action::Walking { to: tree }
            }
        };
    }

    /// One simulation step. `site` is where the unit reincarnates.
    pub fn tick(&mut self, terrain: &Heightmap, site: Option<&ReincarnationSite>) -> Option<UnitEvent> {
        if self.is_alive() && is_sea(terrain, self.cell()) {
            self.action = Action::Drowning;
            self.teleport_to = None;
        }
        match self.action {
            Action::Idle | Action::Praying => self.heal(),
            Action::AroundFire { fire, point } => {
                self.heal();
                let next = (point + 1) % campfire::RING_POINTS;
                let pace = self.kind.speed() * AROUND_FIRE_PACE.0 / AROUND_FIRE_PACE.1;
                // Blocked (the sea or a cliff across the ring): aim for the point after.
                if self.step_towards(campfire::ring_point(fire, next), terrain, pace) != Step::Moved {
                    self.action = Action::AroundFire { fire, point: next };
                }
            }
            Action::Walking { to } | Action::Stranded { to } => {
                if self.planned_on != Some(terrain.revision()) {
                    self.plan(to, terrain, true);
                }
                if let Action::Walking { .. } = self.action {
                    self.heal();
                    self.follow_route(to, terrain);
                    if self.action == Action::Idle {
                        if let Some((fire, point)) = self.to_fire.take() {
                            self.action = Action::AroundFire { fire, point };
                        } else if let Some(tree) = self.to_tree.take() {
                            let (dx, dz) = (torus_delta(self.x, tree.0), torus_delta(self.z, tree.1));
                            if (dx, dz) != (0, 0) {
                                self.facing = octant(dx, dz);
                            }
                            self.action = Action::Chopping { tree, left: CHOP_TICKS };
                        } else if let Some(at) = self.to_wood.take() {
                            return Some(UnitEvent::PickedUp { at });
                        }
                    }
                } else if self.hurt() {
                    return Some(UnitEvent::Died);
                }
            }
            Action::Holding { left } if left > 1 => self.action = Action::Holding { left: left - 1 },
            Action::Holding { .. } => {
                self.action = Action::Idle;
                return Some(UnitEvent::PutDown);
            }
            Action::Chopping { tree, left } if left > 1 => self.action = Action::Chopping { tree, left: left - 1 },
            Action::Chopping { tree, .. } => {
                self.action = Action::Idle;
                return Some(UnitEvent::Chopped { tree });
            }
            Action::Casting { left } if left > 1 => self.action = Action::Casting { left: left - 1 },
            Action::Casting { .. } => {
                self.action = Action::Idle;
                if let Some(to) = self.teleport_to.take().filter(|&to| self.mobility().passable(terrain, (cell_of(to.0), cell_of(to.1)))) {
                    (self.x, self.z) = to;
                    self.action = Action::Landing { left: LANDING_TICKS };
                }
            }
            Action::Landing { left } => self.action = if left > 1 { Action::Landing { left: left - 1 } } else { Action::Idle },
            Action::Drowning => {
                self.health = self.health.saturating_sub(DROWN_DAMAGE);
                if self.health == 0 {
                    self.die();
                    return Some(UnitEvent::Died);
                }
                if !is_sea(terrain, self.cell()) {
                    self.action = Action::Idle;
                }
            }
            Action::Dying { left } => {
                self.action = if left > 1 { Action::Dying { left: left - 1 } } else { Action::Dead { left: RESPAWN_TICKS } };
            }
            Action::Dead { .. } if self.kind != UnitKind::Shaman => {}
            Action::Dead { left } if left > 1 => self.action = Action::Dead { left: left - 1 },
            Action::Dead { .. } => {
                let site = site?;
                *self = Unit { facing: self.facing, ..Unit::shaman(self.id, site) };
                return Some(UnitEvent::Reincarnated);
            }
        }
        None
    }

    fn heal(&mut self) {
        if self.health >= self.max_health() {
            self.regen = 0;
            return;
        }
        self.regen += 1;
        if self.regen >= REGEN_EVERY {
            self.regen = 0;
            self.health += 1;
        }
    }

    /// Stranded: one health point lost every few ticks; true when that kills her.
    fn hurt(&mut self) -> bool {
        self.regen += 1;
        if self.regen >= STRANDED_HURT_EVERY {
            self.regen = 0;
            self.health = self.health.saturating_sub(1);
        }
        if self.health == 0 {
            self.die();
        }
        self.health == 0
    }

    fn die(&mut self) {
        (self.route, self.planned_on, self.teleport_to, self.to_fire) = (Vec::new(), None, None, None);
        (self.to_tree, self.to_wood) = (None, None);
        self.queue.clear();
        self.action = Action::Dying { left: DYING_TICKS };
    }

    /// Path to `to` on the current terrain: walking if there is one, else stranded (the shaman
    /// just stays idle).
    fn plan(&mut self, to: (u16, u16), terrain: &Heightmap, straighten: bool) {
        let route = path::route(terrain, self.mobility(), (self.x, self.z), to, straighten);
        if route.is_none() && matches!(self.action, Action::Walking { .. }) {
            self.regen = 0;
        }
        self.action = match route {
            Some(_) => Action::Walking { to },
            None if self.kind == UnitKind::Shaman => Action::Idle,
            None => Action::Stranded { to },
        };
        self.route = route.map(|r| r.into_iter().rev().collect()).unwrap_or_default();
        self.planned_on = Some(terrain.revision());
    }

    /// One step along the route; idle once at `to`. A step that would end in a cell she cannot
    /// cross (a straight leg grazing its corner) replans cell by cell from the current cell centre.
    fn follow_route(&mut self, to: (u16, u16), terrain: &Heightmap) {
        let Some(&next) = self.route.last() else {
            self.action = Action::Idle;
            return;
        };
        match self.step_towards(next, terrain, self.kind.speed()) {
            Step::Moved => {}
            Step::Arrived => {
                self.route.pop();
                if self.route.is_empty() {
                    self.action = Action::Idle;
                }
            }
            Step::Blocked => self.plan(to, terrain, false),
        }
    }

    /// Straight towards `to` at `flat_speed` (world units per tick on flat ground), shortest way
    /// around the torus, never into a cell she cannot cross.
    fn step_towards(&mut self, to: (u16, u16), terrain: &Heightmap, flat_speed: i32) -> Step {
        let (dx, dz) = (torus_delta(self.x, to.0), torus_delta(self.z, to.1));
        if dx == 0 && dz == 0 {
            return Step::Arrived;
        }
        self.facing = octant(dx, dz);
        let dist = isqrt((dx * dx + dz * dz) as u32) as i32;
        let speed = slope_speed(flat_speed, self.grade_ahead((dx, dz), dist, terrain));
        let (sx, sz) = if dist <= speed { (dx, dz) } else { (dx * speed / dist, dz * speed / dist) };
        let (nx, nz) = (self.x.wrapping_add(sx as u16), self.z.wrapping_add(sz as u16));
        let next = (cell_of(nx), cell_of(nz));
        if next != self.cell() && !self.mobility().passable(terrain, next) {
            return Step::Blocked;
        }
        (self.x, self.z) = (nx, nz);
        if (nx, nz) == to { Step::Arrived } else { Step::Moved }
    }

    /// Slope (height per cell, positive uphill) over one flat-ground step towards `(dx, dz)`.
    fn grade_ahead(&self, (dx, dz): (i32, i32), dist: i32, terrain: &Heightmap) -> i32 {
        let (px, pz) = (self.x.wrapping_add((dx * SHAMAN_SPEED / dist) as u16), self.z.wrapping_add((dz * SHAMAN_SPEED / dist) as u16));
        let h = |x: u16, z: u16| terrain.height_at(x as u32, z as u32, WORLD_UNITS_PER_CELL);
        (h(px, pz) - h(self.x, self.z)) * WORLD_UNITS_PER_CELL as i32 / SHAMAN_SPEED
    }
}

#[derive(PartialEq, Eq)]
enum Step {
    Moved,
    Arrived,
    Blocked,
}

/// Walking speed on a slope (height per cell, positive uphill): slower up, faster down, clamped.
pub fn slope_speed(flat: i32, grade: i32) -> i32 {
    (flat * slope_factor(grade) / 256).max(1)
}

/// Walking speed factor in 1/256 on a slope (see `slope_speed`).
pub fn slope_factor(grade: i32) -> i32 {
    let per_100 = if grade > 0 { SLOPE_SLOWDOWN_UP } else { SLOPE_SPEEDUP_DOWN };
    (256 - grade * per_100 / 100).clamp(SLOPE_FACTOR_RANGE.0, SLOPE_FACTOR_RANGE.1)
}

fn cell_of(v: u16) -> i32 {
    (v as u32 / WORLD_UNITS_PER_CELL) as i32
}

/// Open sea: all four corners of the cell are at sea level (shore cells are still walkable).
pub fn is_sea(terrain: &Heightmap, (x, z): (i32, i32)) -> bool {
    [(0, 0), (1, 0), (0, 1), (1, 1)].iter().all(|&(dx, dz)| terrain.is_water(x + dx, z + dz))
}

/// Signed shortest offset from `a` to `b` on the 65536-wide torus.
pub fn torus_delta(a: u16, b: u16) -> i32 {
    b.wrapping_sub(a) as i16 as i32
}

/// Nearest of 8 headings for a direction (0 = +z, 2 = +x, 4 = -z, 6 = -x), integers only.
pub fn octant(dx: i32, dz: i32) -> u8 {
    // tan(22.5 deg) ~ 29/70.
    let (ax, az) = (dx.abs() as i64, dz.abs() as i64);
    if ax * 70 < az * 29 {
        if dz >= 0 { 0 } else { 4 }
    } else if az * 70 < ax * 29 {
        if dx > 0 { 2 } else { 6 }
    } else {
        match (dx > 0, dz > 0) {
            (true, true) => 1,
            (true, false) => 3,
            (false, false) => 5,
            (false, true) => 7,
        }
    }
}

pub fn isqrt(n: u32) -> u32 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn land() -> Heightmap {
        let mut t = Heightmap::new(128);
        for z in 0..128 {
            for x in 0..128 {
                t.set(x, z, 100);
            }
        }
        t
    }

    fn shaman_at(cell: (i32, i32)) -> (Unit, ReincarnationSite) {
        let site = ReincarnationSite::at_cell(0, cell);
        (Unit::shaman(1, &site), site)
    }

    fn run(u: &mut Unit, t: &Heightmap, site: &ReincarnationSite, ticks: usize) -> Vec<UnitEvent> {
        (0..ticks).filter_map(|_| u.tick(t, Some(site))).collect()
    }

    #[test]
    fn walks_to_the_target_then_idles() {
        let t = land();
        let (mut u, site) = shaman_at((10, 10));
        let to = (u.x + 3 * 512, u.z);
        u.order(Order::MoveTo { x: to.0, z: to.1 });
        run(&mut u, &t, &site, 23);
        assert_eq!(u.action, Action::Walking { to });
        assert_eq!(u.facing, 2, "heading +x");
        run(&mut u, &t, &site, 2);
        assert_eq!((u.x, u.z, u.action), (to.0, to.1, Action::Idle));
    }

    #[test]
    fn walks_the_short_way_across_the_map_edge() {
        let t = land();
        let (mut u, site) = shaman_at((127, 0));
        u.order(Order::MoveTo { x: 256, z: u.z });
        u.tick(&t, Some(&site));
        assert_eq!(u.x, 127 * 512 + 256 + SHAMAN_SPEED as u16);
        assert_eq!(u.facing, 2);
    }

    #[test]
    fn slower_uphill_faster_downhill() {
        assert_eq!(slope_speed(SHAMAN_SPEED, 0), SHAMAN_SPEED);
        assert_eq!(slope_speed(SHAMAN_SPEED, 100), SHAMAN_SPEED / 4, "steep hill: quarter speed");
        assert_eq!(slope_speed(SHAMAN_SPEED, 30), 49, "gentle ramp: ~22% slower");
        assert_eq!(slope_speed(SHAMAN_SPEED, -100), SHAMAN_SPEED * 3 / 2, "steep descent: 1.5x");
        assert!(slope_speed(SHAMAN_SPEED, 30) < SHAMAN_SPEED && slope_speed(SHAMAN_SPEED, -30) > SHAMAN_SPEED, "gentle ramp");
        assert_eq!(slope_speed(SHAMAN_SPEED, 1000), SHAMAN_SPEED / 8, "cliffs: clamped, never stuck");
        assert_eq!(slope_speed(SHAMAN_SPEED, -1000), SHAMAN_SPEED * 3 / 2);
    }

    #[test]
    fn climbing_a_ramp_takes_longer_than_coming_down() {
        let mut t = land();
        for z in 0..128 {
            for x in 10..=20 {
                t.set(x, z, 100 + (x as u16 - 10) * 50);
            }
            for x in 21..30 {
                t.set(x, z, 600);
            }
        }
        let ticks_to = |from: (i32, i32), to_cell: i32| {
            let (mut u, site) = shaman_at(from);
            let to = (to_cell as u16 * 512 + 256, u.z);
            u.order(Order::MoveTo { x: to.0, z: to.1 });
            (1..500).find(|_| {
                u.tick(&t, Some(&site));
                u.action == Action::Idle
            })
        };
        let (up, down, flat) = (ticks_to((10, 5), 20).unwrap(), ticks_to((20, 5), 10).unwrap(), ticks_to((40, 5), 50).unwrap());
        assert!(down < flat && flat < up, "down {down} < flat {flat} < up {up}");
    }

    fn sea_cell(t: &mut Heightmap, (x, z): (i32, i32)) {
        for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            t.set(x + dx, z + dz, 0);
        }
    }

    /// Ticks until idle at `to` (None if not within `max`), asserting she never stands in the sea.
    fn walk(u: &mut Unit, t: &Heightmap, site: &ReincarnationSite, to: (u16, u16), max: usize) -> Option<usize> {
        u.order(Order::MoveTo { x: to.0, z: to.1 });
        (1..=max).find(|_| {
            u.tick(t, Some(site));
            assert!(!is_sea(t, u.cell()), "in the sea at {:?}", u.cell());
            u.action == Action::Idle
        })
    }

    #[test]
    fn walks_around_a_lake() {
        let mut t = land();
        for z in 6..15 {
            for x in 13..17 {
                sea_cell(&mut t, (x, z));
            }
        }
        let (mut u, site) = shaman_at((10, 10));
        let to = (20 * 512 + 256, u.z);
        let ticks = walk(&mut u, &t, &site, to, 300).expect("arrives");
        assert_eq!((u.x, u.z), to);
        assert!(ticks > 10 * 8, "longer than straight across: {ticks}");
    }

    #[test]
    fn walks_around_a_cliff() {
        let mut t = land();
        for z in 6..15 {
            for x in 14..17 {
                t.set(x, z, 500);
            }
        }
        let (mut u, site) = shaman_at((10, 10));
        let to = (20 * 512 + 256, u.z);
        assert!(walk(&mut u, &t, &site, to, 300).is_some());
        assert_eq!((u.x, u.z), to);
        let (mut u, site) = shaman_at((10, 10));
        let start = (u.x, u.z);
        walk(&mut u, &t, &site, (15 * 512 + 256, start.1), 10);
        assert_eq!((u.action, u.x, u.z), (Action::Idle, start.0, start.1), "top of the cliff: unreachable");
    }

    #[test]
    fn replans_when_the_terrain_changes_on_the_way() {
        let mut t = land();
        let (mut u, site) = shaman_at((10, 10));
        let to = (30 * 512 + 256, u.z);
        u.order(Order::MoveTo { x: to.0, z: to.1 });
        run(&mut u, &t, &site, 20);
        for z in 0..20 {
            sea_cell(&mut t, (22, z));
        }
        assert!(walk(&mut u, &t, &site, to, 600).is_some());
        assert_eq!((u.x, u.z), to);
    }

    #[test]
    fn unreachable_target_strands_her_until_a_path_opens() {
        let mut t = land();
        for z in 0..128 {
            sea_cell(&mut t, (15, z));
            sea_cell(&mut t, (5, z));
        }
        let (u, site) = shaman_at((10, 10));
        let mut u = Unit { kind: UnitKind::Brave, ..u };
        let start = (u.x, u.z);
        let to = (20 * 512 + 256, u.z);
        u.order(Order::MoveTo { x: to.0, z: to.1 });
        run(&mut u, &t, &site, 3 * STRANDED_HURT_EVERY as usize);
        assert_eq!((u.action, u.x, u.z), (Action::Stranded { to }, start.0, start.1), "arms up, not moving");
        assert_eq!(u.health, SHAMAN_MAX_HEALTH - 3);
        for z in 0..128 {
            for x in 15..17 {
                t.set(x, z, 100);
            }
        }
        assert!(walk(&mut u, &t, &site, to, 200).is_some(), "bridge built: walks over");
    }

    #[test]
    fn stranded_until_dead_unless_ordered_otherwise() {
        let mut t = land();
        sea_cell(&mut t, (20, 10));
        let (u, site) = shaman_at((10, 10));
        let mut u = Unit { kind: UnitKind::Brave, ..u };
        u.order(Order::MoveTo { x: 20 * 512 + 256, z: u.z });
        run(&mut u, &t, &site, 1);
        u.order(Order::Stop);
        assert_eq!(u.action, Action::Idle, "orders still work");
        u.order(Order::MoveTo { x: 20 * 512 + 256, z: u.z });
        let events = run(&mut u, &t, &site, STRANDED_HURT_EVERY as usize * SHAMAN_MAX_HEALTH as usize);
        assert_eq!((events, u.health, u.is_alive()), (vec![UnitEvent::Died], 0, false));
    }

    #[test]
    fn the_shaman_does_not_move_towards_an_unreachable_target() {
        let mut t = land();
        sea_cell(&mut t, (20, 10));
        let (mut u, site) = shaman_at((10, 10));
        let start = (u.x, u.z);
        u.order(Order::MoveTo { x: 20 * 512 + 256, z: u.z });
        run(&mut u, &t, &site, 10);
        assert_eq!((u.action, u.x, u.z, u.health), (Action::Idle, start.0, start.1, SHAMAN_MAX_HEALTH));
    }

    #[test]
    fn cast_jump_ends_back_idle() {
        let t = land();
        let (mut u, site) = shaman_at((10, 10));
        u.order(Order::Cast);
        assert_eq!(u.action.elapsed(), Some(0));
        run(&mut u, &t, &site, CAST_TICKS as usize - 1);
        assert_eq!(u.action, Action::Casting { left: 1 });
        run(&mut u, &t, &site, 1);
        assert_eq!(u.action, Action::Idle);
    }

    #[test]
    fn teleport_jumps_first_then_moves() {
        let mut t = land();
        let (mut u, site) = shaman_at((10, 10));
        let (start, to) = ((u.x, u.z), (30 * 512 + 100, 40 * 512 + 7));
        u.cast_teleport(to);
        run(&mut u, &t, &site, CAST_TICKS as usize - 1);
        assert_eq!((u.x, u.z, u.action), (start.0, start.1, Action::Casting { left: 1 }), "still jumping where she was");
        run(&mut u, &t, &site, 1);
        assert_eq!((u.x, u.z, u.action), (to.0, to.1, Action::Landing { left: LANDING_TICKS }), "at the target, floating down");
        run(&mut u, &t, &site, LANDING_TICKS as usize);
        assert_eq!(u.action, Action::Idle, "landed");
        sea_cell(&mut t, (5, 5));
        u.cast_teleport((5 * 512, 5 * 512));
        run(&mut u, &t, &site, CAST_TICKS as usize);
        assert_eq!((u.x, u.z), to, "the target became sea meanwhile: stays");
        u.cast_teleport(start);
        u.order(Order::Stop);
        run(&mut u, &t, &site, CAST_TICKS as usize);
        assert_eq!((u.x, u.z), to, "an order cancels it");
    }

    #[test]
    fn drowns_dies_and_reincarnates_at_the_site() {
        let mut t = land();
        let (mut u, site) = shaman_at((10, 10));
        u.order(Order::Pray);
        u.x += 20 * 512;
        let (cx, cz) = u.cell();
        for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            t.set(cx + dx, cz + dz, 0);
        }
        u.tick(&t, Some(&site));
        assert_eq!((u.action, u.health), (Action::Drowning, SHAMAN_MAX_HEALTH - DROWN_DAMAGE));
        u.order(Order::MoveTo { x: 0, z: 0 });
        assert_eq!(u.action, Action::Drowning, "no orders while drowning");
        let events = run(&mut u, &t, &site, 24);
        assert_eq!((events, u.health, u.is_alive()), (vec![UnitEvent::Died], 0, false));
        let events = run(&mut u, &t, &site, (DYING_TICKS + RESPAWN_TICKS) as usize);
        assert_eq!(events, vec![UnitEvent::Reincarnated]);
        assert_eq!((u.x, u.z, u.health, u.action), (site.x, site.z, SHAMAN_MAX_HEALTH, Action::Idle));
    }

    #[test]
    fn only_the_shaman_reincarnates() {
        let mut t = land();
        let (shaman, site) = shaman_at((10, 10));
        let mut brave = Unit::new(7, 0, UnitKind::Brave, (shaman.x + 20 * 512, shaman.z));
        assert_eq!(brave.health, UnitKind::Brave.max_health());
        sea_cell(&mut t, brave.cell());
        let events = run(&mut brave, &t, &site, 200);
        assert_eq!(events, vec![UnitEvent::Died], "no reincarnation");
        assert!(matches!(brave.action, Action::Dead { .. }) && brave.kind == UnitKind::Brave);
    }

    #[test]
    fn kinds_walk_at_their_own_speed() {
        let t = land();
        let ticks = |kind: UnitKind| {
            let (s, site) = shaman_at((10, 10));
            let mut u = Unit::new(2, 0, kind, (s.x, s.z));
            walk(&mut u, &t, &site, (s.x + 10 * 512, s.z), 500).unwrap()
        };
        assert!(ticks(UnitKind::Spy) < ticks(UnitKind::Brave) && ticks(UnitKind::Brave) < ticks(UnitKind::Warrior));
    }

    #[test]
    fn heals_slowly_on_land() {
        let t = land();
        let (mut u, site) = shaman_at((10, 10));
        u.health = 50;
        run(&mut u, &t, &site, 5 * REGEN_EVERY as usize);
        assert_eq!(u.health, 55);
    }

    #[test]
    fn octants_and_sqrt() {
        assert_eq!([octant(0, 5), octant(5, 5), octant(5, 0), octant(5, -5)], [0, 1, 2, 3]);
        assert_eq!([octant(0, -5), octant(-5, -5), octant(-5, 0), octant(-5, 5)], [4, 5, 6, 7]);
        assert_eq!(octant(10, 3), 2);
        assert_eq!((isqrt(0), isqrt(15), isqrt(16), isqrt(u32::MAX)), (0, 3, 4, 65535));
        assert_eq!(torus_delta(65000, 100), 636);
    }
}
