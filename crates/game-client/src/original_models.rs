//! Original 3D objects (`pop3_format::ObjectBank`) as Bevy meshes, textured from the theme's
//! `bl320` atlas. Only used when the original files are present (never with `--no-original`).

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use pop3_format::objects::{ATLAS_HEIGHT, ATLAS_WIDTH, TILE};
use pop3_format::{Atlas, Object, ObjectBank, WORLD_UNITS_PER_CELL};
use std::path::Path;

/// Rows appended under the atlas holding the 256 palette colours, for untextured faces.
const PALETTE_ROWS: usize = 8;
pub const IMAGE_HEIGHT: usize = ATLAS_HEIGHT + PALETTE_ROWS;
/// Capped stone pillar used around reincarnation sites (see docs/specs/objects.md).
pub const RS_PILLAR: usize = 76;

/// Bank 0 of the original objects, loaded once when original files are allowed.
#[derive(Resource, Default)]
pub struct OriginalObjects(pub Option<ObjectBank>);

impl OriginalObjects {
    pub fn load(objects_dir: &Path) -> Self {
        OriginalObjects(
            ObjectBank::load(objects_dir, 0).map_err(|e| warn!("objects {}: {e}", objects_dir.display())).ok(),
        )
    }
}

#[derive(Default, Debug, PartialEq)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

/// Flat-shaded triangles (quads split 0-1-2, 0-2-3), in cells (1 cell = 512 units).
pub fn object_mesh(obj: &Object) -> MeshData {
    let scale = 1.0 / WORLD_UNITS_PER_CELL as f32;
    let mut m = MeshData::default();
    for face in &obj.faces {
        let corners: Vec<([f32; 3], [f32; 2])> = face
            .points
            .iter()
            .zip(&face.uv)
            .map(|(&i, &uv)| {
                let p = obj.points[i as usize];
                ([p[0] as f32 * scale, p[1] as f32 * scale, p[2] as f32 * scale], face_uv(face.tile, face.colour, uv))
            })
            .collect();
        for tri in [[0, 1, 2], [0, 2, 3]].iter().take(corners.len() - 2) {
            let [a, b, c] = tri.map(|k| corners[k].0);
            let n = (Vec3::from(b) - Vec3::from(a)).cross(Vec3::from(c) - Vec3::from(a)).normalize_or(Vec3::Y);
            for k in tri {
                m.indices.push(m.positions.len() as u32);
                m.positions.push(corners[*k].0);
                m.normals.push(n.into());
                m.uvs.push(corners[*k].1);
            }
        }
    }
    m
}

/// Normalised UV in the atlas image (atlas + palette rows); flat faces sample their palette texel.
pub fn face_uv(tile: Option<u16>, colour: u8, (u, v): (i32, i32)) -> [f32; 2] {
    let (w, h) = (ATLAS_WIDTH as f32, IMAGE_HEIGHT as f32);
    match tile {
        Some(t) => {
            let (tx, ty) = Atlas::tile_origin(t);
            let texel = |c: i32| (c as f32 / 65536.0).clamp(0.5, TILE as f32 - 0.5);
            [(tx as f32 + texel(u)) / w, (ty as f32 + texel(v)) / h]
        }
        None => [(colour as f32 + 0.5) / w, (ATLAS_HEIGHT as f32 + PALETTE_ROWS as f32 / 2.0) / h],
    }
}

/// RGBA pixels of the atlas followed by the palette rows (index 0 is transparent black in the game,
/// kept opaque here).
pub fn atlas_rgba(atlas: &Atlas, palette: &[[u8; 3]]) -> Vec<u8> {
    let rgba = |i: u8| {
        let c = palette.get(i as usize).copied().unwrap_or([255, 0, 255]);
        [c[0], c[1], c[2], 255]
    };
    let palette_row = (0..=255u8).flat_map(rgba);
    atlas
        .pixels
        .iter()
        .flat_map(|&i| rgba(i))
        .chain(std::iter::repeat_n(palette_row, PALETTE_ROWS).flatten())
        .collect()
}

pub fn to_mesh(data: MeshData) -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, data.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, data.uvs)
        .with_inserted_indices(Indices::U32(data.indices))
}

pub fn atlas_image(atlas: &Atlas, palette: &[[u8; 3]]) -> Image {
    let mut image = Image::new(
        Extent3d { width: ATLAS_WIDTH as u32, height: IMAGE_HEIGHT as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        atlas_rgba(atlas, palette),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Nearest,
        min_filter: ImageFilterMode::Nearest,
        ..default()
    });
    image
}

#[cfg(test)]
mod tests {
    use super::*;
    use pop3_format::Face;

    fn quad() -> Object {
        Object {
            points: vec![[0, 0, 0], [512, 0, 0], [512, 512, 0], [0, 512, 0]],
            faces: vec![Face { tile: Some(9), colour: 0, points: vec![0, 1, 2, 3], uv: vec![(0, 0); 4] }],
        }
    }

    #[test]
    fn quad_becomes_two_triangles_in_cells() {
        let m = object_mesh(&quad());
        assert_eq!(m.indices.len(), 6);
        assert_eq!(m.positions[2], [1.0, 1.0, 0.0]);
        assert_eq!(m.normals[0], [0.0, 0.0, 1.0]);
    }

    #[test]
    fn uvs_point_into_tile_or_palette_row() {
        let [u, v] = face_uv(Some(9), 0, (16 << 16, 40 << 16));
        assert_eq!(u * 256.0, 48.0);
        assert_eq!(v * IMAGE_HEIGHT as f32, 63.5, "clamped inside the tile");
        let [u, v] = face_uv(None, 7, (0, 0));
        assert_eq!(u * 256.0, 7.5);
        assert!(v * IMAGE_HEIGHT as f32 > ATLAS_HEIGHT as f32);
    }

    #[test]
    fn atlas_rgba_appends_palette() {
        let atlas = Atlas { pixels: vec![1; ATLAS_WIDTH * ATLAS_HEIGHT] };
        let mut pal = vec![[0u8; 3]; 256];
        pal[1] = [10, 20, 30];
        let px = atlas_rgba(&atlas, &pal);
        assert_eq!(px.len(), ATLAS_WIDTH * IMAGE_HEIGHT * 4);
        assert_eq!(&px[..4], &[10, 20, 30, 255]);
        let row = ATLAS_HEIGHT * ATLAS_WIDTH * 4;
        assert_eq!(&px[row + 4..row + 8], &[10, 20, 30, 255]);
    }
}
