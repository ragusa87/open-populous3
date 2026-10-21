//! Fully generated landscape theme, same shape as the original files
//! (palette + bigfade + disp + water noise), so the texture bake is identical with or
//! without the original game data.

use pop3_format::theme::{Theme, BIGFADE_LAND_ROW, BIGFADE_ROWS, BIGFADE_WIDTH, CLIFF_ROWS, CLIFF_WIDTH, DISP_SIZE, FADE_ROWS, FADE_WIDTH};

/// Palette = 8 colour bands x 32 brightness levels.
const LEVELS: usize = 32;
/// First bigfade row where each of the 8 fallback bands starts (terrain-textures.md "Themes seen", TODO.md
/// "Terrain" — recalibrated 2026-10-21 for the real `height + 75 + (75 if blend != 0)` row formula, which the
/// old `POP3_ROW_SCALE`-era boundaries below were never updated for: `blend` alone adds ~150 to almost any
/// non-flat land cell, so ordinary modest terrain (height a few hundred, not a mountain) now reaches rows these
/// boundaries used to reserve for steep/tall ground, reading unexpectedly dark/near-black. Shifted so "grass"
/// (not "dark grass"/"rock") covers the bulk of ordinary terrain, and the top band only the genuine high end —
/// `altitude_cell` clamps the row actually picked per-cell to 1022, so nothing above that ever triggers from
/// height alone (only `row_jitter`'s own roll-over can still reach further into the table).
///
/// "Sand" starts well *below* `BIGFADE_LAND_ROW` (128), not at it (found 2026-10-21, `sandbox_spells`'
/// reincarnation site reading as deep water): `row_jitter`'s own scale fix (TODO.md "Terrain") made it a
/// genuine per-ROW mover, and this generator's low-frequency `disp_noise` commonly dips tens of rows negative
/// across a *whole region*, not just scattered pixels - flat land just above the threshold (height 64, blend 0,
/// `altitude` 139) can read dozens of rows lower than its own altitude across its entire footprint, landing
/// solidly in rows `BAND_STARTS` used to call "shallow water". Since `ground.wgsl`'s shared formula must stay
/// exe-faithful (no clamp to add there) and this fallback's own bigfade table is the one thing fully ours to
/// shape, rows 100..127 - reachable by both the dedicated water lookup and an unlucky land jitter - are "sand"
/// now, so a land cell that dips there reads as a plausible wet shore, not committed ocean.
const BAND_STARTS: [usize; 8] = [0, 32, 100, 170, 260, 620, 860, 990];

/// Base colour per band, same order as `BAND_STARTS`. The last (highest) band is deliberately not a fixed
/// "snow white": real themes diverge a lot at the row ceiling (terrain-textures.md "Themes seen" — only one of
/// the sampled themes clearly goes near-white there, others lean pale blue or don't peak at all, TODO.md
/// "Terrain"), so a generated world picks one of a few plausible extreme-ground tones per seed instead of
/// guaranteeing snow every time, same spirit as the rest of this generator being "a theme", not "the theme".
const HIGH_GROUND: [[u8; 3]; 4] = [
    [236, 236, 240], // pale snow
    [190, 200, 210], // pale blue-grey rock
    [168, 150, 120], // sun-bleached sandy rock
    [140, 130, 122], // plain weathered rock, barely lighter than the band below
];

fn band_colours(seed: u32) -> [[u8; 3]; 8] {
    let high = HIGH_GROUND[(hash01(0xC0FFEE, 0, seed) * HIGH_GROUND.len() as f32) as usize % HIGH_GROUND.len()];
    [[18, 52, 88], [36, 92, 118], [206, 186, 128], [138, 160, 72], [84, 128, 52], [60, 92, 40], [118, 104, 88], high]
}

pub fn generate(seed: u32) -> Theme {
    let colours = band_colours(seed);
    let mut palette = Vec::with_capacity(256);
    for (band, rgb) in colours.iter().enumerate() {
        for level in 0..LEVELS {
            let light = 1.0 - level as f32 / (LEVELS - 1) as f32 * 0.85;
            let tint = 0.94 + 0.06 * hash01(band as u32, level as u32, seed);
            palette.push(rgb.map(|c| (c as f32 * light * tint).min(255.0) as u8));
        }
    }
    let mut bigfade = Vec::with_capacity(BIGFADE_WIDTH * BIGFADE_ROWS);
    for row in 0..BIGFADE_ROWS {
        for col in 0..BIGFADE_WIDTH {
            let jitter = (hash01(row as u32, col as u32, seed ^ 0xB16F) * 48.0) as i32 - 24;
            let band = band_for(row as i32 + jitter, row);
            bigfade.push((band * LEVELS + col * LEVELS / BIGFADE_WIDTH) as u8);
        }
    }
    Theme { palette, bigfade, disp: disp_noise(seed, DISP_AMPLITUDE), water: water_noise(seed ^ 0x5EA), cliff: synthetic_cliff(), fade: identity_fade() }
}

/// Our own shape, not traced data (terrain-textures.md "cliff": row 0 identity, higher rows shift toward
/// rockier colours), only loosely following the original file's description: `Theme::cliff`'s own land-colour
/// guard (`colour < CLIFF_WIDTH`) means only this generator's water/sand/light-grass bands (palette indices
/// 0..127, see `BANDS`) are ever reachable through it - lower fidelity here than the original-data path, as
/// elsewhere in `--no-original` mode.
fn synthetic_cliff() -> Vec<u8> {
    let mut cliff = Vec::with_capacity(CLIFF_WIDTH * CLIFF_ROWS);
    for row in 0..CLIFF_ROWS {
        let strength = ((row as i32 - 16).max(0) as f32 / (CLIFF_ROWS - 16) as f32).min(1.0);
        for colour in 0..CLIFF_WIDTH {
            cliff.push((colour + (strength * 40.0) as usize).min(CLIFF_WIDTH - 1) as u8);
        }
    }
    cliff
}

/// Identity: nothing reads `fade` yet (terrain-textures.md "fade", TODO.md "Terrain").
fn identity_fade() -> Vec<u8> {
    (0..FADE_ROWS * FADE_WIDTH).map(|i| (i % FADE_WIDTH) as u8).collect()
}

/// Band for a (jittered) row; water and land never bleed into each other.
fn band_for(jittered: i32, row: usize) -> usize {
    let is_land = row >= BIGFADE_LAND_ROW;
    let r = if is_land { jittered.max(BIGFADE_LAND_ROW as i32) } else { jittered.min(BIGFADE_LAND_ROW as i32 - 1) };
    BAND_STARTS.iter().rposition(|&start| r >= start as i32).unwrap_or(0)
}

/// How far `disp`'s own noise swings from a *signed* zero, as a fraction of the full signed range (TODO.md
/// "Terrain", 2026-10-21, `sandbox_spells`' reincarnation site): at the full range, `row_jitter`'s now-faithful
/// row scale (fixed from the old column-scale bug) swings flat, walkable land near the water threshold down
/// across the bigfade table's own water rows, visible as a fine vein pattern threading through otherwise-dry
/// ground - `disp_sensitivity`'s own 320 floor (never smaller, even on the lowest/flattest land) means there is
/// no "gentle near the shore" built into the shared, faithful formula to lean on instead. A value noise "centred
/// on 128" (the UNSIGNED byte's own midpoint) is actually centred on the SIGNED range's own *extreme* once read
/// as `i8` (raw 128 decodes to signed -128, not a small value - signed-near-zero needs raw bytes near 0 or near
/// 255), so taming it has to shrink the signed swing directly, not just narrow how close the raw byte gets to
/// 128. `water` keeps the full, uncompressed range (its own `water_noise`) - it isn't vulnerable to this, reads
/// the byte unsigned, not signed, and the sea's own colour variety (shallow vs. deep) wants the contrast.
const DISP_AMPLITUDE: f32 = 0.35;

/// `water`'s own noise: unsigned, full range, centred on the unsigned midpoint 128 (`water_at`'s own reading is
/// a plain 0..255 unsigned byte, no sign reinterpretation) - see `disp_noise`'s own doc comment for why `disp`
/// needs a different mapping instead of reusing this one at a smaller scale.
fn water_noise(seed: u32) -> Vec<u8> {
    (0..DISP_SIZE * DISP_SIZE).map(|i| (noise01(i % DISP_SIZE, i / DISP_SIZE, seed) * 255.0) as u8).collect()
}

/// `disp`'s own noise: signed, centred on the signed zero (raw byte 0, wrapping to 255 going negative), swinging
/// `amplitude` (0..1) of the full signed range before wrapping into a raw byte - see the doc comment on
/// `DISP_AMPLITUDE` for why this is a different centring than `water_noise`'s, not just a smaller version of it.
fn disp_noise(seed: u32, amplitude: f32) -> Vec<u8> {
    (0..DISP_SIZE * DISP_SIZE)
        .map(|i| {
            let centred = noise01(i % DISP_SIZE, i / DISP_SIZE, seed) * 2.0 - 1.0; // -1..1
            (centred * 127.0 * amplitude) as i32 as u8
        })
        .collect()
}

/// Tileable value noise, 4 octaves, in 0..1.
fn noise01(x: usize, y: usize, seed: u32) -> f32 {
    let mut v = 0.0;
    let mut amp = 0.5;
    for (octave, period) in [64usize, 32, 16, 4].into_iter().enumerate() {
        v += amp * value_noise(x, y, period, seed.wrapping_add(octave as u32));
        amp *= 0.5;
    }
    v / 0.9375
}

fn value_noise(x: usize, y: usize, period: usize, seed: u32) -> f32 {
    let cells = (DISP_SIZE / period) as u32;
    let (cx, cy) = ((x / period) as u32, (y / period) as u32);
    let (fx, fy) = ((x % period) as f32 / period as f32, (y % period) as f32 / period as f32);
    let corner = |dx: u32, dy: u32| hash01((cx + dx) % cells, (cy + dy) % cells, seed);
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (smooth(fx), smooth(fy));
    let top = corner(0, 0) + (corner(1, 0) - corner(0, 0)) * sx;
    let bot = corner(0, 1) + (corner(1, 1) - corner(0, 1)) * sx;
    top + (bot - top) * sy
}

fn hash01(x: u32, y: u32, seed: u32) -> f32 {
    let mut v = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77) ^ seed.wrapping_mul(0xC2B2_AE3D);
    v ^= v >> 15;
    v = v.wrapping_mul(0x2C1B_3C6D);
    v ^= v >> 13;
    (v >> 8) as f32 / (1u32 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_original_shape_and_is_deterministic() {
        let t = generate(1);
        assert_eq!(t.palette.len(), 256);
        assert_eq!(t.bigfade.len(), BIGFADE_WIDTH * BIGFADE_ROWS);
        assert_eq!(t.disp.len(), DISP_SIZE * DISP_SIZE);
        assert_eq!(t.bigfade, generate(1).bigfade);
    }

    #[test]
    fn water_rows_are_blue_and_land_is_not() {
        let t = generate(3);
        let [r, _, b] = t.shade(10, 0);
        assert!(b > r, "water is blue");
        let [r, _, b] = t.shade(BIGFADE_LAND_ROW + 2, 0);
        assert!(r > b, "shore is sand");
    }

    #[test]
    fn the_high_ground_band_is_not_always_snow_white() {
        // Not every seed should land on the one pure-white `HIGH_GROUND` entry (terrain-textures.md "Themes
        // seen": only one sampled theme clearly does this in the real game) - a handful of seeds should land on
        // every other entry too.
        let distinct: std::collections::HashSet<_> = (0..40).map(|seed| band_colours(seed)[7]).collect();
        assert!(distinct.len() > 1, "varies across seeds instead of being hardcoded");
    }

    #[test]
    fn the_high_ground_band_still_reads_lighter_than_rock_below_it() {
        for seed in 0..10 {
            let t = generate(seed);
            let sum = |c: [u8; 3]| c.iter().map(|&v| v as u32).sum::<u32>();
            assert!(
                sum(t.shade(BIGFADE_ROWS - 1, 0)) > sum(t.shade(860, 0)),
                "seed {seed}: the top band should still read lighter than rock, whichever tone it picked"
            );
        }
    }

    #[test]
    fn ordinary_modest_terrain_reads_as_grass_not_rock() {
        // The real row formula (`height + 75 + 75 if blend != 0`) adds ~150 to almost any non-flat land cell -
        // a modest hill (height 300) should still land in "grass", not the "dark grass"/"rock" bands the old,
        // uncalibrated boundaries pushed it into (TODO.md "Terrain").
        let row = 300 + 150; // altitude_cell's own formula, blend != 0
        let band = band_for(row as i32, row);
        assert_eq!(band, 4, "row {row} should fall in the \"grass\" band (index 4), not dark grass/rock");
    }

    #[test]
    fn a_flat_land_cells_own_jitter_undershoot_still_reads_sandy_not_deep_water() {
        // sandbox_spells' reincarnation site (TODO.md "Terrain", 2026-10-21): flat land just above the
        // land/water threshold (height 64, blend 0 -> altitude 139) whose own `row_jitter` dips tens of rows
        // negative (this generator's low-frequency `disp_noise` commonly does, across a whole region, not just
        // scattered pixels) must not land on a row that reads as deep/shallow water - band index 0 or 1.
        let row = 139 - 35; // altitude 139, a realistic jitter undershoot observed in practice
        let band = band_for(row as i32, row);
        assert!(band >= 2, "row {row} (band {band}) should read as sand or better, not water");
    }

    #[test]
    fn brightness_columns_darken() {
        let t = generate(5);
        let sum = |c: [u8; 3]| c.iter().map(|&v| v as u32).sum::<u32>();
        assert!(sum(t.shade(400, 0)) > sum(t.shade(400, 255)));
    }

    #[test]
    fn disp_tiles_seamlessly() {
        let t = generate(7);
        let d = |x, y| t.disp(x, y) as i32;
        assert!((d(255, 10) - d(0, 10)).abs() < 40, "wraps without a seam");
    }
}
