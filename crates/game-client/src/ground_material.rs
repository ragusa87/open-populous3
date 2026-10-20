//! The terrain's ground material: `ground.wgsl` evaluates `terrain_texture.rs`'s per-cell data per fragment
//! (see that module's doc for why a shader, not a baked texture). `world.rs` owns the one `GroundMaterial`
//! instance (`GroundTerrainMaterial`), created once with placeholder textures at startup (`placeholder`) and
//! filled with the real per-level theme/terrain data on first bake and every level switch (`fill`); `sync`
//! pushes a tick's worth of dirty-rect updates (`terrain_texture::rebake_cells`'s own output) to the GPU.

use crate::terrain_texture::GroundCells;
use bevy::asset::embedded_asset;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use pop3_format::theme::{BIGFADE_ROWS, BIGFADE_WIDTH, CLIFF_ROWS, CLIFF_WIDTH, DISP_SIZE};
use pop3_format::Theme;

/// x: map size (cells), y: grid overlay on/off (0/1), z: `PX_PER_CELL` (the shader's own `px`/`py` integer pixel
/// coordinates per cell, `disp`/`water`'s lookup granularity) — a uniform, not a WGSL constant, so this one Rust
/// constant is the only place it's spelled out. w: unused.
pub fn params(size: usize, grid: bool) -> Vec4 {
    Vec4::new(size as f32, if grid { 1.0 } else { 0.0 }, crate::terrain_texture::PX_PER_CELL as f32, 0.0)
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct GroundMaterial {
    #[uniform(0)]
    pub params: Vec4,
    #[texture(1)]
    #[sampler(2)]
    pub channels: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    pub altitude: Handle<Image>,
    #[texture(5)]
    pub palette: Handle<Image>,
    #[texture(6, sample_type = "u_int")]
    pub bigfade: Handle<Image>,
    #[texture(7, sample_type = "u_int")]
    pub cliff: Handle<Image>,
    /// Signed (R8Snorm, the raw on-disk byte reinterpreted bit-for-bit as i8/127.0 — exactly the signed read
    /// `pop3_format::theme::Theme::disp` always meant): bilinear, repeat-wrapped, unlike `bigfade`/`cliff`'s exact
    /// `textureLoad`. A `textureLoad`-only lookup here (as `bigfade`/`cliff` use) reproduced the per-pixel jitter
    /// faithfully but with none of the smoothing the old baked-texture pipeline got for free from the GPU's own
    /// bilinear *magnification* of its coarse 8 px/cell texture — on steep, high terrain (`disp_sensitivity` near
    /// its max) that showed up as harsh, blocky light/dark patches a real screen pixel's neighbour never had
    /// before. Bilinear sampling here restores that smoothing while staying per-fragment.
    #[texture(8)]
    #[sampler(9)]
    pub disp: Handle<Image>,
    /// Unsigned (R8Unorm, the raw on-disk `watdisp.dat` byte): bilinear for the same reason as `disp`.
    #[texture(10)]
    #[sampler(11)]
    pub water: Handle<Image>,
}

impl Material for GroundMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://game_client/ground.wgsl".into()
    }
}

fn filterable_sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    })
}

fn lut_sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Nearest,
        min_filter: ImageFilterMode::Nearest,
        ..default()
    })
}

fn image(size: (u32, u32), format: TextureFormat, data: Vec<u8>, sampler: ImageSampler) -> Image {
    let mut image = Image::new(
        Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        format,
        bevy::asset::RenderAssetUsages::default(),
    );
    image.sampler = sampler;
    image
}

/// `altitude` packed as half-float bytes (R16Float: filterable on every backend without extra device features,
/// unlike R32Float — see docs/plans/terrain-fragment-shader.md "Open questions" in git history).
fn altitude_bytes(altitude: &[f32]) -> Vec<u8> {
    altitude.iter().flat_map(|&v| half::f16::from_f32(v).to_le_bytes()).collect()
}

fn channels_image(cells: &GroundCells) -> Image {
    let side = cells.side as u32;
    image((side, side), TextureFormat::Rgba8Unorm, cells.channels.clone(), filterable_sampler())
}

fn altitude_image(cells: &GroundCells) -> Image {
    let side = cells.side as u32;
    image((side, side), TextureFormat::R16Float, altitude_bytes(&cells.altitude), filterable_sampler())
}

fn palette_image(theme: &Theme) -> Image {
    image((256, 1), TextureFormat::Rgba8UnormSrgb, crate::terrain_texture::palette_rgba(theme), lut_sampler())
}

fn bigfade_image(theme: &Theme) -> Image {
    image((BIGFADE_WIDTH as u32, BIGFADE_ROWS as u32), TextureFormat::R8Uint, theme.bigfade.clone(), lut_sampler())
}

fn cliff_image(theme: &Theme) -> Image {
    image((CLIFF_WIDTH as u32, CLIFF_ROWS as u32), TextureFormat::R8Uint, theme.cliff.clone(), lut_sampler())
}

fn disp_image(theme: &Theme) -> Image {
    image((DISP_SIZE as u32, DISP_SIZE as u32), TextureFormat::R8Snorm, theme.disp.clone(), filterable_sampler())
}

fn water_image(theme: &Theme) -> Image {
    image((DISP_SIZE as u32, DISP_SIZE as u32), TextureFormat::R8Unorm, theme.water.clone(), filterable_sampler())
}

/// A material whose texture handles are all valid (1x1 stand-ins) so it can be attached to the terrain mesh
/// before the first real theme/heightmap is ever baked (`fill` replaces all of it on the first `rebuild_terrain`
/// pass).
pub fn placeholder(images: &mut Assets<Image>) -> GroundMaterial {
    GroundMaterial {
        params: Vec4::ZERO,
        channels: images.add(image((1, 1), TextureFormat::Rgba8Unorm, vec![0, 0, 0, 255], lut_sampler())),
        altitude: images.add(image((1, 1), TextureFormat::R16Float, vec![0, 0], filterable_sampler())),
        palette: images.add(image((1, 1), TextureFormat::Rgba8UnormSrgb, vec![0, 0, 0, 255], lut_sampler())),
        bigfade: images.add(image((1, 1), TextureFormat::R8Uint, vec![0], lut_sampler())),
        cliff: images.add(image((1, 1), TextureFormat::R8Uint, vec![0], lut_sampler())),
        disp: images.add(image((1, 1), TextureFormat::R8Snorm, vec![0], filterable_sampler())),
        water: images.add(image((1, 1), TextureFormat::R8Unorm, vec![0], filterable_sampler())),
    }
}

/// Replaces every texture (a new theme or level: `world::rebuild_terrain`'s own "no bake yet" branch) and the
/// uniform params.
pub fn fill(material: &mut GroundMaterial, images: &mut Assets<Image>, cells: &GroundCells, theme: &Theme, grid: bool) {
    material.params = params(cells.side, grid);
    material.channels = images.add(channels_image(cells));
    material.altitude = images.add(altitude_image(cells));
    material.palette = images.add(palette_image(theme));
    material.bigfade = images.add(bigfade_image(theme));
    material.cliff = images.add(cliff_image(theme));
    material.disp = images.add(disp_image(theme));
    material.water = images.add(water_image(theme));
}

/// Pushes `cells`'s current content to the GPU (a tick's worth of `terrain_texture::rebake_cells` dirty-rect
/// writes) and the grid toggle, without touching the theme LUTs or reallocating the per-cell textures.
pub fn sync(material: &GroundMaterial, images: &mut Assets<Image>, cells: &GroundCells) {
    if let Some(mut image) = images.get_mut(&material.channels) {
        image.data = Some(cells.channels.clone());
    }
    if let Some(mut image) = images.get_mut(&material.altitude) {
        image.data = Some(altitude_bytes(&cells.altitude));
    }
}

pub struct GroundMaterialPlugin;

impl Plugin for GroundMaterialPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "ground.wgsl");
        app.add_plugins(MaterialPlugin::<GroundMaterial>::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_packs_size_grid_flag_and_px_per_cell() {
        assert_eq!(params(128, true), Vec4::new(128.0, 1.0, crate::terrain_texture::PX_PER_CELL as f32, 0.0));
        assert_eq!(params(128, false).y, 0.0);
    }

    #[test]
    fn altitude_bytes_roundtrip_through_half_precision() {
        let bytes = altitude_bytes(&[0.0, 500.0, 1022.0]);
        assert_eq!(bytes.len(), 6);
        let back: Vec<f32> = bytes.chunks_exact(2).map(|c| half::f16::from_le_bytes([c[0], c[1]]).to_f32()).collect();
        assert_eq!(back, vec![0.0, 500.0, 1022.0], "every altitude value is an integer well within f16's exact range");
    }
}
