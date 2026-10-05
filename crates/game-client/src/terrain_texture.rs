//! Bake the ground texture the way the original does it: per pixel, the (bilinear)
//! height plus `disp` noise picks a `bigfade` row, slope lighting picks the brightness
//! column, the palette gives the colour. Pure function, no Bevy.

use game_core::terrain::Heightmap;
use pop3_format::theme::{Theme, BIGFADE_LAND_ROW};

pub const PX_PER_CELL: usize = 8;

/// Light direction used for baked shading (normalised in `bake`).
pub const SUN: [f32; 3] = [0.6, 0.65, 0.45];

/// Returns RGBA8 pixels, `(size * PX_PER_CELL)²`, tiling seamlessly like the map.
pub fn bake(map: &Heightmap, theme: &Theme, height_scale: f32) -> (usize, Vec<u8>) {
    let side = map.size() * PX_PER_CELL;
    let len = (SUN[0] * SUN[0] + SUN[1] * SUN[1] + SUN[2] * SUN[2]).sqrt();
    let sun = SUN.map(|c| c / len);
    let step = 1.0 / PX_PER_CELL as f32;
    let mut out = Vec::with_capacity(side * side * 4);
    for py in 0..side {
        for px in 0..side {
            let (x, z) = (px as f32 * step, py as f32 * step);
            let h = map.sample(x, z);
            let disp = theme.disp(px, py) as i32;
            let row = if h < 1.0 {
                (disp / 2) as usize
            } else {
                (BIGFADE_LAND_ROW as i32 + h as i32 + (disp - 128) / 2).max(BIGFADE_LAND_ROW as i32) as usize
            };
            let gx = (map.sample(x + 0.5, z) - map.sample(x - 0.5, z)) * height_scale;
            let gz = (map.sample(x, z + 0.5) - map.sample(x, z - 0.5)) * height_scale;
            let n_len = (gx * gx + 1.0 + gz * gz).sqrt();
            let lambert = ((-gx * sun[0] + sun[1] - gz * sun[2]) / n_len).max(0.0);
            let brightness = ((1.0 - (0.3 + 0.7 * lambert)) * 255.0).clamp(0.0, 255.0) as u8;
            let [r, g, b] = theme.shade(row, brightness);
            out.extend([r, g, b, 255]);
        }
    }
    (side, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pop3_format::theme::{BIGFADE_ROWS, BIGFADE_WIDTH, DISP_SIZE};

    /// Palette index = row band (0 water, 1 land) so we can check the row choice.
    fn test_theme() -> Theme {
        let mut pal = vec![0u8; 1024];
        pal[4..7].copy_from_slice(&[0, 200, 0]);
        pal[0..3].copy_from_slice(&[0, 0, 200]);
        let mut big = vec![0u8; BIGFADE_WIDTH * BIGFADE_ROWS];
        big[BIGFADE_LAND_ROW * BIGFADE_WIDTH..].fill(1);
        Theme::parse(&pal, &big, &vec![128; DISP_SIZE * DISP_SIZE]).unwrap()
    }

    #[test]
    fn water_and_land_use_their_rows() {
        let mut m = Heightmap::new(4);
        m.set(2, 2, 500);
        let (side, px) = bake(&m, &test_theme(), 1.0 / 384.0);
        assert_eq!(side, 4 * PX_PER_CELL);
        assert_eq!(&px[0..3], &[0, 0, 200], "cell (0,0) is water");
        let i = (2 * PX_PER_CELL * side + 2 * PX_PER_CELL) * 4;
        assert_eq!(&px[i..i + 3], &[0, 200, 0], "cell (2,2) is land");
    }
}
