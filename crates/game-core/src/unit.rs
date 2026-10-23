//! Units (shaman, later braves, warriors...). Integer state, advanced one tick at a time.
//! Positions are fixed-point (1 cell = 512 units, like the original), wrapping at 65536: plain
//! `u16` wrapping arithmetic walks around the torus.

use crate::building::BuildingId;
use crate::campfire;
use crate::health::Health;
use crate::id::Id;
use crate::motion::{Motion, Velocity};
use crate::path::{self, Ground, Mobility};
use crate::physics;
use crate::site::ReincarnationSite;
use crate::spell::Spell;
use crate::status::Statuses;
use crate::terrain::Heightmap;
use crate::time::{Countdown, Every, Tick, Ticks};
use pop3_format::WORLD_UNITS_PER_CELL;

pub const SHAMAN_MAX_HEALTH: u16 = UnitKind::Shaman.stats().life;
/// World units per tick on flat ground (1.24 cells per second).
pub const SHAMAN_SPEED: i32 = 53;
/// Walking speed factor change in 1/256 per 100 of slope (height per cell). Uphill: the sandbox
/// ramp (30 per cell) is ~22% slower, the steep hill (100 per cell) quarter speed. Downhill: the
/// steep hill is 1.5x faster.
pub const SLOPE_SLOWDOWN_UP: i32 = 192;
pub const SLOPE_SPEEDUP_DOWN: i32 = 128;
/// Walking speed factor bounds in 1/256 (steep climbs crawl, steep descents are capped).
pub const SLOPE_FACTOR_RANGE: (i32, i32) = (32, 384);
/// Health lost per tick while in the sea.
pub const DROWN_DAMAGE: u16 = 60;
/// Health lost at a time while stranded (a guess).
pub const STRANDED_DAMAGE: u16 = 20;
/// Between two `STRANDED_DAMAGE`s, at each unit's own phase (the shaman's health lasts 33 s; a guess).
pub const STRANDED_HURT_EVERY: Every = Every::new(4);
/// Healing comes on every eighth turn of the game, the same for everyone (pop3-rev-analysis.md "Health and
/// healing" [exe, measured]); how much is the kind's (`KindStats::heal`).
pub const HEAL_EVERY: Every = Every::new(8);
/// The cast: the original's cast state.
pub const CAST_TICKS: Ticks = Ticks::new(10);
/// Into the cast, when the spell leaves her (pop3-rev-analysis.md "From cast to effect": the shot flies at +6).
pub const CAST_RELEASE: Ticks = Ticks::new(6);
/// After a teleport she appears this high above the target (terrain height units, one cell) and falls
/// (`physics::step`): 5 ticks to the ground.
pub const LANDING_DROP: i32 = 256;
/// The fall when dying, and the shaman's wait before reincarnation.
pub const DYING_TICKS: Ticks = Ticks::millis(1500);
pub const RESPAWN_TICKS: Ticks = Ticks::secs(3);
/// Others' spirit rising from the body after the fall, before they are gone (a guess).
pub const SPIRIT_TICKS: Ticks = Ticks::secs(3);
/// Chopping one piece of wood off a tree: the original's `CHOP_TIME` at a tree (pop3-rev-analysis.md "Chopping").
pub const CHOP_TICKS: Ticks = Ticks::new(20);
/// Picking up a piece of wood lying on the ground: the original's `CHOP_TIME` at a wood pile.
pub const PICK_TICKS: Ticks = Ticks::new(3);
/// With nothing more to do, a brave holds his piece of wood this long before putting it down.
pub const HOLD_TICKS: Ticks = Ticks::secs(3);
/// A brave's jump flattening one height point of a building site.
pub const JUMP_TICKS: Ticks = Ticks::millis(800);
/// A brave building one piece of wood into a building (a guess).
pub const BUILD_TICKS: Ticks = Ticks::secs(5);
/// Going round a camp fire: this fraction (numerator, denominator) of the walking speed.
pub const AROUND_FIRE_PACE: (i32, i32) = (1, 2);
/// Panic (pop3-rev-analysis.md "Shared person and building states", person state 0x1A): the run's length in turns.
pub const PANIC_TICKS: Ticks = Ticks::new(64);
/// The panic run's speed, world units per tick on flat ground (pop3-rev-analysis.md "Walking speeds": 110).
pub const PANIC_SPEED: i32 = 110;
/// Length of a panic heading (`panic_heading`), world units: beyond a step, so a step never reaches its end.
const PANIC_REACH: i32 = 1024;
/// On fire (pop3-rev-analysis.md "Volcano lava flow", person state 31): the run's length in turns, confirmed.
pub const BURN_TICKS: Ticks = Ticks::new(70);
/// On fire: lost a turn is its own max health divided by this, confirmed.
pub const BURN_DIVISOR: u16 = 32;

/// A unit's stable id (`GameMap::new_id_of`).
pub type UnitId = Id<Unit>;

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

    pub fn max_health(self) -> u16 {
        self.stats().life
    }

    /// World units per tick on flat ground: spies are the quickest, warriors and preachers the slowest.
    pub fn speed(self) -> i32 {
        match self {
            UnitKind::Shaman | UnitKind::Brave => SHAMAN_SPEED,
            UnitKind::Spy => 60,
            UnitKind::Firewarrior => 50,
            UnitKind::Warrior | UnitKind::Preacher | UnitKind::Wildman => 47,
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
    /// Jumping with the spell in her hands, `left` ticks to go.
    Casting { left: Countdown },
    /// Just teleported: falling onto the ground (`Unit::motion`), Idle on touchdown.
    Landing,
    /// The ground under her is sea: loses health until dead or the land comes back.
    Drowning,
    /// Flung through the air or rolling on the ground by a spell, moved by `Unit::velocity` at
    /// `Unit::lift`: no orders until still on the ground again.
    Tumbling,
    /// Running in panic towards `away` (a heading, `panic_heading`) at `PANIC_SPEED`, out by the door first when
    /// inside: no orders until `left` runs out.
    Panicking { away: (i16, i16), left: Countdown },
    /// On fire (pop3-rev-analysis.md "Volcano lava flow", person state 31): losing `BURN_DIVISOR` of its own max
    /// health a turn for up to `BURN_TICKS`, almost always dead well before then. Moves like `Panicking` towards
    /// `away` while it burns — the trace only says "runs about", no numbers of its own, so this reuses Panic's own
    /// movement as a guess, tagged here rather than traced.
    OnFire { away: (i16, i16), left: Countdown },
    /// Falling down dead.
    Dying { left: Countdown },
    /// The shaman, lying dead: waiting to reincarnate at her site.
    Dead { left: Countdown },
    /// Anyone else after the fall: its spirit rises from the body, then the unit is gone from the map.
    Spirit { left: Countdown },
    /// Going round the camp fire at `fire` (its centre), last at its ring point `point`
    /// (`campfire::ring_point`), at `AROUND_FIRE_PACE` of her walking speed.
    AroundFire { fire: (u16, u16), point: u8 },
    /// A brave cutting one piece of wood off the tree at `tree` (its position), `left` ticks to go.
    Chopping { tree: (u16, u16), left: Countdown },
    /// A brave picking up the piece of wood at `at` (its position), `left` ticks to go: the original's same
    /// gather state as chopping, shorter.
    PickingUpWood { at: (u16, u16), left: Countdown },
    /// Standing with a piece of wood and nothing more to do: puts it down when `left` runs out.
    Holding { left: Countdown },
    /// A brave jumping on the height point `at` (cells) of the site it works on, to flatten it.
    Flattening { at: (i32, i32), left: Countdown },
    /// A brave building one piece of wood from the pile into the building it works on.
    Building { left: Countdown },
    /// Just seated in its vehicle (`Unit::aboard`): jumping in from `from` (where it stood), `left` turns to go
    /// (`vehicle::BOARD_JUMP`). Drawn only: it is aboard all along.
    Boarding { from: (u16, u16), left: Countdown },
    /// Just off its vehicle: jumping down from `from` at height `height` (where the vehicle was) to where it stands,
    /// `left` turns to go. Drawn only: it is on the ground all along.
    Alighting { from: (u16, u16), height: u16, left: Countdown },
    /// Walking in by the door, straight to `to` inside the building (`Unit::inside`).
    Entering { to: (u16, u16) },
    /// A brave inside a building under construction with nothing to build yet, hammering away until
    /// wood comes or it is built.
    Hammering,
    /// The shaman praying at the door of the vault of knowledge whose stored corner is `site`
    /// (`GameMap::tend_vaults`).
    Worshipping { site: (u16, u16) },
    /// On its way to attack `target`, whose place it last aimed at is `to` (`GameMap::tend_attacks` keeps it up to date).
    Attacking { target: Target, to: (u16, u16) },
    /// Its `role` in the challenge `challenge` (challenge.rs).
    Challenge { challenge: crate::challenge::ChallengeId, role: crate::challenge::Role },
    /// A firewarrior throwing fire at `target` (firebolt.rs), `left` turns to go; then back to its attack.
    Throwing { target: Target, left: Countdown },
    /// Hacking at the building `building` (wreck.rs), `left` turns to go; first at its door.
    Hacking { building: crate::building::BuildingId, left: Countdown, at_door: bool },
    /// Waiting in the queue of the training unit `building` (training.rs), standing at or walking to its spot `at`.
    Queuing { building: crate::building::BuildingId, at: (u16, u16) },
    /// Sitting, listening to `preacher` (sermon.rs): `left` to its next conversion chance, `attempts` so far
    /// missed (debug overlay); locked until converted or the sermon ends.
    Listening { preacher: UnitId, sitting: Countdown, left: Countdown, attempts: u32 },
    /// A preacher standing, preaching to its listeners (sermon.rs).
    Preaching,
    /// Fighting in the melee `melee` (melee.rs): its place in the ring, its stance.
    Melee { melee: crate::melee::MeleeId, place: (u16, u16), stance: crate::melee::Stance },
    /// Held by the test-only coordinator `gathering` (coordinator.rs).
    #[cfg(test)]
    Gathered { gathering: crate::id::Id<crate::coordinator::tests::Gathering> },
}

impl Action {
    pub fn name(&self) -> &'static str {
        match self {
            Action::Idle => "Idle",
            Action::Walking { .. } => "Walking",
            Action::Stranded { .. } => "Stranded",
            Action::Worshipping { .. } => "Worshipping",
            Action::Casting { .. } => "Casting",
            Action::Landing => "Landing",
            Action::Drowning => "Drowning",
            Action::Tumbling => "Tumbling",
            Action::Panicking { .. } => "Panicking",
            Action::OnFire { .. } => "Burning",
            Action::Dying { .. } => "Dying",
            Action::Dead { .. } => "Dead",
            Action::Spirit { .. } => "Dead",
            Action::AroundFire { .. } => "Around the fire",
            Action::Chopping { .. } => "Cutting wood",
            Action::PickingUpWood { .. } => "Picking up wood",
            Action::Holding { .. } => "Holding wood",
            Action::Flattening { .. } => "Flattening",
            Action::Building { .. } => "Building",
            Action::Entering { .. } => "Entering",
            Action::Boarding { .. } => "Boarding",
            Action::Alighting { .. } => "Getting off",
            Action::Hammering => "Hammering",
            Action::Attacking { .. } => "Attacking",
            Action::Challenge { .. } | Action::Melee { .. } => "Fighting",
            Action::Listening { .. } => "Listening",
            Action::Queuing { .. } => "Queuing",
            Action::Preaching => "Preaching",
            Action::Hacking { .. } | Action::Throwing { .. } => "Attacking",
            #[cfg(test)]
            Action::Gathered { .. } => "Gathered",
        }
    }

    /// The coordinator this action takes part in, if any (coordinator.rs). `Listening` is locked the same way but
    /// backed by no coordinator (sermon.rs), so it never links.
    pub fn link(&self) -> Option<crate::coordinator::Link> {
        match *self {
            Action::Challenge { challenge, .. } => return Some(crate::coordinator::Link::Challenge(challenge)),
            Action::Melee { melee, .. } => return Some(crate::coordinator::Link::Melee(melee)),
            _ => {}
        }
        #[cfg(test)]
        if let Action::Gathered { gathering } = *self {
            return Some(crate::coordinator::Link::Gathering(gathering));
        }
        None
    }

    /// The countdown of a timed action (for one-shot animations), None for open-ended ones.
    pub fn countdown(&self) -> Option<Countdown> {
        match *self {
            Action::Casting { left }
            | Action::Dying { left }
            | Action::Spirit { left }
            | Action::Dead { left }
            | Action::Chopping { left, .. }
            | Action::PickingUpWood { left, .. }
            | Action::Holding { left }
            | Action::Flattening { left, .. }
            | Action::Building { left }
            | Action::Hacking { left, .. }
            | Action::Throwing { left, .. }
            | Action::Boarding { left, .. }
            | Action::Alighting { left, .. }
            | Action::Listening { sitting: left, .. } => Some(left),
            Action::Challenge { role: crate::challenge::Role::Shoving { left, .. }, .. } => Some(left),
            Action::Melee { stance: crate::melee::Stance::Striking { left, .. } | crate::melee::Stance::Reeling { left, .. }, .. } => Some(left),
            _ => None,
        }
    }

    pub(crate) fn can_take_orders(&self) -> bool {
        !matches!(self, Action::Drowning | Action::Tumbling | Action::Panicking { .. } | Action::OnFire { .. } | Action::Listening { .. } | Action::Dying { .. } | Action::Dead { .. } | Action::Spirit { .. })
    }
}

/// What a player tells their shaman to do (carried by `Command::Order`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    MoveTo { x: u16, z: u16 },
    /// Walk to point `point` of the ring of the camp fire at `fire` (its centre), then go round it.
    Campfire { fire: (u16, u16), point: u8 },
    Cast,
    Stop,
    /// Braves: cut one piece of wood off the tree at `tree` (its position), or the nearest tree with
    /// wood to spare if that one has none (`GameMap::start_order`).
    CutTree { tree: (u16, u16) },
    /// Braves: get one piece of wood from the nearest wood, a piece on the floor or a tree.
    FetchWood,
    /// Braves: pick up the piece of wood nearest to `at` (a pile clicked on), within `PICK_RADIUS`.
    PickUp { at: (u16, u16) },
    /// Braves: work on the building whose stored corner is `site` (`Building::x`, `z`) until it is
    /// built: flatten, fetch wood, build (`GameMap::work`).
    Build { site: (u16, u16) },
    /// Followers: walk in by the door of the hut whose stored corner is `site` and rest inside, if it
    /// has room (`GameMap::start_order`).
    Enter { site: (u16, u16) },
    /// The shaman: walk to the door of the vault of knowledge whose stored corner is `site` and pray
    /// there until it lets her in (`GameMap::go_worship`).
    Worship { site: (u16, u16) },
    /// Fighters: attack an enemy person or building (`GameMap::go_attack`, combat.md).
    Attack { target: Target },
    /// Walk next to the vehicle `vehicle` and get on, if it has a free seat (`GameMap::go_board`, vehicles.md).
    Board { vehicle: crate::vehicle::VehicleId },
    /// Aboard a vehicle: get off it, if there is ground to get off on (`GameMap::get_off`, vehicles.md).
    GetOff,
}

/// What an attack is aimed at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Unit(UnitId),
    /// A building, by id.
    Building(BuildingId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitEvent {
    Died,
    /// Its spirit has risen: the caller removes it from the map.
    Gone,
    /// Back at her site: the caller levels its ground.
    Reincarnated,
    /// Done holding a piece of wood: the caller puts it down where she stands.
    PutDown,
    /// Done chopping the tree at `tree`: the caller cuts it and gives the piece.
    Chopped { tree: (u16, u16) },
    /// Done picking up the piece of wood at `at`: the caller gives it.
    PickedUp { at: (u16, u16) },
    /// Landed from a jump on the height point `at` (cells): the caller flattens it a step.
    Jumped { at: (i32, i32) },
    /// Done building a piece of wood in: the caller counts it.
    Built,
    /// At the door of the building `building` (id): the caller lets her in if there is room.
    AtDoor { building: BuildingId },
    /// Her cast jump let `spell` go: the caller makes it take effect.
    Released { spell: Spell },
    /// Next to the vehicle `vehicle` (id): the caller seats her if she may get on.
    AtVehicle { vehicle: crate::vehicle::VehicleId },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    pub id: UnitId,
    pub owner: u8,
    pub kind: UnitKind,
    pub x: u16,
    pub z: u16,
    /// Heading in eighths of a turn: 0 = +z, 2 = +x, 4 = -z, 6 = -x.
    pub facing: u8,
    pub health: Health,
    /// Free motion when flung or falling (still while walking or standing).
    pub motion: Motion,
    pub statuses: Statuses,
    pub action: Action,
    /// Turns before a firewarrior throws again (`firebolt::reload`).
    pub reload: u8,
    /// Pieces of wood carried (braves, 0 or 1).
    pub carrying: u8,
    /// Waypoints left on the way to `Walking::to`, next one last.
    route: Vec<(u16, u16)>,
    /// Ground revision (`Ground::revision`) the route was planned on; None = plan on the next tick.
    planned_on: Option<(u32, u32)>,
    /// Where the current cast jump takes her when it ends (Teleport).
    teleport_to: Option<(u16, u16)>,
    /// The spell she is walking into range to cast (`walk_to_cast`), or that her cast jump releases (`CAST_RELEASE`).
    spell: Option<Spell>,
    /// The camp fire (centre) and ring point she is walking to, to go round it once there.
    to_fire: Option<((u16, u16), u8)>,
    /// The tree (position) she is walking to, to cut it once there.
    to_tree: Option<(u16, u16)>,
    /// The piece of wood (position) she is walking to, to pick it up once there.
    to_wood: Option<(u16, u16)>,
    /// The building (stored corner) she is walking to the door of, to go in once there.
    to_house: Option<BuildingId>,
    /// The vault (stored corner) she is walking to the door of, to pray there once there.
    to_shrine: Option<(u16, u16)>,
    /// The vehicle she is walking to, to get on once there.
    to_vehicle: Option<crate::vehicle::VehicleId>,
    /// Chained orders, next first: started one by one each time the unit is idle (`GameMap::tick`).
    queue: Vec<Order>,
    /// The building (id) this brave is assigned to, `Order::Build`.
    pub work: Option<BuildingId>,
    /// The building the unit is in: it walks out by its door before going anywhere.
    pub inside: Option<Inside>,
    /// The vehicle carrying it (`crate::vehicle`): it stands where the vehicle is and goes nowhere on its own.
    pub aboard: Option<crate::vehicle::VehicleId>,
}

/// A unit on the ground and not tumbling: only `Unit::pickable` makes one, so whatever takes it can
/// never pick a flung or lifted unit.
#[derive(Clone, Copy, Debug)]
pub struct PickableUnit<'a>(&'a Unit);

impl std::ops::Deref for PickableUnit<'_> {
    type Target = Unit;

    fn deref(&self) -> &Unit {
        self.0
    }
}

/// A building a unit is in (or walking into): its stored corner and its door.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inside {
    pub building: BuildingId,
    pub door: (u16, u16),
}

impl Unit {
    pub fn shaman(id: UnitId, site: &ReincarnationSite) -> Self {
        Unit::new(id, site.owner, UnitKind::Shaman, site.spawn_point())
    }

    /// A unit standing idle at `(x, z)` (world units), at full health.
    pub fn new(id: UnitId, owner: u8, kind: UnitKind, (x, z): (u16, u16)) -> Self {
        Unit {
            id,
            owner,
            kind,
            x,
            z,
            facing: 0,
            health: Health::full(kind.max_health()),
            motion: Motion::STILL,
            statuses: Statuses::default(),
            action: Action::Idle,
            carrying: 0,
            reload: 0,
            route: Vec::new(),
            planned_on: None,
            teleport_to: None,
            spell: None,
            to_fire: None,
            to_tree: None,
            to_wood: None,
            to_house: None,
            to_shrine: None,
            to_vehicle: None,
            queue: Vec::new(),
            work: None,
            inside: None,
            aboard: None,
        }
    }

    /// Every person walks (vehicles will sail or fly).
    pub fn mobility(&self) -> Mobility {
        Mobility::Walk
    }

    /// On a task: building, carrying wood, or on the way to a tree, wood, a door, a place to pray, a
    /// camp fire or a vehicle.
    pub fn busy(&self) -> bool {
        self.work.is_some() || self.carrying > 0 || self.to_tree.is_some() || self.to_wood.is_some() || self.to_house.is_some() || self.to_shrine.is_some() || self.to_fire.is_some() || self.to_vehicle.is_some()
    }

    /// Neither in a building nor aboard a vehicle: out where spells, fire and fighters reach it.
    pub fn in_the_open(&self) -> bool {
        self.inside.is_none() && self.aboard.is_none()
    }

    pub fn is_alive(&self) -> bool {
        !matches!(self.action, Action::Dying { .. } | Action::Dead { .. } | Action::Spirit { .. })
    }

    /// Flung (tumbling) or off the ground: it cannot be picked, nor taken on.
    pub fn thrown(&self) -> bool {
        self.action == Action::Tumbling || self.motion.airborne()
    }

    /// Thrown, or listening to a preacher: no order reaches it and its player cannot select it.
    pub fn locked(&self) -> bool {
        self.thrown() || matches!(self.action, Action::Listening { .. })
    }

    /// Orders change what it does: not while drowning, tumbling, dying or dead, nor off the ground.
    fn takes_orders(&self) -> bool {
        self.action.can_take_orders() && !self.motion.airborne()
    }

    /// The unit as something the player may pick (hover, select), None while `thrown`.
    pub fn pickable(&self) -> Option<PickableUnit<'_>> {
        (!self.thrown()).then_some(PickableUnit(self))
    }

    /// Cell holding the unit.
    pub fn cell(&self) -> (i32, i32) {
        (cell_of(self.x), cell_of(self.z))
    }

    /// Teleport: the cast jump, then she is at `to` (if she can still stand there by then).
    /// Ignored while drowning, dying or dead; another order before the jump ends cancels it.
    pub fn cast_teleport(&mut self, to: (u16, u16)) {
        if self.takes_orders() {
            self.order(Order::Cast);
            self.teleport_to = Some(to);
        }
    }

    /// Her cast jump, releasing `spell` `CAST_RELEASE` into it (`UnitEvent::Released`); whether she took it.
    /// Ignored while drowning, dying or dead; another order before the release cancels it.
    pub fn cast_spell(&mut self, spell: Spell) -> bool {
        if !self.takes_orders() {
            return false;
        }
        self.order(Order::Cast);
        self.spell = Some(spell);
        true
    }

    /// Walks to `to` (world units) to cast `spell` once within range (`GameMap::cast_when_in_range`); whether she
    /// took it. Another order on the way cancels it.
    pub fn walk_to_cast(&mut self, spell: Spell, to: (u16, u16)) -> bool {
        if !self.takes_orders() {
            return false;
        }
        self.order(Order::MoveTo { x: to.0, z: to.1 });
        self.spell = Some(spell);
        true
    }

    /// Aboard a vehicle that carries her towards where `spell` is aimed, she casts it once within range
    /// (`GameMap::cast_when_in_range`); whether she took it. Another order on the way cancels it.
    pub fn ride_to_cast(&mut self, spell: Spell) -> bool {
        if !self.takes_orders() || self.aboard.is_none() {
            return false;
        }
        self.spell = Some(spell);
        true
    }

    /// The spell she is walking into range to cast, if any.
    pub fn walking_to_cast(&self) -> Option<Spell> {
        self.spell.filter(|_| !matches!(self.action, Action::Casting { .. }))
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
        if !self.takes_orders() {
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

    /// The piece of wood (position) she is walking to or picking up, if any.
    pub fn fetching(&self) -> Option<(u16, u16)> {
        match self.action {
            Action::PickingUpWood { at, .. } => Some(at),
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

    /// Walks to `door`, then goes into the building `building` (id, `UnitEvent::AtDoor`).
    pub fn go_enter(&mut self, building: BuildingId, door: (u16, u16)) {
        self.start(Order::MoveTo { x: door.0, z: door.1 });
        if self.action.can_take_orders() {
            self.to_house = Some(building);
        }
    }

    /// Walks to `door`, then prays there for the vault at `site` (`Action::Worshipping`).
    pub fn go_worship(&mut self, site: (u16, u16), door: (u16, u16)) {
        self.start(Order::MoveTo { x: door.0, z: door.1 });
        if self.action.can_take_orders() {
            self.to_shrine = Some(site);
        }
    }

    /// Walks to `stand`, then gets on the vehicle `vehicle` (`UnitEvent::AtVehicle`).
    pub fn go_board(&mut self, vehicle: crate::vehicle::VehicleId, stand: (u16, u16)) {
        self.start(Order::MoveTo { x: stand.0, z: stand.1 });
        if self.action.can_take_orders() {
            self.to_vehicle = Some(vehicle);
        }
    }

    /// Walks onto the piece of wood at `at`, then picks it up.
    pub fn go_pick(&mut self, at: (u16, u16)) {
        self.start(Order::MoveTo { x: at.0, z: at.1 });
        if self.action.can_take_orders() {
            self.to_wood = Some(at);
        }
    }

    /// Standing by the door of the building `inside`, walks in straight to `to`.
    pub fn enter(&mut self, inside: Inside, to: (u16, u16)) {
        if self.action.can_take_orders() {
            self.start(Order::Stop);
            self.inside = Some(inside);
            self.action = Action::Entering { to };
        }
    }

    /// Idle, or only holding a piece of wood before putting it down: free for the next order.
    pub fn is_free(&self) -> bool {
        matches!(self.action, Action::Idle | Action::Holding { .. } | Action::Hammering)
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

    /// Chains `order` for when it is back on its feet, thrown or not: what it was doing is over.
    pub(crate) fn chain_after_landing(&mut self, order: Order) {
        if self.is_alive() {
            self.queue = vec![order];
        }
    }

    /// Takes the next chained order, if any.
    pub fn pop_queued(&mut self) -> Option<Order> {
        (!self.queue.is_empty()).then(|| self.queue.remove(0))
    }

    /// Starts `order` now, keeping the chained ones. Ignored while drowning, dying or dead.
    pub fn start(&mut self, order: Order) {
        if !self.takes_orders() {
            return;
        }
        (self.route, self.planned_on, self.teleport_to, self.spell, self.to_fire) = (Vec::new(), None, None, None, None);
        (self.to_tree, self.to_wood, self.to_house, self.to_shrine, self.to_vehicle) = (None, None, None, None, None);
        self.action = match order {
            Order::MoveTo { x, z } => Action::Walking { to: (x, z) },
            Order::Campfire { fire, point } => {
                self.to_fire = Some((fire, point));
                Action::Walking { to: campfire::ring_point(fire, point) }
            }
            Order::Cast => Action::Casting { left: Countdown::new(CAST_TICKS) },
            Order::Stop | Order::FetchWood | Order::PickUp { .. } | Order::Build { .. } | Order::Enter { .. } | Order::Worship { .. } | Order::Attack { .. } | Order::Board { .. } | Order::GetOff => Action::Idle,
            Order::CutTree { tree } => {
                self.to_tree = Some(tree);
                Action::Walking { to: tree }
            }
        };
    }

    /// One simulation step. `site` is where the unit reincarnates.
    pub fn tick(&mut self, now: Tick, ground: &impl Ground, site: Option<&ReincarnationSite>) -> Option<UnitEvent> {
        let terrain = ground.terrain();
        if self.motion.airborne() {
            let below = |(x, z): (u16, u16)| terrain.height_at(x as u32, z as u32, WORLD_UNITS_PER_CELL);
            let touchdown;
            ((self.x, self.z), self.motion, touchdown) = physics::step((self.x, self.z), self.motion, below);
            if touchdown.is_some() && let Some(event) = self.touch_down(ground) {
                return Some(event);
            }
        }
        if self.aboard.is_some() && self.is_alive() && !matches!(self.action, Action::Casting { .. } | Action::Boarding { .. } | Action::Attacking { .. } | Action::Throwing { .. }) {
            // Carried: only a cast, the jump in, or a firewarrior's attack goes on aboard.
            self.action = Action::Idle;
            return None;
        }
        if self.is_alive() && self.aboard.is_none() && !self.motion.airborne() && is_sea(terrain, self.cell()) {
            self.action = Action::Drowning;
            (self.teleport_to, self.spell) = (None, None);
        }
        match self.action {
            Action::Idle | Action::Hammering | Action::Worshipping { .. } => {}
            Action::AroundFire { fire, point } => {
                let next = (point + 1) % campfire::RING_POINTS;
                let pace = self.kind.speed() * AROUND_FIRE_PACE.0 / AROUND_FIRE_PACE.1;
                // Blocked (the sea or a cliff across the ring): aim for the point after.
                if self.step_towards(campfire::ring_point(fire, next), ground, pace) != Step::Moved {
                    self.action = Action::AroundFire { fire, point: next };
                }
            }
            Action::Walking { to } | Action::Stranded { to } if self.inside.is_some() => {
                // Out by the door first, then on.
                let door = self.inside.map_or(to, |i| i.door);
                self.action = Action::Walking { to };
                match self.step_towards(door, ground, self.kind.speed()) {
                    Step::Moved => {}
                    Step::Arrived => (self.inside, self.planned_on) = (None, None),
                    // The door is blocked: she stays in.
                    Step::Blocked => self.action = Action::Idle,
                }
            }
            Action::Attacking { to, .. } if self.in_the_open() => self.approach(to, ground),
            Action::Queuing { at, .. } => {
                self.approach(at, ground);
                if self.route.is_empty() && (self.x, self.z) != at && self.planned_on == Some(ground.revision()) {
                    self.action = Action::Stranded { to: at };
                }
            }
            Action::Attacking { .. } => {}
            Action::Hacking { .. } => {}
            Action::Throwing { target, mut left } => {
                self.action = match left.tick() {
                    false => Action::Throwing { target, left },
                    true => Action::Attacking { target, to: (self.x, self.z) },
                };
            }
            Action::Challenge { challenge, role } => self.action = Action::Challenge { challenge, role: self.play(role, ground) },
            Action::Melee { melee, place, stance } => self.action = Action::Melee { melee, place, stance: self.fight(place, stance, ground) },
            Action::Listening { preacher, mut sitting, left, attempts } => {
                sitting.tick();
                self.action = Action::Listening { preacher, sitting, left, attempts };
            }
            Action::Preaching => {}
            Action::Entering { to } => {
                if self.step_towards(to, ground, self.kind.speed()) != Step::Moved {
                    self.action = Action::Idle;
                }
            }
            Action::Walking { to } | Action::Stranded { to } => {
                if self.planned_on != Some(ground.revision()) {
                    self.plan(to, ground, true);
                }
                if let Action::Walking { .. } = self.action {
                    self.follow_route(to, ground);
                    if self.action == Action::Idle {
                        if let Some((fire, point)) = self.to_fire.take() {
                            self.action = Action::AroundFire { fire, point };
                        } else if let Some(tree) = self.to_tree.take() {
                            let (dx, dz) = (torus_delta(self.x, tree.0), torus_delta(self.z, tree.1));
                            if (dx, dz) != (0, 0) {
                                self.facing = octant(dx, dz);
                            }
                            self.action = Action::Chopping { tree, left: Countdown::new(CHOP_TICKS) };
                        } else if let Some(at) = self.to_wood.take() {
                            self.action = Action::PickingUpWood { at, left: Countdown::new(PICK_TICKS) };
                        } else if let Some(building) = self.to_house.take() {
                            return Some(UnitEvent::AtDoor { building });
                        } else if let Some(site) = self.to_shrine.take() {
                            self.action = Action::Worshipping { site };
                        } else if let Some(vehicle) = self.to_vehicle.take() {
                            return Some(UnitEvent::AtVehicle { vehicle });
                        }
                    }
                } else if self.hurt(now) {
                    return Some(UnitEvent::Died);
                }
            }
            Action::Holding { mut left } => {
                if left.tick() {
                    self.action = Action::Idle;
                    return Some(UnitEvent::PutDown);
                }
                self.action = Action::Holding { left };
            }
            Action::Flattening { at, mut left } => {
                if left.tick() {
                    self.action = Action::Idle;
                    return Some(UnitEvent::Jumped { at });
                }
                self.action = Action::Flattening { at, left };
            }
            Action::Building { mut left } => {
                if left.tick() {
                    self.action = Action::Idle;
                    return Some(UnitEvent::Built);
                }
                self.action = Action::Building { left };
            }
            Action::PickingUpWood { at, mut left } => {
                if left.tick() {
                    self.action = Action::Idle;
                    return Some(UnitEvent::PickedUp { at });
                }
                self.action = Action::PickingUpWood { at, left };
            }
            Action::Chopping { tree, mut left } => {
                if left.tick() {
                    self.action = Action::Idle;
                    return Some(UnitEvent::Chopped { tree });
                }
                self.action = Action::Chopping { tree, left };
            }
            Action::Casting { mut left } => {
                if !left.tick() {
                    self.action = Action::Casting { left };
                    let release = (left.done() == CAST_RELEASE).then(|| self.spell.take()).flatten();
                    return release.map(|spell| UnitEvent::Released { spell });
                }
                self.action = Action::Idle;
                if let Some(to) = self.teleport_to.take().filter(|&to| ground.passable(self.mobility(), (cell_of(to.0), cell_of(to.1)))) {
                    (self.x, self.z) = to;
                    (self.inside, self.aboard) = (None, None);
                    let below = terrain.height_at(to.0 as u32, to.1 as u32, WORLD_UNITS_PER_CELL);
                    self.motion = Motion::flying(Velocity::ZERO, below + LANDING_DROP);
                    self.action = Action::Landing;
                }
            }
            Action::Landing => {}
            Action::Boarding { from, mut left } => self.action = if left.tick() { Action::Idle } else { Action::Boarding { from, left } },
            Action::Alighting { from, height, mut left } => self.action = if left.tick() { Action::Idle } else { Action::Alighting { from, height, left } },
            Action::Panicking { away, mut left } => {
                if left.tick() {
                    self.action = Action::Idle;
                    return None;
                }
                self.action = Action::Panicking { away: self.run_in_panic(away, ground), left };
            }
            Action::OnFire { away, mut left } => {
                if self.health.damage(self.health.max() / BURN_DIVISOR) {
                    self.die();
                    return Some(UnitEvent::Died);
                }
                if left.tick() {
                    self.action = Action::Idle;
                    return None;
                }
                self.action = Action::OnFire { away: self.run_in_panic(away, ground), left };
            }
            #[cfg(test)]
            Action::Gathered { .. } => {}
            Action::Tumbling => {
                if self.motion.is_still() {
                    self.action = Action::Idle;
                }
            }
            Action::Drowning => {
                if self.health.damage(DROWN_DAMAGE) {
                    self.die();
                    return Some(UnitEvent::Died);
                }
                if !is_sea(terrain, self.cell()) {
                    self.action = Action::Idle;
                }
            }
            Action::Dying { mut left } => {
                self.action = match left.tick() {
                    false => Action::Dying { left },
                    true if self.kind == UnitKind::Shaman => Action::Dead { left: Countdown::new(RESPAWN_TICKS) },
                    true => Action::Spirit { left: Countdown::new(SPIRIT_TICKS) },
                };
            }
            Action::Spirit { mut left } => {
                if left.tick() {
                    return Some(UnitEvent::Gone);
                }
                self.action = Action::Spirit { left };
            }
            Action::Dead { mut left } => {
                if !left.tick() {
                    self.action = Action::Dead { left };
                    return None;
                }
                let site = site?;
                *self = Unit { facing: self.facing, ..Unit::shaman(self.id, site) };
                return Some(UnitEvent::Reincarnated);
            }
        }
        None
    }

    /// Whether it heals on a `HEAL_EVERY` turn: alive and on the ground, fighting or not, but not while drowning or
    /// stranded (losing life), nor for a kind that never heals.
    pub fn heals(&self) -> bool {
        self.kind.stats().heal.is_some()
            && self.is_alive()
            && !self.motion.airborne()
            && !matches!(self.action, Action::Drowning | Action::Stranded { .. } | Action::OnFire { .. })
    }

    /// Its turn of healing (`HEAL_EVERY`), if it `heals`.
    pub fn heal(&mut self) {
        if let Some(points) = self.kind.stats().heal.filter(|_| self.heals()) {
            self.health.heal(points);
        }
    }

    /// Stranded: `STRANDED_DAMAGE` lost on its `STRANDED_HURT_EVERY` turns; true when that kills her.
    fn hurt(&mut self, now: Tick) -> bool {
        if STRANDED_HURT_EVERY.fires(now, self.id.raw()) {
            self.health.damage(STRANDED_DAMAGE);
        }
        if self.health.is_zero() {
            self.die();
        }
        self.health.is_zero()
    }

    /// Runs off in panic towards `away` (`panic_heading`), forgetting its plans; nothing while it takes no orders
    /// (already panicking, tumbling, drowning, dead).
    pub fn panic(&mut self, away: (i16, i16)) {
        if !self.takes_orders() {
            return;
        }
        self.drop_plans();
        self.action = Action::Panicking { away, left: Countdown::new(PANIC_TICKS) };
    }

    /// Catches fire (lava, pop3-rev-analysis.md "Volcano lava flow"): drops its plans and burns for a fresh
    /// `BURN_TICKS`, regardless of what it was doing — unlike `panic`, this also re-arms an already-burning unit
    /// (it is "set on fire... every turn it stays on a hot cell"), so it is gated on `is_alive` and not already
    /// falling/dead rather than on `takes_orders` (which `OnFire` itself is excluded from).
    pub fn catch_fire(&mut self, away: (i16, i16)) {
        if !self.is_alive() || matches!(self.action, Action::Tumbling | Action::Drowning) || self.motion.airborne() {
            return;
        }
        self.drop_plans();
        self.action = Action::OnFire { away, left: Countdown::new(BURN_TICKS) };
    }

    /// Leaves its coordinator, if it is in one: back to Idle (`WorldEvent::Release`, or its coordinator gone).
    pub fn release(&mut self) {
        if self.action.link().is_some() {
            self.action = Action::Idle;
        }
    }

    /// Loses `amount` health (`GameMap::damage` decides whether it is hurt at all); true when that kills it. In the
    /// air it falls on: death comes on landing (`touch_down`; pop3-rev-analysis.md "Health and healing").
    pub fn take_damage(&mut self, amount: u16) -> bool {
        if !self.is_alive() {
            return false;
        }
        let dead = self.health.damage(amount);
        if dead && !self.motion.airborne() {
            self.die();
        }
        dead
    }

    fn die(&mut self) {
        self.drop_plans();
        self.action = Action::Dying { left: Countdown::new(DYING_TICKS) };
    }

    /// Back on the ground after flying (`physics::Touchdown`). On ground it can stand on (by its mobility: a boat's
    /// is the sea) it stands up, or dies with no health left; in the sea it dies whatever its health. Anywhere
    /// else (a cliff, a building) it stands up for now.
    fn touch_down(&mut self, ground: &impl Ground) -> Option<UnitEvent> {
        if !self.is_alive() {
            return None;
        }
        let cell = self.cell();
        if self.health.is_zero() || is_sea(ground.terrain(), cell) {
            self.die();
            return Some(UnitEvent::Died);
        }
        if self.action.link().is_none() {
            self.action = Action::Idle;
        }
        None
    }

    /// Forgets where it was going, what it was to do there and its work.
    fn drop_plans(&mut self) {
        (self.route, self.planned_on, self.teleport_to, self.spell, self.to_fire) = (Vec::new(), None, None, None, None);
        (self.to_tree, self.to_wood, self.to_house, self.to_shrine, self.to_vehicle) = (None, None, None, None, None);
        self.queue.clear();
        self.work = None;
    }

    /// Thrown by a spell: `push` adds to its velocity, it leaves the ground (`ground`: the height under it) and
    /// tumbles, forgetting its plans. Nothing for the dead.
    pub fn fling(&mut self, push: Velocity, ground: i32) {
        if !self.is_alive() {
            return;
        }
        self.drop_plans();
        let v = self.motion.velocity;
        let velocity = Velocity::new(v.x.saturating_add(push.x), v.y.saturating_add(push.y), v.z.saturating_add(push.z));
        self.motion = Motion::flying(velocity, self.motion.height.unwrap_or(ground));
        self.action = Action::Tumbling;
    }

    /// Thrown in a fight: like `fling`, but it stays in its fight (a challenge's target `Shoved`).
    pub fn knock(&mut self, push: Velocity, ground: i32) {
        let Action::Challenge { challenge, .. } = self.action else { return self.fling(push, ground) };
        if !self.is_alive() {
            return;
        }
        let v = self.motion.velocity;
        let velocity = Velocity::new(v.x.saturating_add(push.x), v.y.saturating_add(push.y), v.z.saturating_add(push.z));
        self.motion = Motion::flying(velocity, self.motion.height.unwrap_or(ground));
        self.action = Action::Challenge { challenge, role: crate::challenge::Role::Shoved };
    }

    /// A preacher starts preaching where it stands (sermon.rs): no more way to go.
    pub(crate) fn join_preaching(&mut self) {
        (self.route, self.planned_on) = (Vec::new(), None);
        self.action = Action::Preaching;
    }

    /// Joins `tribe`, as a new person in its place would: its plans, orders, work and wood gone, full health, no
    /// status, standing idle (combat.md "Sermon"). Keeps its kind (preacher conversion: a warrior stays a
    /// warrior) — except a wildman, which has no tribe by definition and becomes a brave (the Convert spell,
    /// `convert::effect`; the free welcome at a reincarnation site spawns its braves directly instead, never
    /// calling this on an existing wildman).
    pub fn change_owner(&mut self, tribe: u8) {
        self.drop_plans();
        self.owner = tribe;
        if self.kind == UnitKind::Wildman {
            self.kind = UnitKind::Brave;
        }
        self.health = Health::full(self.kind.max_health());
        (self.statuses, self.motion, self.carrying, self.reload) = (Statuses::default(), Motion::STILL, 0, 0);
        self.action = Action::Idle;
    }

    /// Joins a coordinator, or a sermon's `Listening` (locked the same way but backed by no coordinator), with
    /// `action`, leaving its way, its chained orders and the spell it was to cast.
    pub(crate) fn join(&mut self, action: Action) {
        debug_assert!(action.link().is_some() || matches!(action, Action::Listening { .. }));
        (self.route, self.planned_on, self.teleport_to, self.spell) = (Vec::new(), None, None, None);
        self.queue.clear();
        self.action = action;
    }

    /// Its coordinator sends it to `at` (`WorldEvent::Placed`).
    pub fn placed(&mut self, at: (u16, u16)) {
        match self.action {
            Action::Challenge { challenge, role: crate::challenge::Role::Charging { speed, .. } } => {
                self.action = Action::Challenge { challenge, role: crate::challenge::Role::Charging { to: at, speed } };
            }
            Action::Melee { melee, stance, .. } => self.action = Action::Melee { melee, place: at, stance },
            _ => {}
        }
    }

    /// Turns to face `at`.
    pub fn face(&mut self, at: (u16, u16)) {
        let (dx, dz) = (torus_delta(self.x, at.0), torus_delta(self.z, at.1));
        if (dx, dz) != (0, 0) {
            self.facing = octant(dx, dz);
        }
    }

    /// One turn of its stance in a melee: to its place, on guard, striking, reeling then thrown back.
    fn fight(&mut self, place: (u16, u16), stance: crate::melee::Stance, ground: &impl Ground) -> crate::melee::Stance {
        use crate::melee::{Stance, PLACE_REACH};
        let near = |u: &Self| torus_delta(u.x, place.0).abs() <= PLACE_REACH && torus_delta(u.z, place.1).abs() <= PLACE_REACH;
        match stance {
            Stance::ToPlace | Stance::OnGuard if near(self) => Stance::OnGuard,
            Stance::ToPlace | Stance::OnGuard => {
                self.step_towards(place, ground, self.kind.speed());
                if near(self) { Stance::OnGuard } else { Stance::ToPlace }
            }
            Stance::Striking { blow, mut left } => match left.tick() {
                false => Stance::Striking { blow, left },
                true => Stance::ToPlace,
            },
            Stance::Reeling { mut left, knock } => match (left.tick(), knock) {
                (false, _) => Stance::Reeling { left, knock },
                (true, None) => Stance::ToPlace,
                (true, Some(push)) => {
                    let below = ground.terrain().height_at(self.x as u32, self.z as u32, WORLD_UNITS_PER_CELL);
                    self.motion = Motion::flying(push, below);
                    Stance::Staggering
                }
            },
            Stance::Staggering if self.motion.is_still() => Stance::ToPlace,
            Stance::Staggering => Stance::Staggering,
        }
    }

    /// One turn of its part in a challenge: running straight at the target, shoving, standing up again once still.
    fn play(&mut self, role: crate::challenge::Role, ground: &impl Ground) -> crate::challenge::Role {
        use crate::challenge::Role;
        match role {
            Role::Waiting => Role::Waiting,
            Role::Charging { to, speed } => {
                self.step_towards(to, ground, speed);
                role
            }
            Role::Shoving { mut left, speed } => match left.tick() {
                false => Role::Shoving { left, speed },
                true => Role::Charging { to: (self.x, self.z), speed },
            },
            Role::Shoved if self.motion.is_still() => Role::Waiting,
            Role::Shoved => Role::Shoved,
        }
    }

    /// One step of a panic run towards `away`, out by the door first when inside; the heading it goes on with,
    /// turned a quarter left when the way is blocked.
    fn run_in_panic(&mut self, away: (i16, i16), ground: &impl Ground) -> (i16, i16) {
        if let Some(inside) = self.inside {
            match self.step_towards(inside.door, ground, PANIC_SPEED) {
                Step::Moved => {}
                Step::Arrived => (self.inside, self.planned_on) = (None, None),
                Step::Blocked => {}
            }
            return away;
        }
        let to = (self.x.wrapping_add(away.0 as u16), self.z.wrapping_add(away.1 as u16));
        match self.step_towards(to, ground, PANIC_SPEED) {
            Step::Blocked => (-away.1, away.0),
            Step::Moved | Step::Arrived => away,
        }
    }

    /// Sets off to attack `target`, standing at `to`; `to` follows the target (`retarget`).
    pub fn attack(&mut self, target: Target, to: (u16, u16)) {
        self.start(Order::Stop);
        if self.action.can_take_orders() {
            self.action = Action::Attacking { target, to };
        }
    }

    /// The building it is walking to the door of, to go in.
    pub fn heading_into(&self) -> Option<crate::building::BuildingId> {
        self.to_house.filter(|_| matches!(self.action, Action::Walking { .. } | Action::Stranded { .. }))
    }

    /// Joins the queue of the training unit `building`, standing where it is until given its spot (`queue_at`).
    pub(crate) fn queue_up(&mut self, building: crate::building::BuildingId) {
        self.start(Order::Stop);
        if self.action.can_take_orders() {
            self.action = Action::Queuing { building, at: (self.x, self.z) };
        }
    }

    /// Its spot in its queue is now `at` (the queue moved up): it walks there.
    pub(crate) fn queue_at(&mut self, at: (u16, u16)) {
        if let Action::Queuing { building, at: was } = self.action
            && was != at
        {
            self.action = Action::Queuing { building, at };
            self.planned_on = None;
        }
    }

    /// Stops where it stands, still attacking (a thrower in reach).
    pub fn hold(&mut self) {
        if let Action::Attacking { target, .. } = self.action {
            self.action = Action::Attacking { target, to: (self.x, self.z) };
            self.route.clear();
        }
    }

    /// The target of its attack moved to `to`: aims there, planning again once it is a cell away from where it
    /// aimed.
    pub fn retarget(&mut self, to: (u16, u16)) {
        if let Action::Attacking { target, to: aimed } = self.action
            && (torus_delta(aimed.0, to.0).abs() >= WORLD_UNITS_PER_CELL as i32 || torus_delta(aimed.1, to.1).abs() >= WORLD_UNITS_PER_CELL as i32)
        {
            self.action = Action::Attacking { target, to };
            self.planned_on = None;
        }
    }

    /// One step of the way to `to` while attacking: a path planned again when the ground changes, standing once
    /// there or when no way leads there.
    fn approach(&mut self, to: (u16, u16), ground: &impl Ground) {
        if self.planned_on != Some(ground.revision()) {
            let route = path::route(ground, self.mobility(), (self.x, self.z), to, true);
            self.route = route.map(|r| r.into_iter().rev().collect()).unwrap_or_default();
            self.planned_on = Some(ground.revision());
        }
        let Some(&next) = self.route.last() else { return };
        match self.step_towards(next, ground, self.kind.speed()) {
            Step::Moved => {}
            Step::Arrived => _ = self.route.pop(),
            Step::Blocked => self.planned_on = None,
        }
    }

    /// Path to `to` on the current terrain: walking if there is one, else stranded (the shaman
    /// just stays idle).
    fn plan(&mut self, to: (u16, u16), ground: &impl Ground, straighten: bool) {
        let route = path::route(ground, self.mobility(), (self.x, self.z), to, straighten);
        let walled = ground.walled((cell_of(to.0), cell_of(to.1)));
        self.action = match route {
            Some(_) => Action::Walking { to },
            // Behind walls nobody gets to: no point waiting for the ground to open a way.
            None if self.kind == UnitKind::Shaman || walled => Action::Idle,
            None => Action::Stranded { to },
        };
        self.route = route.map(|r| r.into_iter().rev().collect()).unwrap_or_default();
        self.planned_on = Some(ground.revision());
    }

    /// One step along the route; idle once at `to`. A step that would end in a cell she cannot
    /// cross (a straight leg grazing its corner) replans cell by cell from the current cell centre.
    fn follow_route(&mut self, to: (u16, u16), ground: &impl Ground) {
        let Some(&next) = self.route.last() else {
            self.action = Action::Idle;
            return;
        };
        match self.step_towards(next, ground, self.kind.speed()) {
            Step::Moved => {}
            Step::Arrived => {
                self.route.pop();
                if self.route.is_empty() {
                    self.action = Action::Idle;
                }
            }
            Step::Blocked => self.plan(to, ground, false),
        }
    }

    /// Straight towards `to` at `flat_speed` (world units per tick on flat ground), shortest way
    /// around the torus, never into a cell she cannot cross.
    fn step_towards(&mut self, to: (u16, u16), ground: &impl Ground, flat_speed: i32) -> Step {
        let (dx, dz) = (torus_delta(self.x, to.0), torus_delta(self.z, to.1));
        if dx == 0 && dz == 0 {
            return Step::Arrived;
        }
        self.facing = octant(dx, dz);
        let dist = isqrt((dx * dx + dz * dz) as u32) as i32;
        let speed = slope_speed(flat_speed, self.grade_ahead((dx, dz), dist, ground.terrain()));
        let (sx, sz) = if dist <= speed { (dx, dz) } else { (dx * speed / dist, dz * speed / dist) };
        let (nx, nz) = (self.x.wrapping_add(sx as u16), self.z.wrapping_add(sz as u16));
        let next = (cell_of(nx), cell_of(nz));
        if next != self.cell() && !ground.passable(self.mobility(), next) {
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

pub(crate) fn cell_of(v: u16) -> i32 {
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

/// A panic run's heading from two random numbers: any direction, `PANIC_REACH` long.
pub fn panic_heading(a: u32, b: u32) -> (i16, i16) {
    let (dx, dz) = ((a % 2049) as i32 - 1024, (b % 2049) as i32 - 1024);
    let len = isqrt((dx * dx + dz * dz) as u32) as i32;
    if len == 0 {
        return (PANIC_REACH as i16, 0);
    }
    ((dx * PANIC_REACH / len) as i16, (dz * PANIC_REACH / len) as i16)
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

    #[test]
    fn change_owner_keeps_a_kind_but_turns_a_wildman_into_a_brave() {
        let mut warrior = Unit::new(Id::from_raw(1), 255, UnitKind::Warrior, (0, 0));
        warrior.change_owner(0);
        assert_eq!((warrior.owner, warrior.kind), (0, UnitKind::Warrior), "keeps its kind");
        let mut wildman = Unit::new(Id::from_raw(2), 255, UnitKind::Wildman, (0, 0));
        wildman.change_owner(1);
        assert_eq!((wildman.owner, wildman.kind), (1, UnitKind::Brave), "a wildman has no tribe of its own: becomes a brave");
        assert!(wildman.health.is_full() && wildman.health.max() == UnitKind::Brave.max_health(), "health set from its new kind");
    }

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
        let site = ReincarnationSite::at_cell(0, cell, 0);
        (Unit::shaman(Id::from_raw(1), &site), site)
    }

    fn run(u: &mut Unit, t: &Heightmap, site: &ReincarnationSite, ticks: usize) -> Vec<UnitEvent> {
        (1..=ticks).filter_map(|k| u.tick(Tick::from_wire(k as u32), t, Some(site))).collect()
    }

    fn run_until(u: &mut Unit, t: &Heightmap, site: &ReincarnationSite, done: impl Fn(&Unit) -> bool) -> usize {
        (1..500).find(|_| {
            u.tick(Tick::ZERO, t, Some(site));
            done(u)
        }).unwrap()
    }

    #[test]
    fn jumps_and_builds_then_tells_the_caller() {
        let t = land();
        let (mut u, site) = shaman_at((10, 10));
        u.action = Action::Flattening { at: (10, 11), left: Countdown::new(JUMP_TICKS) };
        assert_eq!(u.action.countdown().map(Countdown::done), Some(Ticks::ZERO));
        assert_eq!(run(&mut u, &t, &site, JUMP_TICKS.get() as usize), vec![UnitEvent::Jumped { at: (10, 11) }]);
        u.action = Action::Building { left: Countdown::new(BUILD_TICKS) };
        assert_eq!(run(&mut u, &t, &site, BUILD_TICKS.get() as usize - 1), vec![]);
        assert_eq!(run(&mut u, &t, &site, 1), vec![UnitEvent::Built]);
        assert_eq!(u.action, Action::Idle);
    }

    #[test]
    fn walks_to_the_target_then_idles() {
        let t = land();
        let (mut u, site) = shaman_at((10, 10));
        let to = (u.x + 3 * 512, u.z);
        u.order(Order::MoveTo { x: to.0, z: to.1 });
        run(&mut u, &t, &site, 28);
        assert_eq!(u.action, Action::Walking { to });
        assert_eq!(u.facing, 2, "heading +x");
        run(&mut u, &t, &site, 1);
        assert_eq!((u.x, u.z, u.action), (to.0, to.1, Action::Idle));
    }

    #[test]
    fn panic_headings_are_full_length_in_any_direction() {
        for (a, b) in [(0, 0), (2048, 1024), (1024, 1024), (7, 1500), (u32::MAX, 3)] {
            let (dx, dz) = panic_heading(a, b);
            let len = isqrt((dx as i32 * dx as i32 + dz as i32 * dz as i32) as u32) as i32;
            assert!((PANIC_REACH - 2..=PANIC_REACH).contains(&len), "{a},{b}: {len}");
        }
        assert_eq!(panic_heading(1024, 1024), (PANIC_REACH as i16, 0), "no direction: +x");
    }

    #[test]
    fn panics_runs_straight_at_its_speed_without_orders_then_idles() {
        let t = land();
        let (mut u, site) = shaman_at((10, 10));
        let start = u.x;
        u.order(Order::MoveTo { x: u.x, z: u.z + 2048 });
        u.panic((PANIC_REACH as i16, 0));
        u.order(Order::MoveTo { x: u.x, z: u.z + 2048 });
        run(&mut u, &t, &site, 10);
        assert_eq!((u.x - start, u.facing), (10 * PANIC_SPEED as u16, 2), "ignores orders, runs +x");
        u.panic((0, PANIC_REACH as i16));
        assert_eq!(u.facing, 2, "panicking already: no new run");
        run(&mut u, &t, &site, PANIC_TICKS.get() as usize - 11);
        assert!(matches!(u.action, Action::Panicking { .. }));
        run(&mut u, &t, &site, 1);
        assert_eq!(u.action, Action::Idle);
    }

    #[test]
    fn a_panic_run_turns_away_from_the_sea_and_leaves_by_the_door_first() {
        let mut t = land();
        let (mut u, site) = shaman_at((10, 10));
        sea_cell(&mut t, (11, 10));
        u.panic((PANIC_REACH as i16, 0));
        run(&mut u, &t, &site, 6);
        assert_eq!(u.cell(), (10, 11), "blocked going +x, turned to +z");

        let t = land();
        let (mut u, _) = shaman_at((10, 10));
        let door = (u.x, u.z - 1024);
        u.inside = Some(Inside { building: Id::from_raw(7), door });
        u.panic((PANIC_REACH as i16, 0));
        run(&mut u, &t, &site, 10);
        assert_eq!((u.inside, u.x), (None, door.0), "out by the door, then the run");
        u.tick(Tick::ZERO, &t, Some(&site));
        assert_eq!(u.x, door.0 + PANIC_SPEED as u16);
    }

    #[test]
    fn walks_the_short_way_across_the_map_edge() {
        let t = land();
        let (mut u, site) = shaman_at((127, 0));
        u.order(Order::MoveTo { x: 256, z: u.z });
        u.tick(Tick::ZERO, &t, Some(&site));
        assert_eq!(u.x, 127 * 512 + 256 + SHAMAN_SPEED as u16);
        assert_eq!(u.facing, 2);
    }

    #[test]
    fn slower_uphill_faster_downhill() {
        assert_eq!(slope_speed(SHAMAN_SPEED, 0), SHAMAN_SPEED);
        assert_eq!(slope_speed(SHAMAN_SPEED, 100), SHAMAN_SPEED / 4, "steep hill: quarter speed");
        assert_eq!(slope_speed(SHAMAN_SPEED, 30), 41, "gentle ramp: ~22% slower");
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
                u.tick(Tick::ZERO, &t, Some(&site));
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
            u.tick(Tick::ZERO, t, Some(site));
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
        run(&mut u, &t, &site, 3 * STRANDED_HURT_EVERY.period() as usize);
        assert_eq!((u.action, u.x, u.z), (Action::Stranded { to }, start.0, start.1), "arms up, not moving");
        assert_eq!(u.health.current(), SHAMAN_MAX_HEALTH - 3 * STRANDED_DAMAGE);
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
        let events = run(&mut u, &t, &site, STRANDED_HURT_EVERY.period() as usize * (SHAMAN_MAX_HEALTH / STRANDED_DAMAGE) as usize);
        assert_eq!((events, u.health.current(), u.is_alive()), (vec![UnitEvent::Died], 0, false));
    }

    #[test]
    fn the_shaman_does_not_move_towards_an_unreachable_target() {
        let mut t = land();
        sea_cell(&mut t, (20, 10));
        let (mut u, site) = shaman_at((10, 10));
        let start = (u.x, u.z);
        u.order(Order::MoveTo { x: 20 * 512 + 256, z: u.z });
        run(&mut u, &t, &site, 10);
        assert_eq!((u.action, u.x, u.z, u.health.current()), (Action::Idle, start.0, start.1, SHAMAN_MAX_HEALTH));
    }

    #[test]
    fn cast_jump_ends_back_idle() {
        let t = land();
        let (mut u, site) = shaman_at((10, 10));
        u.order(Order::Cast);
        assert_eq!(u.action.countdown().map(Countdown::done), Some(Ticks::ZERO));
        run(&mut u, &t, &site, CAST_TICKS.get() as usize - 1);
        assert!(matches!(u.action, Action::Casting { left } if left.left() == Ticks::new(1)));
        run(&mut u, &t, &site, 1);
        assert_eq!(u.action, Action::Idle);
    }

    #[test]
    fn the_spell_leaves_her_into_the_jump_unless_an_order_comes_first() {
        let t = land();
        let (mut u, site) = shaman_at((10, 10));
        let spell = Spell::Blast { at: (100, 100) };
        assert!(u.cast_spell(spell));
        let events: Vec<_> = (1..=CAST_TICKS.get()).map(|n| (n, u.tick(Tick::ZERO, &t, Some(&site)))).filter(|e| e.1.is_some()).collect();
        assert_eq!(events, [(CAST_RELEASE.get(), Some(UnitEvent::Released { spell }))]);
        assert!(u.cast_spell(spell));
        u.order(Order::Stop);
        run(&mut u, &t, &site, CAST_TICKS.get() as usize);
        assert_eq!(u.action, Action::Idle, "cancelled, nothing released");
    }

    #[test]
    fn pulled_into_a_fight_she_drops_the_spell_she_held() {
        let (mut u, _) = shaman_at((10, 10));
        assert!(u.cast_spell(Spell::Blast { at: (100, 100) }));
        u.join(Action::Melee { melee: crate::id::Id::from_raw(1), place: (u.x, u.z), stance: crate::melee::Stance::ToPlace });
        assert_eq!(u.walking_to_cast(), None, "not cast again once out of it");
    }

    #[test]
    fn teleport_jumps_first_then_moves() {
        let mut t = land();
        let (mut u, site) = shaman_at((10, 10));
        let (start, to) = ((u.x, u.z), (30 * 512 + 100, 40 * 512 + 7));
        u.cast_teleport(to);
        run(&mut u, &t, &site, CAST_TICKS.get() as usize - 1);
        assert_eq!((u.x, u.z), start, "still jumping where she was");
        assert!(matches!(u.action, Action::Casting { left } if left.left() == Ticks::new(1)));
        run(&mut u, &t, &site, 1);
        assert_eq!((u.x, u.z, u.action), (to.0, to.1, Action::Landing), "at the target, up in the air");
        assert_eq!(u.motion, Motion::flying(Velocity::ZERO, 100 + LANDING_DROP));
        u.order(Order::Stop);
        run(&mut u, &t, &site, 4);
        assert_eq!((u.action, u.motion.height), (Action::Landing, Some(100 + LANDING_DROP - 192)), "falling, no orders");
        run(&mut u, &t, &site, 1);
        assert_eq!((u.action, u.motion), (Action::Idle, Motion::STILL), "landed");
        sea_cell(&mut t, (5, 5));
        u.cast_teleport((5 * 512, 5 * 512));
        run(&mut u, &t, &site, CAST_TICKS.get() as usize);
        assert_eq!((u.x, u.z), to, "the target became sea meanwhile: stays");
        u.cast_teleport(start);
        u.order(Order::Stop);
        run(&mut u, &t, &site, CAST_TICKS.get() as usize);
        assert_eq!((u.x, u.z), to, "an order cancels it");
    }

    #[test]
    fn drowns_dies_and_reincarnates_at_the_site() {
        let mut t = land();
        let (mut u, site) = shaman_at((10, 10));
        u.order(Order::Stop);
        u.x += 20 * 512;
        let (cx, cz) = u.cell();
        for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            t.set(cx + dx, cz + dz, 0);
        }
        u.tick(Tick::ZERO, &t, Some(&site));
        assert_eq!((u.action, u.health.current()), (Action::Drowning, SHAMAN_MAX_HEALTH - DROWN_DAMAGE));
        u.order(Order::MoveTo { x: 0, z: 0 });
        assert_eq!(u.action, Action::Drowning, "no orders while drowning");
        let events = run(&mut u, &t, &site, (SHAMAN_MAX_HEALTH / DROWN_DAMAGE) as usize);
        assert_eq!((events, u.health.current(), u.is_alive()), (vec![UnitEvent::Died], 0, false));
        let events = run(&mut u, &t, &site, (DYING_TICKS + RESPAWN_TICKS).get() as usize);
        assert_eq!(events, vec![UnitEvent::Reincarnated]);
        assert_eq!((u.x, u.z, u.health.current(), u.action), (site.x, site.z, SHAMAN_MAX_HEALTH, Action::Idle));
    }

    #[test]
    fn only_the_shaman_reincarnates() {
        let mut t = land();
        let (shaman, site) = shaman_at((10, 10));
        let mut brave = Unit::new(Id::from_raw(7), 0, UnitKind::Brave, (shaman.x + 20 * 512, shaman.z));
        assert_eq!(brave.health, Health::full(UnitKind::Brave.max_health()));
        sea_cell(&mut t, brave.cell());
        let ticks = run_until(&mut brave, &t, &site, |u| !u.is_alive());
        assert!(matches!(brave.action, Action::Dying { .. }));
        run(&mut brave, &t, &site, DYING_TICKS.get() as usize);
        assert!(matches!(brave.action, Action::Spirit { .. }), "its spirit rises, no reincarnation");
        assert_eq!(run(&mut brave, &t, &site, SPIRIT_TICKS.get() as usize), [UnitEvent::Gone]);
        assert!(ticks > 1 && brave.kind == UnitKind::Brave);
    }

    #[test]
    fn kinds_walk_at_their_own_speed() {
        let t = land();
        let ticks = |kind: UnitKind| {
            let (s, site) = shaman_at((10, 10));
            let mut u = Unit::new(Id::from_raw(2), 0, kind, (s.x, s.z));
            walk(&mut u, &t, &site, (s.x + 10 * 512, s.z), 500).unwrap()
        };
        assert!(ticks(UnitKind::Spy) < ticks(UnitKind::Brave) && ticks(UnitKind::Brave) < ticks(UnitKind::Warrior));
    }

    #[test]
    fn tumbling_takes_no_orders_and_settles_once_still_on_the_ground() {
        let t = land();
        let (mut u, site) = shaman_at((10, 10));
        let (x, z) = (u.x, u.z);
        u.action = Action::Tumbling;
        u.motion = Motion::flying(Velocity::new(10, 40, 0), 100);
        u.order(Order::MoveTo { x: 0, z: 0 });
        run(&mut u, &t, &site, 2);
        assert_eq!((u.action, u.motion.height, u.x), (Action::Tumbling, Some(148), x + 20), "no orders, flying");
        run(&mut u, &t, &site, 3);
        assert_eq!((u.action, u.motion, (u.x, u.z)), (Action::Idle, Motion::STILL, (x + 40, z)), "landed and settled the same tick");
        (u.action, u.motion.velocity) = (Action::Tumbling, Velocity::new(5, 0, 0));
        run(&mut u, &t, &site, 3);
        assert_eq!((u.action, u.x), (Action::Tumbling, x + 40), "rolling on the ground: not moved, no friction yet");
    }

    #[test]
    fn locked_and_not_pickable_while_tumbling_or_lifted() {
        let (mut u, _) = shaman_at((10, 10));
        assert_eq!(u.pickable().map(|p| p.id), Some(u.id));
        u.motion.height = Some(1000);
        assert!(u.locked() && u.pickable().is_none(), "lifted");
        (u.motion.height, u.action) = (None, Action::Tumbling);
        assert!(u.locked() && u.pickable().is_none(), "rolling on the ground");
    }

    #[test]
    fn flung_it_forgets_its_plans_and_flies() {
        let t = land();
        let (mut u, site) = shaman_at((10, 10));
        u.order(Order::MoveTo { x: u.x + 5 * 512, z: u.z });
        u.fling(Velocity::new(70, 49, 0), 100);
        assert_eq!((u.action, u.motion), (Action::Tumbling, Motion::flying(Velocity::new(70, 49, 0), 100)));
        u.fling(Velocity::new(10, 10, 0), 0);
        assert_eq!(u.motion, Motion::flying(Velocity::new(80, 59, 0), 100), "pushes add up, the height is kept");
        let x = u.x;
        let ticks = run_until(&mut u, &t, &site, |u| u.action == Action::Idle);
        assert_eq!((ticks, u.x), (5, x + 5 * 80), "flew, landed, then stays: the walk is forgotten");
        run(&mut u, &t, &site, 20);
        assert_eq!(u.x, x + 5 * 80);
        let mut dead = Unit { action: Action::Dead { left: Countdown::new(RESPAWN_TICKS) }, ..u.clone() };
        dead.fling(Velocity::new(10, 10, 0), 100);
        assert!(matches!(dead.action, Action::Dead { .. }) && dead.motion == Motion::STILL);
    }

    #[test]
    fn landing_in_the_sea_kills_whatever_the_health() {
        let mut t = land();
        let (mut u, site) = shaman_at((10, 10));
        for z in -1..=1 {
            for x in 1..=4 {
                sea_cell(&mut t, (u.cell().0 + x, u.cell().1 + z));
            }
        }
        u.fling(Velocity::new(120, 60, 0), 100);
        let mut events = Vec::new();
        while u.motion.airborne() {
            events.extend(u.tick(Tick::ZERO, &t, Some(&site)));
        }
        assert_eq!((is_sea(&t, u.cell()), events), (true, vec![UnitEvent::Died]));
        assert!(matches!(u.action, Action::Dying { .. }) && u.health.is_full(), "dies at once, the dying pose");
    }

    #[test]
    fn landing_on_ground_stands_up_unless_no_health_is_left() {
        let mut t = land();
        let (mut u, site) = shaman_at((10, 10));
        u.fling(Velocity::new(0, 40, 0), 100);
        assert_eq!(run_until(&mut u, &t, &site, |u| !u.motion.airborne()), 4);
        assert_eq!(u.action, Action::Idle);
        u.health = Health::new(0, SHAMAN_MAX_HEALTH);
        u.fling(Velocity::new(0, 40, 0), 100);
        assert_eq!(run(&mut u, &t, &site, 4), [UnitEvent::Died]);
        assert!(matches!(u.action, Action::Dying { .. }));
        let (mut u, _) = shaman_at((20, 20));
        t.set(u.cell().0 + 1, u.cell().1, 1000);
        u.fling(Velocity::new(0, 40, 0), 100);
        run(&mut u, &t, &site, 4);
        assert_eq!(u.action, Action::Idle, "on a cliff: stands up for now");
    }

    #[test]
    fn landing_with_no_health_left_dies_even_on_a_cliff() {
        // A fatal hit taken mid-flight (`take_damage`'s `!airborne` guard) only calls `die` on landing
        // (`touch_down`): landing anywhere neither standing ground nor sea must still kill it, not just
        // the two terrain-specific cases, or a zero-health unit silently comes back to life as Idle.
        let mut t = land();
        let (mut u, site) = shaman_at((20, 20));
        t.set(u.cell().0 + 1, u.cell().1, 1000);
        u.health = Health::new(0, SHAMAN_MAX_HEALTH);
        u.fling(Velocity::new(0, 40, 0), 100);
        assert_eq!(run(&mut u, &t, &site, 4), [UnitEvent::Died]);
        assert!(matches!(u.action, Action::Dying { .. }), "dies on landing, cliff or not");
    }

    #[test]
    fn no_drowning_in_the_air_over_the_sea() {
        let mut t = land();
        let (mut u, site) = shaman_at((10, 10));
        sea_cell(&mut t, u.cell());
        (u.action, u.motion) = (Action::Tumbling, Motion::flying(Velocity::ZERO, 50));
        run(&mut u, &t, &site, 1);
        assert_eq!((u.action, u.health.is_full()), (Action::Tumbling, true));
        u.motion = Motion::STILL;
        run(&mut u, &t, &site, 1);
        assert_eq!(u.action, Action::Drowning);
    }

    #[test]
    fn each_kind_heals_its_own_amount_but_never_past_full() {
        let (s, _) = shaman_at((10, 10));
        let healed = |kind: UnitKind| {
            let mut u = Unit { kind, health: Health::new(100, kind.max_health()), ..s.clone() };
            u.heal();
            u.health.current() - 100
        };
        assert_eq!(UnitKind::ALL.map(healed), [32, 4, 6, 4, 4, 6, 0]);
        let mut full = s.clone();
        full.heal();
        assert!(full.health.is_full());
    }

    #[test]
    fn no_healing_in_the_air_drowning_stranded_or_dead() {
        let (s, _) = shaman_at((10, 10));
        let hurt = Unit { health: Health::new(100, SHAMAN_MAX_HEALTH), ..s };
        let flung = Unit { motion: Motion::flying(Velocity::ZERO, 50), action: Action::Tumbling, ..hurt.clone() };
        let drowning = Unit { action: Action::Drowning, ..hurt.clone() };
        let stranded = Unit { action: Action::Stranded { to: (0, 0) }, ..hurt.clone() };
        let dying = Unit { action: Action::Dying { left: Countdown::new(DYING_TICKS) }, ..hurt.clone() };
        let burning = Unit { action: Action::OnFire { away: (0, 0), left: Countdown::new(BURN_TICKS) }, ..hurt.clone() };
        assert!(hurt.heals());
        assert!([flung, drowning, stranded, dying, burning].iter().all(|u| !u.heals()));
    }

    #[test]
    fn catching_fire_loses_max_over_32_health_a_turn_ignoring_orders_until_dead() {
        let t = land();
        let (mut u, site) = shaman_at((10, 10));
        u.catch_fire((PANIC_REACH as i16, 0));
        assert_eq!(u.action, Action::OnFire { away: (PANIC_REACH as i16, 0), left: Countdown::new(BURN_TICKS) });
        u.order(Order::MoveTo { x: 0, z: 0 });
        run(&mut u, &t, &site, 1);
        assert_eq!(u.health.current(), SHAMAN_MAX_HEALTH - SHAMAN_MAX_HEALTH / BURN_DIVISOR, "no orders while burning");
        let events = run(&mut u, &t, &site, BURN_TICKS.get() as usize);
        assert_eq!((events, u.health.current(), u.is_alive()), (vec![UnitEvent::Died], 0, false), "dead well before the 70 turns run out");
    }

    #[test]
    fn burns_out_on_its_own_once_the_countdown_ends_if_kept_alive() {
        // 70 turns losing max/32 a turn is always fatal first (confirmed: "dies in about 32 turns") — so this
        // isolates the countdown's own end from the damage by topping health back up externally each turn.
        let t = land();
        let (mut u, site) = shaman_at((10, 10));
        u.catch_fire((PANIC_REACH as i16, 0));
        for k in 1..=BURN_TICKS.get() {
            u.health = Health::full(SHAMAN_MAX_HEALTH);
            u.tick(Tick::from_wire(k), &t, Some(&site));
        }
        assert_eq!(u.action, Action::Idle, "the countdown ran out on its own");
    }

    #[test]
    fn re_ignites_an_already_burning_unit_instead_of_being_ignored() {
        let (mut u, _) = shaman_at((10, 10));
        u.catch_fire((PANIC_REACH as i16, 0));
        let Action::OnFire { mut left, .. } = u.action else { unreachable!() };
        left.tick();
        u.action = Action::OnFire { away: (PANIC_REACH as i16, 0), left };
        u.catch_fire((0, PANIC_REACH as i16));
        assert_eq!(u.action, Action::OnFire { away: (0, PANIC_REACH as i16), left: Countdown::new(BURN_TICKS) }, "re-armed, not left alone");
    }

    #[test]
    fn hurt_to_death_in_the_air_it_dies_on_landing() {
        let t = land();
        let (mut u, site) = shaman_at((10, 10));
        let ground = t.height_at(u.x as u32, u.z as u32, WORLD_UNITS_PER_CELL);
        u.fling(Velocity::new(0, 40, 0), ground);
        assert!(u.take_damage(SHAMAN_MAX_HEALTH), "a deadly blow");
        assert_eq!(u.action, Action::Tumbling, "still flying");
        let fall = run_until(&mut u, &t, &site, |u| !u.is_alive());
        assert!(fall > 1 && !u.motion.airborne(), "dead once landed, after {fall} ticks");
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
