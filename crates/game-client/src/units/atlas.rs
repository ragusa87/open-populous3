//! Packs many frames into one image, so a kind+tribe can share one texture/material
//! instead of one per frame (TODO.md "Performance (60 fps)", "Unit sprites"). Pure pixel math, no
//! Bevy types, so the packer itself is unit-tested without a render context.

use super::art::Frame;

/// Where one packed frame landed in the atlas, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasRect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl AtlasRect {
    /// This rect's `[u_min, v_min, u_max, v_max]` within an atlas of `atlas_width` x `atlas_height`.
    pub fn uv(&self, atlas_width: usize, atlas_height: usize) -> [f32; 4] {
        let (w, h) = (atlas_width as f32, atlas_height as f32);
        [self.x as f32 / w, self.y as f32 / h, (self.x + self.width) as f32 / w, (self.y + self.height) as f32 / h]
    }
}

/// One RGBA image holding every input frame; `rects[i]` is where `frames[i]` landed.
pub struct Atlas {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    pub rects: Vec<AtlasRect>,
}

/// Where frames of the given sizes land, each with a `pad`-wide empty border of its own: `(width, height, rects)`.
/// Shelf-packs left to right, wrapping to a new row once a frame would cross the target row width (picked from the
/// total area, so the atlas stays roughly square; widened to fit the single widest frame). A simple shelf packer is
/// enough here: one kind's frames are all a similar handful of sizes (a walk cycle, a few poses x 8 directions).
pub fn pack(sizes: &[(usize, usize)], pad: usize) -> (usize, usize, Vec<AtlasRect>) {
    if sizes.is_empty() {
        return (0, 0, Vec::new());
    }
    let slot = |(w, h): (usize, usize)| (w + 2 * pad, h + 2 * pad);
    let total_area: usize = sizes.iter().map(|&s| slot(s).0 * slot(s).1).sum();
    let widest = sizes.iter().map(|&s| slot(s).0).max().unwrap_or(1);
    let target_width = widest.max((total_area as f64).sqrt().ceil() as usize);

    let mut rects = Vec::with_capacity(sizes.len());
    let (mut x, mut y, mut row_height, mut width) = (0usize, 0usize, 0usize, 0usize);
    for &(w, h) in sizes {
        let (sw, sh) = slot((w, h));
        if x > 0 && x + sw > target_width {
            x = 0;
            y += row_height;
            row_height = 0;
        }
        rects.push(AtlasRect { x: x + pad, y: y + pad, width: w, height: h });
        x += sw;
        width = width.max(x);
        row_height = row_height.max(sh);
    }
    (width, y + row_height, rects)
}

/// Copies `src` (`rect.width` x `rect.height` pixels of `channels` values each) into `dst`, `atlas_width` pixels wide.
pub fn blit<T: Copy>(dst: &mut [T], atlas_width: usize, channels: usize, src: &[T], rect: &AtlasRect) {
    let row = rect.width * channels;
    for y in 0..rect.height {
        let at = ((rect.y + y) * atlas_width + rect.x) * channels;
        dst[at..at + row].copy_from_slice(&src[y * row..(y + 1) * row]);
    }
}

/// Packs `frames` into one RGBA image (`pack`), each with a `pad`-wide transparent border.
pub fn build_atlas(frames: &[Frame], pad: usize) -> Atlas {
    let sizes: Vec<_> = frames.iter().map(|f| (f.width, f.height)).collect();
    let (width, height, rects) = pack(&sizes, pad);
    let mut rgba = vec![0u8; width * height * 4];
    for (f, r) in frames.iter().zip(&rects) {
        blit(&mut rgba, width, 4, &f.rgba, r);
    }
    Atlas { width, height, rgba, rects }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: usize, height: usize, colour: [u8; 4]) -> Frame {
        Frame { width, height, origin: (width / 2, height), rgba: colour.repeat(width * height), scale: 1 }
    }

    #[test]
    fn every_input_frame_gets_a_rect_and_none_overlap() {
        let frames = vec![solid(4, 6, [255, 0, 0, 255]), solid(8, 3, [0, 255, 0, 255]), solid(2, 2, [0, 0, 255, 255]), solid(5, 9, [1, 2, 3, 255])];
        let atlas = build_atlas(&frames, 0);
        assert_eq!(atlas.rects.len(), frames.len());
        for (i, a) in atlas.rects.iter().enumerate() {
            for (j, b) in atlas.rects.iter().enumerate().skip(i + 1) {
                let disjoint = a.x + a.width <= b.x || b.x + b.width <= a.x || a.y + a.height <= b.y || b.y + b.height <= a.y;
                assert!(disjoint, "rects {i} and {j} overlap: {a:?} vs {b:?}");
            }
            assert!(a.x + a.width <= atlas.width && a.y + a.height <= atlas.height, "rect {i} {a:?} falls outside the {}x{} atlas", atlas.width, atlas.height);
        }
    }

    #[test]
    fn packed_pixels_match_the_source_frame() {
        let frames = vec![solid(3, 2, [10, 20, 30, 255]), solid(5, 4, [40, 50, 60, 255]), solid(6, 1, [70, 80, 90, 200])];
        let atlas = build_atlas(&frames, 0);
        for (f, r) in frames.iter().zip(&atlas.rects) {
            for row in 0..f.height {
                for col in 0..f.width {
                    let si = (row * f.width + col) * 4;
                    let di = ((r.y + row) * atlas.width + r.x + col) * 4;
                    assert_eq!(&atlas.rgba[di..di + 4], &f.rgba[si..si + 4]);
                }
            }
        }
    }

    #[test]
    fn a_padded_frame_keeps_an_empty_border_all_round() {
        let frames = vec![solid(3, 2, [9, 9, 9, 255]), solid(2, 4, [8, 8, 8, 255]), solid(5, 1, [7, 7, 7, 255])];
        let atlas = build_atlas(&frames, 2);
        let opaque = |x: usize, y: usize| atlas.rgba[(y * atlas.width + x) * 4 + 3] != 0;
        for r in &atlas.rects {
            assert!(r.x >= 2 && r.y >= 2 && r.x + r.width + 2 <= atlas.width && r.y + r.height + 2 <= atlas.height, "{r:?}");
            for y in r.y - 2..r.y + r.height + 2 {
                for x in r.x - 2..r.x + r.width + 2 {
                    let inside = (r.x..r.x + r.width).contains(&x) && (r.y..r.y + r.height).contains(&y);
                    assert_eq!(opaque(x, y), inside, "({x}, {y}) around {r:?}");
                }
            }
        }
    }

    #[test]
    fn uv_rect_is_normalized_to_the_atlas_size() {
        let r = AtlasRect { x: 10, y: 5, width: 20, height: 10 };
        assert_eq!(r.uv(100, 50), [0.1, 0.1, 0.3, 0.3]);
    }

    #[test]
    fn empty_input_makes_an_empty_atlas() {
        let atlas = build_atlas(&[], 0);
        assert_eq!((atlas.width, atlas.height, atlas.rects.len()), (0, 0, 0));
    }
}
