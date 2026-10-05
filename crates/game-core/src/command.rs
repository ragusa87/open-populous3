//! Player inputs: the only thing exchanged in lockstep multiplayer.

use crate::spell::Spell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Cast { player: u8, spell: Spell },
}
