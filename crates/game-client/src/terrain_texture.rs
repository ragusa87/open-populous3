//! Bake the ground texture the way the original does it: per pixel, the (bilinear)
//! height plus `disp` noise picks a `bigfade` row (the sea: `watdisp` noise over the water rows),
//! slope lighting picks the brightness column, the palette gives the colour. Land, however low,
//! starts past the theme's watery shore rows (`land_row`) so it never looks like the sea the
//! simulation does not see there, and the coast is drawn round every walkable cell (`coast_land`).
//! Pure function, no Bevy.

use game_core::ashes::{Ashes, DARK, FADED};
use game_core::terrain::{DirtyRect, Heightmap};
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

/// Merges `rects` that touch or overlap into their union, repeatedly until none do. A tick can dirty a cluster
/// of adjacent single-cell edits (lava spreading one cell at a time, `game_core::lava`'s own `grow`): baking each
/// separately redoes `REBAKE_MARGIN`'s neighbourhood over and over across the whole cluster, instead of once over
/// its actual extent. Rects from the same local edit never cross the map's own wrap seam, so this compares raw
/// coordinates and does not itself need to wrap; it only ever reduces the rect count, never a rect's own union, so
/// skipping it (or an edit case it misses) costs performance, not correctness.
pub fn merge_dirty_rects(rects: &[DirtyRect]) -> Vec<DirtyRect> {
    let touches = |a: &DirtyRect, b: &DirtyRect| a.min.0 <= b.max.0 + 1 && b.min.0 <= a.max.0 + 1 && a.min.1 <= b.max.1 + 1 && b.min.1 <= a.max.1 + 1;
    let union = |a: &DirtyRect, b: &DirtyRect| DirtyRect { min: (a.min.0.min(b.min.0), a.min.1.min(b.min.1)), max: (a.max.0.max(b.max.0), a.max.1.max(b.max.1)) };
    let mut out = rects.to_vec();
    loop {
        let mut merged_any = false;
        let mut i = 0;
        while i < out.len() {
            let mut j = i + 1;
            while j < out.len() {
                if touches(&out[i], &out[j]) {
                    out[i] = union(&out[i], &out[j]);
                    out.remove(j);
                    merged_any = true;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
        if !merged_any {
            return out;
        }
    }
}

/// Returns RGBA8 pixels, `(size * PX_PER_CELL)²`, tiling seamlessly like the map.
/// `row_scale` multiplies height before picking the bigfade row (1.0 = 1:1, unverified).
pub fn bake(map: &Heightmap, theme: &Theme, height_scale: f32, row_scale: f32) -> (usize, Vec<u8>) {
    let side = map.size() * PX_PER_CELL;
    let mut out = vec![0; side * side * 4];
    let all = DirtyRect { min: (0, 0), max: (map.size() as i32, map.size() as i32) };
    rebake(map, &walkable_cells(map), theme, height_scale, row_scale, false, all, &mut out);
    (side, out)
}

/// Cells around a height change whose pixels it reaches: the slope light and the coast read half a cell
/// and one cell around each point.
pub const REBAKE_MARGIN: i32 = 2;

/// Bakes again, into the whole texture `pixels` (`bake`), the pixels of the cells in `rect` (wrapping) and
/// `REBAKE_MARGIN` around them, after their heights changed; `grid`: with the cell borders (`draw_grid`).
/// `walkable` (`walkable_cells`) is the caller's to compute once and share: a whole-map scan, not worth repeating
/// per rect when a single tick can dirty hundreds of them (an ongoing terrain effect).
pub fn rebake(map: &Heightmap, walkable: &[bool], theme: &Theme, height_scale: f32, row_scale: f32, grid: bool, rect: DirtyRect, pixels: &mut [u8]) {
    let (n, side) = (map.size() as i32, map.size() * PX_PER_CELL);
    let len = (SUN[0] * SUN[0] + SUN[1] * SUN[1] + SUN[2] * SUN[2]).sqrt();
    let sun = SUN.map(|c| c / len);
    let step = 1.0 / PX_PER_CELL as f32;
    let (disp_mean, land_start) = (theme.disp_mean(), theme.land_start_row());
    let span = |lo: i32, hi: i32| {
        let (lo, hi) = (lo - REBAKE_MARGIN, hi + REBAKE_MARGIN);
        if hi - lo >= n { 0..n } else { lo..hi }
    };
    for cz in span(rect.min.1, rect.max.1) {
        for cx in span(rect.min.0, rect.max.0) {
            let (cx, cz) = (cx.rem_euclid(n) as usize, cz.rem_euclid(n) as usize);
            for py in cz * PX_PER_CELL..(cz + 1) * PX_PER_CELL {
                for px in cx * PX_PER_CELL..(cx + 1) * PX_PER_CELL {
                    let (x, z) = (px as f32 * step, py as f32 * step);
                    let h = map.sample(x, z);
                    let disp = theme.disp(px, py) as i32;
                    let row = if !coast_land(walkable, map.size(), x, z) {
                        sea_row(theme.water(px, py))
                    } else {
                        land_row(h * row_scale, disp, disp_mean, land_start)
                    };
                    let gx = (map.sample(x + 0.5, z) - map.sample(x - 0.5, z)) * height_scale;
                    let gz = (map.sample(x, z + 0.5) - map.sample(x, z - 0.5)) * height_scale;
                    let n_len = (gx * gx + 1.0 + gz * gz).sqrt();
                    let lambert = ((-gx * sun[0] + sun[1] - gz * sun[2]) / n_len).max(0.0);
                    let brightness = ((1.0 - (0.3 + 0.7 * lambert)) * 255.0).clamp(0.0, 255.0) as u8;
                    let mut rgb = theme.shade(row, brightness);
                    if grid && (px % PX_PER_CELL == 0 || py % PX_PER_CELL == 0) {
                        rgb = rgb.map(grid_shade);
                    }
                    let i = (py * side + px) * 4;
                    pixels[i..i + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
                }
            }
        }
    }
}

/// Our colour of ashes, and how much of it covers the ground at its darkest.
const ASH: [f32; 3] = [38.0, 33.0, 30.0];
const ASH_COVER: f32 = 0.75;

/// `cell`'s own ash coverage, 0 (no ashes there) to `ASH_COVER` (as dark as ashes ever get): sampled at that one
/// cell directly, no averaging with neighbours yet (pop3-rev-analysis.md "Building ashes": "each corner samples
/// that ONE cell's own [blend] directly") — `char_ashes` is what blends it into its neighbours.
fn ash_k(ashes: &Ashes, cell: (i32, i32)) -> f32 {
    ashes.darkness(cell).map_or(0.0, |dark| ASH_COVER * dark.saturating_sub(FADED) as f32 / (DARK - FADED) as f32)
}

/// `ash_k` bilinearly interpolated between the 4 cells around the point `(x, z)` (cell coordinates, fractional,
/// wrapped), the exact same technique `Heightmap::sample`/`land_row`'s own height lookup already uses: this
/// per-pixel smoothing, not anything ash-specific, is what gives a patch its soft, organic-looking edge instead
/// of a hard-edged block per cell — the very same mechanism `lava.rs` [client]'s own `ground_mesh_grid` uses for
/// still-molten lava, so a patch keeps the identical shape as it cools from one into the other (module doc there).
fn ash_k_bilinear(ashes: &Ashes, size: i32, x: f32, z: f32) -> f32 {
    let (x0, z0) = (x.floor(), z.floor());
    let (fx, fz) = (x - x0, z - z0);
    let (x0, z0) = (x0 as i32, z0 as i32);
    let k = |dx: i32, dz: i32| ash_k(ashes, ((x0 + dx).rem_euclid(size), (z0 + dz).rem_euclid(size)));
    let top = k(0, 0) * (1.0 - fx) + k(1, 0) * fx;
    let bot = k(0, 1) * (1.0 - fx) + k(1, 1) * fx;
    top * (1.0 - fz) + bot * fz
}

/// Chars the pixels of the cells under ashes around `rect` (as `rebake` spans it), per pixel by `ash_k_bilinear`:
/// call it after baking those cells.
pub fn char_ashes(ashes: &Ashes, size: usize, rect: DirtyRect, pixels: &mut [u8]) {
    let (n, side) = (size as i32, size * PX_PER_CELL);
    let step = 1.0 / PX_PER_CELL as f32;
    let span = |lo: i32, hi: i32| {
        let (lo, hi) = (lo - REBAKE_MARGIN, hi + REBAKE_MARGIN);
        if hi - lo >= n { 0..n } else { lo..hi }
    };
    for cz in span(rect.min.1, rect.max.1) {
        for cx in span(rect.min.0, rect.max.0) {
            let (cx, cz) = (cx.rem_euclid(n) as usize, cz.rem_euclid(n) as usize);
            for py in cz * PX_PER_CELL..(cz + 1) * PX_PER_CELL {
                for px in cx * PX_PER_CELL..(cx + 1) * PX_PER_CELL {
                    let (x, z) = (px as f32 * step, py as f32 * step);
                    let k = ash_k_bilinear(ashes, n, x, z);
                    if k <= 0.0 {
                        continue;
                    }
                    let i = (py * side + px) * 4;
                    for (c, ash) in pixels[i..i + 3].iter_mut().zip(ASH) {
                        *c = (*c as f32 * (1.0 - k) + ash * k).round() as u8;
                    }
                }
            }
        }
    }
}

/// Brightness (out of 256) of the cell borders `draw_grid` darkens.
const GRID_SHADE: u16 = 190;

fn grid_shade(c: u8) -> u8 {
    (c as u16 * GRID_SHADE / 256) as u8
}

/// Darkens the first row and column of pixels of every cell: a grid along the cell borders.
pub fn draw_grid(pixels: &mut [u8], side: usize) {
    for (i, px) in pixels.chunks_exact_mut(4).enumerate() {
        let (x, y) = (i % side, i / side);
        if x % PX_PER_CELL == 0 || y % PX_PER_CELL == 0 {
            for c in &mut px[..3] {
                *c = grid_shade(*c);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pop3_format::theme::{BIGFADE_ROWS, BIGFADE_WIDTH, DISP_SIZE};

    #[test]
    fn merge_dirty_rects_joins_touching_rects_but_leaves_far_ones_apart() {
        let a = DirtyRect { min: (0, 0), max: (2, 2) };
        let b = DirtyRect { min: (3, 0), max: (4, 2) }; // touches a: one cell past its edge
        let far = DirtyRect { min: (50, 50), max: (51, 51) };
        let merged = merge_dirty_rects(&[a, b, far]);
        assert_eq!(merged.len(), 2, "a and b joined, far stays its own rect");
        assert!(merged.contains(&DirtyRect { min: (0, 0), max: (4, 2) }));
        assert!(merged.contains(&far));
    }

    #[test]
    fn merge_dirty_rects_chains_through_a_middle_rect() {
        // c only touches b, not a directly: still one rect once b pulls them both in.
        let a = DirtyRect { min: (0, 0), max: (0, 0) };
        let b = DirtyRect { min: (1, 0), max: (1, 0) };
        let c = DirtyRect { min: (2, 0), max: (2, 0) };
        assert_eq!(merge_dirty_rects(&[a, c, b]), [DirtyRect { min: (0, 0), max: (2, 0) }]);
    }

    #[test]
    fn merge_dirty_rects_leaves_a_lone_rect_untouched() {
        let a = DirtyRect { min: (5, 5), max: (6, 7) };
        assert_eq!(merge_dirty_rects(&[a]), [a]);
        assert_eq!(merge_dirty_rects(&[]), Vec::<DirtyRect>::new());
    }

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

    #[test]
    fn ashes_char_their_cells_less_as_they_fade() {
        let mut m = Heightmap::new(8);
        (0..8).for_each(|z| (0..8).for_each(|x| m.set(x, z, 500)));
        let (side, mut px) = bake(&m, &test_theme(), 1.0 / 384.0, 1.0);
        // Exactly at a cell's own grid point: `ash_k_bilinear`'s peak there, no blending with any neighbour.
        let at = |px: &[u8], cell: usize| px[(cell * PX_PER_CELL * side + cell * PX_PER_CELL) * 4 + 1];
        let ground = at(&px, 3);
        let mut ashes = Ashes::default();
        ashes.add(vec![(3, 3)]);
        let all = DirtyRect { min: (0, 0), max: (8, 8) };
        char_ashes(&ashes, 8, all, &mut px);
        assert!(at(&px, 3) < ground / 2, "charred, right at its own peak");
        assert_eq!(at(&px, 5), ground, "untouched, far from any ash cell");
        let (_, mut faded) = bake(&m, &test_theme(), 1.0 / 384.0, 1.0);
        ashes.patches[0].darkness = FADED + 4;
        char_ashes(&ashes, 8, all, &mut faded);
        assert!(at(&faded, 3) > at(&px, 3) && at(&faded, 3) < ground, "fading");
    }

    #[test]
    fn ash_k_is_zero_without_ashes_and_full_strength_right_at_a_fresh_patch_s_own_cell() {
        let ashes = Ashes::default();
        assert_eq!(ash_k(&ashes, (3, 3)), 0.0);
        let mut ashes = Ashes::default();
        ashes.add(vec![(3, 3)]);
        assert_eq!(ash_k(&ashes, (3, 3)), ASH_COVER, "a fresh patch starts at DARK, the darkest it gets");
    }

    #[test]
    fn ash_k_bilinear_blends_smoothly_from_an_ash_cell_towards_its_untouched_neighbour() {
        let mut ashes = Ashes::default();
        ashes.add(vec![(3, 3)]);
        let size = 16;
        assert_eq!(ash_k_bilinear(&ashes, size, 3.0, 3.0), ASH_COVER, "right at the ash cell's own point");
        assert_eq!(ash_k_bilinear(&ashes, size, 4.0, 3.0), 0.0, "right at the untouched neighbour's own point");
        let halfway = ash_k_bilinear(&ashes, size, 3.5, 3.0);
        assert!(halfway > 0.0 && halfway < ASH_COVER, "a graded step in between, not a hard edge: {halfway}");
    }

    #[test]
    fn ash_k_bilinear_wraps_on_the_torus() {
        let mut ashes = Ashes::default();
        ashes.add(vec![(0, 0)]);
        assert_eq!(ash_k_bilinear(&ashes, 16, 15.5, 0.0), ash_k_bilinear(&ashes, 16, -0.5, 0.0));
        assert!(ash_k_bilinear(&ashes, 16, 15.5, 0.0) > 0.0, "the far edge wraps right back to the ash cell");
    }

    #[test]
    fn rebaking_a_change_matches_a_full_bake_and_leaves_the_rest() {
        let mut m = Heightmap::new(16);
        m.set(4, 4, 500);
        let theme = test_theme();
        let (side, mut px) = bake(&m, &theme, 1.0 / 384.0, 1.0);
        draw_grid(&mut px, side);
        let far = (4 * PX_PER_CELL * side + 4 * PX_PER_CELL + 3) * 4;
        px[far] = 7;
        m.set(15, 8, 600);
        m.set(0, 8, 600);
        rebake(&m, &walkable_cells(&m), &theme, 1.0 / 384.0, 1.0, true, DirtyRect { min: (15, 8), max: (17, 9) }, &mut px);
        assert_eq!(px[far], 7, "far cells untouched");
        px[far] = 0;
        let mut full = bake(&m, &theme, 1.0 / 384.0, 1.0).1;
        draw_grid(&mut full, side);
        full[far] = 0;
        assert_eq!(px, full, "across the map's edge too");
    }
}
