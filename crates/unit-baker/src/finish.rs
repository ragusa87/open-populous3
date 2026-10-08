//! From a rendered cell to a game frame: hard alpha, outline, water line, then cropped to its pixels.

/// Rendered cell size in pixels (4 per base pixel): a lying body and the cast jump fit.
pub const CELL: (usize, usize) = (320, 288);
pub const FEET: (usize, usize) = (160, 256);
const OUTLINE: [u8; 4] = [30, 18, 10, 255];
const WATER: [u8; 4] = [170, 210, 255, 255];

/// Hard alpha, a dark outline 2 px wide, and for sunk poses nothing under the water line but ripples.
pub fn finish_cell(rgba: &mut [u8], sunk: bool) {
    let (w, h) = CELL;
    for px in rgba.chunks_exact_mut(4) {
        if px[3] < 128 {
            px.copy_from_slice(&[0; 4]);
        } else {
            px[3] = 255;
        }
    }
    let waterline = FEET.1;
    if sunk {
        rgba[waterline * w * 4..].fill(0);
    }
    let src = rgba.to_vec();
    let opaque = |x: i32, y: i32| x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && src[(y as usize * w + x as usize) * 4 + 3] != 0;
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if !opaque(x, y) && (-2..=2).any(|dy| (-2..=2).any(|dx| dx * dx + dy * dy <= 5 && opaque(x + dx, y + dy))) {
                rgba[(y as usize * w + x as usize) * 4..][..4].copy_from_slice(&OUTLINE);
            }
        }
    }
    if sunk {
        for x in (FEET.0 - 70)..(FEET.0 + 70) {
            if (x / 10) % 3 != 2 {
                for y in waterline - 2..waterline + 2 {
                    rgba[(y * w + x) * 4..][..4].copy_from_slice(&WATER);
                }
            }
        }
    }
}

/// A frame cropped out of its cell, the feet at `origin`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cropped {
    pub width: usize,
    pub height: usize,
    pub origin: (usize, usize),
    pub rgba: Vec<u8>,
}

/// The cell cropped to its opaque pixels, the feet pixel kept inside.
pub fn crop(rgba: &[u8]) -> Cropped {
    let (w, h) = CELL;
    let (mut l, mut t, mut r, mut b) = (FEET.0, FEET.1, FEET.0 + 1, FEET.1 + 1);
    for y in 0..h {
        for x in 0..w {
            if rgba[(y * w + x) * 4 + 3] != 0 {
                (l, t, r, b) = (l.min(x), t.min(y), r.max(x + 1), b.max(y + 1));
            }
        }
    }
    let mut out = Vec::with_capacity((r - l) * (b - t) * 4);
    for y in t..b {
        out.extend_from_slice(&rgba[(y * w + l) * 4..(y * w + r) * 4]);
    }
    Cropped { width: r - l, height: b - t, origin: (FEET.0 - l, FEET.1 - t), rgba: out }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell_with(pixels: &[(usize, usize)]) -> Vec<u8> {
        let mut rgba = vec![0u8; CELL.0 * CELL.1 * 4];
        for &(x, y) in pixels {
            rgba[(y * CELL.0 + x) * 4..][..4].copy_from_slice(&[200, 0, 200, 200]);
        }
        rgba
    }

    fn alpha(rgba: &[u8], x: usize, y: usize) -> u8 {
        rgba[(y * CELL.0 + x) * 4 + 3]
    }

    #[test]
    fn hard_edges_and_a_2px_outline() {
        let mut rgba = cell_with(&[(100, 100)]);
        rgba[(50 * CELL.0 + 50) * 4 + 3] = 100;
        finish_cell(&mut rgba, false);
        assert!(rgba.chunks_exact(4).all(|p| p[3] == 0 || p[3] == 255));
        assert_eq!(alpha(&rgba, 50, 50), 0, "faint pixels dropped");
        assert_eq!(&rgba[(100 * CELL.0 + 102) * 4..][..4], &OUTLINE);
        assert_eq!(&rgba[(101 * CELL.0 + 102) * 4..][..4], &OUTLINE);
        assert_eq!(alpha(&rgba, 102, 102), 0, "round corners");
        assert_eq!(alpha(&rgba, 103, 100), 0);
    }

    #[test]
    fn sunk_poses_end_at_the_water_line() {
        let mut rgba = cell_with(&[(FEET.0, FEET.1 + 10), (FEET.0, FEET.1 - 20)]);
        finish_cell(&mut rgba, true);
        assert_eq!(alpha(&rgba, FEET.0, FEET.1 + 10), 0);
        assert_eq!(alpha(&rgba, FEET.0, FEET.1 - 20), 255);
        assert_eq!(&rgba[(FEET.1 * CELL.0 + FEET.0) * 4..][..4], &WATER, "ripples");
        assert!((FEET.1 + 2..CELL.1).all(|y| alpha(&rgba, FEET.0, y) == 0));
    }

    #[test]
    fn crops_around_the_pixels_and_the_feet() {
        let f = crop(&cell_with(&[(150, 200), (170, 255)]));
        assert_eq!((f.width, f.height, f.origin), (21, 57, (10, 56)));
        assert_eq!(f.rgba.len(), 21 * 57 * 4);
        assert_eq!(f.rgba[3], 200, "top-left pixel kept");
        let empty = crop(&cell_with(&[]));
        assert_eq!((empty.width, empty.height, empty.origin), (1, 1, (0, 0)), "just the feet");
    }
}
