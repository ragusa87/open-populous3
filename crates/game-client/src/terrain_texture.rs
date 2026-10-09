//! Bake the ground texture the way the original does it: per pixel, the (bilinear)
//! height plus `disp` noise picks a `bigfade` row (the sea: `watdisp` noise over the water rows),
//! slope lighting picks the brightness column, the palette gives the colour. Land, however low,
//! starts past the theme's watery shore rows (`land_row`) so it never looks like the sea the
//! simulation does not see there, and the coast is drawn round every walkable cell (`coast_land`).
//! Pure function, no Bevy.

use game_core::terrain::Heightmap;
use game_core::unit::is_sea;
use pop3_format::theme::{Theme, BIGFADE_LAND_ROW};

pub const PX_PER_CELL: usize = 8;

/// Light direction used for baked shading (normalised in `bake`).
pub const SUN: [f32; 3] = [0.6, 0.65, 0.45];

/// The water row (below `BIGFADE_LAND_ROW`) for a sea pixel's noise.
pub fn sea_row(water: u8) -> usize {
    water as usize * BIGFADE_LAND_ROW / 256
}

/// The land row for height `h` (scaled) and a `disp` texel: from `land_start` up, the noise
/// centred on the file's mean so it adds detail without shifting the bands.
pub fn land_row(h: f32, disp: i32, disp_mean: i32, land_start: usize) -> usize {
    (land_start as i32 + h as i32 + (disp - disp_mean) / 2).max(land_start as i32) as usize
}

/// Where the walkable-cell field (1 at the centre of a cell walkers may cross, 0 at sea)
/// starts counting as ground: low enough to cover the corner of a lone walkable cell (0.25 there),
/// so the coast passes 0.3 cell past the walkable edge and rounds the corners.
pub const COAST_THRESHOLD: f32 = 0.2;

/// Per cell (row-major), whether walkers may cross it: not sea on all four corners.
pub fn walkable_cells(map: &Heightmap) -> Vec<bool> {
    let n = map.size() as i32;
    (0..n * n).map(|i| !is_sea(map, (i % n, i / n))).collect()
}

/// Whether the point `(x, z)` (cells) is drawn as ground: the walkable field, bilinear between
/// cell centres, reaches `COAST_THRESHOLD`.
pub fn coast_land(walkable: &[bool], size: usize, x: f32, z: f32) -> bool {
    let (x, z) = (x - 0.5, z - 0.5);
    let (x0, z0) = (x.floor(), z.floor());
    let (fx, fz) = (x - x0, z - z0);
    let n = size as i32;
    let w = |dx: i32, dz: i32| {
        let (cx, cz) = ((x0 as i32 + dx).rem_euclid(n), (z0 as i32 + dz).rem_euclid(n));
        walkable[(cz * n + cx) as usize] as u8 as f32
    };
    let top = w(0, 0) * (1.0 - fx) + w(1, 0) * fx;
    let bot = w(0, 1) * (1.0 - fx) + w(1, 1) * fx;
    top * (1.0 - fz) + bot * fz >= COAST_THRESHOLD
}

/// Returns RGBA8 pixels, `(size * PX_PER_CELL)²`, tiling seamlessly like the map.
/// `row_scale` multiplies height before picking the bigfade row (1.0 = 1:1, unverified).
pub fn bake(map: &Heightmap, theme: &Theme, height_scale: f32, row_scale: f32) -> (usize, Vec<u8>) {
    let side = map.size() * PX_PER_CELL;
    let len = (SUN[0] * SUN[0] + SUN[1] * SUN[1] + SUN[2] * SUN[2]).sqrt();
    let sun = SUN.map(|c| c / len);
    let step = 1.0 / PX_PER_CELL as f32;
    let (disp_mean, land_start) = (theme.disp_mean(), theme.land_start_row());
    let walkable = walkable_cells(map);
    let mut out = Vec::with_capacity(side * side * 4);
    for py in 0..side {
        for px in 0..side {
            let (x, z) = (px as f32 * step, py as f32 * step);
            let h = map.sample(x, z);
            let disp = theme.disp(px, py) as i32;
            let row = if !coast_land(&walkable, map.size(), x, z) {
                sea_row(theme.water(px, py))
            } else {
                land_row(h * row_scale, disp, disp_mean, land_start)
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

/// Brightness (out of 256) of the cell borders `draw_grid` darkens.
const GRID_SHADE: u16 = 190;

/// Darkens the first row and column of pixels of every cell: a grid along the cell borders.
pub fn draw_grid(pixels: &mut [u8], side: usize) {
    for (i, px) in pixels.chunks_exact_mut(4).enumerate() {
        let (x, y) = (i % side, i / side);
        if x % PX_PER_CELL == 0 || y % PX_PER_CELL == 0 {
            for c in &mut px[..3] {
                *c = (*c as u16 * GRID_SHADE / 256) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pop3_format::theme::{BIGFADE_ROWS, BIGFADE_WIDTH, DISP_SIZE};

    #[test]
    fn grid_darkens_the_cell_borders_only() {
        let side = 2 * PX_PER_CELL;
        let mut px = vec![200u8; side * side * 4];
        draw_grid(&mut px, side);
        let at = |x: usize, y: usize| px[(y * side + x) * 4];
        assert!(at(0, 3) < 200 && at(PX_PER_CELL, 5) < 200 && at(4, PX_PER_CELL) < 200);
        assert_eq!(at(3, 3), 200);
        assert_eq!(px[3], 200, "alpha kept");
    }

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
    fn low_land_is_past_the_shore_rows_and_noise_is_centred() {
        assert_eq!(land_row(1.0, 45, 88, 146), 146, "low ground on theme 1 stays land");
        assert_eq!(land_row(100.0, 88, 88, 146), 246, "the mean adds nothing");
        assert_eq!(land_row(100.0, 98, 88, 146), 251);
        assert_eq!(land_row(100.0, 162, 162, 136), land_row(100.0, 128, 128, 136), "every theme centred alike");
    }

    #[test]
    fn coast_covers_every_walkable_cell_and_rounds_its_corners() {
        let mut m = Heightmap::new(8);
        m.set(4, 4, 500);
        let walk = walkable_cells(&m);
        let land = |x, z| coast_land(&walk, 8, x, z);
        assert_eq!(walk.iter().filter(|&&w| w).count(), 4, "the four cells around the peak");
        for (x, z) in [(3.0, 3.0), (5.0, 5.0), (3.0, 5.0), (4.0, 4.0), (3.01, 4.0), (4.99, 4.0)] {
            assert!(land(x, z), "walkable point ({x}, {z}) drawn as ground");
        }
        assert!(land(2.8, 4.0), "the coast passes a little past the walkable edge");
        assert!(!land(2.5, 4.0) && !land(4.0, 6.5));
        assert!(!land(2.75, 2.75), "convex corner rounded off");
    }

    #[test]
    fn sea_rows_span_the_water_rows() {
        assert_eq!((sea_row(0), sea_row(128), sea_row(255)), (0, 64, BIGFADE_LAND_ROW - 1));
    }

    #[test]
    fn sea_takes_the_water_noise() {
        let mut pal = vec![0u8; 1024];
        pal[4 * 9..4 * 9 + 3].copy_from_slice(&[9, 9, 9]);
        let mut big = vec![0u8; BIGFADE_WIDTH * BIGFADE_ROWS];
        big[sea_row(250) * BIGFADE_WIDTH..(sea_row(250) + 1) * BIGFADE_WIDTH].fill(9);
        let theme = Theme::parse(&pal, &big, &vec![0; DISP_SIZE * DISP_SIZE]).unwrap().with_water(&vec![250; DISP_SIZE * DISP_SIZE]).unwrap();
        let (_, px) = bake(&Heightmap::new(4), &theme, 1.0 / 384.0, 1.0);
        assert_eq!(&px[0..3], &[9, 9, 9], "watdisp picks the sea row, not disp");
    }

    #[test]
    fn water_and_land_use_their_rows() {
        let mut m = Heightmap::new(4);
        m.set(2, 2, 500);
        let (side, px) = bake(&m, &test_theme(), 1.0 / 384.0, 1.0);
        assert_eq!(side, 4 * PX_PER_CELL);
        assert_eq!(&px[0..3], &[0, 0, 200], "cell (0,0) is water");
        let j = (PX_PER_CELL * side + PX_PER_CELL + 1) * 4;
        assert_eq!(&px[j..j + 3], &[0, 200, 0], "a walkable cell's height-0 corner is ground");
        let i = (2 * PX_PER_CELL * side + 2 * PX_PER_CELL) * 4;
        assert_eq!(&px[i..i + 3], &[0, 200, 0], "cell (2,2) is land");
    }
}
