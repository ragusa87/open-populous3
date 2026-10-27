//! `levels/constant.dat`, the original's balance numbers (docs/specs/constants.md): an obfuscated text file
//! of `P3CONST_NAME = value` lines, 255 of them in a stock install. Read from the player's own install when
//! present; never shipped (see "Use here" in the spec) — generated/no-original mode keeps its own defaults,
//! written in code, not copied from a parsed file.

use crate::level::LevelError;
use std::collections::HashMap;
use std::path::Path;

/// The two bytes a decoded file starts with (`constants.md` "Encoding"); probably a marker, not confirmed.
const MARKER: [u8; 2] = [0x9F, 0xC1];
/// The name prefix every constant has, stripped from the keys stored here.
const PREFIX: &str = "P3CONST_";

/// Every `P3CONST_<NAME>` line as `NAME => value`, the prefix stripped. A name read twice (the file's own
/// `CONV_SPY`, once per group) keeps the last value: nothing we read here repeats, so this is unobserved.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Constants(HashMap<String, u32>);

impl Constants {
    /// `data`, XOR-decoded (`constants.md` "Encoding": byte `i` XORed with `!(1 << ((i + 5) % 8))`) if it
    /// starts with the decoded marker, else taken as already-plain text (the community's own editable form,
    /// which the game is said to also accept) when it looks like printable text outright. Neither: an error.
    /// `[ours]`: which of the two the real loader actually tests for is unconfirmed ("Unverified here").
    pub fn parse(data: &[u8]) -> Result<Self, LevelError> {
        let decoded = decode(data);
        let text = if decoded.get(..2) == Some(&MARKER[..]) {
            std::str::from_utf8(&decoded[2..])
        } else if looks_like_plain_text(data) {
            std::str::from_utf8(data)
        } else {
            return Err(LevelError::BadFormat("neither the encoded marker nor plain text".into()));
        };
        let text = text.map_err(|e| LevelError::BadFormat(format!("not valid text: {e}")))?;
        let mut values = HashMap::new();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            let Some((name, value)) = line.split_once('=') else { continue };
            let Some(name) = name.trim().strip_prefix(PREFIX) else { continue };
            let Ok(value) = value.trim().parse() else { continue };
            values.insert(name.to_string(), value);
        }
        Ok(Constants(values))
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, LevelError> {
        Self::parse(&std::fs::read(path)?)
    }

    /// `name` without its `P3CONST_` prefix, e.g. `"SPELL_BLAST"`.
    pub fn get(&self, name: &str) -> Option<u32> {
        self.0.get(name).copied()
    }
}

fn decode(data: &[u8]) -> Vec<u8> {
    data.iter().enumerate().map(|(i, &b)| b ^ !(1u8 << ((i + 5) % 8))).collect()
}

/// Whether `data` already looks like the format's own plain-text form: nothing but printable ASCII, tab or
/// CR/LF (`constants.md`: "no byte outside printable ASCII, tab and CR/LF").
fn looks_like_plain_text(data: &[u8]) -> bool {
    !data.is_empty() && data.iter().all(|&b| b == b'\t' || b == b'\r' || b == b'\n' || (0x20..=0x7e).contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The encoding is its own inverse (XOR), so this is also how a real file would be produced.
    fn encode(text: &str) -> Vec<u8> {
        let mut data = Vec::with_capacity(text.len() + 2);
        data.extend_from_slice(&MARKER);
        data.extend_from_slice(text.as_bytes());
        decode(&data)
    }

    #[test]
    fn decodes_the_xor_obfuscated_form() {
        let data = encode("P3CONST_SPELL_BLAST\t=\t10000\t# Blast's cast cost\r\nP3CONST_SPELL_BOLT = 80000");
        let c = Constants::parse(&data).unwrap();
        assert_eq!(c.get("SPELL_BLAST"), Some(10_000));
        assert_eq!(c.get("SPELL_BOLT"), Some(80_000));
        assert_eq!(c.get("SPELL_WWIND"), None, "not in the file");
    }

    #[test]
    fn accepts_the_communitys_plain_text_form_directly() {
        let text = b"##########\n# POP3 BALANCE FILE\n##########\n\nP3CONST_START_MANA = 30000\n";
        let c = Constants::parse(text).unwrap();
        assert_eq!(c.get("START_MANA"), Some(30_000));
    }

    #[test]
    fn a_malformed_line_is_skipped_not_fatal() {
        let text = b"P3CONST_GOOD = 1\nnot a constant line at all\nP3CONST_BAD = not_a_number\nP3CONST_OK = 2\n";
        let c = Constants::parse(text).unwrap();
        assert_eq!((c.get("GOOD"), c.get("BAD"), c.get("OK")), (Some(1), None, Some(2)));
    }

    #[test]
    fn rejects_data_that_is_neither_encoded_nor_plain() {
        let garbage: Vec<u8> = (0..32).collect();
        assert!(matches!(Constants::parse(&garbage), Err(LevelError::BadFormat(_))));
    }

    #[test]
    fn an_unprefixed_or_commented_out_line_is_ignored() {
        let text = b"NOT_PREFIXED = 5\n# P3CONST_COMMENTED = 6\nP3CONST_REAL = 7 # a trailing comment\n";
        let c = Constants::parse(text).unwrap();
        assert_eq!((c.get("NOT_PREFIXED"), c.get("COMMENTED"), c.get("REAL")), (None, None, Some(7)));
    }
}
