//! Player inputs: the only thing exchanged in lockstep multiplayer.

use crate::building::{BuildingId, BuildingKind};
use crate::effect::Effect;
use crate::id::Id;
use crate::spell::Spell;
use crate::spell_book::SpellKind;
use crate::unit::{Order, Target, UnitId};
use crate::vehicle::VehicleId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// A spell aimed at a spot: uses a charge of the player's book (`Spell::kind`), ignored without one.
    Cast { player: u8, spell: Spell },
    /// A spell not aimed (C): uses a charge of `kind` and the shaman casts (its effect comes later).
    CastSelf { player: u8, kind: SpellKind },
    /// Stops (true) or resumes the recharge of the player's known spell `kind`.
    PauseSpell { player: u8, kind: SpellKind, paused: bool },
    /// An order to the player's shaman.
    Order { player: u8, order: Order },
    /// An order to one of the player's units (by `Unit::id`); ignored if the unit is not theirs.
    OrderUnit { player: u8, unit: UnitId, order: Order },
    /// Chains `order` after the unit's current action and chained orders (Ctrl + click): it starts
    /// once the unit is idle. Ignored if the unit is not theirs.
    QueueOrder { player: u8, unit: UnitId, order: Order },
    /// Light a camp fire in the cell holding world point `at` (`campfire::can_place`).
    PlaceCampfire { player: u8, at: (u16, u16) },
    /// Put out the player's camp fire in the cell holding world point `at`; whoever went round it
    /// stops.
    RemoveCampfire { player: u8, at: (u16, u16) },
    /// Place a building plan with its stored corner at `at` (`placement::can_place`); braves are
    /// sent to it with `Order::Build`.
    PlaceBuilding { player: u8, kind: BuildingKind, at: (u16, u16), facing: u8 },
    /// Remove the player's building plan (not flattened yet) whose footprint holds world point
    /// `at`: its braves stop, the wood brought is lost.
    CancelBuilding { player: u8, at: (u16, u16) },
    /// Switch dismantling of the player's building `building` (id; built or under construction, one
    /// that takes wood): on, the people inside walk out and nobody builds it; off, it is built again
    /// (`GameMap::set_dismantling`).
    Dismantle { player: u8, building: BuildingId, on: bool },
    /// Send the player's manned vehicle `vehicle` to `to` (world units), or as close as it gets
    /// (`GameMap::move_vehicle`).
    MoveVehicle { player: u8, vehicle: VehicleId, to: (u16, u16) },
    /// Everyone aboard the player's vehicle `vehicle` gets off, if there is ground within reach
    /// (`GameMap::unload`).
    Unload { player: u8, vehicle: VehicleId },
    /// Ends the player's own swamp `swamp` early, like a blueprint put away (right click + Ctrl); ignored for
    /// one not theirs (`GameMap::clear_swamp`).
    ClearSwamp { player: u8, swamp: Id<Effect> },
    /// Ctrl+Alt + left click on the ground, the first point of a new patrol sequence (units.md "Patrol
    /// orders"): clears each of `units`' order and whole patrol, shares one new move-type point on all
    /// of them, starts it at once.
    ///
    /// Carries the whole group, unlike every other order command (which resolve a shared target into
    /// one `Command::OrderUnit` per unit before the command exists at all): this one mints a single new
    /// shared point across the group inside one deterministic `apply` call, the same way `PlaceBuilding`
    /// mints one new id, which a one-command-per-unit shape could not do atomically.
    SetPatrol { player: u8, units: Vec<UnitId>, at: (u16, u16) },
    /// Ctrl+Alt + left click on an enemy, the first point of a new patrol sequence: as `SetPatrol`, an
    /// attack-type point instead.
    SetPatrolTarget { player: u8, units: Vec<UnitId>, target: Target },
    /// Ctrl+Alt + left click on the ground after a first point: appends a fresh shared move-type point
    /// to each of `units`' existing patrol instead of replacing it (a no-op per unit already at 8).
    AddPatrolPoint { player: u8, units: Vec<UnitId>, at: (u16, u16) },
    /// Ctrl+Alt + left click on an enemy after a first point: as `AddPatrolPoint`, an attack-type point.
    AddPatrolTargetPoint { player: u8, units: Vec<UnitId>, target: Target },
    /// `N`: releases `units`' whole patrol (its points are pruned once nothing still refers to them).
    ReleasePatrol { player: u8, units: Vec<UnitId> },
}
