//! Player inputs: the only thing exchanged in lockstep multiplayer.

use crate::building::BuildingKind;
use crate::spell::Spell;
use crate::unit::Order;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Cast { player: u8, spell: Spell },
    /// An order to the player's shaman.
    Order { player: u8, order: Order },
    /// An order to one of the player's units (by `Unit::id`); ignored if the unit is not theirs.
    OrderUnit { player: u8, unit: u32, order: Order },
    /// Chains `order` after the unit's current action and chained orders (Ctrl + click): it starts
    /// once the unit is idle. Ignored if the unit is not theirs.
    QueueOrder { player: u8, unit: u32, order: Order },
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
}
