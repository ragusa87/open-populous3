//! Per-cell ground data (pop3-rev-analysis.md "Terrain land colour", "Building ashes" "Drawn"): small buffers fed
//! to `ground_material.rs`'s GPU textures, which `ground.wgsl` samples per fragment to compute the final colour —
//! a `blend` byte (a general slope measure, pushed into their own ranges by lava and building ashes), a `light`
//! byte (precomputed sunlight-derived brightness), an ash coverage and a walkable flag, bilinearly sampled by
//! ordinary hardware texture filtering; height+blend is precombined into one `altitude` value per cell before
//! upload, since blend's effect on it is a step function, not something linear interpolation could apply after
//! the fact. Pure functions, no Bevy: the GPU-side wiring (building/updating `Image`s from these buffers, the
//! `Material`, the shader) lives in `ground_material.rs`.
//!
//! **Why a shader, not a baked texture** (TODO.md "Terrain", 2026-10-20 entry, docs/specs/terrain-textures.md
//! "Our bake"): the exe has no baked ground texture at all — it recomputes this same formula fresh, per rendered
//! screen pixel, every frame (pure CPU rasterization, no D3D texture). A previous version of this module instead
//! baked the formula once into a fixed 8 px/cell RGBA texture and let the GPU filter/magnify/minify it like an
//! ordinary texture — cheaper to write, but it needed its own mip chain to avoid minification aliasing at
//! distance, which the exe's architecture never needed in the first place. Evaluating the formula per fragment in
//! `ground.wgsl` instead (at the engine's own output resolution) matches the exe's own approach and makes that
//! mip-chain problem moot rather than needing a patch; see git history for the deleted `bake`/`rebake`/
//! `pixel_rgb` CPU path this replaces (not unit-testable here any more — no GPU in `cargo test` — verified
//! visually instead, `just shot`/`just profile`).

use game_core::ashes::{Ashes, DARK, FADED};
use game_core::shading::CellShading;
use game_core::terrain::{DirtyRect, Heightmap};
use game_core::unit::is_sea;
use pop3_format::theme::Theme;

/// How many of the shader's `px`/`py` integer pixel coordinates (the old baked texture's own resolution,
/// `disp`/`water`'s lookup granularity, terrain-textures.md "disp") fall inside one cell: matches the deleted
/// baked texture's own 8 px/cell, kept so the detail-noise granularity reads the same as before.
pub const PX_PER_CELL: usize = 8;

/// A fresh ash patch's own coverage (`channels` texture's B channel, 0 none .. this fully charred): never more
/// than this even at a patch's own darkest (`ash_coverage`).
pub const ASH_COVER: f32 = 0.75;

/// Per cell (row-major), whether walkers may cross it: not sea on all four corners.
pub fn walkable_cells(map: &Heightmap) -> Vec<bool> {
    let n = map.size() as i32;
    (0..n * n).map(|i| !is_sea(map, (i % n, i / n))).collect()
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

/// Cells around a height change whose `blend`/`light` shading values it reaches (`CellShading::recompute`'s own
/// neighbour margin): a dirty rect's channel/altitude texels are rewritten this far past its edge too, so a
/// neighbour whose shading changed without itself being in the rect still gets its new value uploaded.
pub const REBAKE_MARGIN: i32 = 2;

/// The altitude index one cell's own corner contributes before bilinear interpolation (terrain-textures.md
/// "bigfade"): height plus 75, doubled unless the cell is perfectly flat (`blend == 0`) — ported into
/// `ground.wgsl`'s `altitude` texture, bilinearly sampled there exactly as this was bilinearly interpolated by
/// hand before (same reasoning as the deleted `pixel_rgb`: blend's effect is a step function per corner, so it
/// must be folded in before interpolation, not after).
pub fn altitude_cell(height: u16, blend: u8) -> f32 {
    (height as i32 + 75 + if blend != 0 { 75 } else { 0 }).clamp(0, 1022) as f32
}

/// `cell`'s own ash coverage, 0 (no ashes there) to `ASH_COVER` (as dark as ashes ever get): sampled at that one
/// cell directly, no averaging with neighbours (pop3-rev-analysis.md "Building ashes": "each corner samples that
/// ONE cell's own [blend] directly") — bilinear sampling of the `channels` texture this feeds does the
/// neighbour-blending instead, same as `blend`/`light`.
fn ash_coverage(ashes: &Ashes, cell: (i32, i32)) -> f32 {
    ashes.darkness(cell).map_or(0.0, |dark| ASH_COVER * dark.saturating_sub(FADED) as f32 / (DARK - FADED) as f32)
}

/// The per-cell data `ground_material.rs` uploads as GPU textures: `channels` (RGBA8, R blend / G light / B ash
/// coverage / A walkable, `side²`) and `altitude` (one f32 per cell, `side²`) — both bilinearly sampled in
/// `ground.wgsl`, reproducing the old hand-written `bilinear(corner_fn, x, z)` exactly via ordinary hardware
/// texture filtering (a linear, repeat-wrapped sampler over grid-vertex-aligned texels).
pub struct GroundCells {
    pub side: usize,
    pub channels: Vec<u8>,
    pub altitude: Vec<f32>,
}

/// Every cell of `map`, from scratch.
pub fn bake_cells(map: &Heightmap, shading: &CellShading, ashes: &Ashes, walkable: &[bool]) -> GroundCells {
    let side = map.size();
    let mut channels = vec![0u8; side * side * 4];
    let mut altitude = vec![0f32; side * side];
    let all = DirtyRect { min: (0, 0), max: (side as i32, side as i32) };
    rebake_cells(map, shading, ashes, walkable, all, &mut channels, &mut altitude, None);
    GroundCells { side, channels, altitude }
}

/// Rewrites `channels`/`altitude` (`bake_cells`'s own buffers, or the live ones `ground_material.rs` keeps) for
/// the cells in `rect` (wrapping) and `REBAKE_MARGIN` around them, after their heights/shading changed.
///
/// `glide = Some((prev, alpha))` blends each touched cell's own `altitude` value between `prev`'s heightmap (as
/// it stood before the tick that produced `rect`) and `map`'s own live value, at `alpha` (`SimClock`'s fraction
/// of the current tick elapsed) — the same documented simplification the old pixel-level glide used (blend/light
/// are not themselves glided, only read from `shading` as it already stands), moved to the one scalar that now
/// feeds the shader instead of to the finished RGB colour it used to blend directly.
pub fn rebake_cells(map: &Heightmap, shading: &CellShading, ashes: &Ashes, walkable: &[bool], rect: DirtyRect, channels: &mut [u8], altitude: &mut [f32], glide: Option<(&Heightmap, f32)>) {
    let n = map.size() as i32;
    let span = |lo: i32, hi: i32| {
        let (lo, hi) = (lo - REBAKE_MARGIN, hi + REBAKE_MARGIN);
        if hi - lo >= n { 0..n } else { lo..hi }
    };
    for cz in span(rect.min.1, rect.max.1) {
        for cx in span(rect.min.0, rect.max.0) {
            let (ux, uz) = (cx.rem_euclid(n) as usize, cz.rem_euclid(n) as usize);
            let i = uz * map.size() + ux;
            let blend = shading.blend(cx, cz);
            let walk = walkable[i];
            channels[i * 4] = blend;
            channels[i * 4 + 1] = shading.light(cx, cz);
            channels[i * 4 + 2] = (ash_coverage(ashes, (cx, cz)) * 255.0).round() as u8;
            channels[i * 4 + 3] = if walk { 255 } else { 0 };
            let now = altitude_cell(map.get(cx, cz), blend);
            altitude[i] = match glide {
                Some((prev, alpha)) => {
                    let before = altitude_cell(prev.get(cx, cz), blend);
                    before + (now - before) * alpha.clamp(0.0, 1.0)
                }
                None => now,
            };
        }
    }
}

/// The palette as RGBA8 (A unused, padding for a 4-byte-aligned texel): `ground.wgsl`'s `palette` texture.
pub fn palette_rgba(theme: &Theme) -> Vec<u8> {
    let mut out = vec![0u8; 256 * 4];
    for (i, &[r, g, b]) in theme.palette.iter().take(256).enumerate() {
        out[i * 4..i * 4 + 3].copy_from_slice(&[r, g, b]);
        out[i * 4 + 3] = 255;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::shading::SunParams;
    use std::collections::HashMap;

    /// A theme's heightmap fully shaded from scratch: the fixture a test starts from.
    fn shading_for(m: &Heightmap) -> CellShading {
        let mut shading = CellShading::new(m.size(), SunParams::default());
        let whole = DirtyRect { min: (0, 0), max: (m.size() as i32, m.size() as i32) };
        shading.recompute(m, &Ashes::default(), &HashMap::new(), &[whole]);
        shading
    }

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
    fn coast_covers_every_walkable_cell() {
        let mut m = Heightmap::new(8);
        m.set(4, 4, 500);
        let walk = walkable_cells(&m);
        assert_eq!(walk.iter().filter(|&&w| w).count(), 4, "the four cells whose own footprint touches the peak corner");
        // A cell (x, z)'s own footprint is corners (x, z)..(x+1, z+1): the peak at corner (4,4) touches cells
        // (3,3), (4,3), (3,4) and (4,4).
        assert!(walk[3 * 8 + 3] && walk[3 * 8 + 4] && walk[4 * 8 + 3] && walk[4 * 8 + 4]);
        assert!(!walk[0]);
    }

    #[test]
    fn altitude_floors_dead_flat_ground_at_height_plus_75() {
        assert_eq!(altitude_cell(1, 0), 76.0);
    }

    #[test]
    fn altitude_adds_another_75_when_blended() {
        assert_eq!(altitude_cell(100, 0), 175.0);
        assert_eq!(altitude_cell(100, 16), 250.0, "a nonzero blend lifts the row by 75 more");
    }

    #[test]
    fn altitude_clamps_to_1022() {
        assert_eq!(altitude_cell(u16::MAX, 16), 1022.0);
    }

    #[test]
    fn ash_coverage_is_zero_without_ashes_and_full_strength_right_at_a_fresh_patch() {
        let ashes = Ashes::default();
        assert_eq!(ash_coverage(&ashes, (3, 3)), 0.0);
        let mut ashes = Ashes::default();
        ashes.add(vec![(3, 3)]);
        assert_eq!(ash_coverage(&ashes, (3, 3)), ASH_COVER, "a fresh patch starts at DARK, the darkest it gets");
    }

    #[test]
    fn ash_coverage_fades_as_the_patch_ages() {
        let mut ashes = Ashes::default();
        ashes.add(vec![(3, 3)]);
        let fresh = ash_coverage(&ashes, (3, 3));
        ashes.patches[0].darkness = FADED + 4;
        let faded = ash_coverage(&ashes, (3, 3));
        assert!(faded > 0.0 && faded < fresh);
    }

    #[test]
    fn bake_cells_reads_blend_light_ash_and_walkable_into_their_own_channel() {
        let mut m = Heightmap::new(8);
        m.set(4, 4, 500);
        let shading = shading_for(&m);
        let mut ashes = Ashes::default();
        ashes.add(vec![(4, 4)]);
        let walk = walkable_cells(&m);
        let cells = bake_cells(&m, &shading, &ashes, &walk);
        assert_eq!(cells.side, 8);
        let i = 4 * 8 + 4;
        assert_eq!(cells.channels[i * 4], shading.blend(4, 4));
        assert_eq!(cells.channels[i * 4 + 1], shading.light(4, 4));
        assert_eq!(cells.channels[i * 4 + 2], (ASH_COVER * 255.0).round() as u8, "freshly ashed");
        assert_eq!(cells.channels[i * 4 + 3], 255, "walkable");
        assert_eq!(cells.channels[3], 0, "sea corner (cell 0,0), not walkable");
        assert_eq!(cells.altitude[i], altitude_cell(500, shading.blend(4, 4)));
    }

    #[test]
    fn rebaking_a_change_matches_a_full_bake_and_leaves_the_rest() {
        let mut m = Heightmap::new(16);
        m.set(4, 4, 500);
        let ashes = Ashes::default();
        let shading = shading_for(&m);
        let walk = walkable_cells(&m);
        let mut cells = bake_cells(&m, &shading, &ashes, &walk);
        let far = 9 * 16 + 9;
        cells.channels[far * 4] = 7;
        m.set(15, 8, 600);
        m.set(0, 8, 600);
        // As in the real pipeline (`GameMap::recompute_shading`, run before the texture is ever rebaked): `shading`
        // already reflects the fully-committed edit by the time `rebake_cells` reads it.
        let shading = shading_for(&m);
        let walk = walkable_cells(&m);
        rebake_cells(&m, &shading, &ashes, &walk, DirtyRect { min: (15, 8), max: (17, 9) }, &mut cells.channels, &mut cells.altitude, None);
        assert_eq!(cells.channels[far * 4], 7, "far cell untouched");
        cells.channels[far * 4] = 0;
        let mut full = bake_cells(&m, &shading, &ashes, &walk);
        full.channels[far * 4] = 0;
        assert_eq!(cells.channels, full.channels, "across the map's edge too");
        assert_eq!(cells.altitude, full.altitude);
    }

    #[test]
    fn rebake_glide_blends_the_altitude_between_prev_and_now() {
        let mut m = Heightmap::new(8);
        for z in 0..8 {
            for x in 0..8 {
                m.set(x, z, 400);
            }
        }
        let prev = m.clone();
        let ashes = Ashes::default();
        let walk = walkable_cells(&m);
        m.set(4, 4, 900);
        let shading = shading_for(&m);
        let mut cells = bake_cells(&prev, &shading_for(&prev), &ashes, &walk);
        let rect = DirtyRect { min: (4, 4), max: (4, 4) };
        rebake_cells(&m, &shading, &ashes, &walk, rect, &mut cells.channels, &mut cells.altitude, Some((&prev, 0.5)));
        let i = 4 * 8 + 4;
        let before = altitude_cell(400, shading.blend(4, 4));
        let now = altitude_cell(900, shading.blend(4, 4));
        assert_eq!(cells.altitude[i], before + (now - before) * 0.5);
    }

    #[test]
    fn rebake_glide_at_alpha_one_matches_a_plain_rebake() {
        let mut m = Heightmap::new(8);
        for z in 0..8 {
            for x in 0..8 {
                m.set(x, z, 400);
            }
        }
        let prev = m.clone();
        let ashes = Ashes::default();
        let walk = walkable_cells(&m);
        m.set(4, 4, 900);
        let shading = shading_for(&m);
        let rect = DirtyRect { min: (4, 4), max: (4, 4) };

        let mut plain = bake_cells(&prev, &shading_for(&prev), &ashes, &walk);
        rebake_cells(&m, &shading, &ashes, &walk, rect, &mut plain.channels, &mut plain.altitude, None);
        let mut glided = bake_cells(&prev, &shading_for(&prev), &ashes, &walk);
        rebake_cells(&m, &shading, &ashes, &walk, rect, &mut glided.channels, &mut glided.altitude, Some((&prev, 1.0)));
        assert_eq!(glided.altitude, plain.altitude);
    }

    fn test_theme() -> Theme {
        let big = vec![0u8; pop3_format::theme::BIGFADE_WIDTH * pop3_format::theme::BIGFADE_ROWS];
        let disp: Vec<u8> = (0..pop3_format::theme::DISP_SIZE * pop3_format::theme::DISP_SIZE).map(|i| (i % 256) as u8).collect();
        let water: Vec<u8> = (0..pop3_format::theme::DISP_SIZE * pop3_format::theme::DISP_SIZE).map(|i| ((i + 1) % 256) as u8).collect();
        let mut pal = vec![0u8; 1024];
        pal[0..4].copy_from_slice(&[10, 20, 30, 0]);
        Theme::parse(&pal, &big, &disp).unwrap().with_water(&water).unwrap()
    }

    #[test]
    fn palette_rgba_packs_rgb_with_full_alpha() {
        let theme = test_theme();
        let out = palette_rgba(&theme);
        assert_eq!(out.len(), 256 * 4);
        assert_eq!(&out[0..4], &[10, 20, 30, 255]);
    }

}
