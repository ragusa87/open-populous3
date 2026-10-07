//! `levlXXXX.dat` / `levlXXXX.hdr` parser (Populous: The Beginning).

use std::fmt;
use std::path::Path;

pub use crate::header::{LevelFlags, LevelHeader, LevelVersion};

/// The map is a square of `MAP_SIZE` x `MAP_SIZE` cells, wrapping on both axes.
pub const MAP_SIZE: usize = 128;
pub const MAP_CELLS: usize = MAP_SIZE * MAP_SIZE;
/// Number of world units per cell edge in the original engine (thing coordinates).
pub const WORLD_UNITS_PER_CELL: u32 = 512;

pub const DAT_SIZE: usize = 192_137;
const HEIGHTS_OFFSET: usize = 0;
const LAYER2_OFFSET: usize = MAP_CELLS * 2;
const LAYER3_OFFSET: usize = LAYER2_OFFSET + MAP_CELLS;
const LAYER4_OFFSET: usize = LAYER3_OFFSET + MAP_CELLS;
const START_INFO_OFFSET: usize = LAYER4_OFFSET + MAP_CELLS;
const START_INFO_SIZE: usize = 16;
pub const PLAYERS: usize = 4;
const SUNLIGHT_OFFSET: usize = START_INFO_OFFSET + START_INFO_SIZE * PLAYERS;
const THINGS_OFFSET: usize = SUNLIGHT_OFFSET + 3;
pub const THING_SIZE: usize = 55;
pub const MAX_THINGS: usize = 2000;
const ACCESS_OFFSET: usize = THINGS_OFFSET + THING_SIZE * MAX_THINGS;
pub const ACCESS_ENTRIES: usize = 50;

/// `Thing::kind` values (subset, see docs/specs/level-format.md).
pub const KIND_PERSON: u8 = 1;
pub const KIND_BUILDING: u8 = 2;
pub const KIND_SCENERY: u8 = 5;
pub const KIND_GENERAL: u8 = 6;
pub const KIND_EFFECT: u8 = 7;
/// General models with a decoded union.
pub const GENERAL_DISCOVERY: u8 = 2;
pub const GENERAL_TRIGGER: u8 = 6;
/// Effect models that store a target (land bridge, lightning bolt).
pub const EFFECT_LIGHTNING_BOLT: u8 = 17;
pub const EFFECT_LAND_BRIDGE: u8 = 24;
/// A full turn in thing angles.
pub const FULL_TURN: u16 = 2048;
/// Trigger target slots per trigger.
pub const TRIGGER_TARGETS: usize = 10;
/// `Thing::model` of a person that is the tribe's shaman.
pub const PERSON_SHAMAN: u8 = 7;
/// Scenery models 1-6 are trees (level 19: 1 cone pines, 2 weeping trees, as seen in the game).
pub const SCENERY_TREES: std::ops::RangeInclusive<u8> = 1..=6;

#[derive(Debug)]
pub enum LevelError {
    Io(std::io::Error),
    BadSize { expected: usize, got: usize },
}

impl fmt::Display for LevelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LevelError::Io(e) => write!(f, "io error: {e}"),
            LevelError::BadSize { expected, got } => {
                write!(f, "bad level size: expected {expected} bytes, got {got}")
            }
        }
    }
}

impl std::error::Error for LevelError {}

impl From<std::io::Error> for LevelError {
    fn from(e: std::io::Error) -> Self {
        LevelError::Io(e)
    }
}

/// What a discovery thing grants and for how long (general model 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    Permanent,
    ThisLevel,
    Once,
    Unknown(u8),
}

impl Availability {
    pub fn from_byte(b: u8) -> Self {
        match b {
            1 => Availability::Permanent,
            2 => Availability::ThisLevel,
            3 => Availability::Once,
            n => Availability::Unknown(n),
        }
    }
}

/// General model 2: what is discovered when a trigger activates it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Discovery {
    /// A thing kind: 11 spell, 2 building, 6 mana.
    pub kind: u8,
    /// The spell or building model (3 for mana).
    pub model: u8,
    pub availability: Availability,
    /// 0 normal, 1 immediate (always 1 in the files).
    pub trigger_type: u8,
    pub mana: i32,
}

/// General model 6: activates its targets when its condition is met (semantics not implemented).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trigger {
    /// 0 proximity, 1 timed, 2 player death, 3 shaman proximity, 4 library, 5 shaman + angel of death.
    pub trigger_type: u8,
    pub cell_radius: u8,
    pub random_value: u8,
    pub occurrences: i8,
    pub count: u16,
    /// Things to activate: **1-based slot indices** (`Level::slot`), 0 = none.
    pub targets: [u16; TRIGGER_TARGETS],
    pub pray_time: i16,
    pub start_inactive: u8,
    pub create_player_owned: u8,
    pub inactive_time: i16,
}

impl Trigger {
    /// The non-empty target slots (1-based).
    pub fn target_slots(&self) -> impl Iterator<Item = u16> + '_ {
        self.targets.iter().copied().filter(|&t| t != 0)
    }
}

/// The decoded type-specific part of a thing (bytes 7-54), when known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThingData {
    Discovery(Discovery),
    Trigger(Trigger),
    /// Land bridge / lightning bolt target, world units.
    EffectTarget { x: u16, z: u16 },
    None,
}

/// A placed object (tribe member, building, tree, trigger...). The type-specific union is decoded
/// by `angle` and `data`; the record is kept raw too.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thing {
    /// 0-based slot in the file (trigger targets are `slot + 1`).
    pub slot: u16,
    pub model: u8,
    pub kind: u8,
    pub owner: u8,
    /// Position in world units (512 per cell), wraps at 65536.
    pub x: u16,
    pub z: u16,
    pub raw: [u8; THING_SIZE],
}

impl Thing {
    fn parse(slot: usize, b: &[u8]) -> Self {
        Thing {
            slot: slot as u16,
            model: b[0],
            kind: b[1],
            owner: b[2],
            x: u16::from_le_bytes([b[3], b[4]]),
            z: u16::from_le_bytes([b[5], b[6]]),
            raw: b.try_into().expect("thing slice is THING_SIZE"),
        }
    }

    fn u8_at(&self, o: usize) -> u8 {
        self.raw[o]
    }

    fn u16_at(&self, o: usize) -> u16 {
        u16::from_le_bytes([self.raw[o], self.raw[o + 1]])
    }

    fn i32_at(&self, o: usize) -> i32 {
        i32::from_le_bytes(self.raw[o..o + 4].try_into().unwrap())
    }

    pub fn is_empty(&self) -> bool {
        self.kind == 0
    }

    pub fn is_shaman(&self) -> bool {
        self.kind == KIND_PERSON && self.model == PERSON_SHAMAN
    }

    /// Rotation about the vertical axis in 2048ths of a turn: buildings `i32@7`, scenery `i16@10`.
    /// None for the other kinds (persons and vehicles store no angle).
    pub fn angle(&self) -> Option<u16> {
        match self.kind {
            KIND_BUILDING => Some(self.i32_at(7).rem_euclid(FULL_TURN as i32) as u16),
            KIND_SCENERY => Some((self.u16_at(10) as i16 as i32).rem_euclid(FULL_TURN as i32) as u16),
            _ => None,
        }
    }

    /// The angle rounded to eighths of a turn (0-7), 0 without one. Buildings only use quarter
    /// turns (0, 2, 4, 6); which way is 0 is not checked against the game yet.
    pub fn facing(&self) -> u8 {
        angle_to_eighths(self.angle().unwrap_or(0))
    }

    /// The decoded union of discoveries, triggers and targeted effects.
    pub fn data(&self) -> ThingData {
        match (self.kind, self.model) {
            (KIND_GENERAL, GENERAL_DISCOVERY) => ThingData::Discovery(Discovery {
                kind: self.u8_at(7),
                model: self.u8_at(8),
                availability: Availability::from_byte(self.u8_at(9)),
                trigger_type: self.u8_at(10),
                mana: self.i32_at(11),
            }),
            (KIND_GENERAL, GENERAL_TRIGGER) => ThingData::Trigger(Trigger {
                trigger_type: self.u8_at(7),
                cell_radius: self.u8_at(8),
                random_value: self.u8_at(9),
                occurrences: self.u8_at(10) as i8,
                count: self.u16_at(11),
                targets: std::array::from_fn(|i| self.u16_at(13 + 2 * i)),
                pray_time: self.u16_at(33) as i16,
                start_inactive: self.u8_at(35),
                create_player_owned: self.u8_at(36),
                inactive_time: self.u16_at(37) as i16,
            }),
            (KIND_EFFECT, EFFECT_LIGHTNING_BOLT | EFFECT_LAND_BRIDGE) => ThingData::EffectTarget { x: self.i32_at(7) as u16, z: self.i32_at(11) as u16 },
            _ => ThingData::None,
        }
    }

    /// The tree type (0-5, scenery model 1-6) if this is a tree.
    pub fn tree_type(&self) -> Option<u8> {
        (self.kind == KIND_SCENERY && SCENERY_TREES.contains(&self.model)).then(|| self.model - 1)
    }
}

/// Per-player start info (a placeholder in every shipped level: (1, 1)..(4, 4), futures 0).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StartInfo {
    pub x: i16,
    pub z: i16,
    pub future: [i32; 3],
}

impl StartInfo {
    fn parse(b: &[u8]) -> Self {
        let i32_at = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        StartInfo { x: i16::from_le_bytes([b[0], b[1]]), z: i16::from_le_bytes([b[2], b[3]]), future: [i32_at(4), i32_at(8), i32_at(12)] }
    }
}

/// Sunlight block (always 28, 15, 64 or 32 in the shipped levels; meaning not confirmed).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sunlight {
    pub shade_start: u8,
    pub shade_range: u8,
    pub inclination: u8,
}

/// Access rights entry (all zero in the shipped levels).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AccessInfo {
    pub model: u8,
    pub kind: u8,
    pub rights: u8,
}

/// 2048ths of a turn to the nearest eighth (0-7).
pub fn angle_to_eighths(angle: u16) -> u8 {
    (((angle as u32 % FULL_TURN as u32) + 128) / 256 % 8) as u8
}

/// Parsed `.dat` file. Cell `(x, z)` is at index `z * MAP_SIZE + x`.
#[derive(Clone, Debug)]
pub struct Level {
    /// Ground height per cell, 0 = sea level, observed max 1024.
    pub heights: Vec<u16>,
    pub layer2: Vec<u8>,
    pub layer3: Vec<u8>,
    pub no_access: Vec<u8>,
    pub start_info: [StartInfo; PLAYERS],
    pub sunlight: Sunlight,
    /// Non-empty things only.
    pub things: Vec<Thing>,
    pub access: Vec<AccessInfo>,
}

impl Level {
    pub fn parse(data: &[u8]) -> Result<Self, LevelError> {
        if data.len() < DAT_SIZE {
            return Err(LevelError::BadSize { expected: DAT_SIZE, got: data.len() });
        }
        let heights = data[HEIGHTS_OFFSET..LAYER2_OFFSET]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let things = data[THINGS_OFFSET..THINGS_OFFSET + THING_SIZE * MAX_THINGS]
            .chunks_exact(THING_SIZE)
            .enumerate()
            .map(|(slot, b)| Thing::parse(slot, b))
            .filter(|t| !t.is_empty())
            .collect();
        Ok(Level {
            heights,
            layer2: data[LAYER2_OFFSET..LAYER3_OFFSET].to_vec(),
            layer3: data[LAYER3_OFFSET..LAYER4_OFFSET].to_vec(),
            no_access: data[LAYER4_OFFSET..START_INFO_OFFSET].to_vec(),
            start_info: std::array::from_fn(|i| StartInfo::parse(&data[START_INFO_OFFSET + i * START_INFO_SIZE..][..START_INFO_SIZE])),
            sunlight: Sunlight { shade_start: data[SUNLIGHT_OFFSET], shade_range: data[SUNLIGHT_OFFSET + 1], inclination: data[SUNLIGHT_OFFSET + 2] },
            things,
            access: data[ACCESS_OFFSET..ACCESS_OFFSET + ACCESS_ENTRIES * 3].chunks_exact(3).map(|c| AccessInfo { model: c[0], kind: c[1], rights: c[2] }).collect(),
        })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, LevelError> {
        Self::parse(&std::fs::read(path)?)
    }

    /// The thing a trigger target points at (1-based slot index), if that slot is not empty.
    pub fn slot(&self, one_based: u16) -> Option<&Thing> {
        let slot = one_based.checked_sub(1)?;
        self.things.binary_search_by_key(&slot, |t| t.slot).ok().map(|i| &self.things[i])
    }

    pub fn height(&self, x: usize, z: usize) -> u16 {
        self.heights[(z % MAP_SIZE) * MAP_SIZE + (x % MAP_SIZE)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic() -> Vec<u8> {
        let mut d = vec![0u8; DAT_SIZE];
        let idx = 3 * MAP_SIZE + 5;
        d[idx * 2..idx * 2 + 2].copy_from_slice(&700u16.to_le_bytes());
        let t = THINGS_OFFSET + THING_SIZE * 2;
        d[t..t + 7].copy_from_slice(&[1, 2, 0, 0x00, 0x03, 0x00, 0xfb]);
        d
    }

    #[test]
    fn parses_heights_and_things() {
        let lvl = Level::parse(&synthetic()).unwrap();
        assert_eq!(lvl.height(5, 3), 700);
        assert_eq!(lvl.height(5 + MAP_SIZE, 3), 700, "wraps");
        assert_eq!(lvl.things.len(), 1);
        let t = &lvl.things[0];
        assert_eq!((t.model, t.kind, t.x, t.z), (1, 2, 0x0300, 0xfb00));
    }

    #[test]
    fn shaman_is_person_model_7() {
        let mut d = synthetic();
        let t = THINGS_OFFSET + THING_SIZE * 5;
        d[t..t + 3].copy_from_slice(&[PERSON_SHAMAN, KIND_PERSON, 1]);
        let lvl = Level::parse(&d).unwrap();
        let shamans: Vec<_> = lvl.things.iter().filter(|t| t.is_shaman()).collect();
        assert_eq!(shamans.len(), 1);
        assert_eq!(shamans[0].owner, 1);
    }

    #[test]
    fn scenery_models_1_to_6_are_trees() {
        let mut d = synthetic();
        for (i, model) in [1u8, 6, 7].iter().enumerate() {
            let t = THINGS_OFFSET + THING_SIZE * (10 + i);
            d[t..t + 2].copy_from_slice(&[*model, KIND_SCENERY]);
        }
        let lvl = Level::parse(&d).unwrap();
        let types: Vec<_> = lvl.things.iter().map(Thing::tree_type).collect();
        assert_eq!(types, vec![None, Some(0), Some(5), None], "the building first, then trees, then a plant");
    }

    #[test]
    fn layout_matches_the_spec() {
        assert_eq!((START_INFO_OFFSET, SUNLIGHT_OFFSET, THINGS_OFFSET, ACCESS_OFFSET), (81_920, 81_984, 81_987, 191_987));
        assert_eq!(ACCESS_OFFSET + ACCESS_ENTRIES * 3, DAT_SIZE);
    }

    #[test]
    fn thing_slot_0_and_blocks_around_things() {
        let mut d = synthetic();
        d[THINGS_OFFSET..THINGS_OFFSET + 2].copy_from_slice(&[2, KIND_BUILDING]);
        d[START_INFO_OFFSET + 16..START_INFO_OFFSET + 20].copy_from_slice(&[2, 0, 3, 0]);
        d[SUNLIGHT_OFFSET..SUNLIGHT_OFFSET + 3].copy_from_slice(&[28, 15, 64]);
        d[ACCESS_OFFSET + 3..ACCESS_OFFSET + 6].copy_from_slice(&[4, 2, 1]);
        let lvl = Level::parse(&d).unwrap();
        assert_eq!((lvl.things[0].model, lvl.things[0].kind), (2, KIND_BUILDING), "slot 0 is kept");
        assert_eq!((lvl.start_info[1].x, lvl.start_info[1].z), (2, 3));
        assert_eq!(lvl.sunlight, Sunlight { shade_start: 28, shade_range: 15, inclination: 64 });
        assert_eq!((lvl.access.len(), lvl.access[1]), (ACCESS_ENTRIES, AccessInfo { model: 4, kind: 2, rights: 1 }));
    }

    fn with_thing(slot: usize, bytes: &[(usize, u8)]) -> Thing {
        let mut d = vec![0u8; DAT_SIZE];
        let t = THINGS_OFFSET + THING_SIZE * slot;
        for &(o, b) in bytes {
            d[t + o] = b;
        }
        Level::parse(&d).unwrap().things.into_iter().find(|t| t.slot as usize == slot).unwrap()
    }

    #[test]
    fn building_and_scenery_angles() {
        let hut = with_thing(0, &[(0, 2), (1, KIND_BUILDING), (8, 0x06)]);
        assert_eq!((hut.angle(), hut.facing()), (Some(1536), 6));
        let tree = with_thing(0, &[(0, 1), (1, KIND_SCENERY), (8, 0x06), (10, 0x00), (11, 0x02)]);
        assert_eq!((tree.angle(), tree.facing()), (Some(512), 2), "scenery angle is i16@10, not byte 8");
        let person = with_thing(0, &[(0, 2), (1, KIND_PERSON), (8, 0x06)]);
        assert_eq!((person.angle(), person.facing()), (None, 0));
    }

    #[test]
    fn angle_rounds_to_the_nearest_eighth() {
        assert_eq!([0, 127, 128, 1024, 1900, 2047].map(angle_to_eighths), [0, 0, 1, 4, 7, 0]);
    }

    #[test]
    fn discovery_union() {
        let t = with_thing(3, &[(0, GENERAL_DISCOVERY), (1, KIND_GENERAL), (7, 6), (8, 3), (9, 2), (10, 1), (11, 0x50), (12, 0xc3)]);
        let ThingData::Discovery(d) = t.data() else { panic!("{:?}", t.data()) };
        assert_eq!((d.kind, d.model, d.availability, d.trigger_type, d.mana), (6, 3, Availability::ThisLevel, 1, 50_000));
    }

    #[test]
    fn trigger_union_and_one_based_slots() {
        let mut d = vec![0u8; DAT_SIZE];
        let put = |d: &mut Vec<u8>, slot: usize, bytes: &[(usize, u8)]| {
            for &(o, b) in bytes {
                d[THINGS_OFFSET + THING_SIZE * slot + o] = b;
            }
        };
        put(&mut d, 0, &[(0, GENERAL_DISCOVERY), (1, KIND_GENERAL)]);
        put(&mut d, 2, &[(0, GENERAL_TRIGGER), (1, KIND_GENERAL), (7, 3), (8, 1), (10, 0xff), (11, 2), (13, 1), (15, 3), (33, 0xe8), (34, 0x03), (37, 0x00), (38, 0x03)]);
        let lvl = Level::parse(&d).unwrap();
        let trigger = lvl.slot(3).unwrap();
        let ThingData::Trigger(t) = trigger.data() else { panic!() };
        assert_eq!((t.trigger_type, t.cell_radius, t.occurrences, t.count, t.pray_time, t.inactive_time), (3, 1, -1, 2, 1000, 768));
        let targets: Vec<_> = t.target_slots().map(|s| lvl.slot(s).map(|t| t.kind)).collect();
        assert_eq!(targets, vec![Some(KIND_GENERAL), Some(KIND_GENERAL)], "slot 1 is the discovery, slot 3 the trigger itself");
        assert_eq!(lvl.slot(2), None, "empty slot");
        assert_eq!(lvl.slot(0), None, "0 = no target");
    }

    #[test]
    fn effect_target() {
        let t = with_thing(0, &[(0, EFFECT_LAND_BRIDGE), (1, KIND_EFFECT), (8, 0xcb), (9, 0xff), (10, 0xff), (12, 0x04)]);
        assert_eq!(t.data(), ThingData::EffectTarget { x: 0xcb00, z: 0x0400 });
        assert_eq!(with_thing(0, &[(0, 1), (1, KIND_EFFECT)]).data(), ThingData::None);
    }

    #[test]
    fn rejects_short_file() {
        assert!(matches!(Level::parse(&[0; 10]), Err(LevelError::BadSize { .. })));
    }
}
