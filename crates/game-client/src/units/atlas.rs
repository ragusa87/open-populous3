//! Packs many `art::Frame`s into one RGBA image, so a kind+tribe can share one texture/material
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

/// Shelf-packs `frames` left to right, wrapping to a new row once a frame would cross the target
/// row width (picked from the total pixel area, so the atlas stays roughly square; widened to fit
/// the single widest frame if that frame alone would not fit a row). A simple shelf packer is
/// enough here: one kind+tribe's frames are all a similar handful of sizes (a walk cycle, a few
/// poses x 8 directions), not a wildly mixed sprite sheet that would need a real bin packer's
/// space efficiency.
pub fn build_atlas(frames: &[Frame]) -> Atlas {
    if frames.is_empty() {
        return Atlas { width: 0, height: 0, rgba: Vec::new(), rects: Vec::new() };
    }
    let total_area: usize = frames.iter().map(|f| f.width * f.height).sum();
    let widest = frames.iter().map(|f| f.width).max().unwrap_or(1);
    let target_width = widest.max((total_area as f64).sqrt().ceil() as usize);

    let mut rects = Vec::with_capacity(frames.len());
    let (mut x, mut y, mut row_height, mut width) = (0usize, 0usize, 0usize, 0usize);
    for f in frames {
        if x > 0 && x + f.width > target_width {
            x = 0;
            y += row_height;
            row_height = 0;
        }
        rects.push(AtlasRect { x, y, width: f.width, height: f.height });
        x += f.width;
        width = width.max(x);
        row_height = row_height.max(f.height);
    }
    let height = y + row_height;

    let mut rgba = vec![0u8; width * height * 4];
    for (f, r) in frames.iter().zip(&rects) {
        for row in 0..f.height {
            let src = row * f.width * 4;
            let dst = ((r.y + row) * width + r.x) * 4;
            rgba[dst..dst + f.width * 4].copy_from_slice(&f.rgba[src..src + f.width * 4]);
        }
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
        let atlas = build_atlas(&frames);
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
        let atlas = build_atlas(&frames);
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
    fn uv_rect_is_normalized_to_the_atlas_size() {
        let r = AtlasRect { x: 10, y: 5, width: 20, height: 10 };
        assert_eq!(r.uv(100, 50), [0.1, 0.1, 0.3, 0.3]);
    }

    #[test]
    fn empty_input_makes_an_empty_atlas() {
        let atlas = build_atlas(&[]);
        assert_eq!((atlas.width, atlas.height, atlas.rects.len()), (0, 0, 0));
    }
}
