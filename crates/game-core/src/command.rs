//! Player inputs: the only thing exchanged in lockstep multiplayer.

use crate::spell::Spell;
use crate::unit::Order;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Cast { player: u8, spell: Spell },
    /// An order to the player's shaman.
    Order { player: u8, order: Order },
}
