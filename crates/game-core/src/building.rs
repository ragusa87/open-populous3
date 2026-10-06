//! Buildings standing on the map, loaded from the level things (kind 2). Only what they are, whose,
//! and where for now: construction, health, people inside and footprints come later.

use pop3_format::level::KIND_BUILDING;
use pop3_format::Level;

/// Building types by thing model, as numbered in the original's scripts (`M_BUILDING_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildingKind {
    /// Villager hut, size 1-3 (models 1-3).
    Hut { size: u8 },
    DrumTower,
    /// Trains preachers.
    Temple,
    SpyTraining,
    WarriorTraining,
    FirewarriorTraining,
    Reconversion,
    WallPiece,
    Gate,
    BoatHut,
    AirshipHut,
    GuardPost,
    /// Vault of knowledge: worshipped to learn a spell or a building.
    Vault,
    Prison,
    /// A model not known yet.
    Other(u8),
}

/// A building's footprint in its own frame (facing 0), world units: half size and centre shift
/// along x and z. Measured from the original objects; a boat hut's jetty is left out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Footprint {
    pub half: (i32, i32),
    pub offset: (i32, i32),
}

const fn footprint(half: (i32, i32), offset: (i32, i32)) -> Footprint {
    Footprint { half, offset }
}

impl BuildingKind {
    /// The ground it stands on (levelled when the map loads).
    pub fn footprint(self) -> Footprint {
        match self {
            BuildingKind::Hut { .. } => footprint((600, 600), (0, 0)),
            BuildingKind::DrumTower => footprint((360, 360), (0, 0)),
            BuildingKind::Temple => footprint((800, 950), (0, -146)),
            BuildingKind::SpyTraining => footprint((370, 370), (0, 0)),
            BuildingKind::WarriorTraining => footprint((600, 600), (44, 0)),
            BuildingKind::FirewarriorTraining => footprint((795, 765), (0, 0)),
            BuildingKind::BoatHut => footprint((600, 446), (0, -66)),
            BuildingKind::AirshipHut => footprint((1037, 700), (358, 0)),
            BuildingKind::Vault => footprint((540, 556), (0, 44)),
            BuildingKind::Prison => footprint((560, 560), (30, 30)),
            _ => footprint((410, 410), (0, 0)),
        }
    }

    pub fn from_model(model: u8) -> Self {
        match model {
            1..=3 => BuildingKind::Hut { size: model },
            4 => BuildingKind::DrumTower,
            5 => BuildingKind::Temple,
            6 => BuildingKind::SpyTraining,
            7 => BuildingKind::WarriorTraining,
            8 => BuildingKind::FirewarriorTraining,
            9 => BuildingKind::Reconversion,
            10 => BuildingKind::WallPiece,
            11 => BuildingKind::Gate,
            13 | 14 => BuildingKind::BoatHut,
            15 | 16 => BuildingKind::AirshipHut,
            17 => BuildingKind::GuardPost,
            18 => BuildingKind::Vault,
            19 => BuildingKind::Prison,
            m => BuildingKind::Other(m),
        }
    }

    pub fn name(self) -> String {
        match self {
            BuildingKind::Hut { size } => format!("Hut {size}"),
            BuildingKind::DrumTower => "Drum tower".into(),
            BuildingKind::Temple => "Temple".into(),
            BuildingKind::SpyTraining => "Spy hut".into(),
            BuildingKind::WarriorTraining => "Warrior hut".into(),
            BuildingKind::FirewarriorTraining => "Firewarrior hut".into(),
            BuildingKind::Reconversion => "Reconversion".into(),
            BuildingKind::WallPiece => "Wall".into(),
            BuildingKind::Gate => "Gate".into(),
            BuildingKind::BoatHut => "Boat hut".into(),
            BuildingKind::AirshipHut => "Airship hut".into(),
            BuildingKind::GuardPost => "Guard post".into(),
            BuildingKind::Vault => "Vault of knowledge".into(),
            BuildingKind::Prison => "Prison".into(),
            BuildingKind::Other(m) => format!("Building {m}"),
        }
    }
}

/// The levelled ground is never lower than this: a building placed on the shore stands on land.
pub const MIN_GROUND: u16 = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Building {
    pub kind: BuildingKind,
    /// Tribe 0-3 (other values as found in the level).
    pub owner: u8,
    /// World units, as placed in the level.
    pub x: u16,
    pub z: u16,
    /// Eighths of a turn (`Thing::facing`).
    pub facing: u8,
}

impl Building {
    /// Levels its footprint above the sea (`Heightmap::level_rect`), turned with its facing.
    pub fn flatten(&self, terrain: &mut crate::terrain::Heightmap) -> crate::terrain::DirtyRect {
        let f = self.kind.footprint();
        terrain.level_rect((self.x, self.z), f.half, f.offset, self.facing / 2, MIN_GROUND)
    }
}

/// The level's buildings, in thing order.
pub fn buildings_from_level(level: &Level) -> Vec<Building> {
    level
        .things
        .iter()
        .filter(|t| t.kind == KIND_BUILDING)
        .map(|t| Building { kind: BuildingKind::from_model(t.model), owner: t.owner, x: t.x, z: t.z, facing: t.facing() })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pop3_format::level::{DAT_SIZE, KIND_PERSON};

    #[test]
    fn models_to_kinds_and_names() {
        assert_eq!(BuildingKind::from_model(2), BuildingKind::Hut { size: 2 });
        assert_eq!(BuildingKind::from_model(4), BuildingKind::DrumTower);
        assert_eq!((BuildingKind::from_model(13), BuildingKind::from_model(14)), (BuildingKind::BoatHut, BuildingKind::BoatHut));
        assert_eq!(BuildingKind::from_model(42), BuildingKind::Other(42));
        assert_eq!(BuildingKind::Hut { size: 3 }.name(), "Hut 3");
        assert_eq!(BuildingKind::Other(42).name(), "Building 42");
    }

    #[test]
    fn only_building_things_become_buildings() {
        let mut d = vec![0u8; DAT_SIZE];
        let base = d.len() - 95 - 2000 * 55;
        d[base..base + 7].copy_from_slice(&[4, KIND_BUILDING, 2, 0x00, 0x0a, 0x00, 0x14]);
        d[base + 8] = 6;
        d[base + 55..base + 62].copy_from_slice(&[2, KIND_PERSON, 0, 0x00, 0x01, 0x00, 0x01]);
        let b = buildings_from_level(&Level::parse(&d).unwrap());
        assert_eq!(b, vec![Building { kind: BuildingKind::DrumTower, owner: 2, x: 0x0a00, z: 0x1400, facing: 6 }]);
    }
}
