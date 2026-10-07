//! 3D object banks from `objects/` (`objs0-N.dat`, `pnts0-N.dat`, `facs0-N.dat`) and the
//! object texture atlas `data/bl320-X.dat`. See docs/specs/objects.md.

use crate::level::LevelError;
use std::path::{Path, PathBuf};

pub const OBJECT_SIZE: usize = 54;
pub const POINT_SIZE: usize = 6;
pub const FACE_SIZE: usize = 60;
/// Atlas: 8 x 32 tiles of 32x32 palette indices.
pub const ATLAS_WIDTH: usize = 256;
pub const ATLAS_HEIGHT: usize = 1024;
pub const TILE: usize = 32;
pub const TILES_PER_ROW: usize = ATLAS_WIDTH / TILE;
/// `Face::tile` value of an untextured face, drawn with `Face::colour`.
const NO_TILE: u16 = 0xffff;
/// `Face::flags` bit of a blended face: its texels are alpha pixels (`blend::AlphaTable`, tint and
/// strength), not palette colours. Only the flame boards (tile 92) of the camp fires, the large
/// huts, the firewarrior training huts and the guard posts have it.
pub const FACE_ALPHA: u8 = 0x20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Face {
    /// Atlas tile, None for a flat-coloured face.
    pub tile: Option<u16>,
    /// Palette index for flat faces.
    pub colour: u8,
    /// 3 or 4 indices into `Object::points`.
    pub points: Vec<u16>,
    /// Per point texel inside the tile, 16.16 fixed point (0..32).
    pub uv: Vec<(i32, i32)>,
    /// Byte 7 of the record: render flags, mostly unknown (`FACE_ALPHA`).
    pub flags: u8,
}

impl Face {
    /// Whether the face is blended through the alpha table (`FACE_ALPHA`): a flame.
    pub fn is_alpha(&self) -> bool {
        self.flags & FACE_ALPHA != 0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Object {
    /// x, y (up), z in world units (512 per cell).
    pub points: Vec<[i16; 3]>,
    pub faces: Vec<Face>,
}

impl Object {
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }
}

#[derive(Clone, Debug, Default)]
pub struct ObjectBank {
    pub objects: Vec<Object>,
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn i32_at(b: &[u8], o: usize) -> i32 {
    i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn parse_face(r: &[u8]) -> Option<Face> {
    let n = r[6] as usize;
    if !(3..=4).contains(&n) {
        return None;
    }
    let tile = u16_at(r, 2);
    Some(Face {
        tile: (tile != NO_TILE).then_some(tile),
        colour: r[0],
        points: (0..n).map(|k| u16_at(r, 40 + k * 2)).collect(),
        uv: (0..n).map(|k| (i32_at(r, 8 + k * 8), i32_at(r, 12 + k * 8))).collect(),
        flags: r[7],
    })
}

impl ObjectBank {
    /// Object records hold 1-based `[start, end)` ranges into the face and point files.
    pub fn parse(objs: &[u8], pnts: &[u8], facs: &[u8]) -> Self {
        let points: Vec<[i16; 3]> = pnts
            .chunks_exact(POINT_SIZE)
            .map(|c| [0, 2, 4].map(|o| i16::from_le_bytes([c[o], c[o + 1]])))
            .collect();
        let face_recs: Vec<&[u8]> = facs.chunks_exact(FACE_SIZE).collect();
        let range = |start: i32, end: i32, len: usize| {
            let (s, e) = ((start - 1).max(0) as usize, (end - 1).max(0) as usize);
            (s <= e && e <= len).then_some(s..e)
        };
        let objects = objs
            .chunks_exact(OBJECT_SIZE)
            .map(|r| {
                let faces = range(i32_at(r, 16), i32_at(r, 20), face_recs.len());
                let pts = range(i32_at(r, 24), i32_at(r, 28), points.len());
                match (faces, pts) {
                    (Some(f), Some(p)) => {
                        let points = points[p].to_vec();
                        let faces = face_recs[f]
                            .iter()
                            .filter_map(|r| parse_face(r))
                            .filter(|f| f.points.iter().all(|&i| (i as usize) < points.len()))
                            .collect();
                        Object { points, faces }
                    }
                    _ => Object::default(),
                }
            })
            .collect();
        ObjectBank { objects }
    }

    /// Bank `n` from the original `objects/` directory (file names are matched case-insensitively).
    pub fn load(objects_dir: &Path, bank: u8) -> Result<Self, LevelError> {
        let read = |p: &str| -> Result<Vec<u8>, LevelError> {
            let name = format!("{p}0-{bank}.dat");
            Ok(std::fs::read(find_file(objects_dir, &name).unwrap_or_else(|| objects_dir.join(&name)))?)
        };
        Ok(Self::parse(&read("objs")?, &read("pnts")?, &read("facs")?))
    }

    pub fn get(&self, index: usize) -> Option<&Object> {
        self.objects.get(index).filter(|o| !o.is_empty())
    }
}

/// `bl320-X.dat`: object textures as palette indices, `ATLAS_WIDTH` x `ATLAS_HEIGHT`.
#[derive(Clone, Debug)]
pub struct Atlas {
    pub pixels: Vec<u8>,
}

impl Atlas {
    pub fn parse(data: &[u8]) -> Result<Self, LevelError> {
        let n = ATLAS_WIDTH * ATLAS_HEIGHT;
        if data.len() < n {
            return Err(LevelError::BadSize { expected: n, got: data.len() });
        }
        Ok(Atlas { pixels: data[..n].to_vec() })
    }

    pub fn load(data_dir: &Path, theme: u8) -> Result<Self, LevelError> {
        let name = format!("bl320-{}.dat", crate::theme_char(theme));
        Self::parse(&std::fs::read(find_file(data_dir, &name).unwrap_or_else(|| data_dir.join(&name)))?)
    }

    /// Top-left pixel of an atlas tile.
    pub fn tile_origin(tile: u16) -> (usize, usize) {
        ((tile as usize % TILES_PER_ROW) * TILE, (tile as usize / TILES_PER_ROW) * TILE)
    }
}

/// The original install mixes upper and lower case file names.
pub fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.file_name().is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object_rec(faces: (i32, i32), points: (i32, i32)) -> Vec<u8> {
        let mut r = vec![0u8; OBJECT_SIZE];
        for (o, v) in [(16, faces.0), (20, faces.1), (24, points.0), (28, points.1)] {
            r[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        r
    }

    fn face_rec(tile: u16, idx: &[u16]) -> Vec<u8> {
        let mut r = vec![0u8; FACE_SIZE];
        r[0] = 9;
        r[2..4].copy_from_slice(&tile.to_le_bytes());
        r[6] = idx.len() as u8;
        for (k, &i) in idx.iter().enumerate() {
            r[8 + k * 8..12 + k * 8].copy_from_slice(&((k as i32) << 16).to_le_bytes());
            r[40 + k * 2..42 + k * 2].copy_from_slice(&i.to_le_bytes());
        }
        r
    }

    #[test]
    fn ranges_are_one_based() {
        let pnts: Vec<u8> = [[9i16, 9, 9], [1, 2, 3], [4, 5, 6], [7, 8, 9]]
            .iter()
            .flat_map(|p| p.iter().flat_map(|v| v.to_le_bytes()))
            .collect();
        let facs = [face_rec(0, &[0, 0, 0]), face_rec(17, &[0, 1, 2]), face_rec(NO_TILE, &[2, 1, 0, 1])].concat();
        let objs = [object_rec((2, 4), (2, 5)), object_rec((0, 0), (0, 0))].concat();
        let bank = ObjectBank::parse(&objs, &pnts, &facs);
        let o = bank.get(0).unwrap();
        assert_eq!(o.points, vec![[1, 2, 3], [4, 5, 6], [7, 8, 9]]);
        assert_eq!(o.faces.len(), 2);
        assert_eq!(o.faces[0].tile, Some(17));
        assert_eq!(o.faces[0].uv[2], (2 << 16, 0));
        assert_eq!((o.faces[1].tile, o.faces[1].colour, o.faces[1].points.len()), (None, 9, 4));
        assert!(bank.get(1).is_none(), "empty slot");
    }

    #[test]
    fn faces_pointing_outside_the_object_are_dropped() {
        let pnts = vec![0u8; POINT_SIZE * 3];
        let facs = face_rec(1, &[0, 1, 7]);
        let bank = ObjectBank::parse(&object_rec((1, 2), (1, 4)), &pnts, &facs);
        assert!(bank.get(0).is_none());
    }

    #[test]
    fn byte_7_flags_blended_faces() {
        let pnts = vec![0u8; POINT_SIZE * 3];
        let mut flame = face_rec(92, &[0, 1, 2]);
        flame[7] = FACE_ALPHA;
        let facs = [face_rec(1, &[0, 1, 2]), flame].concat();
        let o = ObjectBank::parse(&object_rec((1, 3), (1, 4)), &pnts, &facs).objects.remove(0);
        assert_eq!(o.faces.iter().map(Face::is_alpha).collect::<Vec<_>>(), [false, true]);
    }

    /// The camp fire's flame boards are its only blended faces (skipped without an install).
    #[test]
    fn the_camp_fire_flame_is_blended() {
        let Some(dir) = crate::install::find_install().map(|i| crate::install::subdir(&i, "objects")) else { return };
        let Ok(bank) = ObjectBank::load(&dir, 0) else { return };
        let fire = bank.get(crate::catalog::CAMP_FIRE).unwrap();
        let flame: Vec<_> = fire.faces.iter().filter(|f| f.is_alpha()).collect();
        assert_eq!(flame.len(), 8, "two crossed boards, two halves, both sides");
        assert!(flame.iter().all(|f| f.tile == Some(crate::catalog::FLAME_TILE)));
        assert!(fire.faces.iter().filter(|f| f.tile == Some(crate::catalog::FLAME_TILE)).all(Face::is_alpha));
    }

    #[test]
    fn tile_origins() {
        assert_eq!(Atlas::tile_origin(0), (0, 0));
        assert_eq!(Atlas::tile_origin(9), (32, 32));
        assert!(Atlas::parse(&[0; 10]).is_err());
    }
}
