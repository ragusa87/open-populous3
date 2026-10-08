//! Packs a kind's cropped frames into one atlas image and its index (`unit_atlas`).

use crate::finish::Cropped;
use std::collections::BTreeMap;
use unit_atlas::Entry;

pub const WIDTH: usize = 2048;

/// A frame to place: its pose, direction and frame number.
pub struct Placed<'a> {
    pub pose: &'static str,
    pub dir: usize,
    pub frame: usize,
    pub image: &'a Cropped,
}

/// Shelf packing, tallest first: top-left corners of `sizes` in rows `width` wide, and the height used.
pub fn pack(sizes: &[(usize, usize)], width: usize) -> (Vec<(usize, usize)>, usize) {
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(sizes[i].1), std::cmp::Reverse(sizes[i].0), i));
    let mut at = vec![(0, 0); sizes.len()];
    let (mut x, mut y, mut shelf) = (0, 0, 0);
    for i in order {
        let (w, h) = sizes[i];
        assert!(w <= width, "frame wider than the atlas");
        if x + w > width {
            (x, y, shelf) = (0, y + shelf, 0);
        }
        at[i] = (x, y);
        x += w;
        shelf = shelf.max(h);
    }
    (at, y + shelf)
}

/// The atlas (RGBA, `WIDTH` wide) and its index; identical frames share one rectangle.
pub fn build(frames: &[Placed]) -> (Vec<u8>, usize, Vec<Entry>) {
    let mut unique: Vec<&Cropped> = Vec::new();
    let mut seen: BTreeMap<&Cropped, usize> = BTreeMap::new();
    let slot: Vec<usize> = frames
        .iter()
        .map(|f| {
            *seen.entry(f.image).or_insert_with(|| {
                unique.push(f.image);
                unique.len() - 1
            })
        })
        .collect();
    let (at, height) = pack(&unique.iter().map(|c| (c.width, c.height)).collect::<Vec<_>>(), WIDTH);
    let mut rgba = vec![0u8; WIDTH * height * 4];
    for (c, &(x, y)) in unique.iter().zip(&at) {
        for row in 0..c.height {
            let dst = ((y + row) * WIDTH + x) * 4;
            rgba[dst..dst + c.width * 4].copy_from_slice(&c.rgba[row * c.width * 4..(row + 1) * c.width * 4]);
        }
    }
    let entries = frames
        .iter()
        .zip(slot)
        .map(|(f, s)| Entry {
            pose: f.pose.to_string(),
            dir: f.dir,
            frame: f.frame,
            rect: [at[s].0, at[s].1, f.image.width, f.image.height].map(|v| v as u32),
            origin: (f.image.origin.0 as u32, f.image.origin.1 as u32),
        })
        .collect();
    (rgba, height, entries)
}

/// How the atlas pixels are stored in the PNG.
enum Layout {
    Indexed(BTreeMap<[u8; 4], u8>),
    /// Alpha is only 0 or 255 and no opaque pixel is black: RGB, black transparent.
    KeyedRgb,
    Rgba,
}

fn layout(rgba: &[u8]) -> Layout {
    if let Some(p) = palette(rgba) {
        return Layout::Indexed(p);
    }
    let keyed = rgba.chunks_exact(4).all(|px| if px[3] == 0 { px == [0; 4] } else { px[3] == 255 && px[..3] != [0; 3] });
    if keyed { Layout::KeyedRgb } else { Layout::Rgba }
}

/// The smallest lossless PNG of an RGBA image (an oxipng-like search): indexed when it has fewer than
/// 256 colours, else RGB with a transparent key when the alpha is hard, deflate level 9, the best of
/// a few row filters.
pub fn encode_png(rgba: &[u8], width: usize, height: usize) -> Vec<u8> {
    let layout = layout(rgba);
    let data: Vec<u8> = match &layout {
        Layout::Indexed(p) => rgba.chunks_exact(4).map(|px| if px[3] == 0 { 0 } else { p[&[px[0], px[1], px[2], px[3]]] }).collect(),
        Layout::KeyedRgb => rgba.chunks_exact(4).flat_map(|px| [px[0], px[1], px[2]]).collect(),
        Layout::Rgba => rgba.to_vec(),
    };
    [png::Filter::NoFilter, png::Filter::Adaptive, png::Filter::MinEntropy, png::Filter::Paeth]
        .into_iter()
        .map(|filter| {
            let mut out = Vec::new();
            let mut encoder = png::Encoder::new(&mut out, width as u32, height as u32);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_deflate_compression(png::DeflateCompression::Level(9));
            encoder.set_filter(filter);
            match &layout {
                Layout::Indexed(p) => {
                    let mut by_index: Vec<[u8; 4]> = vec![[0; 4]; p.len() + 1];
                    for (c, &i) in p {
                        by_index[i as usize] = *c;
                    }
                    encoder.set_color(png::ColorType::Indexed);
                    encoder.set_palette(by_index.iter().flat_map(|c| [c[0], c[1], c[2]]).collect::<Vec<u8>>());
                    encoder.set_trns(by_index.iter().map(|c| c[3]).collect::<Vec<u8>>());
                }
                Layout::KeyedRgb => {
                    encoder.set_color(png::ColorType::Rgb);
                    encoder.set_trns(vec![0u8; 6]);
                }
                Layout::Rgba => encoder.set_color(png::ColorType::Rgba),
            }
            let mut writer = encoder.write_header().expect("png header");
            writer.write_image_data(&data).expect("png data");
            writer.finish().expect("png end");
            out
        })
        .min_by_key(Vec::len)
        .expect("a filter")
}

/// Palette index of every opaque colour (0 is transparent), if there are fewer than 256.
fn palette(rgba: &[u8]) -> Option<BTreeMap<[u8; 4], u8>> {
    let mut colours = BTreeMap::new();
    for px in rgba.chunks_exact(4).filter(|px| px[3] != 0) {
        colours.insert([px[0], px[1], px[2], px[3]], 0u8);
        if colours.len() > 255 {
            return None;
        }
    }
    for (i, index) in colours.values_mut().enumerate() {
        *index = i as u8 + 1;
    }
    Some(colours)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: usize, h: usize, value: u8) -> Cropped {
        Cropped { width: w, height: h, origin: (w / 2, h - 1), rgba: [value, 0, value, 255].repeat(w * h) }
    }

    fn decode(png_bytes: &[u8]) -> (usize, usize, Vec<u8>) {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        (info.width as usize, info.height as usize, buf[..info.buffer_size()].to_vec())
    }

    #[test]
    fn packed_rectangles_never_overlap() {
        let sizes = [(600, 50), (600, 120), (600, 80), (10, 10), (2048, 5)];
        let (at, height) = pack(&sizes, WIDTH);
        let rects: Vec<_> = at.iter().zip(&sizes).map(|(&(x, y), &(w, h))| (x, y, x + w, y + h)).collect();
        for (i, a) in rects.iter().enumerate() {
            assert!(a.2 <= WIDTH && a.3 <= height);
            for b in &rects[i + 1..] {
                assert!(a.2 <= b.0 || b.2 <= a.0 || a.3 <= b.1 || b.3 <= a.1, "{a:?} {b:?}");
            }
        }
        assert_eq!(height, 120 + 5, "three tall ones and the small one share a shelf");
    }

    #[test]
    fn index_points_at_each_frame_and_shares_repeats() {
        let (a, b) = (frame(3, 4, 10), frame(5, 2, 20));
        let placed = [
            Placed { pose: "fall", dir: 0, frame: 0, image: &a },
            Placed { pose: "fall", dir: 0, frame: 1, image: &b },
            Placed { pose: "fall", dir: 0, frame: 2, image: &b },
        ];
        let (rgba, height, index) = build(&placed);
        assert_eq!(index[1].rect, index[2].rect);
        assert_eq!(index[0].origin, (1, 3));
        for (e, f) in index.iter().zip([&a, &b, &b]) {
            let [x, y, w, h] = e.rect.map(|v| v as usize);
            assert!(y + h <= height);
            let pixels: Vec<u8> = (y..y + h).flat_map(|r| rgba[(r * WIDTH + x) * 4..(r * WIDTH + x + w) * 4].to_vec()).collect();
            assert_eq!(pixels, f.rgba);
        }
    }

    #[test]
    fn png_is_lossless_indexed_or_not() {
        let mut few = vec![0u8; 4 * 3 * 4];
        few[4..8].copy_from_slice(&[200, 0, 200, 255]);
        few[20..24].copy_from_slice(&[30, 18, 10, 255]);
        assert_eq!(decode(&encode_png(&few, 4, 3)), (4, 3, few));
        let mut many: Vec<u8> = (0..600u32).flat_map(|i| [(i % 256) as u8, (i / 256) as u8, 7, 255]).collect();
        many[..4].fill(0);
        assert!(matches!(layout(&many), Layout::KeyedRgb));
        assert_eq!(decode(&encode_png(&many, 600, 1)), (600, 1, many.clone()));
        many[4..8].copy_from_slice(&[0, 0, 0, 255]);
        assert!(matches!(layout(&many), Layout::Rgba), "black is opaque: no key");
        assert_eq!(decode(&encode_png(&many, 600, 1)), (600, 1, many));
    }
}
