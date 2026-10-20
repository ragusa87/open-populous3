//! Per-cell shading derived from the heightmap (pop3-rev-analysis.md "Terrain land colour", "Building ashes"
//! "Drawn"): a `blend` byte (a general slope measure, pushed into their own ranges by lava and building ashes), a
//! `light` byte (a precomputed sunlight-derived brightness) and a `downhill` direction, recomputed after every
//! height edit (`GameMap::recompute_shading`) rather than read live at draw time. Feeds the client's terrain
//! texture bake (the cliff/lava/ash row and the brightness column, terrain-textures.md "cliff"/"bigfade"); not
//! itself simulation state read by any `Command`, so it carries no lockstep weight of its own, only the
//! `Heightmap`/`Ashes`/lava state it is a pure function of.

use crate::ashes::Ashes;
use crate::terrain::{DirtyRect, Heightmap};
use pop3_format::level::Sunlight;
use std::collections::HashMap;

/// Three level-global values derived once from the level's own sunlight block: feeds `light` only, never the
/// bigfade row (terrain-textures.md "bigfade": the row is height + blend alone).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SunParams {
    pub x: i32,
    pub z: i32,
    pub base: i32,
}

impl SunParams {
    /// GUESS, not traced (pop3-rev-analysis.md "Open questions"): the exe derives these three signed globals
    /// from `ShadeStart`/`ShadeRange`/`Inclination` via sin/cos tables, but no concrete resulting value was
    /// recovered this session for any Inclination - only the formula's general shape (`cell_light`) and the
    /// three input bytes' own exe defaults (28/15/32) are confirmed. A fixed horizontal bearing (matching this
    /// renderer's previous invented light direction, `terrain_texture::SUN`) stands in for whatever azimuth the
    /// exe actually bakes in, since the level data carries no separate azimuth field at all; `shade_range` scales
    /// its strength (a wider range reads as more contrast between slopes) and `shade_start` sets the base level.
    /// `inclination` plays no role here yet, for lack of a traced effect to reproduce. Replace with the real
    /// values once a spec session reads 0x937AA8/AA/AC live.
    pub fn from_sunlight(s: Sunlight) -> Self {
        let scale = s.shade_range as i32 * 20;
        SunParams { x: scale * 3 / 5, z: scale * 3 / 4, base: s.shade_start as i32 * 10 }
    }
}

impl Default for SunParams {
    /// For generated maps and sandboxes, which have no level header of their own.
    fn default() -> Self {
        Self::from_sunlight(Sunlight::EXE_DEFAULT)
    }
}

/// For the cell and its 8 neighbours, compass order N, E, S, W, NE, SE, SW, NW (pop3-rev-analysis.md "Volcano
/// lava flow": "the engine's standard +-2-unit 3x3 walk"). `-z` is north, matching `map::OUR_MAPS_CAMERA`'s own
/// "looking north (towards -z)".
const COMPASS: [(i32, i32); 8] = [(0, -1), (1, 0), (0, 1), (-1, 0), (1, -1), (1, 1), (-1, 1), (-1, -1)];

/// The original's own general slope measure (pop3-rev-analysis.md "Building ashes" "Drawn"): `clamp((max -
/// min) / 8, 1, 127)` over the cell and its 8 immediate neighbours, 0 only when all 9 share the same height
/// (terrain-textures.md "bigfade"). Lava and ashes override this for their own cells before it is ever computed
/// (`GameMap::recompute_shading`'s priority resolution): the original's own recompute "skips flagged cells"
/// (pop3-rev-analysis.md "Building ashes" "Drawn").
fn general_slope_blend(terrain: &Heightmap, x: i32, z: i32) -> u8 {
    let heights = std::iter::once((0, 0)).chain(COMPASS).map(|(dx, dz)| terrain.get(x + dx, z + dz));
    let (min, max) = heights.fold((u16::MAX, 0u16), |(min, max), h| (min.min(h), max.max(h)));
    if max == min { 0 } else { (((max - min) / 8) as u8).clamp(1, 127) }
}

/// The cell's own downhill compass direction, 1-8 (`COMPASS`'s order), 0 flat: the running minimum starts at the
/// cell's own height, then each of its 8 neighbours in fixed order replaces it (and the direction) only if
/// strictly lower - so a tie among equally-low neighbours is broken by scan order, never at random
/// (pop3-rev-analysis.md "Volcano lava flow").
fn downhill_of(terrain: &Heightmap, x: i32, z: i32) -> u8 {
    let mut min = terrain.get(x, z);
    let mut dir = 0u8;
    for (i, &(dx, dz)) in COMPASS.iter().enumerate() {
        let h = terrain.get(x + dx, z + dz);
        if h < min {
            min = h;
            dir = (i + 1) as u8;
        }
    }
    dir
}

/// The precomputed sunlight-derived brightness byte (pop3-rev-analysis.md "Terrain land colour"): raw neighbour
/// height deltas, 2 cells over on each axis (not the 8-neighbour slope `blend`/`downhill` use - a different,
/// coarser original function), dotted against the level's own sun vector, plus an ambient term from
/// `ambient_nibble` (the `+0xE` low nibble: a placeholder 0 today, see `CellShading::downhill`'s doc).
fn cell_light(terrain: &Heightmap, sun: &SunParams, ambient_nibble: u8, x: i32, z: i32) -> u8 {
    let dx = terrain.get(x, z) as i32 - terrain.get(x + 2, z) as i32;
    let dz = terrain.get(x, z + 2) as i32 - terrain.get(x, z) as i32;
    let ambient = (ambient_nibble as i32 + 8) * 16;
    ((sun.base + dz * sun.z - dx * sun.x) / 350 + ambient).clamp(0, 255) as u8
}

/// Per-cell state derived from the heightmap (+ ashes/lava blend overrides, + sunlight): a cache kept current by
/// `GameMap::recompute_shading`, never itself read by a `Command` or serialised as simulation state in its own
/// right.
#[derive(Clone, Debug)]
pub struct CellShading {
    size: usize,
    blend: Vec<u8>,
    light: Vec<u8>,
    /// High nibble: downhill compass direction (0 flat, 1-8). Low nibble: the original's own `+0xE` ambient-light
    /// term, feeding `light` (pop3-rev-analysis.md "Terrain land colour") - its own derivation isn't traced,
    /// kept 0 (a documented placeholder, see `pop3-rev-analysis.md` "Open questions").
    downhill: Vec<u8>,
    sun: SunParams,
}

impl CellShading {
    pub fn new(size: usize, sun: SunParams) -> Self {
        CellShading { size, blend: vec![0; size * size], light: vec![0; size * size], downhill: vec![0; size * size], sun }
    }

    fn index(&self, x: i32, z: i32) -> usize {
        let s = self.size as i32;
        (z.rem_euclid(s) * s + x.rem_euclid(s)) as usize
    }

    pub fn blend(&self, x: i32, z: i32) -> u8 {
        self.blend[self.index(x, z)]
    }

    pub fn light(&self, x: i32, z: i32) -> u8 {
        self.light[self.index(x, z)]
    }

    pub fn downhill_dir(&self, x: i32, z: i32) -> u8 {
        self.downhill[self.index(x, z)] >> 4
    }

    /// Recomputes `blend`/`light`/`downhill` for every cell touched by `dirty` plus a 2-cell margin (covers
    /// `blend`/`downhill`'s own 8-neighbour reach and `light`'s own 2-cells-away one), wrapping on the torus.
    /// `lava_blend` (a lit lava cell's own current blend, `lava::Lava::cells_lit`) takes priority over
    /// `ashes` (`Ashes::darkness`), which takes priority over the general slope measure - the same priority the
    /// original's own recompute gives its blend-flagged cells (pop3-rev-analysis.md "Building ashes" "Drawn":
    /// "skips flagged cells"). Called from the two places `GameMap` assembles its own `DirtyRect` output
    /// (`apply`, `tick`), never left to an individual caller to remember.
    pub fn recompute(&mut self, terrain: &Heightmap, ashes: &Ashes, lava_blend: &HashMap<(i32, i32), u8>, dirty: &[DirtyRect]) {
        const MARGIN: i32 = 2;
        let s = self.size as i32;
        for rect in dirty {
            for z in rect.min.1 - MARGIN..rect.max.1 + MARGIN {
                for x in rect.min.0 - MARGIN..rect.max.0 + MARGIN {
                    let (cx, cz) = (x.rem_euclid(s), z.rem_euclid(s));
                    let i = (cz * s + cx) as usize;
                    self.blend[i] = lava_blend.get(&(cx, cz)).copied().or_else(|| ashes.darkness((cx, cz))).unwrap_or_else(|| general_slope_blend(terrain, cx, cz));
                    let ambient = self.downhill[i] & 0xF;
                    self.downhill[i] = (downhill_of(terrain, cx, cz) << 4) | ambient;
                    self.light[i] = cell_light(terrain, &self.sun, ambient, cx, cz);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(size: usize, h: u16) -> Heightmap {
        let mut t = Heightmap::new(size);
        for z in 0..size as i32 {
            for x in 0..size as i32 {
                t.set(x, z, h);
            }
        }
        t
    }

    fn recompute_all(shading: &mut CellShading, terrain: &Heightmap) {
        let whole = DirtyRect { min: (0, 0), max: (terrain.size() as i32, terrain.size() as i32) };
        shading.recompute(terrain, &Ashes::default(), &HashMap::new(), &[whole]);
    }

    #[test]
    fn a_perfectly_flat_map_has_zero_blend_and_flat_downhill_everywhere() {
        let terrain = flat(8, 500);
        let mut shading = CellShading::new(8, SunParams::default());
        recompute_all(&mut shading, &terrain);
        for z in 0..8 {
            for x in 0..8 {
                assert_eq!(shading.blend(x, z), 0, "({x},{z})");
                assert_eq!(shading.downhill_dir(x, z), 0, "({x},{z})");
            }
        }
    }

    #[test]
    fn one_raised_neighbour_gives_the_clamped_slope_and_the_peak_points_downhill() {
        let mut terrain = flat(8, 100);
        terrain.set(4, 4, 228); // +128 over its neighbours: (228-100)/8 = 16
        let mut shading = CellShading::new(8, SunParams::default());
        recompute_all(&mut shading, &terrain);
        assert_eq!(shading.blend(4, 4), 16);
        assert_eq!(shading.blend(4, 3), 16, "a neighbour reads the same slope between it and the peak");
        assert_eq!(shading.downhill_dir(4, 4), 1, "every neighbour is lower: north wins the tie");
    }

    #[test]
    fn blend_clamps_to_127_on_a_steep_enough_slope() {
        let mut terrain = flat(8, 0);
        terrain.set(4, 4, 1024);
        let mut shading = CellShading::new(8, SunParams::default());
        recompute_all(&mut shading, &terrain);
        assert_eq!(shading.blend(4, 4), 127);
    }

    #[test]
    fn downhill_ties_are_broken_by_fixed_compass_order_not_at_random() {
        let mut terrain = flat(8, 100);
        // North and east are both lower, equally: compass order (N, E, S, W, NE, SE, SW, NW) means north wins.
        terrain.set(4, 3, 50); // north of (4,4)
        terrain.set(5, 4, 50); // east of (4,4)
        let mut shading = CellShading::new(8, SunParams::default());
        recompute_all(&mut shading, &terrain);
        assert_eq!(shading.downhill_dir(4, 4), 1, "north (index 0 + 1) wins the tie over east");
    }

    #[test]
    fn downhill_needs_a_strictly_lower_neighbour_a_tie_with_self_stays_flat() {
        let terrain = flat(8, 100);
        let mut shading = CellShading::new(8, SunParams::default());
        recompute_all(&mut shading, &terrain);
        assert_eq!(downhill_of(&terrain, 4, 4), 0);
    }

    #[test]
    fn light_clamps_at_both_ends() {
        let terrain = flat(8, 100);
        let bright = SunParams { x: 0, z: 0, base: 1_000_000 };
        let dark = SunParams { x: 0, z: 0, base: -1_000_000 };
        assert_eq!(cell_light(&terrain, &bright, 0, 4, 4), 255);
        assert_eq!(cell_light(&terrain, &dark, 0, 4, 4), 0);
    }

    #[test]
    fn light_reacts_to_a_slope_along_each_axis() {
        let mut terrain = flat(8, 100);
        terrain.set(6, 4, 900); // steep rise 2 cells to the east of (4,4): dx = 100 - 900 = -800 (negative)
        let sun = SunParams { x: 100, z: 100, base: 0 };
        let flat_light = cell_light(&terrain, &sun, 0, 1, 1);
        let sloped_light = cell_light(&terrain, &sun, 0, 4, 4);
        assert_ne!(flat_light, sloped_light);
    }

    #[test]
    fn recomputing_only_the_dirty_rect_matches_a_full_recompute_from_scratch() {
        let mut terrain = flat(16, 200);
        terrain.set(8, 8, 400);
        let mut incremental = CellShading::new(16, SunParams::default());
        recompute_all(&mut incremental, &terrain); // seed it on the old terrain
        terrain.set(5, 5, 900);
        let dirty = DirtyRect { min: (5, 5), max: (6, 6) };
        incremental.recompute(&terrain, &Ashes::default(), &HashMap::new(), &[dirty]);
        let mut from_scratch = CellShading::new(16, SunParams::default());
        recompute_all(&mut from_scratch, &terrain);
        assert_eq!(incremental.blend, from_scratch.blend);
        assert_eq!(incremental.light, from_scratch.light);
        assert_eq!(incremental.downhill, from_scratch.downhill);
    }

    #[test]
    fn lava_blend_overrides_the_general_slope_measure() {
        let terrain = flat(8, 100);
        let mut shading = CellShading::new(8, SunParams::default());
        let lava = HashMap::from([((4, 4), 200u8)]);
        let whole = DirtyRect { min: (0, 0), max: (8, 8) };
        shading.recompute(&terrain, &Ashes::default(), &lava, &[whole]);
        assert_eq!(shading.blend(4, 4), 200, "flat ground would otherwise read blend 0");
    }

    #[test]
    fn ashes_darkness_overrides_the_general_slope_measure_when_no_lava_covers_the_cell() {
        let terrain = flat(8, 100);
        let mut ashes = Ashes::default();
        ashes.add(vec![(4, 4)]);
        let mut shading = CellShading::new(8, SunParams::default());
        let whole = DirtyRect { min: (0, 0), max: (8, 8) };
        shading.recompute(&terrain, &ashes, &HashMap::new(), &[whole]);
        assert_eq!(shading.blend(4, 4), crate::ashes::DARK);
    }
}
