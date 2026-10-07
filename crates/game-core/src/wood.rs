//! Pieces of wood lying on the ground: cut from a tree, dropped by a brave or taken out of a
//! building, waiting to be picked up by any construction. One piece each, integer position.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WoodPiece {
    /// World units.
    pub x: u16,
    pub z: u16,
}

impl WoodPiece {
    pub fn new(x: u16, z: u16) -> Self {
        WoodPiece { x, z }
    }
}
