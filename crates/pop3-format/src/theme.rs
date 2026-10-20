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
/// `cliff0-X.dat` (terrain-textures.md "cliff"): `cliff[row * CLIFF_WIDTH + colour]`, row = blend / 4 (0..63),
/// colour a land colour (bigfade output, < `CLIFF_WIDTH`).
pub const CLIFF_ROWS: usize = 64;
pub const CLIFF_WIDTH: usize = 128;
/// `fade0-X.dat` (terrain-textures.md "fade"): a palette-space light table for object/sprite shading and fog of
/// war, loaded but not consumed by this renderer yet (TODO.md "Terrain").
pub const FADE_ROWS: usize = 64;
pub const FADE_WIDTH: usize = 256;
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
    /// `CLIFF_ROWS` rows of `CLIFF_WIDTH` land-colour remaps ("cliff" above); identity (every row the same as
    /// row 0, `colour -> colour`) until `with_cliff` loads the real file.
    pub cliff: Vec<u8>,
    /// `FADE_ROWS` rows of `FADE_WIDTH` palette-index remaps ("fade" above), loaded but not consumed by this
    /// renderer yet; identity until `with_fade` loads the real file.
    pub fade: Vec<u8>,
}

/// Theme index (from the level header) to its file suffix: 0-9 then a-z.
pub fn theme_char(index: u8) -> char {
    std::char::from_digit(index as u32 % 36, 36).unwrap_or('0')
}

/// How much the `disp` noise jitters a bigfade row at that altitude index (terrain-textures.md "disp",
/// pop3-rev-analysis.md "Terrain land colour"): flat 320 below row 129, a +3/row ramp to 1024 at row 362, flat
/// above - the same raw `disp` byte barely moves the row on low/flat ground and up to ~3.2x as much on high
/// ground. A fixed curve baked once at level load in the original, a pure function of `row` here.
pub fn disp_sensitivity(row: usize) -> i32 {
    (3 * row as i32 - 64).clamp(320, 1024)
}

impl Theme {
    pub fn parse(pal: &[u8], bigfade: &[u8], disp: &[u8]) -> Result<Self, LevelError> {
        let check = |d: &[u8], n: usize| {
            if d.len() < n { Err(LevelError::BadSize { expected: n, got: d.len() }) } else { Ok(()) }
        };
        check(pal, 1024)?;
        check(bigfade, BIGFADE_WIDTH * BIGFADE_ROWS)?;
        check(disp, DISP_SIZE * DISP_SIZE)?;
        let identity = |width: usize, rows: usize| (0..rows * width).map(|i| (i % width) as u8).collect();
        Ok(Theme {
            palette: pal.chunks_exact(4).take(256).map(|c| [c[0], c[1], c[2]]).collect(),
            bigfade: bigfade[..BIGFADE_WIDTH * BIGFADE_ROWS].to_vec(),
            disp: disp[..DISP_SIZE * DISP_SIZE].to_vec(),
            water: disp[..DISP_SIZE * DISP_SIZE].to_vec(),
            cliff: identity(CLIFF_WIDTH, CLIFF_ROWS),
            fade: identity(FADE_WIDTH, FADE_ROWS),
        })
    }

    /// Uses `watdisp` as the sea noise.
    pub fn with_water(self, watdisp: &[u8]) -> Result<Self, LevelError> {
        let water = watdisp.get(..DISP_SIZE * DISP_SIZE).ok_or(LevelError::BadSize { expected: DISP_SIZE * DISP_SIZE, got: watdisp.len() })?;
        Ok(Theme { water: water.to_vec(), ..self })
    }

    /// Uses `cliff0-X.dat` for the cliff/lava/ash remap (terrain-textures.md "cliff").
    pub fn with_cliff(self, data: &[u8]) -> Result<Self, LevelError> {
        let cliff = data.get(..CLIFF_WIDTH * CLIFF_ROWS).ok_or(LevelError::BadSize { expected: CLIFF_WIDTH * CLIFF_ROWS, got: data.len() })?;
        Ok(Theme { cliff: cliff.to_vec(), ..self })
    }

    /// Uses `fade0-X.dat` (terrain-textures.md "fade"), loaded but not consumed yet.
    pub fn with_fade(self, data: &[u8]) -> Result<Self, LevelError> {
        let fade = data.get(..FADE_WIDTH * FADE_ROWS).ok_or(LevelError::BadSize { expected: FADE_WIDTH * FADE_ROWS, got: data.len() })?;
        Ok(Theme { fade: fade.to_vec(), ..self })
    }

    pub fn load(data_dir: &Path, index: u8) -> Result<Self, LevelError> {
        let c = theme_char(index);
        let read = |p: &str| std::fs::read(data_dir.join(format!("{p}0-{c}.dat")));
        let theme = Self::parse(&read("pal")?, &read("bigf")?, &read("disp")?)?;
        let theme = match read("cliff") {
            Ok(d) => theme.with_cliff(&d)?,
            Err(_) => theme,
        };
        let theme = match read("fade") {
            Ok(d) => theme.with_fade(&d)?,
            Err(_) => theme,
        };
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

    /// `bigfade` read as one flat array (terrain-textures.md "bigfade"/"Our bake"): the original's own row-jitter
    /// is added directly to this flat index rather than to the row alone, so a large jitter or brightness value
    /// can legitimately roll over into the next row - not guarded against there, so not guarded against here
    /// either, only clamped to the table's own bounds.
    pub fn bigfade_flat(&self, index: i32) -> u8 {
        let max = (self.bigfade.len() - 1) as i32;
        self.bigfade[index.clamp(0, max) as usize]
    }

    /// The cliff/lava/ash remap (terrain-textures.md "cliff"): `row` = blend / 4 (0..63), `colour` a land colour
    /// (a `shade`/`bigfade_flat` output < `CLIFF_WIDTH`) - anything else (a water colour) passes through
    /// unchanged, the table has no entry for it.
    pub fn cliff(&self, row: usize, colour: u8) -> u8 {
        if colour as usize >= CLIFF_WIDTH {
            return colour;
        }
        let row = row.min(CLIFF_ROWS - 1);
        self.cliff[row * CLIFF_WIDTH + colour as usize]
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

    #[test]
    fn cliff_is_identity_until_a_real_file_is_loaded() {
        let t = Theme::parse(&[0; 1024], &vec![0; BIGFADE_WIDTH * BIGFADE_ROWS], &[0; DISP_SIZE * DISP_SIZE]).unwrap();
        assert_eq!((t.cliff(0, 5), t.cliff(63, 5)), (5, 5), "identity row, every row the same");
        assert_eq!(t.cliff(0, 200), 200, "not a land colour (>= CLIFF_WIDTH): passed through");
    }

    #[test]
    fn with_cliff_loads_the_real_table_rows_clamp_and_sizes_are_checked() {
        let t = Theme::parse(&[0; 1024], &vec![0; BIGFADE_WIDTH * BIGFADE_ROWS], &[0; DISP_SIZE * DISP_SIZE]).unwrap();
        let mut data = vec![0u8; CLIFF_WIDTH * CLIFF_ROWS];
        data[5 * CLIFF_WIDTH + 7] = 99;
        data[(CLIFF_ROWS - 1) * CLIFF_WIDTH + 7] = 42;
        let t = t.with_cliff(&data).unwrap();
        assert_eq!(t.cliff(5, 7), 99);
        assert_eq!(t.cliff(500, 7), 42, "row clamps to the last one");
        assert!(Theme::parse(&[0; 1024], &vec![0; BIGFADE_WIDTH * BIGFADE_ROWS], &[0; DISP_SIZE * DISP_SIZE]).unwrap().with_cliff(&[1, 2]).is_err());
    }

    #[test]
    fn with_fade_checks_its_size_but_nothing_reads_it_yet() {
        let t = Theme::parse(&[0; 1024], &vec![0; BIGFADE_WIDTH * BIGFADE_ROWS], &[0; DISP_SIZE * DISP_SIZE]).unwrap();
        assert!(t.clone().with_fade(&vec![0; FADE_WIDTH * FADE_ROWS]).is_ok());
        assert!(t.with_fade(&[1, 2]).is_err());
    }

    #[test]
    fn bigfade_flat_reads_the_table_as_one_array_and_clamps_at_its_bounds() {
        let mut big = vec![0u8; BIGFADE_WIDTH * BIGFADE_ROWS];
        big[300] = 9;
        big[BIGFADE_WIDTH * BIGFADE_ROWS - 1] = 77;
        let t = Theme::parse(&[0; 1024], &big, &[0; DISP_SIZE * DISP_SIZE]).unwrap();
        assert_eq!(t.bigfade_flat(300), 9);
        assert_eq!(t.bigfade_flat(-5), t.bigfade_flat(0), "clamps below zero");
        assert_eq!(t.bigfade_flat(1_000_000), 77, "clamps past the end, a large jitter rolls no further");
    }

    #[test]
    fn disp_sensitivity_is_flat_below_129_ramps_then_flat_above_362() {
        assert_eq!(disp_sensitivity(0), 320);
        assert_eq!(disp_sensitivity(128), 320);
        assert_eq!(disp_sensitivity(200), 3 * 200 - 64);
        assert_eq!(disp_sensitivity(362), 1022);
        assert_eq!(disp_sensitivity(1000), 1024);
    }
}
