//! `cpscrNNN.dat` byte layout (12 552 bytes, little endian). Layout from docs/specs/ai-scripts.md
//! "`cpscr` layout". Only this exact size is read: the smaller `cpscr081`/`cpscr099` layouts are
//! unused by any active level slot.

use std::fmt;

pub const DAT_SIZE: usize = 12_552;
pub const CODE_WORDS: usize = 4_096;
pub const FIELD_COUNT: usize = 512;
pub const USER_VAR_COUNT: usize = 64;

const CODE_OFFSET: usize = 0;
const FIELDS_OFFSET: usize = CODE_OFFSET + CODE_WORDS * 2;
const USER_VARS_OFFSET: usize = FIELDS_OFFSET + FIELD_COUNT * 8;
const POINTERS_OFFSET: usize = USER_VARS_OFFSET + USER_VAR_COUNT * 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawError {
    BadSize { expected: usize, found: usize },
}

impl fmt::Display for RawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RawError::BadSize { expected, found } => {
                write!(f, "expected {expected} bytes, found {found}")
            }
        }
    }
}

impl std::error::Error for RawError {}

/// A field's raw (type, value) pair, undecoded: type 0 constant, 1 user variable, 2 internal
/// variable; anything else (notably the `0x03030303` padding of an unused field) is neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawField {
    pub ty: u32,
    pub value: i32,
}

/// The decoded byte layout of one `cpscr` file, before grammar parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawScript {
    /// Word 0 of the code stream, 12 in every real file.
    pub version: u16,
    pub code: Vec<u16>,
    pub fields: Vec<RawField>,
    /// On-disk user variable storage (always 0 on disk; runtime-only).
    pub user_vars: [i32; USER_VAR_COUNT],
}

impl RawScript {
    pub fn parse(data: &[u8]) -> Result<Self, RawError> {
        if data.len() != DAT_SIZE {
            return Err(RawError::BadSize { expected: DAT_SIZE, found: data.len() });
        }
        let code: Vec<u16> = data[CODE_OFFSET..FIELDS_OFFSET]
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        let fields: Vec<RawField> = data[FIELDS_OFFSET..USER_VARS_OFFSET]
            .chunks_exact(8)
            .map(|b| RawField {
                ty: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                value: i32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            })
            .collect();
        let mut user_vars = [0i32; USER_VAR_COUNT];
        for (i, b) in data[USER_VARS_OFFSET..POINTERS_OFFSET].chunks_exact(4).enumerate() {
            user_vars[i] = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        }
        Ok(RawScript { version: code[0], code, fields, user_vars })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic() -> Vec<u8> {
        let mut d = vec![0u8; DAT_SIZE];
        d[0..2].copy_from_slice(&12u16.to_le_bytes());
        d
    }

    #[test]
    fn decodes_version_and_sizes() {
        let raw = RawScript::parse(&synthetic()).unwrap();
        assert_eq!(raw.version, 12);
        assert_eq!(raw.code.len(), CODE_WORDS);
        assert_eq!(raw.fields.len(), FIELD_COUNT);
        assert_eq!(raw.user_vars, [0; USER_VAR_COUNT]);
    }

    #[test]
    fn decodes_a_field_and_a_user_var() {
        let mut d = synthetic();
        d[FIELDS_OFFSET..FIELDS_OFFSET + 8].copy_from_slice(&[0x01, 0, 0, 0, 0x2a, 0, 0, 0]);
        d[USER_VARS_OFFSET..USER_VARS_OFFSET + 4].copy_from_slice(&7i32.to_le_bytes());
        let raw = RawScript::parse(&d).unwrap();
        assert_eq!(raw.fields[0], RawField { ty: 1, value: 42 });
        assert_eq!(raw.user_vars[0], 7);
    }

    #[test]
    fn rejects_wrong_size() {
        assert_eq!(RawScript::parse(&[0; 10]), Err(RawError::BadSize { expected: DAT_SIZE, found: 10 }));
    }
}
