//! Language files (`language/langNN.dat`): every text of the game, one list per language, a text's
//! number being its position (the same number in every language). See docs/specs/language.md.
//!
//! No header: texts follow each other, each ending with a 0 unit. Units are 16-bit little endian but
//! each holds one byte of a Windows code page (1252, Polish 1250), never above 255.

use crate::level::LevelError;
use std::path::Path;

/// Texts in every full language file of the release checked.
pub const TEXTS: usize = 1318;

/// Language of each file number; the others are 3-byte placeholders.
pub const LANGUAGES: [(u8, &str); 7] = [(0, "English"), (1, "French"), (2, "German"), (4, "Spanish"), (5, "Swedish"), (6, "Dutch"), (7, "Polish")];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codepage {
    /// Windows-1252, western European.
    Western,
    /// Windows-1250, central European (Polish).
    CentralEuropean,
}

impl Codepage {
    /// The code page of language file `number`.
    pub fn of(number: u8) -> Self {
        if number == 7 { Codepage::CentralEuropean } else { Codepage::Western }
    }

    /// The character of byte `b`.
    pub fn char(self, b: u8) -> char {
        match (self, b) {
            (_, 0..=0x7f) => b as char,
            (Codepage::Western, 0x80..=0x9f) => CP1252_80[b as usize - 0x80],
            (Codepage::Western, _) => b as char,
            (Codepage::CentralEuropean, _) => CP1250_80[b as usize - 0x80],
        }
    }
}

/// Windows-1252 0x80-0x9F (above, Latin-1); U+FFFD where it has no character.
const CP1252_80: [char; 32] = [
    '\u{20ac}', '\u{fffd}', '\u{201a}', '\u{0192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{02c6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{fffd}', '\u{017d}', '\u{fffd}',
    '\u{fffd}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{02dc}', '\u{2122}', '\u{0161}', '\u{203a}', '\u{0153}', '\u{fffd}', '\u{017e}', '\u{0178}',
];

/// Windows-1250 0x80-0xFF; U+FFFD where it has no character.
const CP1250_80: [char; 128] = [
    '\u{20ac}', '\u{fffd}', '\u{201a}', '\u{fffd}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{fffd}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{015a}', '\u{0164}', '\u{017d}', '\u{0179}',
    '\u{fffd}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{fffd}', '\u{2122}', '\u{0161}', '\u{203a}', '\u{015b}', '\u{0165}', '\u{017e}', '\u{017a}',
    '\u{00a0}', '\u{02c7}', '\u{02d8}', '\u{0141}', '\u{00a4}', '\u{0104}', '\u{00a6}', '\u{00a7}', '\u{00a8}', '\u{00a9}', '\u{015e}', '\u{00ab}', '\u{00ac}', '\u{00ad}', '\u{00ae}', '\u{017b}',
    '\u{00b0}', '\u{00b1}', '\u{02db}', '\u{0142}', '\u{00b4}', '\u{00b5}', '\u{00b6}', '\u{00b7}', '\u{00b8}', '\u{0105}', '\u{015f}', '\u{00bb}', '\u{013d}', '\u{02dd}', '\u{013e}', '\u{017c}',
    '\u{0154}', '\u{00c1}', '\u{00c2}', '\u{0102}', '\u{00c4}', '\u{0139}', '\u{0106}', '\u{00c7}', '\u{010c}', '\u{00c9}', '\u{0118}', '\u{00cb}', '\u{011a}', '\u{00cd}', '\u{00ce}', '\u{010e}',
    '\u{0110}', '\u{0143}', '\u{0147}', '\u{00d3}', '\u{00d4}', '\u{0150}', '\u{00d6}', '\u{00d7}', '\u{0158}', '\u{016e}', '\u{00da}', '\u{0170}', '\u{00dc}', '\u{00dd}', '\u{0162}', '\u{00df}',
    '\u{0155}', '\u{00e1}', '\u{00e2}', '\u{0103}', '\u{00e4}', '\u{013a}', '\u{0107}', '\u{00e7}', '\u{010d}', '\u{00e9}', '\u{0119}', '\u{00eb}', '\u{011b}', '\u{00ed}', '\u{00ee}', '\u{010f}',
    '\u{0111}', '\u{0144}', '\u{0148}', '\u{00f3}', '\u{00f4}', '\u{0151}', '\u{00f6}', '\u{00f7}', '\u{0159}', '\u{016f}', '\u{00fa}', '\u{0171}', '\u{00fc}', '\u{00fd}', '\u{0163}', '\u{02d9}',
];

/// The texts of one language, by number.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Language {
    pub texts: Vec<String>,
}

impl Language {
    /// Texts of `data` (16-bit units, each ending with 0) in `codepage`. A last text without its 0
    /// is kept; an odd trailing byte is ignored. A placeholder file (3 bytes) gives no text.
    pub fn parse(data: &[u8], codepage: Codepage) -> Self {
        let mut texts = Vec::new();
        let mut text = String::new();
        for unit in data.chunks_exact(2).map(|u| u16::from_le_bytes([u[0], u[1]])) {
            if unit == 0 {
                texts.push(std::mem::take(&mut text));
            } else {
                text.push(u8::try_from(unit).map_or(char::REPLACEMENT_CHARACTER, |b| codepage.char(b)));
            }
        }
        if !text.is_empty() {
            texts.push(text);
        }
        if data.len() < 4 {
            texts.clear();
        }
        Language { texts }
    }

    /// `langNN.dat` of `language_dir` (the install's `language/`), in its code page.
    pub fn load(language_dir: &Path, number: u8) -> Result<Self, LevelError> {
        let name = format!("lang{number:02}.dat");
        let data = std::fs::read(crate::find_file(language_dir, &name).unwrap_or_else(|| language_dir.join(name)))?;
        Ok(Self::parse(&data, Codepage::of(number)))
    }

    /// Text number `n`.
    pub fn get(&self, n: usize) -> Option<&str> {
        self.texts.get(n).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(text: &[u16]) -> Vec<u8> {
        text.iter().flat_map(|u| u.to_le_bytes()).collect()
    }

    #[test]
    fn texts_are_numbered_by_position() {
        let data = units(&[b'a' as u16, 0, 0, b'b' as u16, b'c' as u16, 0]);
        let lang = Language::parse(&data, Codepage::Western);
        assert_eq!(lang.texts, ["a", "", "bc"]);
        assert_eq!((lang.get(2), lang.get(3)), (Some("bc"), None));
    }

    #[test]
    fn units_are_code_page_bytes() {
        let crise = units(&[b'C' as u16, 0xe9, 0x85, 0]);
        assert_eq!(Language::parse(&crise, Codepage::Western).texts, ["Cé…"], "1252: é, ellipsis");
        let polish = units(&[0xb3, 0xb9, 0]);
        assert_eq!(Language::parse(&polish, Codepage::CentralEuropean).texts, ["łą"], "1250");
        assert_eq!(Codepage::of(7), Codepage::CentralEuropean);
        assert_eq!(Codepage::of(1), Codepage::Western);
    }

    #[test]
    fn a_placeholder_has_no_text_and_a_last_text_needs_no_end() {
        assert!(Language::parse(b" \r\n", Codepage::Western).texts.is_empty());
        assert_eq!(Language::parse(&units(&[b'x' as u16, b'y' as u16]), Codepage::Western).texts, ["xy"]);
        assert_eq!(Language::parse(&units(&[0x100, 0]), Codepage::Western).texts, ["\u{fffd}"], "never above 255");
    }
}
