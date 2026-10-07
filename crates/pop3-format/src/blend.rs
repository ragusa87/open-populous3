//! Blend tables of a theme: `al0-X.dat`, the alpha table of the blended sprites (`hfx0-0.dat`
//! effects). See docs/specs/sprites.md.

use crate::level::LevelError;
use crate::theme::theme_char;
use std::path::Path;

pub const TABLE_SIZE: usize = 256 * 256;
/// Tints and strengths per alpha pixel (`tint << 4 | strength`).
pub const TINTS: usize = 16;
pub const STRENGTHS: usize = 16;
/// `hfx0-0.dat` sprites whose pixels are alpha pixels, not palette colours.
pub const ALPHA_SPRITES: [std::ops::RangeInclusive<usize>; 2] = [1090..=1499, 1538..=1592];

/// Whether an `hfx0-0.dat` sprite is blended (its pixels go through the alpha table).
pub fn is_alpha_sprite(index: usize) -> bool {
    ALPHA_SPRITES.iter().any(|r| r.contains(&index))
}

/// `al[pixel * 256 + background]` = the palette index seen when an alpha pixel is drawn over
/// `background`. Strength 0 leaves the background, 15 gives about the tint.
#[derive(Clone, Debug)]
pub struct AlphaTable {
    pub table: Vec<u8>,
}

impl AlphaTable {
    pub fn parse(data: &[u8]) -> Result<Self, LevelError> {
        let table = data.get(..TABLE_SIZE).ok_or(LevelError::BadSize { expected: TABLE_SIZE, got: data.len() })?;
        Ok(AlphaTable { table: table.to_vec() })
    }

    pub fn load(data_dir: &Path, theme: u8) -> Result<Self, LevelError> {
        let name = format!("al0-{}.dat", theme_char(theme));
        Self::parse(&std::fs::read(crate::find_file(data_dir, &name).unwrap_or_else(|| data_dir.join(name)))?)
    }

    /// Palette index of `pixel` drawn over the palette index `background`.
    pub fn blend(&self, pixel: u8, background: u8) -> u8 {
        self.table[pixel as usize * 256 + background as usize]
    }

    /// The colour of each tint: the full-strength row over every background, averaged.
    pub fn tints(&self, palette: &[[u8; 3]]) -> [[u8; 3]; TINTS] {
        std::array::from_fn(|tint| {
            let row = (tint * STRENGTHS + STRENGTHS - 1) as u8;
            let mut sum = [0u32; 3];
            for bg in 0..=255u8 {
                let c = palette.get(self.blend(row, bg) as usize).copied().unwrap_or_default();
                (0..3).for_each(|k| sum[k] += c[k] as u32);
            }
            sum.map(|s| (s / 256) as u8)
        })
    }

    /// RGBA of every alpha pixel, for blending on the GPU: the tint's colour, opacity
    /// strength / 15 (the table's blend is linear in the strength, within a few percent).
    pub fn rgba(&self, palette: &[[u8; 3]]) -> [[u8; 4]; 256] {
        let tints = self.tints(palette);
        std::array::from_fn(|pixel| {
            let [r, g, b] = tints[pixel / STRENGTHS];
            [r, g, b, (pixel % STRENGTHS * 255 / (STRENGTHS - 1)) as u8]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two colours (0 black, 1 white) and the rest grey; tint 1 goes to white, the others to black.
    fn fixture() -> (AlphaTable, Vec<[u8; 3]>) {
        let mut palette = vec![[128, 128, 128]; 256];
        palette[0] = [0, 0, 0];
        palette[1] = [255, 255, 255];
        let mut table = vec![0u8; TABLE_SIZE];
        for pixel in 0..256 {
            for bg in 0..256 {
                let full = pixel % 16 == 15;
                table[pixel * 256 + bg] = if !full { bg as u8 } else if pixel / 16 == 1 { 1 } else { 0 };
            }
        }
        (AlphaTable::parse(&table).unwrap(), palette)
    }

    #[test]
    fn blends_over_the_background() {
        let (al, _) = fixture();
        assert_eq!(al.blend(0x10, 42), 42, "strength 0 keeps the background");
        assert_eq!(al.blend(0x1f, 42), 1, "full strength gives the tint");
    }

    #[test]
    fn rgba_is_tint_and_strength() {
        let (al, pal) = fixture();
        let rgba = al.rgba(&pal);
        assert_eq!(rgba[0x1f], [255, 255, 255, 255]);
        assert_eq!(rgba[0x10], [255, 255, 255, 0]);
        assert_eq!(rgba[0x85], [0, 0, 0, 85], "dark tints shade");
    }

    #[test]
    fn alpha_sprite_ranges() {
        assert!(!is_alpha_sprite(23), "the wood pile is a plain sprite");
        assert!(is_alpha_sprite(1090) && is_alpha_sprite(1499) && is_alpha_sprite(1592));
        assert!(!is_alpha_sprite(1500) && !is_alpha_sprite(1600));
        assert!(AlphaTable::parse(&[0; 10]).is_err());
    }
}
