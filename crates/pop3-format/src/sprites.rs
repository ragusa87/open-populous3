//! "PSFB" sprite banks (e.g. `data/POINT0-0.DAT`, mouse pointers and UI icons).
//! See docs/specs/sprites.md.

use crate::level::LevelError;
use std::path::Path;

const MAGIC: &[u8; 4] = b"PSFB";

/// Mouse pointers in `POINT0-0.DAT`.
pub const POINTER_FILE: &str = "point0-0.dat";
/// Standard arrow pointer.
pub const POINTER_ARROW: usize = 14;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sprite {
    pub width: usize,
    pub height: usize,
    /// Row-major palette indices, None = transparent.
    pub pixels: Vec<Option<u8>>,
}

impl Sprite {
    /// Top-most, then left-most opaque pixel: the tip of an arrow pointer.
    pub fn tip(&self) -> Option<(usize, usize)> {
        let i = self.pixels.iter().position(Option::is_some)?;
        Some((i % self.width, i / self.width))
    }
}

#[derive(Clone, Debug, Default)]
pub struct SpriteBank {
    pub sprites: Vec<Sprite>,
}

fn bad(expected: usize, got: usize) -> LevelError {
    LevelError::BadSize { expected, got }
}

/// Header: magic, u32 count, then count x (u16 width, u16 height, u32 offset). Each row is a run
/// list ended by 0: a positive byte n is followed by n pixels, a negative one skips -n pixels.
fn decode(data: &[u8], width: usize, height: usize, mut p: usize) -> Result<Sprite, LevelError> {
    let mut pixels = vec![None; width * height];
    for y in 0..height {
        let mut x = 0usize;
        loop {
            let run = *data.get(p).ok_or_else(|| bad(p + 1, data.len()))? as i8;
            p += 1;
            match run {
                0 => break,
                n if n < 0 => x += n.unsigned_abs() as usize,
                n => {
                    let n = n as usize;
                    let row = data.get(p..p + n).ok_or_else(|| bad(p + n, data.len()))?;
                    for (k, &c) in row.iter().enumerate() {
                        if x + k < width {
                            pixels[y * width + x + k] = Some(c);
                        }
                    }
                    x += n;
                    p += n;
                }
            }
        }
    }
    Ok(Sprite { width, height, pixels })
}

impl SpriteBank {
    pub fn parse(data: &[u8]) -> Result<Self, LevelError> {
        if data.len() < 8 || &data[..4] != MAGIC {
            return Err(bad(8, data.len()));
        }
        let count = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
        let table = data.get(8..8 + count * 8).ok_or_else(|| bad(8 + count * 8, data.len()))?;
        let sprites = table
            .chunks_exact(8)
            .map(|e| {
                let (w, h) = (u16::from_le_bytes([e[0], e[1]]), u16::from_le_bytes([e[2], e[3]]));
                let off = u32::from_le_bytes([e[4], e[5], e[6], e[7]]) as usize;
                decode(data, w as usize, h as usize, off)
            })
            .collect::<Result<_, _>>()?;
        Ok(SpriteBank { sprites })
    }

    pub fn load(data_dir: &Path, name: &str) -> Result<Self, LevelError> {
        let path = crate::find_file(data_dir, name).unwrap_or_else(|| data_dir.join(name));
        Self::parse(&std::fs::read(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bank(sprites: &[(u16, u16, &[u8])]) -> Vec<u8> {
        let mut d = MAGIC.to_vec();
        d.extend((sprites.len() as u32).to_le_bytes());
        let mut off = 8 + sprites.len() * 8;
        for (w, h, rle) in sprites {
            d.extend(w.to_le_bytes());
            d.extend(h.to_le_bytes());
            d.extend((off as u32).to_le_bytes());
            off += rle.len();
        }
        for (_, _, rle) in sprites {
            d.extend(*rle);
        }
        d
    }

    #[test]
    fn decodes_runs_and_skips() {
        // 3x2: row 0 "skip 1, 2 pixels", row 1 "1 pixel".
        let rle = [0xff, 2, 7, 8, 0, 1, 9, 0];
        let b = SpriteBank::parse(&bank(&[(3, 2, &rle)])).unwrap();
        let s = &b.sprites[0];
        assert_eq!(s.pixels, vec![None, Some(7), Some(8), Some(9), None, None]);
        assert_eq!(s.tip(), Some((1, 0)));
    }

    #[test]
    fn rejects_bad_files() {
        assert!(SpriteBank::parse(b"NOPE\0\0\0\0").is_err());
        assert!(SpriteBank::parse(&bank(&[(2, 1, &[2, 1])])).is_err(), "truncated run");
    }
}
