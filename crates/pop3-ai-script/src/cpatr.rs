//! `cpatrNNN.dat` (144 bytes): a computer player's starting attributes, name and masks (docs/specs/
//! ai-scripts.md "`cpatr` layout").

use std::fmt;

pub const DAT_SIZE: usize = 144;
pub const ATTRIBUTE_COUNT: usize = 48;

const NAME_OFFSET: usize = 48;
const NAME_LEN: usize = 32;
const STARTING_SPELLS_OFFSET: usize = 80;
const STARTING_BUILDINGS_OFFSET: usize = 84;
const VEHICLES_OFFSET: usize = 132;
const TRAINING_MANA_OFF_OFFSET: usize = 134;
const FLAGS_OFFSET: usize = 135;
const SPARE1_OFFSET: usize = 136;
const SPARE2_OFFSET: usize = 140;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpatrError {
    BadSize { expected: usize, found: usize },
}

impl fmt::Display for CpatrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CpatrError::BadSize { expected, found } => write!(f, "expected {expected} bytes, found {found}"),
        }
    }
}

impl std::error::Error for CpatrError {}

fn c_string(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

/// A computer player's starting setup, same numbers as its `cpscrNNN.dat` script number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputerPlayerAttrs {
    /// Bytes 33-47 always 0; the editor's own names for these (`INT_ATTR_*`) don't fit the
    /// observed values, so this crate exposes the raw bytes rather than guessing a mapping.
    pub attributes: [u8; ATTRIBUTE_COUNT],
    /// E.g. "Doc 010".
    pub name: String,
    /// Bit N = spell model N available from the start.
    pub starting_spells: u32,
    /// Bit N = building model N available from the start.
    pub starting_buildings: u32,
    /// Bit N = vehicle model N available from the start.
    pub vehicles: u16,
    pub training_mana_off: u8,
    pub flags: u8,
    /// The file's own number in most files (10 in the stubs, 99 in `cpatr081`).
    pub spare1: u32,
    pub spare2: u32,
}

impl ComputerPlayerAttrs {
    pub fn parse(data: &[u8]) -> Result<Self, CpatrError> {
        if data.len() != DAT_SIZE {
            return Err(CpatrError::BadSize { expected: DAT_SIZE, found: data.len() });
        }
        let mut attributes = [0u8; ATTRIBUTE_COUNT];
        attributes.copy_from_slice(&data[0..ATTRIBUTE_COUNT]);
        let u32_at = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
        Ok(ComputerPlayerAttrs {
            attributes,
            name: c_string(&data[NAME_OFFSET..NAME_OFFSET + NAME_LEN]),
            starting_spells: u32_at(STARTING_SPELLS_OFFSET),
            starting_buildings: u32_at(STARTING_BUILDINGS_OFFSET),
            vehicles: u16::from_le_bytes([data[VEHICLES_OFFSET], data[VEHICLES_OFFSET + 1]]),
            training_mana_off: data[TRAINING_MANA_OFF_OFFSET],
            flags: data[FLAGS_OFFSET],
            spare1: u32_at(SPARE1_OFFSET),
            spare2: u32_at(SPARE2_OFFSET),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic() -> Vec<u8> {
        vec![0u8; DAT_SIZE]
    }

    #[test]
    fn decodes_name_masks_and_spares() {
        let mut d = synthetic();
        d[NAME_OFFSET..NAME_OFFSET + 7].copy_from_slice(b"Doc 010");
        d[STARTING_SPELLS_OFFSET..STARTING_SPELLS_OFFSET + 4].copy_from_slice(&0x20404u32.to_le_bytes());
        d[STARTING_BUILDINGS_OFFSET..STARTING_BUILDINGS_OFFSET + 4].copy_from_slice(&146u32.to_le_bytes());
        d[VEHICLES_OFFSET..VEHICLES_OFFSET + 2].copy_from_slice(&30u16.to_le_bytes());
        d[TRAINING_MANA_OFF_OFFSET] = 1;
        d[FLAGS_OFFSET] = 2;
        d[SPARE1_OFFSET..SPARE1_OFFSET + 4].copy_from_slice(&10u32.to_le_bytes());
        let attrs = ComputerPlayerAttrs::parse(&d).unwrap();
        assert_eq!(attrs.name, "Doc 010");
        assert_eq!(attrs.starting_spells, 0x20404);
        assert_eq!(attrs.starting_buildings, 146);
        assert_eq!(attrs.vehicles, 30);
        assert_eq!((attrs.training_mana_off, attrs.flags), (1, 2));
        assert_eq!(attrs.spare1, 10);
        assert_eq!(attrs.spare2, 0);
    }

    #[test]
    fn rejects_wrong_size() {
        assert_eq!(ComputerPlayerAttrs::parse(&[0; 10]), Err(CpatrError::BadSize { expected: DAT_SIZE, found: 10 }));
    }
}
