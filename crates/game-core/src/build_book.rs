//! A tribe's buildable kinds (the Build tab): which ones it may build, which it must discover first.
//! Construction itself comes later (docs/specs/buildings.md).

use crate::building::BuildingKind;
use pop3_format::level::{ThingData, KIND_BUILDING};
use pop3_format::{Level, LevelHeader};

/// The original panel's buildings, in its order: hut (placed at size 1), drum tower, temple, spy,
/// warrior and firewarrior training, boat hut, airship hut.
pub const BUILDABLE: [BuildingKind; 8] = [
    BuildingKind::Hut { size: 1 },
    BuildingKind::DrumTower,
    BuildingKind::Temple,
    BuildingKind::SpyTraining,
    BuildingKind::WarriorTraining,
    BuildingKind::FirewarriorTraining,
    BuildingKind::BoatHut,
    BuildingKind::AirshipHut,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildAvailability {
    /// Not in this level.
    Hidden,
    /// Shown as "?": its plans must be discovered first.
    Discoverable,
    Available,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildSlot {
    pub kind: BuildingKind,
    pub availability: BuildAvailability,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildBook {
    pub slots: Vec<BuildSlot>,
}

/// The panel kind a level model stands for: any hut size is the hut (built at size 1); others as is.
fn panel_kind(model: u8) -> Option<BuildingKind> {
    let kind = match BuildingKind::from_model(model) {
        BuildingKind::Hut { .. } => BuildingKind::Hut { size: 1 },
        k => k,
    };
    BUILDABLE.contains(&kind).then_some(kind)
}

/// The header models that make `kind` available: hut 1, the boat and airship huts' first model.
fn header_model(kind: BuildingKind) -> u8 {
    match kind {
        BuildingKind::Hut { .. } => 1,
        BuildingKind::DrumTower => 4,
        BuildingKind::Temple => 5,
        BuildingKind::SpyTraining => 6,
        BuildingKind::WarriorTraining => 7,
        BuildingKind::FirewarriorTraining => 8,
        BuildingKind::BoatHut => 13,
        BuildingKind::AirshipHut => 15,
        _ => 0,
    }
}

impl BuildBook {
    /// Every panel building, all with the same availability.
    pub fn all(availability: BuildAvailability) -> Self {
        BuildBook { slots: BUILDABLE.iter().map(|&kind| BuildSlot { kind, availability }).collect() }
    }

    /// An original level's: the header's `BuildingsAvailable` kinds (bit N = model N) are
    /// available, the buildings of its discovery things (`DiscoveryType 2`) not yet available are "?".
    pub fn from_level(header: &LevelHeader, level: &Level) -> Self {
        let mut book = BuildBook::all(BuildAvailability::Hidden);
        let discoveries = level.things.iter().filter_map(|t| match t.data() {
            ThingData::Discovery(d) if d.kind == KIND_BUILDING => panel_kind(d.model),
            _ => None,
        });
        for kind in discoveries {
            book.set(kind, BuildAvailability::Discoverable);
        }
        for kind in BUILDABLE {
            if header.building_available(header_model(kind)) {
                book.set(kind, BuildAvailability::Available);
            }
        }
        book
    }

    pub fn slot(&self, kind: BuildingKind) -> Option<&BuildSlot> {
        self.slots.iter().find(|s| s.kind == kind)
    }

    pub fn set(&mut self, kind: BuildingKind, availability: BuildAvailability) {
        if let Some(s) = self.slots.iter_mut().find(|s| s.kind == kind) {
            s.availability = availability;
        }
    }

    /// The plans are found: "?" becomes available.
    pub fn discover(&mut self, kind: BuildingKind) {
        if self.slot(kind).is_some_and(|s| s.availability == BuildAvailability::Discoverable) {
            self.set(kind, BuildAvailability::Available);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pop3_format::level::{DAT_SIZE, GENERAL_DISCOVERY, KIND_GENERAL, KIND_SPELL};

    fn header(models: &[u8]) -> LevelHeader {
        let mask = models.iter().fold(0xfffe_0001u32, |m, &b| m | 1 << b);
        let mut hdr = vec![0u8; 616];
        hdr[4..8].copy_from_slice(&mask.to_le_bytes());
        LevelHeader::parse(&hdr)
    }

    fn level(discoveries: &[(u8, u8)]) -> Level {
        let mut d = vec![0u8; DAT_SIZE];
        for (i, &(kind, model)) in discoveries.iter().enumerate() {
            let t = 81_987 + i * 55;
            d[t..t + 2].copy_from_slice(&[GENERAL_DISCOVERY, KIND_GENERAL]);
            d[t + 7..t + 10].copy_from_slice(&[kind, model, 1]);
        }
        Level::parse(&d).unwrap()
    }

    #[test]
    fn level_1_huts_and_the_warrior_hut_to_discover() {
        let book = BuildBook::from_level(&header(&[1, 2, 3, 18, 19]), &level(&[(KIND_BUILDING, 7), (KIND_SPELL, 7)]));
        let avail = |k| book.slot(k).unwrap().availability;
        assert_eq!(avail(BuildingKind::Hut { size: 1 }), BuildAvailability::Available);
        assert_eq!(avail(BuildingKind::WarriorTraining), BuildAvailability::Discoverable);
        assert_eq!(avail(BuildingKind::Temple), BuildAvailability::Hidden, "a spell discovery of model 7 is not a building");
        assert_eq!(book.slots.len(), BUILDABLE.len(), "vault and prison are never built");
    }

    #[test]
    fn boat_and_airship_huts_from_their_first_model() {
        let book = BuildBook::from_level(&header(&[13, 16]), &level(&[(KIND_BUILDING, 15)]));
        assert_eq!(book.slot(BuildingKind::BoatHut).unwrap().availability, BuildAvailability::Available);
        assert_eq!(book.slot(BuildingKind::AirshipHut).unwrap().availability, BuildAvailability::Discoverable);
    }

    #[test]
    fn available_wins_over_a_discovery_and_discover_unlocks() {
        let mut book = BuildBook::from_level(&header(&[4]), &level(&[(KIND_BUILDING, 4), (KIND_BUILDING, 3), (KIND_BUILDING, 6)]));
        assert_eq!(book.slot(BuildingKind::DrumTower).unwrap().availability, BuildAvailability::Available);
        assert_eq!(book.slot(BuildingKind::Hut { size: 1 }).unwrap().availability, BuildAvailability::Discoverable, "any hut size");
        book.discover(BuildingKind::SpyTraining);
        assert_eq!(book.slot(BuildingKind::SpyTraining).unwrap().availability, BuildAvailability::Available);
    }
}
