//! Landscape theme files from `data/` (one set per theme char `0-9a-z`):
//! `pal0-X.dat` palette, `bigf0-X.dat` height x brightness colour table,
//! `disp0-X.dat` detail noise. See docs/specs/terrain-textures.md.

use crate::level::LevelError;
use std::path::Path;

pub const BIGFADE_WIDTH: usize = 256;
pub const BIGFADE_ROWS: usize = 1152;
/// Rows below this are water/shore; land starts here.
pub const BIGFADE_LAND_ROW: usize = 128;
pub const DISP_SIZE: usize = 256;

#[derive(Clone, Debug)]
pub struct Theme {
    pub palette: Vec<[u8; 3]>,
    /// `BIGFADE_ROWS` rows (height) of `BIGFADE_WIDTH` palette indices (0 bright .. 255 dark).
    pub bigfade: Vec<u8>,
    pub disp: Vec<u8>,
}

/// Theme index (from the level header) to its file suffix: 0-9 then a-z.
pub fn theme_char(index: u8) -> char {
    std::char::from_digit(index as u32 % 36, 36).unwrap_or('0')
}

impl Theme {
    pub fn parse(pal: &[u8], bigfade: &[u8], disp: &[u8]) -> Result<Self, LevelError> {
        let check = |d: &[u8], n: usize| {
            if d.len() < n { Err(LevelError::BadSize { expected: n, got: d.len() }) } else { Ok(()) }
        };
        check(pal, 1024)?;
        check(bigfade, BIGFADE_WIDTH * BIGFADE_ROWS)?;
        check(disp, DISP_SIZE * DISP_SIZE)?;
        Ok(Theme {
            palette: pal.chunks_exact(4).take(256).map(|c| [c[0], c[1], c[2]]).collect(),
            bigfade: bigfade[..BIGFADE_WIDTH * BIGFADE_ROWS].to_vec(),
            disp: disp[..DISP_SIZE * DISP_SIZE].to_vec(),
        })
    }

    pub fn load(data_dir: &Path, index: u8) -> Result<Self, LevelError> {
        let c = theme_char(index);
        let read = |p: &str| std::fs::read(data_dir.join(format!("{p}0-{c}.dat")));
        Self::parse(&read("pal")?, &read("bigf")?, &read("disp")?)
    }

    /// RGB for a bigfade row (clamped) and brightness column (0 = brightest).
    pub fn shade(&self, row: usize, brightness: u8) -> [u8; 3] {
        let row = row.min(BIGFADE_ROWS - 1);
        self.palette[self.bigfade[row * BIGFADE_WIDTH + brightness as usize] as usize]
    }

    /// Detail noise, wraps.
    pub fn disp(&self, x: usize, y: usize) -> u8 {
        self.disp[(y % DISP_SIZE) * DISP_SIZE + x % DISP_SIZE]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_chars() {
        assert_eq!(theme_char(0), '0');
        assert_eq!(theme_char(12), 'c');
        assert_eq!(theme_char(35), 'z');
    }

    #[test]
    fn shade_uses_palette() {
        let mut pal = vec![0u8; 1024];
        pal[4 * 7..4 * 7 + 3].copy_from_slice(&[1, 2, 3]);
        let mut big = vec![0u8; BIGFADE_WIDTH * BIGFADE_ROWS];
        big[(BIGFADE_ROWS - 1) * BIGFADE_WIDTH + 5] = 7;
        let t = Theme::parse(&pal, &big, &[0; DISP_SIZE * DISP_SIZE]).unwrap();
        assert_eq!(t.shade(5000, 5), [1, 2, 3], "row clamps");
        assert!(Theme::parse(&pal, &[], &[]).is_err());
    }
}
