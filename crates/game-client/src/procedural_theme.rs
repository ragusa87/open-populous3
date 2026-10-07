//! Fully generated landscape theme, same shape as the original files
//! (palette + bigfade + disp + water noise), so the texture bake is identical with or
//! without the original game data.

use pop3_format::theme::{Theme, BIGFADE_LAND_ROW, BIGFADE_ROWS, BIGFADE_WIDTH, DISP_SIZE};

/// Palette = 8 colour bands x 32 brightness levels.
const LEVELS: usize = 32;
/// sRGB base colour per band and the first bigfade row where it starts.
const BANDS: [([u8; 3], usize); 8] = [
    ([18, 52, 88], 0),     // deep water
    ([36, 92, 118], 64),   // shallow water
    ([206, 186, 128], BIGFADE_LAND_ROW), // sand
    ([138, 160, 72], 180), // light grass
    ([84, 128, 52], 320),  // grass
    ([60, 92, 40], 560),   // dark grass
    ([118, 104, 88], 760), // rock
    ([236, 236, 240], 960), // snow
];

pub fn generate(seed: u32) -> Theme {
    let mut palette = Vec::with_capacity(256);
    for (band, (rgb, _)) in BANDS.iter().enumerate() {
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
    Theme { palette, bigfade, disp: disp_noise(seed), water: disp_noise(seed ^ 0x5EA) }
}

/// Band for a (jittered) row; water and land never bleed into each other.
fn band_for(jittered: i32, row: usize) -> usize {
    let is_land = row >= BIGFADE_LAND_ROW;
    let r = if is_land { jittered.max(BIGFADE_LAND_ROW as i32) } else { jittered.min(BIGFADE_LAND_ROW as i32 - 1) };
    BANDS.iter().rposition(|&(_, start)| r >= start as i32).unwrap_or(0)
}

/// Tileable value noise, 4 octaves, centred on 128.
fn disp_noise(seed: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(DISP_SIZE * DISP_SIZE);
    for y in 0..DISP_SIZE {
        for x in 0..DISP_SIZE {
            let mut v = 0.0;
            let mut amp = 0.5;
            for (octave, period) in [64usize, 32, 16, 4].into_iter().enumerate() {
                v += amp * value_noise(x, y, period, seed.wrapping_add(octave as u32));
                amp *= 0.5;
            }
            out.push((v / 0.9375 * 255.0) as u8);
        }
    }
    out
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
        let [r, g, b] = t.shade(BIGFADE_ROWS - 1, 0);
        assert!(r > 200 && g > 200 && b > 200, "peaks are snow");
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
