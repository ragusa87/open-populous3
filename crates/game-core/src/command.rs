//! Player inputs: the only thing exchanged in lockstep multiplayer.

use crate::spell::Spell;
use crate::unit::Order;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Cast { player: u8, spell: Spell },
    /// An order to the player's shaman.
    Order { player: u8, order: Order },
    /// An order to one of the player's units (by `Unit::id`); ignored if the unit is not theirs.
    OrderUnit { player: u8, unit: u32, order: Order },
    /// Light a camp fire in the cell holding world point `at` (`campfire::can_place`).
    PlaceCampfire { player: u8, at: (u16, u16) },
    /// Put out the player's camp fire in the cell holding world point `at`; whoever went round it
    /// stops.
    RemoveCampfire { player: u8, at: (u16, u16) },
}
