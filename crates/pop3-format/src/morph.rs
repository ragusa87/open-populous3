//! Morph animations of 3D objects (`objects/morph0-N.dat`): a textured base object whose points are replaced, frame
//! by frame, by a loop through untextured key frames. See docs/specs/objects.md "Records".

use crate::level::LevelError;
use crate::objects::find_file;
use std::path::Path;

pub const RECORD_SIZE: usize = 368;
const MAX_KEYS: usize = 20;

/// One morph record: its base object, its key frames and the steps from each key to the next (the last loops to
/// the first). The file's own segment table is ignored, as the original does: rebuilt from these two lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Morph {
    pub base: usize,
    pub keys: Vec<usize>,
    pub steps: Vec<u32>,
}

/// A frame of a morph: `num / den` of the way from object `from` to object `to`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MorphFrame {
    pub from: usize,
    pub to: usize,
    pub num: u32,
    pub den: u32,
}

impl Morph {
    /// Frames in one loop: each key and the steps after it.
    pub fn cycle_len(&self) -> u32 {
        self.steps.iter().map(|s| s + 1).sum()
    }

    /// Frame `f` of the loop (wrapping).
    pub fn frame(&self, f: u32) -> Option<MorphFrame> {
        let len = self.cycle_len();
        if len == 0 {
            return None;
        }
        let mut t = f % len;
        for (i, &steps) in self.steps.iter().enumerate() {
            if t <= steps {
                return Some(MorphFrame { from: self.keys[i], to: self.keys[(i + 1) % self.keys.len()], num: t, den: steps + 1 });
            }
            t -= steps + 1;
        }
        None
    }
}

/// Points `num / den` of the way from `from` to `to`, integer arithmetic truncating toward zero.
pub fn morph_points(from: &[[i16; 3]], to: &[[i16; 3]], num: u32, den: u32) -> Vec<[i16; 3]> {
    let den = den.max(1) as i32;
    from.iter().zip(to).map(|(a, b)| std::array::from_fn(|k| a[k] + ((b[k] as i32 - a[k] as i32) * num as i32 / den) as i16)).collect()
}

#[derive(Clone, Debug, Default)]
pub struct Morphs {
    pub records: Vec<Morph>,
}

fn i32_at(b: &[u8], o: usize) -> i32 {
    i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

impl Morphs {
    /// Records with a base object and at least one key frame; unused ones (base −1, count 0) are skipped.
    pub fn parse(data: &[u8]) -> Self {
        let records = data
            .chunks_exact(RECORD_SIZE)
            .filter_map(|r| {
                let (base, n) = (i32_at(r, 0), i32_at(r, 4));
                if base < 0 || n <= 0 || n as usize > MAX_KEYS {
                    return None;
                }
                let n = n as usize;
                let keys = (0..n).map(|i| i32_at(r, 8 + i * 4)).collect::<Vec<_>>();
                let steps = (0..n).map(|i| i32_at(r, 88 + i * 4)).collect::<Vec<_>>();
                if keys.iter().chain(&steps).any(|&v| v < 0) {
                    return None;
                }
                Some(Morph { base: base as usize, keys: keys.into_iter().map(|k| k as usize).collect(), steps: steps.into_iter().map(|s| s as u32).collect() })
            })
            .collect();
        Morphs { records }
    }

    /// `morph0-N.dat` of bank `bank` (file name matched case-insensitively).
    pub fn load(objects_dir: &Path, bank: u8) -> Result<Self, LevelError> {
        let name = format!("morph0-{bank}.dat");
        Ok(Self::parse(&std::fs::read(find_file(objects_dir, &name).unwrap_or_else(|| objects_dir.join(&name)))?))
    }

    /// The morph whose base object is `base`.
    pub fn for_base(&self, base: usize) -> Option<&Morph> {
        self.records.iter().find(|m| m.base == base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(base: i32, keys: &[i32], steps: &[i32]) -> Vec<u8> {
        let mut r = vec![0u8; RECORD_SIZE];
        r[0..4].copy_from_slice(&base.to_le_bytes());
        r[4..8].copy_from_slice(&(keys.len() as i32).to_le_bytes());
        for (i, (k, s)) in keys.iter().zip(steps).enumerate() {
            r[8 + i * 4..12 + i * 4].copy_from_slice(&k.to_le_bytes());
            r[88 + i * 4..92 + i * 4].copy_from_slice(&s.to_le_bytes());
        }
        r
    }

    #[test]
    fn parses_used_records_and_skips_empty_ones() {
        let mut data = record(100, &[101, 102, 103, 104, 105, 106], &[1; 6]);
        data.extend(record(-1, &[], &[]));
        data.extend(record(1, &[2, 3, 4], &[8, 8, 8]));
        let m = Morphs::parse(&data);
        assert_eq!(m.records.len(), 2);
        assert_eq!(m.for_base(100).unwrap().keys, vec![101, 102, 103, 104, 105, 106]);
        assert_eq!(m.for_base(1).unwrap().steps, vec![8, 8, 8]);
        assert!(m.for_base(7).is_none());
    }

    #[test]
    fn six_keys_of_one_step_loop_over_twelve_frames_key_then_half_way() {
        let m = Morph { base: 100, keys: vec![101, 102, 103, 104, 105, 106], steps: vec![1; 6] };
        assert_eq!(m.cycle_len(), 12);
        assert_eq!(m.frame(0), Some(MorphFrame { from: 101, to: 102, num: 0, den: 2 }));
        assert_eq!(m.frame(1), Some(MorphFrame { from: 101, to: 102, num: 1, den: 2 }));
        assert_eq!(m.frame(2), Some(MorphFrame { from: 102, to: 103, num: 0, den: 2 }));
        assert_eq!(m.frame(11), Some(MorphFrame { from: 106, to: 101, num: 1, den: 2 }), "the last loops to the first");
        assert_eq!(m.frame(12), m.frame(0), "wraps");
    }

    #[test]
    fn uneven_steps_give_segments_of_their_own_length() {
        let m = Morph { base: 7, keys: vec![8, 9, 10, 11], steps: vec![1, 1, 1, 6] };
        assert_eq!(m.cycle_len(), 13);
        assert_eq!(m.frame(6), Some(MorphFrame { from: 11, to: 8, num: 0, den: 7 }));
        assert_eq!(m.frame(12), Some(MorphFrame { from: 11, to: 8, num: 6, den: 7 }));
    }

    #[test]
    fn points_move_a_fraction_of_the_way_truncating_toward_zero() {
        let from = [[0, 10, -10]];
        let to = [[3, 13, -13]];
        assert_eq!(morph_points(&from, &to, 0, 2), vec![[0, 10, -10]]);
        assert_eq!(morph_points(&from, &to, 1, 2), vec![[1, 11, -11]]);
        assert_eq!(morph_points(&from, &to, 2, 2), vec![[3, 13, -13]]);
    }
}
