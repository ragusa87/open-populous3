//! Ghost blend table of a theme: `ghost0-X.dat`, used for transparency (ghost / see-through units). See
//! docs/specs/sprites.md "Ghost".

use crate::level::LevelError;
use crate::theme::theme_char;
use std::path::Path;

pub const TABLE_SIZE: usize = 256 * 256;
/// The fitted mix fraction (docs/specs/sprites.md "Ghost": 0.66-0.67 across every theme checked, matching
/// PopResourceEditor's own generator default) - confirmed enough to build a test fixture from, not a guess.
pub const MIX: f64 = 0.66;

/// `ghost[a * 256 + b]` = nearest palette colour to `pal[a] + (pal[b] - pal[a]) * MIX`. Not symmetric; which
/// operand (`a` or `b`) is the sprite and which the background is unknown (sprites.md).
#[derive(Clone, Debug)]
pub struct GhostTable {
    pub table: Vec<u8>,
}

impl GhostTable {
    pub fn parse(data: &[u8]) -> Result<Self, LevelError> {
        let table = data.get(..TABLE_SIZE).ok_or(LevelError::BadSize { expected: TABLE_SIZE, got: data.len() })?;
        Ok(GhostTable { table: table.to_vec() })
    }

    pub fn load(data_dir: &Path, theme: u8) -> Result<Self, LevelError> {
        let name = format!("ghost0-{}.dat", theme_char(theme));
        Self::parse(&std::fs::read(crate::find_file(data_dir, &name).unwrap_or_else(|| data_dir.join(name)))?)
    }

    /// The palette index of `a` blended with `b` (`a * MIX` away from `a`, towards `b`).
    pub fn blend(&self, a: u8, b: u8) -> u8 {
        self.table[a as usize * 256 + b as usize]
    }
}

/// The palette index nearest `pal[a] + (pal[b] - pal[a]) * mix` (squared-distance nearest neighbour in `pal`):
/// how `ghost0-X.dat` itself is built (sprites.md "Ghost"), used here only to derive a test fixture from the
/// formula rather than hand-picking table bytes.
pub fn nearest_mix(pal: &[[u8; 3]], a: u8, b: u8, mix: f64) -> u8 {
    let mixed = std::array::from_fn::<f64, 3, _>(|k| pal[a as usize][k] as f64 + (pal[b as usize][k] as f64 - pal[a as usize][k] as f64) * mix);
    (0..256)
        .min_by(|&x, &y| {
            let dist = |i: usize| (0..3).map(|k| (pal[i][k] as f64 - mixed[k]).powi(2)).sum::<f64>();
            dist(x).partial_cmp(&dist(y)).unwrap()
        })
        .unwrap() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic palette (black, white, a few greys) and a table built straight from `nearest_mix`, same
    /// shape as `blend.rs`'s own hand-built fixture - no real game file involved.
    fn fixture() -> (GhostTable, Vec<[u8; 3]>) {
        let palette: Vec<[u8; 3]> = (0..256).map(|i| [i as u8, (255 - i) as u8, i as u8 / 2]).collect();
        let mut table = vec![0u8; TABLE_SIZE];
        for a in 0..256usize {
            for b in 0..256usize {
                table[a * 256 + b] = nearest_mix(&palette, a as u8, b as u8, MIX);
            }
        }
        (GhostTable::parse(&table).unwrap(), palette)
    }

    #[test]
    fn diagonal_is_identity() {
        let (ghost, _) = fixture();
        for a in 0..=255u8 {
            assert_eq!(ghost.blend(a, a), a, "a colour blended with itself is itself");
        }
    }

    #[test]
    fn blend_matches_the_mix_formula() {
        let (ghost, pal) = fixture();
        assert_eq!(ghost.blend(0, 255), nearest_mix(&pal, 0, 255, MIX));
        assert_eq!(ghost.blend(10, 200), nearest_mix(&pal, 10, 200, MIX));
        assert_ne!(ghost.blend(10, 200), ghost.blend(200, 10), "not symmetric");
    }

    #[test]
    fn parse_rejects_a_short_table() {
        assert!(GhostTable::parse(&[0; 10]).is_err());
    }
}
