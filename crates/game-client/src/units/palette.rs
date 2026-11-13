//! Unit art as colour indices plus a colour table per tribe, so one atlas serves every tribe (the shader looks each
//! index up in the tribe's table). Pure pixel math, no Bevy. Each source shows tribes its own way (the original:
//! tribe layers drawn over a blue body, a shaman per tribe; the sheets: a hue swap; generated: drawn in the tribe
//! colour), so nothing is assumed: an index stands for a pixel's colours in all the tribes at once.

use super::art::{Frame, TribeArt};
use super::atlas::{blit, pack, AtlasRect};

/// A colour index; `TRANSPARENT` in every tribe's table.
pub type ColourIndex = u16;
pub const TRANSPARENT: ColourIndex = 0;
const RGBA_NONE: [u8; 4] = [0; 4];

/// A frame's pixels as colour indices, row by row.
#[derive(Clone, Debug, PartialEq)]
pub struct IndexedFrame {
    pub width: usize,
    pub height: usize,
    pub origin: (usize, usize),
    pub scale: usize,
    pub index: Vec<ColourIndex>,
}

/// One set of index frames (`poses[pose][dir][frame]`) and the colour table of each tribe drawn from it.
pub struct IndexedArt {
    pub poses: Vec<Vec<Vec<IndexedFrame>>>,
    /// `(tribe, table)`, `table[index]` its colour.
    pub tables: Vec<(usize, Vec<[u8; 4]>)>,
}

impl IndexedArt {
    /// Every frame in `poses` order.
    pub fn frames(&self) -> impl Iterator<Item = &IndexedFrame> {
        self.poses.iter().flatten().flatten()
    }
}

/// Every frame of an `IndexedArt` packed into one index image (`atlas::pack`), `rects` in `IndexedArt::frames` order.
pub struct IndexAtlas {
    pub width: usize,
    pub height: usize,
    pub index: Vec<ColourIndex>,
    pub rects: Vec<AtlasRect>,
}

pub fn build_index_atlas(art: &IndexedArt, pad: usize) -> IndexAtlas {
    let sizes: Vec<_> = art.frames().map(|f| (f.width, f.height)).collect();
    let (width, height, rects) = pack(&sizes, pad);
    let mut index = vec![TRANSPARENT; width * height];
    for (f, r) in art.frames().zip(&rects) {
        blit(&mut index, width, 1, &f.index, r);
    }
    IndexAtlas { width, height, index, rects }
}

/// Indices one set of frames can use: far above every source today (the sheets need about 31k).
const MAX_COLOURS: usize = ColourIndex::MAX as usize + 1;

fn colour(f: &Frame, i: usize) -> [u8; 4] {
    let px: [u8; 4] = f.rgba[i * 4..i * 4 + 4].try_into().expect("4 bytes");
    if px[3] == 0 { RGBA_NONE } else { px }
}

/// The colour of `f` at `(x, y)` of a box whose feet are at `origin`: `f` drawn with its own feet there.
fn colour_at(f: &Frame, origin: (usize, usize), x: usize, y: usize) -> [u8; 4] {
    let (fx, fy) = ((x + f.origin.0).wrapping_sub(origin.0), (y + f.origin.1).wrapping_sub(origin.1));
    if fx < f.width && fy < f.height { colour(f, fy * f.width + fx) } else { RGBA_NONE }
}

/// The smallest box holding every one of `frames` drawn with their feet on the same spot: `(width, height, origin)`.
fn union_box(frames: &[&Frame]) -> (usize, usize, (usize, usize)) {
    let max = |g: &dyn Fn(&Frame) -> usize| frames.iter().map(|f| g(f)).max().unwrap_or(0);
    let (left, top) = (max(&|f| f.origin.0), max(&|f| f.origin.1));
    (left + max(&|f| f.width - f.origin.0), top + max(&|f| f.height - f.origin.1), (left, top))
}

/// Every `[pose][dir][frame]` of `tribes` side by side, or None when their poses, directions, frame counts or
/// scales differ.
fn aligned<'a>(tribes: &[&'a TribeArt]) -> Option<Vec<Vec<Vec<Vec<&'a Frame>>>>> {
    let first = tribes.first()?;
    let lines_up = |t: &&TribeArt| {
        t.poses.len() == first.poses.len()
            && t.poses.iter().zip(&first.poses).all(|(dirs, first_dirs)| {
                dirs.len() == first_dirs.len() && dirs.iter().zip(first_dirs).all(|(fs, first_fs)| fs.len() == first_fs.len() && fs.iter().zip(first_fs).all(|(f, g)| f.scale == g.scale))
            })
    };
    if !tribes.iter().all(lines_up) {
        return None;
    }
    let slots = first
        .poses
        .iter()
        .enumerate()
        .map(|(p, dirs)| dirs.iter().enumerate().map(|(d, fs)| (0..fs.len()).map(|i| tribes.iter().map(|t| &t.poses[p][d][i]).collect()).collect()).collect())
        .collect();
    Some(slots)
}

/// `tribes` as one set of index frames: each index stands for the colours its pixels have in each tribe, so the
/// tables draw every tribe exactly whatever differs between them (a layer drawn over, other shades, a wider frame:
/// frames are aligned on their feet). None when their poses or frame counts differ, or past `MAX_COLOURS`.
fn index_together(tribes: &[(usize, &TribeArt)]) -> Option<IndexedArt> {
    let arts: Vec<&TribeArt> = tribes.iter().map(|t| t.1).collect();
    let slots = aligned(&arts)?;
    let mut keys: Vec<u128> = vec![0];
    let mut lookup = std::collections::HashMap::from([(0u128, TRANSPARENT)]);
    let pack = |cs: &mut dyn Iterator<Item = [u8; 4]>| cs.fold(0u128, |k, c| k << 32 | u32::from_le_bytes(c) as u128);
    let mut poses = Vec::with_capacity(slots.len());
    for dirs in &slots {
        let mut out_dirs = Vec::with_capacity(dirs.len());
        for frames in dirs {
            let mut out = Vec::with_capacity(frames.len());
            for same in frames {
                let (width, height, origin) = union_box(same);
                let mut index = Vec::with_capacity(width * height);
                for y in 0..height {
                    for x in 0..width {
                        let key = pack(&mut same.iter().map(|f| colour_at(f, origin, x, y)));
                        let next = keys.len();
                        let k = *lookup.entry(key).or_insert(next as ColourIndex);
                        if k as usize == next {
                            if next >= MAX_COLOURS {
                                return None;
                            }
                            keys.push(key);
                        }
                        index.push(k);
                    }
                }
                out.push(IndexedFrame { width, height, origin, scale: same[0].scale, index });
            }
            out_dirs.push(out);
        }
        poses.push(out_dirs);
    }
    let n = tribes.len();
    let tables = tribes
        .iter()
        .enumerate()
        .map(|(slot, &(tribe, _))| (tribe, keys.iter().map(|&k| ((k >> (32 * (n - 1 - slot))) as u32).to_le_bytes()).collect()))
        .collect();
    Some(IndexedArt { poses, tables })
}

/// Every tribe's art as colour indices: one set of frames for all the tribes when they line up frame for frame
/// (`index_together`), else each tribe's own.
pub fn palettize(tribes: &[TribeArt]) -> Vec<IndexedArt> {
    let all: Vec<(usize, &TribeArt)> = tribes.iter().enumerate().collect();
    if tribes.len() <= 4
        && let Some(art) = index_together(&all)
    {
        return vec![art];
    }
    all.iter().map(|&one| index_together(&[one]).expect("one tribe always lines up with itself")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::art::{generated_art, Pose, DIRS};
    use game_core::unit::UnitKind;

    fn expand(f: &IndexedFrame, table: &[[u8; 4]]) -> Vec<u8> {
        f.index.iter().flat_map(|&k| table[k as usize]).collect()
    }

    fn see_through_as_zero(f: &Frame) -> Vec<u8> {
        f.rgba.chunks_exact(4).flat_map(|p| if p[3] == 0 { RGBA_NONE } else { p.try_into().unwrap() }).collect()
    }

    fn art(frames: Vec<Frame>) -> TribeArt {
        TribeArt { poses: vec![vec![frames]] }
    }

    fn frame(rgba: &[[u8; 4]]) -> Frame {
        Frame { width: rgba.len(), height: 1, origin: (0, 1), rgba: rgba.concat(), scale: 1 }
    }

    #[test]
    fn generated_tribes_share_one_set_of_frames_and_draw_back_exactly() {
        for kind in UnitKind::ALL {
            let tribes = generated_art(kind);
            let indexed = palettize(&tribes);
            assert_eq!(indexed.len(), 1, "{kind:?}: tribes differ only by colour");
            let shared = &indexed[0];
            assert_eq!(shared.tables.len(), tribes.len());
            for (tribe, table) in &shared.tables {
                assert_eq!(table[TRANSPARENT as usize], RGBA_NONE);
                for &pose in &Pose::ALL {
                    for dir in 0..DIRS {
                        for (ix, f) in shared.poses[pose as usize][dir].iter().zip(tribes[*tribe].frames(pose, dir)) {
                            assert_eq!(expand(ix, table), see_through_as_zero(f), "{kind:?} tribe {tribe} {pose:?} {dir}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn tribes_drawn_differently_still_share_frames_aligned_on_their_feet() {
        let (grey, red, blue, skin) = ([90, 90, 90, 255], [200, 0, 0, 255], [0, 0, 200, 255], [210, 160, 120, 255]);
        let wider = Frame { width: 3, height: 1, origin: (1, 1), rgba: [red, blue, skin].concat(), scale: 1 };
        let tribes = [art(vec![frame(&[grey, grey])]), art(vec![frame(&[red, blue])]), art(vec![wider])];
        let indexed = palettize(&tribes);
        assert_eq!(indexed.len(), 1);
        let f = &indexed[0].poses[0][0][0];
        assert_eq!((f.width, f.origin), (3, (1, 1)));
        let drawn: Vec<Vec<u8>> = indexed[0].tables.iter().map(|(_, t)| expand(f, t)).collect();
        assert_eq!(drawn, vec![[RGBA_NONE, grey, grey].concat(), [RGBA_NONE, red, blue].concat(), [red, blue, skin].concat()]);
    }

    #[test]
    fn tribes_with_other_frame_counts_get_frames_of_their_own() {
        let (red, blue) = ([200, 0, 0, 255], [0, 0, 200, 255]);
        let tribes = [art(vec![frame(&[blue])]), art(vec![frame(&[red]), frame(&[red])])];
        let indexed = palettize(&tribes);
        assert_eq!(indexed.iter().map(|a| a.tables.iter().map(|t| t.0).collect::<Vec<_>>()).collect::<Vec<_>>(), vec![vec![0], vec![1]]);
        assert_eq!(expand(&indexed[1].poses[0][0][1], &indexed[1].tables[0].1), red.to_vec());
    }

    #[test]
    fn see_through_colours_and_partial_alpha_keep_their_alpha() {
        let glow = [250, 200, 40, 128];
        let tribes = [art(vec![frame(&[[1, 2, 3, 0], glow])]), art(vec![frame(&[[9, 9, 9, 0], glow])])];
        let indexed = palettize(&tribes);
        let f = &indexed[0].poses[0][0][0];
        assert_eq!(f.index[0], TRANSPARENT, "an invisible pixel's colour doesn't matter");
        assert_eq!(indexed[0].tables[1].1[f.index[1] as usize], glow);
        assert_eq!(indexed[0].tables[1].1[TRANSPARENT as usize], RGBA_NONE);
    }

    #[test]
    fn the_index_atlas_holds_every_frame_where_its_rect_says() {
        let tribes = generated_art(UnitKind::Brave);
        let indexed = palettize(&tribes);
        let atlas = build_index_atlas(&indexed[0], 3);
        assert_eq!(atlas.rects.len(), indexed[0].frames().count());
        for (f, r) in indexed[0].frames().zip(&atlas.rects) {
            for y in 0..f.height {
                assert_eq!(&atlas.index[(r.y + y) * atlas.width + r.x..][..f.width], &f.index[y * f.width..][..f.width]);
            }
        }
    }

    /// Index atlas sizes with the player's own install (`cargo test -- --ignored --nocapture originals_index`).
    #[test]
    #[ignore]
    fn originals_index_atlas_bytes_per_kind() {
        let Some(install) = pop3_format::install::find_install() else { return };
        let originals = crate::units::art::Originals::load(&pop3_format::install::subdir(&install, "data")).unwrap();
        let mut total = 0;
        for kind in UnitKind::ALL {
            let Ok(tribes) = originals.art(kind) else { continue };
            for art in palettize(&tribes) {
                let atlas = build_index_atlas(&art, 3);
                let colours = art.tables[0].1.len();
                let bytes = atlas.width * atlas.height * 2 + colours.div_ceil(256) * 256 * 4 * art.tables.len();
                total += bytes;
                println!("{kind:?} tribes {:?}: {}x{}, {colours} colours, {} KB", art.tables.iter().map(|t| t.0).collect::<Vec<_>>(), atlas.width, atlas.height, bytes / 1024);
            }
        }
        println!("total {} MB", total / (1024 * 1024));
    }

    #[test]
    fn the_open_source_sheets_fit_their_tables() {
        for kind in UnitKind::ALL {
            let Some(tribes) = crate::units::sheets::sheet_art(kind) else { continue };
            for art in palettize(&tribes) {
                for (_, table) in &art.tables {
                    assert!(table.len() < MAX_COLOURS, "{kind:?}: {} colours", table.len());
                }
            }
        }
    }
}

