//! The unit sprites' material (`unit_sprite.wgsl`): a kind's frames as colour indices (`palette`) shared by every
//! tribe, each tribe's colours looked up in its rows of a colour table, filtered by hand (the indices can't be
//! filtered). The same material draws a hovered unit's halo (`outline`).

use super::palette::{ColourIndex, IndexAtlas};
use bevy::asset::{embedded_asset, RenderAssetUsages};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, RenderPipelineDescriptor, SpecializedMeshPipelineError, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;

/// Colours per row of the colour table.
pub const TABLE_WIDTH: usize = 256;
/// Empty texels around each frame in a kind's index atlas, so filtering near a frame's edge never reads its
/// neighbour's pixels.
pub const ATLAS_PAD: usize = 3;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct UnitSpriteMaterial {
    /// `R16Uint` colour indices (`palette::IndexAtlas`).
    #[texture(0, sample_type = "u_int")]
    index: Handle<Image>,
    /// Every tribe's colours, `rows` rows each (`colour_table`).
    #[texture(1)]
    colours: Handle<Image>,
    /// x: this tribe's first row in `colours`, y: alpha multiplier, z: outline width in texels (0: the sprite
    /// itself), w: unused.
    #[uniform(2)]
    params: Vec4,
    #[uniform(3)]
    outline_colour: LinearRgba,
    /// The outlined frame's `[u_min, v_min, u_max, v_max]` in `index`.
    #[uniform(4)]
    frame_rect: Vec4,
    blend: bool,
}

impl UnitSpriteMaterial {
    /// One tribe's look of a kind: its colours start at row `first_row` of `colours`; `blend` for frames with
    /// see-through pixels (`AlphaMode::Blend`), else cut out.
    pub fn sprite(index: Handle<Image>, colours: Handle<Image>, first_row: usize, blend: bool) -> Self {
        UnitSpriteMaterial { index, colours, params: Vec4::new(first_row as f32, 1.0, 0.0, 0.0), outline_colour: LinearRgba::NONE, frame_rect: Vec4::ZERO, blend }
    }

    /// This look, see-through at `alpha`.
    pub fn ghost(&self, alpha: f32) -> Self {
        UnitSpriteMaterial { params: Vec4::new(self.params.x, alpha, self.params.z, 0.0), blend: true, ..self.clone() }
    }

    /// Only the pixels within `width` texels outside the frame at `frame_rect`, in `colour`.
    pub fn outline(&self, colour: Color, width: f32, frame_rect: Vec4) -> Self {
        UnitSpriteMaterial { params: Vec4::new(self.params.x, 1.0, width, 0.0), outline_colour: colour.into(), frame_rect, blend: false, ..self.clone() }
    }

    pub fn index(&self) -> &Handle<Image> {
        &self.index
    }
}

impl Material for UnitSpriteMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://game_client/units/unit_sprite.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        if self.blend { AlphaMode::Blend } else { AlphaMode::Mask(0.5) }
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    /// Both sides: some face the camera with their back.
    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

pub struct UnitSpriteMaterialPlugin;

impl Plugin for UnitSpriteMaterialPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "unit_sprite.wgsl");
        app.add_plugins(MaterialPlugin::<UnitSpriteMaterial>::default());
    }
}

/// Rows of `TABLE_WIDTH` colours one tribe's table of `colours` colours takes.
pub fn table_rows(colours: usize) -> usize {
    colours.div_ceil(TABLE_WIDTH).max(1)
}

/// Every table one after the other, each padded to whole rows (`table_rows` of the longest): RGBA bytes,
/// `TABLE_WIDTH` wide, and the rows each takes.
pub fn colour_table(tables: &[&[[u8; 4]]]) -> (Vec<u8>, usize) {
    let rows = table_rows(tables.iter().map(|t| t.len()).max().unwrap_or(0));
    let mut rgba = vec![0u8; rows * TABLE_WIDTH * 4 * tables.len()];
    for (i, t) in tables.iter().enumerate() {
        let at = i * rows * TABLE_WIDTH * 4;
        rgba[at..at + t.len() * 4].copy_from_slice(t.as_flattened());
    }
    (rgba, rows)
}

/// Whether a frame with these indices shows see-through pixels in a tribe's `table` (drawn blended).
pub fn shows_partial_alpha(index: &[ColourIndex], table: &[[u8; 4]]) -> bool {
    index.iter().any(|&k| table.get(k as usize).is_some_and(|c| c[3] != 0 && c[3] != 255))
}

pub fn index_image(atlas: IndexAtlas) -> Image {
    let bytes = atlas.index.iter().flat_map(|k| k.to_le_bytes()).collect();
    Image::new(Extent3d { width: atlas.width as u32, height: atlas.height as u32, depth_or_array_layers: 1 }, TextureDimension::D2, bytes, TextureFormat::R16Uint, RenderAssetUsages::RENDER_WORLD)
}

pub fn colour_image(rgba: Vec<u8>) -> Image {
    let height = (rgba.len() / (TABLE_WIDTH * 4)) as u32;
    Image::new(Extent3d { width: TABLE_WIDTH as u32, height, depth_or_array_layers: 1 }, TextureDimension::D2, rgba, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_table_starts_on_its_own_row_block() {
        let (a, b) = (vec![[1, 1, 1, 255]; 3], vec![[2, 2, 2, 255]; 300]);
        let (rgba, rows) = colour_table(&[&a, &b]);
        assert_eq!(rows, 2);
        assert_eq!(rgba.len(), 2 * rows * TABLE_WIDTH * 4);
        assert_eq!(&rgba[..4], &[1, 1, 1, 255]);
        assert_eq!(&rgba[3 * 4..4 * 4], &[0, 0, 0, 0], "padding past a table is transparent");
        let second = rows * TABLE_WIDTH * 4;
        assert_eq!(&rgba[second..second + 4], &[2, 2, 2, 255]);
        assert_eq!(&rgba[second + 299 * 4..second + 300 * 4], &[2, 2, 2, 255]);
    }

    #[test]
    fn an_empty_table_still_takes_a_row() {
        assert_eq!(table_rows(0), 1);
        assert_eq!(table_rows(256), 1);
        assert_eq!(table_rows(257), 2);
    }

    #[test]
    fn partial_alpha_depends_on_the_tribe_table() {
        let solid = [[0, 0, 0, 0], [9, 9, 9, 255]];
        let glowing = [[0, 0, 0, 0], [9, 9, 9, 120]];
        assert!(!shows_partial_alpha(&[0, 1], &solid));
        assert!(shows_partial_alpha(&[0, 1], &glowing));
        assert!(!shows_partial_alpha(&[0], &glowing));
    }
}
