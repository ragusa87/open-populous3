//! `levlXXXX.dat` / `levlXXXX.hdr` parser (Populous: The Beginning).

use std::fmt;
use std::path::Path;

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
const MISC_OFFSET: usize = LAYER4_OFFSET + MAP_CELLS;
const MISC_SIZE: usize = 122;
const THINGS_OFFSET: usize = MISC_OFFSET + MISC_SIZE;
pub const THING_SIZE: usize = 55;
pub const MAX_THINGS: usize = 2000;

/// `Thing::kind` values (subset, see docs/specs/level-format.md).
pub const KIND_PERSON: u8 = 1;
pub const KIND_BUILDING: u8 = 2;
pub const KIND_SCENERY: u8 = 5;
pub const KIND_GENERAL: u8 = 6;
/// `Thing::model` of a person that is the tribe's shaman.
pub const PERSON_SHAMAN: u8 = 7;

const HDR_NAME_OFFSET: usize = 56;
const HDR_NAME_LEN: usize = 32;
const HDR_THEME_OFFSET: usize = 96;

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

/// A placed object (tribe member, building, tree, trigger...). Only the first
/// bytes are understood; the rest is kept raw for later reverse engineering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thing {
    pub model: u8,
    pub kind: u8,
    pub owner: u8,
    /// Position in world units (512 per cell), wraps at 65536.
    pub x: u16,
    pub z: u16,
    pub raw: [u8; THING_SIZE],
}

impl Thing {
    fn parse(b: &[u8]) -> Self {
        Thing {
            model: b[0],
            kind: b[1],
            owner: b[2],
            x: u16::from_le_bytes([b[3], b[4]]),
            z: u16::from_le_bytes([b[5], b[6]]),
            raw: b.try_into().expect("thing slice is THING_SIZE"),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.kind == 0
    }

    pub fn is_shaman(&self) -> bool {
        self.kind == KIND_PERSON && self.model == PERSON_SHAMAN
    }
}

/// Parsed `.dat` file. Cell `(x, z)` is at index `z * MAP_SIZE + x`.
#[derive(Clone, Debug)]
pub struct Level {
    /// Ground height per cell, 0 = sea level, observed max 1024.
    pub heights: Vec<u16>,
    pub layer2: Vec<u8>,
    pub layer3: Vec<u8>,
    pub no_access: Vec<u8>,
    pub misc: Vec<u8>,
    /// Non-empty things only.
    pub things: Vec<Thing>,
}

impl Level {
    pub fn parse(data: &[u8]) -> Result<Self, LevelError> {
        if data.len() < THINGS_OFFSET + THING_SIZE * MAX_THINGS {
            return Err(LevelError::BadSize { expected: DAT_SIZE, got: data.len() });
        }
        let heights = data[HEIGHTS_OFFSET..LAYER2_OFFSET]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let things = data[THINGS_OFFSET..THINGS_OFFSET + THING_SIZE * MAX_THINGS]
            .chunks_exact(THING_SIZE)
            .map(Thing::parse)
            .filter(|t| !t.is_empty())
            .collect();
        Ok(Level {
            heights,
            layer2: data[LAYER2_OFFSET..LAYER3_OFFSET].to_vec(),
            layer3: data[LAYER3_OFFSET..LAYER4_OFFSET].to_vec(),
            no_access: data[LAYER4_OFFSET..MISC_OFFSET].to_vec(),
            misc: data[MISC_OFFSET..THINGS_OFFSET].to_vec(),
            things,
        })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, LevelError> {
        Self::parse(&std::fs::read(path)?)
    }

    pub fn height(&self, x: usize, z: usize) -> u16 {
        self.heights[(z % MAP_SIZE) * MAP_SIZE + (x % MAP_SIZE)]
    }
}

/// Parsed `.hdr` file (616 bytes). Only name and landscape theme are decoded for now.
#[derive(Clone, Debug, Default)]
pub struct LevelHeader {
    pub name: String,
    /// Landscape theme index, see `theme::theme_char`.
    pub theme: u8,
    pub raw: Vec<u8>,
}

impl LevelHeader {
    pub fn parse(data: &[u8]) -> Self {
        let name = data
            .get(HDR_NAME_OFFSET..HDR_NAME_OFFSET + HDR_NAME_LEN)
            .map(|b| {
                let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
                String::from_utf8_lossy(&b[..end]).into_owned()
            })
            .unwrap_or_default();
        let theme = data.get(HDR_THEME_OFFSET).copied().unwrap_or(0);
        LevelHeader { name, theme, raw: data.to_vec() }
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, LevelError> {
        Ok(Self::parse(&std::fs::read(path)?))
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
    fn rejects_short_file() {
        assert!(matches!(Level::parse(&[0; 10]), Err(LevelError::BadSize { .. })));
    }

    #[test]
    fn header_name() {
        let mut h = vec![0u8; 616];
        h[HDR_NAME_OFFSET..HDR_NAME_OFFSET + 7].copy_from_slice(b"Level 1");
        h[HDR_THEME_OFFSET] = 12;
        let hdr = LevelHeader::parse(&h);
        assert_eq!((hdr.name.as_str(), hdr.theme), ("Level 1", 12));
    }
}
