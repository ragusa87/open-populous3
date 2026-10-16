//! Spells as cast on the map: terrain spells are brushes over the heightmap; Teleport moves the
//! caster's shaman (`GameMap::apply`). Mana, cooldowns and effects on units/buildings come later
//! (see docs/specs/spells.md).

use crate::spell_book::SpellKind;
use crate::terrain::{DirtyRect, Heightmap};

pub type Cell = (i32, i32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spell {
    LandBridge { from: Cell, to: Cell },
    /// Level the ground around the height point `at` to its height (`flatten`); `cast`: the editor's instant brush.
    Flatten { at: Cell },
    Erode { at: Cell },
    Raise { at: Cell },
    /// Move the caster's shaman to `to` (world units), only onto ground she can walk.
    Teleport { to: (u16, u16) },
    /// A blast at `at` (world units): `blast::Blast` pushes the people around (no shot flying there yet).
    Blast { at: (u16, u16) },
    /// A bolt at `at` (world units): `lightning::effect`.
    Lightning { at: (u16, u16) },
    /// A swamp at `at` (world units): `swamp::effect`.
    Swamp { at: (u16, u16) },
    /// Shields the `SHIELD_NUM_PEOPLE` nearest own followers around the shaman, as she stands when it releases
    /// (self-cast, never wire-encoded as a `Spell`: `Command::CastSelf` carries its `SpellKind` instead, see
    /// `game-net`).
    MagicalShield,
    /// Bloodlusts the `BLOODLUST_NUM_PEOPLE` nearest own followers around the shaman (self-cast, see
    /// `MagicalShield`).
    Bloodlust,
    /// Turns the `INVIS_NUM_PEOPLE` nearest own followers around the shaman invisible (self-cast, see
    /// `MagicalShield`).
    Invisibility,
    /// The nearest wildman within range of `at` (world units) becomes a brave of the caster, every few turns for a
    /// while: `convert::effect`.
    Convert { at: (u16, u16) },
    /// The `HYPNO_NUM_PEOPLE` nearest people of other tribes around `at` (world units) join the caster's tribe for
    /// a while: `hypnotism::effect`.
    Hypnotism { at: (u16, u16) },
    /// A volcano at `at` (world units): `volcano::effect`.
    Volcano { at: (u16, u16) },
    /// A firestorm at `at` (world units): `firestorm::effect`.
    Firestorm { at: (u16, u16) },
    /// An earthquake at `at` (world units): `earthquake::effect`.
    Earthquake { at: (u16, u16) },
    /// Stops the match and settles it (`GameMap::start_armageddon`), centred on `at` (world units).
    Armageddon { at: (u16, u16) },
}

impl Spell {
    /// The book spell this cast uses; None for the terrain brushes (editor and dev tools, not cast from a book yet).
    pub fn kind(&self) -> Option<SpellKind> {
        match self {
            Spell::Teleport { .. } => Some(SpellKind::Teleport),
            Spell::Blast { .. } => Some(SpellKind::Blast),
            Spell::Lightning { .. } => Some(SpellKind::Lightning),
            Spell::Swamp { .. } => Some(SpellKind::Swamp),
            Spell::MagicalShield => Some(SpellKind::MagicalShield),
            Spell::Bloodlust => Some(SpellKind::Bloodlust),
            Spell::Invisibility => Some(SpellKind::Invisibility),
            Spell::Convert { .. } => Some(SpellKind::Convert),
            Spell::Hypnotism { .. } => Some(SpellKind::Hypnotism),
            Spell::Volcano { .. } => Some(SpellKind::Volcano),
            Spell::Firestorm { .. } => Some(SpellKind::Firestorm),
            Spell::Earthquake { .. } => Some(SpellKind::Earthquake),
            Spell::LandBridge { .. } => Some(SpellKind::LandBridge),
            Spell::Flatten { .. } => Some(SpellKind::Flatten),
            Spell::Armageddon { .. } => Some(SpellKind::Armageddon),
            Spell::Erode { .. } | Spell::Raise { .. } => None,
        }
    }

    /// Apply the terrain part of the spell and return the region to refresh (None: no terrain part).
    pub fn cast(&self, terrain: &mut Heightmap) -> Option<DirtyRect> {
        Some(match *self {
            Spell::LandBridge { from, to } => terrain.land_bridge(from, to, 32),
            Spell::Flatten { at } => terrain.flatten(at, 4),
            Spell::Erode { at } => terrain.raise(at, 4, -120),
            Spell::Raise { at } => terrain.raise(at, 3, 64),
            Spell::Teleport { .. }
            | Spell::Blast { .. }
            | Spell::Lightning { .. }
            | Spell::Armageddon { .. }
            | Spell::Swamp { .. }
            | Spell::MagicalShield
            | Spell::Bloodlust
            | Spell::Invisibility
            | Spell::Convert { .. }
            | Spell::Hypnotism { .. }
            | Spell::Volcano { .. }
            | Spell::Firestorm { .. }
            | Spell::Earthquake { .. } => return None,
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
