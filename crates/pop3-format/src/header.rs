//! `levlXXXX.hdr` (level settings, 616 bytes) and `levlXXXX.ver` (author, 68 bytes) parsers.
//! Layout in docs/specs/level-format.md.

use crate::level::LevelError;
use std::path::Path;

pub const HDR_SIZE: usize = 616;
pub const VER_SIZE: usize = 68;
/// Tribes in a level, in owner order: blue, red, yellow, green.
pub const TRIBES: usize = 4;
pub const MARKERS: usize = 256;

const NAME_OFFSET: usize = 56;
const NAME_LEN: usize = 32;
const AI_SCRIPTS_OFFSET: usize = 89;
const ALLIES_OFFSET: usize = 92;
const MARKERS_OFFSET: usize = 100;

/// Level flags (`.hdr` byte 98).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LevelFlags(pub u8);

impl LevelFlags {
    pub const FOG_OF_WAR: u8 = 0x01;
    /// "God mode" in the editor (level 25).
    pub const SHAMAN_OMNI: u8 = 0x02;
    pub const FORCE_640X480: u8 = 0x04;
    pub const LEVEL_EDIT: u8 = 0x08;
    pub const NO_GUEST_SPELLS: u8 = 0x10;
    pub const NO_REINCARNATION_TIME: u8 = 0x20;

    pub fn has(self, flag: u8) -> bool {
        self.0 & flag != 0
    }
}

/// Parsed `.hdr` file. Masks: bit N = model N; the bits that are not models are always set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelHeader {
    pub spells: u32,
    pub buildings: u32,
    pub buildings_level: u32,
    pub buildings_once: u32,
    pub spells_level: u32,
    pub spells_once: [u8; 32],
    pub vehicles: u16,
    pub training_mana_off: u8,
    pub options: u8,
    pub name: String,
    /// Number of tribes playing (2-4).
    pub tribes: u8,
    /// AI script number (`cpscrNNN.dat`) per tribe in owner order, 0 = none. Junk beyond `tribes`.
    pub ai_scripts: [u8; TRIBES],
    /// Per tribe, bitmask of allied tribes (1 blue, 2 red, 4 yellow, 8 green), own bit included.
    pub allies: [u8; TRIBES],
    /// Landscape theme index, see `theme::theme_char`.
    pub theme: u8,
    /// Object bank (`objects.md`).
    pub object_bank: u8,
    pub flags: LevelFlags,
    /// Raw markers: low byte = x * 2, high byte = z * 2 (`cell_of`).
    pub markers: [u16; MARKERS],
    /// Start camera position, encoded like the markers.
    pub start: u16,
    /// Start camera angle, 2048ths of a turn.
    pub start_angle: u16,
}

impl Default for LevelHeader {
    fn default() -> Self {
        Self::parse(&[])
    }
}

/// The cell `(x, z)` of a marker or start position (low byte x * 2, high byte z * 2).
pub fn cell_of(packed: u16) -> (u8, u8) {
    ((packed & 0xff) as u8 / 2, (packed >> 8) as u8 / 2)
}

fn c_string(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).trim_end().to_owned()
}

impl LevelHeader {
    /// Short files are zero-padded.
    pub fn parse(data: &[u8]) -> Self {
        let mut d = [0u8; HDR_SIZE];
        let n = data.len().min(HDR_SIZE);
        d[..n].copy_from_slice(&data[..n]);
        let u16_at = |o: usize| u16::from_le_bytes([d[o], d[o + 1]]);
        let u32_at = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let ai = &d[AI_SCRIPTS_OFFSET..AI_SCRIPTS_OFFSET + 3];
        LevelHeader {
            spells: u32_at(0),
            buildings: u32_at(4),
            buildings_level: u32_at(8),
            buildings_once: u32_at(12),
            spells_level: u32_at(16),
            spells_once: d[20..52].try_into().unwrap(),
            vehicles: u16_at(52),
            training_mana_off: d[54],
            options: d[55],
            name: c_string(&d[NAME_OFFSET..NAME_OFFSET + NAME_LEN]),
            tribes: d[88],
            ai_scripts: [d[99], ai[0], ai[1], ai[2]],
            allies: d[ALLIES_OFFSET..ALLIES_OFFSET + TRIBES].try_into().unwrap(),
            theme: d[96],
            object_bank: d[97],
            flags: LevelFlags(d[98]),
            markers: std::array::from_fn(|i| u16_at(MARKERS_OFFSET + 2 * i)),
            start: u16_at(612),
            start_angle: u16_at(614),
        }
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, LevelError> {
        Ok(Self::parse(&std::fs::read(path)?))
    }

    pub fn spell_available(&self, model: u8) -> bool {
        model < 32 && self.spells & (1 << model) != 0
    }

    pub fn building_available(&self, model: u8) -> bool {
        model < 32 && self.buildings & (1 << model) != 0
    }

    pub fn vehicle_available(&self, model: u8) -> bool {
        model < 16 && self.vehicles & (1 << model) != 0
    }

    /// Whether tribes `a` and `b` (owners 0-3) are allied by default.
    pub fn allied(&self, a: u8, b: u8) -> bool {
        (a as usize) < TRIBES && b < 8 && self.allies[a as usize] & (1 << b) != 0
    }

    pub fn marker_cell(&self, index: usize) -> (u8, u8) {
        cell_of(self.markers[index % MARKERS])
    }

    pub fn start_cell(&self) -> (u8, u8) {
        cell_of(self.start)
    }
}

/// Parsed `.ver` file. The version is 11 and the checksum 0 in every shipped level.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LevelVersion {
    pub version: i32,
    pub created_by: String,
    pub created_on: String,
    pub checksum: i32,
}

impl LevelVersion {
    pub fn parse(data: &[u8]) -> Result<Self, LevelError> {
        if data.len() < VER_SIZE {
            return Err(LevelError::BadSize { expected: VER_SIZE, got: data.len() });
        }
        let i32_at = |o: usize| i32::from_le_bytes(data[o..o + 4].try_into().unwrap());
        Ok(LevelVersion { version: i32_at(0), created_by: c_string(&data[4..36]), created_on: c_string(&data[36..64]), checksum: i32_at(64) })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, LevelError> {
        Self::parse(&std::fs::read(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> Vec<u8> {
        let mut h = vec![0u8; HDR_SIZE];
        h[0..4].copy_from_slice(&0xffc0_0009u32.to_le_bytes());
        h[4..8].copy_from_slice(&0xfffe_0003u32.to_le_bytes());
        h[52..54].copy_from_slice(&0xffe7u16.to_le_bytes());
        h[NAME_OFFSET..NAME_OFFSET + 7].copy_from_slice(b"Level 1");
        h[88] = 4;
        h[89..92].copy_from_slice(&[10, 12, 13]);
        h[92..96].copy_from_slice(&[1, 0xe, 0xe, 0xe]);
        h[96] = 12;
        h[97] = 6;
        h[98] = 0x11;
        h[MARKERS_OFFSET + 2..MARKERS_OFFSET + 4].copy_from_slice(&0xa62au16.to_le_bytes());
        h[612..614].copy_from_slice(&0xa62au16.to_le_bytes());
        h[614..616].copy_from_slice(&1144u16.to_le_bytes());
        h
    }

    #[test]
    fn name_theme_bank_flags() {
        let hdr = LevelHeader::parse(&header());
        assert_eq!((hdr.name.as_str(), hdr.theme, hdr.object_bank, hdr.tribes), ("Level 1", 12, 6, 4));
        assert!(hdr.flags.has(LevelFlags::FOG_OF_WAR) && hdr.flags.has(LevelFlags::NO_GUEST_SPELLS));
        assert!(!hdr.flags.has(LevelFlags::SHAMAN_OMNI));
    }

    #[test]
    fn masks_bit_n_is_model_n() {
        let hdr = LevelHeader::parse(&header());
        assert!(hdr.spell_available(3) && !hdr.spell_available(1) && !hdr.spell_available(40));
        assert!(hdr.building_available(1) && !hdr.building_available(2));
        assert!(hdr.vehicle_available(1) && hdr.vehicle_available(2) && !hdr.vehicle_available(3));
    }

    #[test]
    fn ai_scripts_in_owner_order_and_allies() {
        let hdr = LevelHeader::parse(&header());
        assert_eq!(hdr.ai_scripts, [0, 10, 12, 13], "blue at 99, then red, yellow, green");
        assert!(hdr.allied(1, 2) && hdr.allied(0, 0) && !hdr.allied(0, 1) && !hdr.allied(3, 0));
    }

    #[test]
    fn markers_and_start_low_byte_is_x() {
        let hdr = LevelHeader::parse(&header());
        assert_eq!(hdr.marker_cell(1), (21, 83));
        assert_eq!(hdr.marker_cell(0), (0, 0));
        assert_eq!((hdr.start_cell(), hdr.start_angle), ((21, 83), 1144));
    }

    #[test]
    fn short_header_is_padded() {
        assert_eq!(LevelHeader::parse(b"\x01").spells, 1);
        assert_eq!(LevelHeader::default().name, "");
    }

    #[test]
    fn version_file() {
        let mut v = vec![0u8; VER_SIZE];
        v[0] = 11;
        v[4..11].copy_from_slice(b"acullum");
        v[36..56].copy_from_slice(b"Sep 21 1998 17:09:26");
        let ver = LevelVersion::parse(&v).unwrap();
        assert_eq!((ver.version, ver.created_by.as_str(), ver.created_on.as_str(), ver.checksum), (11, "acullum", "Sep 21 1998 17:09:26", 0));
        assert!(LevelVersion::parse(&v[..10]).is_err());
    }
}
