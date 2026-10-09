//! Landscape theme files from `data/` (one set per theme char `0-9a-z`):
//! `pal0-X.dat` palette, `bigf0-X.dat` height x brightness colour table,
//! `disp0-X.dat` detail noise, `sky0-X.dat` clouds, and `watdisp.dat` (the sea's noise, one file
//! for all themes). See docs/specs/terrain-textures.md.

use crate::level::LevelError;
use std::path::Path;

pub const BIGFADE_WIDTH: usize = 256;
pub const BIGFADE_ROWS: usize = 1152;
/// Rows below this are water/shore; land starts here.
pub const BIGFADE_LAND_ROW: usize = 128;
/// The first rows above `BIGFADE_LAND_ROW` still look like water in most themes (`land_start_row`),
/// never more than this many.
pub const MAX_SHORE_ROWS: usize = 32;
pub const DISP_SIZE: usize = 256;
/// Water displacement, shared by every theme (same size as `disp`).
pub const WATER_FILE: &str = "watdisp.dat";
/// `sky0-X.dat`: 512 x 512 values 0..15, colour `palette[SKY_PALETTE + v]`.
pub const SKY_SIZE: usize = 512;
pub const SKY_PALETTE: usize = 112;
/// Theme 0's older sky: 640 x 480 raw palette indices.
pub const OLD_SKY: (usize, usize) = (640, 480);

#[derive(Clone, Debug)]
pub struct Theme {
    pub palette: Vec<[u8; 3]>,
    /// `BIGFADE_ROWS` rows (height) of `BIGFADE_WIDTH` palette indices (0 bright .. 255 dark).
    pub bigfade: Vec<u8>,
    pub disp: Vec<u8>,
    /// Sea noise (`watdisp.dat`, 256 x 256), picks the water row; `disp` when the file is missing.
    pub water: Vec<u8>,
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
            water: disp[..DISP_SIZE * DISP_SIZE].to_vec(),
        })
    }

    /// Uses `watdisp` as the sea noise.
    pub fn with_water(self, watdisp: &[u8]) -> Result<Self, LevelError> {
        let water = watdisp.get(..DISP_SIZE * DISP_SIZE).ok_or(LevelError::BadSize { expected: DISP_SIZE * DISP_SIZE, got: watdisp.len() })?;
        Ok(Theme { water: water.to_vec(), ..self })
    }

    pub fn load(data_dir: &Path, index: u8) -> Result<Self, LevelError> {
        let c = theme_char(index);
        let read = |p: &str| std::fs::read(data_dir.join(format!("{p}0-{c}.dat")));
        let theme = Self::parse(&read("pal")?, &read("bigf")?, &read("disp")?)?;
        match crate::find_file(data_dir, WATER_FILE).map(std::fs::read) {
            Some(Ok(w)) => theme.with_water(&w),
            _ => Ok(theme),
        }
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

    /// Mean of the detail noise: the files are not centred on 128 (theme 1: 88, theme 5: 162).
    pub fn disp_mean(&self) -> i32 {
        (self.disp.iter().map(|&d| d as u64).sum::<u64>() / self.disp.len().max(1) as u64) as i32
    }

    /// The first row from `BIGFADE_LAND_ROW` up where most columns are neither water colours
    /// (colours of rows 0..127) nor sea-like ones (`sea_like`), at most `MAX_SHORE_ROWS` above it:
    /// the lowest ground drawn as land.
    pub fn land_start_row(&self) -> usize {
        let water: std::collections::BTreeSet<u8> = self.bigfade[..BIGFADE_LAND_ROW * BIGFADE_WIDTH].iter().copied().collect();
        let land = |c: &&u8| !water.contains(c) && !sea_like(self.palette[**c as usize]);
        (BIGFADE_LAND_ROW..BIGFADE_LAND_ROW + MAX_SHORE_ROWS)
            .find(|&r| self.bigfade[r * BIGFADE_WIDTH..(r + 1) * BIGFADE_WIDTH].iter().filter(land).count() > BIGFADE_WIDTH / 2)
            .unwrap_or(BIGFADE_LAND_ROW + MAX_SHORE_ROWS)
    }

    /// Sea noise, wraps.
    pub fn water(&self, x: usize, y: usize) -> u8 {
        self.water[(y % DISP_SIZE) * DISP_SIZE + x % DISP_SIZE]
    }
}

/// Blue over green over red: the wet grey-blue of the lowest rows (theme c: 77, 99, 102), not
/// the purple or blue land some themes have (theme i: 92, 88, 173).
pub fn sea_like([r, g, b]: [u8; 3]) -> bool {
    b > g && g >= r
}

/// A theme's sky: a tiling cloud layer, RGB row-major.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sky {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<[u8; 3]>,
}

impl Sky {
    /// `sky0-X.dat` with the theme's palette: 512 x 512 sky values (`palette[112 + v]`), or theme
    /// 0's 640 x 480 raw indices.
    pub fn parse(data: &[u8], palette: &[[u8; 3]]) -> Result<Self, LevelError> {
        let colour = |i: usize| palette.get(i).copied().unwrap_or_default();
        let (width, height, base) = match data.len() {
            n if n == SKY_SIZE * SKY_SIZE => (SKY_SIZE, SKY_SIZE, SKY_PALETTE),
            n if n == OLD_SKY.0 * OLD_SKY.1 => (OLD_SKY.0, OLD_SKY.1, 0),
            n => return Err(LevelError::BadSize { expected: SKY_SIZE * SKY_SIZE, got: n }),
        };
        let rgb = data.iter().map(|&v| colour(base + if base == 0 { v as usize } else { v as usize & 15 })).collect();
        Ok(Sky { width, height, rgb })
    }

    pub fn load(data_dir: &Path, index: u8, palette: &[[u8; 3]]) -> Result<Self, LevelError> {
        let name = format!("sky0-{}.dat", theme_char(index));
        Self::parse(&std::fs::read(crate::find_file(data_dir, &name).unwrap_or_else(|| data_dir.join(name)))?, palette)
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
    fn land_starts_past_the_watery_rows() {
        let mut big = vec![0u8; BIGFADE_WIDTH * BIGFADE_ROWS];
        big[BIGFADE_LAND_ROW * BIGFADE_WIDTH..].fill(1);
        big[..BIGFADE_WIDTH].fill(5);
        for r in BIGFADE_LAND_ROW..BIGFADE_LAND_ROW + 6 {
            big[r * BIGFADE_WIDTH..r * BIGFADE_WIDTH + 200].fill(5);
        }
        let mut disp = vec![60u8; DISP_SIZE * DISP_SIZE];
        disp[0] = 60 + 128;
        let t = Theme::parse(&[0; 1024], &big, &disp).unwrap();
        assert_eq!(t.land_start_row(), BIGFADE_LAND_ROW + 6);
        assert_eq!(t.disp_mean(), 60);
        big[BIGFADE_LAND_ROW * BIGFADE_WIDTH..].fill(5);
        let all_water = Theme::parse(&[0; 1024], &big, &disp).unwrap();
        assert_eq!(all_water.land_start_row(), BIGFADE_LAND_ROW + MAX_SHORE_ROWS, "capped");
    }

    #[test]
    fn sea_like_rows_are_not_land() {
        let mut big = vec![0u8; BIGFADE_WIDTH * BIGFADE_ROWS];
        big[BIGFADE_LAND_ROW * BIGFADE_WIDTH..].fill(2);
        big[BIGFADE_LAND_ROW * BIGFADE_WIDTH..(BIGFADE_LAND_ROW + 3) * BIGFADE_WIDTH].fill(1);
        let mut pal = vec![0u8; 1024];
        pal[4..7].copy_from_slice(&[77, 99, 102]);
        pal[8..11].copy_from_slice(&[92, 88, 173]);
        let t = Theme::parse(&pal, &big, &vec![128; DISP_SIZE * DISP_SIZE]).unwrap();
        assert_eq!(t.land_start_row(), BIGFADE_LAND_ROW + 3, "grey-blue skipped, purple land kept");
    }

    #[test]
    fn sky_values_are_palette_112_up() {
        let mut palette = vec![[0u8; 3]; 256];
        palette[112 + 4] = [10, 20, 30];
        palette[4] = [99, 99, 99];
        let mut data = vec![0u8; SKY_SIZE * SKY_SIZE];
        data[1] = 4;
        let sky = Sky::parse(&data, &palette).unwrap();
        assert_eq!((sky.width, sky.height, sky.rgb[1]), (512, 512, [10, 20, 30]));
        let old = Sky::parse(&vec![4u8; 640 * 480], &palette).unwrap();
        assert_eq!((old.width, old.rgb[0]), (640, [99, 99, 99]), "theme 0: raw indices");
        assert!(Sky::parse(&[0; 100], &palette).is_err());
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
        assert_eq!(t.water(3, 4), 0, "disp until a watdisp is given");
        let mut wat = vec![0u8; DISP_SIZE * DISP_SIZE];
        wat[4 * DISP_SIZE + 3] = 200;
        let t = t.with_water(&wat).unwrap();
        assert_eq!((t.water(3, 4), t.water(3 + DISP_SIZE, 4), t.disp(3, 4)), (200, 200, 0));
        assert!(t.with_water(&[1, 2]).is_err());
    }
}
