//! A level's plants (scenery 7 and 8, pop3-rev-analysis.md "Scenery looks: plants and prayer heads"): decoration
//! only, each drawn as one camera-facing sprite. Nothing ticks them.

use pop3_format::Level;
use pop3_format::level::KIND_SCENERY;

/// A plant's look: scenery 7 a tuft of dry reeds, 8 reeds with flower heads (bulrushes) [exe].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlantKind {
    Reeds,
    Bulrushes,
}

impl PlantKind {
    /// The scenery model of a plant, None for any other.
    pub fn from_model(model: u8) -> Option<Self> {
        match model {
            7 => Some(PlantKind::Reeds),
            8 => Some(PlantKind::Bulrushes),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plant {
    pub kind: PlantKind,
    /// World units (512 per cell).
    pub x: u16,
    pub z: u16,
}

/// Every plant of `level`, in file order.
pub fn plants_from_level(level: &Level) -> Vec<Plant> {
    level
        .things
        .iter()
        .filter(|t| t.kind == KIND_SCENERY)
        .filter_map(|t| PlantKind::from_model(t.model).map(|kind| Plant { kind, x: t.x, z: t.z }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenery_7_and_8_are_the_plants() {
        assert_eq!(PlantKind::from_model(7), Some(PlantKind::Reeds));
        assert_eq!(PlantKind::from_model(8), Some(PlantKind::Bulrushes));
        assert_eq!(PlantKind::from_model(9), None, "a totem");
        assert_eq!(PlantKind::from_model(1), None, "a tree");
    }
}
