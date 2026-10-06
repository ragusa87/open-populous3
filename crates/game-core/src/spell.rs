//! Spells as cast on the map: terrain spells are brushes over the heightmap; Teleport moves the
//! caster's shaman (`GameMap::apply`). Mana, cooldowns and effects on units/buildings come later
//! (see docs/specs/spells.md).

use crate::terrain::{DirtyRect, Heightmap};

pub type Cell = (i32, i32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spell {
    LandBridge { from: Cell, to: Cell },
    Flatten { at: Cell },
    Erode { at: Cell },
    Raise { at: Cell },
    /// Move the caster's shaman to `to` (world units), only onto ground she can walk.
    Teleport { to: (u16, u16) },
}

impl Spell {
    /// Apply the terrain part of the spell and return the region to refresh (None: no terrain part).
    pub fn cast(&self, terrain: &mut Heightmap) -> Option<DirtyRect> {
        Some(match *self {
            Spell::LandBridge { from, to } => terrain.land_bridge(from, to, 32),
            Spell::Flatten { at } => terrain.flatten(at, 4),
            Spell::Erode { at } => terrain.raise(at, 4, -120),
            Spell::Raise { at } => terrain.raise(at, 3, 64),
            Spell::Teleport { .. } => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erode_lowers_ground() {
        let mut t = Heightmap::new(16);
        t.raise((8, 8), 5, 300);
        let before = t.get(8, 8);
        Spell::Erode { at: (8, 8) }.cast(&mut t);
        assert!(t.get(8, 8) < before);
    }
}
