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
/// constant is the only place it's spelled out. w: the sea's own scroll phase, `set_turn`'s own field - starts at
/// 0 here, `world.rs::animate_water` drives it every frame after.
pub fn params(size: usize, grid: bool) -> Vec4 {
    Vec4::new(size as f32, if grid { 1.0 } else { 0.0 }, crate::terrain_texture::PX_PER_CELL as f32, 0.0)
}

/// `ground.wgsl`'s own `WATER_FREQUENCY` and `WATER_OPENNESS_RADIUS`: the ripple's texels per cell pixel, and how
/// far from land (cells) it reaches full height.
const WATER_FREQUENCY: i32 = 24;
const WATER_OPENNESS_RADIUS: i32 = 3;

/// How far over the ground the sea is drawn at its corner `(x, z)` at `turn`, height units: `ground.wgsl`'s
/// `vertex` (pop3-rev-analysis.md "Water rendering (watdisp.dat)": two counter-scrolling `water` reads, summed, / 8)
/// times its open-water factor, 0 within a cell of land (`walkable`) rising to 1 at `WATER_OPENNESS_RADIUS`.
pub fn water_lift_at_corner(water: &[u8], turn: u32, (x, z): (i32, i32), walkable: &impl Fn(i32, i32) -> bool) -> f32 {
    if water.len() < DISP_SIZE * DISP_SIZE {
        return 0.0;
    }
    let (shift, step, n) = ((turn % 256) as i32, crate::terrain_texture::PX_PER_CELL as i32 * WATER_FREQUENCY, DISP_SIZE as i32);
    let texel = |d: i32| water[((z * step + d).rem_euclid(n) * n + (x * step + d).rem_euclid(n)) as usize] as f32;
    let r = WATER_OPENNESS_RADIUS;
    let mut d2 = ((r + 1) * (r + 1)) as f32;
    for dz in -r..=r {
        for dx in -r..=r {
            let here = (dx * dx + dz * dz) as f32;
            if here < d2 && walkable(x + dx, z + dz) {
                d2 = here;
            }
        }
    }
    let t = ((d2.sqrt() - (r - 1) as f32) / 1.0).clamp(0.0, 1.0);
    (texel(-shift) + texel(shift)) / 8.0 * t * t * (3.0 - 2.0 * t)
}

/// How far over the ground the sea is drawn at `at` (cells, fractional), height units: its four corners'
/// `water_lift_at_corner`, blended across the cell.
pub fn water_lift(water: &[u8], turn: u32, at: Vec2, walkable: &impl Fn(i32, i32) -> bool) -> f32 {
    let (c, f) = (at.floor(), at - at.floor());
    let corner = |dx: i32, dz: i32| water_lift_at_corner(water, turn, (c.x as i32 + dx, c.y as i32 + dz), walkable);
    let near = corner(0, 0) + (corner(1, 0) - corner(0, 0)) * f.x;
    let far = corner(0, 1) + (corner(1, 1) - corner(0, 1)) * f.x;
    near + (far - near) * f.y
}

/// `water_lift` against the live terrain material (its `water` texture and scroll turn) and `map`'s coast, height
/// units; 0 before the material is filled.
pub fn drawn_water_lift(mats: &Assets<GroundMaterial>, images: &Assets<Image>, material: &Handle<GroundMaterial>, map: &game_core::terrain::Heightmap, at: Vec2) -> f32 {
    let Some(m) = mats.get(material) else { return 0.0 };
    let Some(water) = images.get(&m.water).and_then(|i| i.data.as_deref()) else { return 0.0 };
    water_lift(water, m.params.w as u32, at, &|x, z| !crate::terrain_texture::looks_like_sea(map, (x, z)))
}

/// `params.w` (pop3-rev-analysis.md "Water rendering (watdisp.dat)", confirmed by peterderivaz/populous, README
/// "References"): `turn % 256`, the sea's own diagonal scroll phase (two counter-scrolling samples, so there is
/// no net drift in any direction - only local swirls). Set every frame (`world.rs::animate_water`), independently
/// of `fill`/`sync` - the sea must keep rippling even on a frame `rebuild_terrain` skips entirely because nothing
/// else changed.
pub fn set_turn(material: &mut GroundMaterial, turn: u32) {
    material.params.w = (turn % 256) as f32;
}

/// `water_params.x` (`ground.wgsl`'s vertex stage): the live `terrain_mesh::CurveParams::height_scale` (the
/// same height-units-to-render-units conversion the CPU mesh build already uses, `camera::CurveParamsRes` - a
/// relief slider can change it at runtime, `hud/view_menu.rs`), so the sea's own ripple displacement stays in the
/// same units as everything else instead of a separately-guessed constant. Set every frame alongside `set_turn`.
pub fn set_height_scale(material: &mut GroundMaterial, height_scale: f32) {
    material.water_params.x = height_scale;
}

/// `GroundMaterial::highlight` for the cell the mouse hovers, `None` when there is none to show (not dev mode,
/// the grid is off, or the cursor isn't over the map).
pub fn highlight_uniform(cell: Option<(i32, i32)>) -> Vec4 {
    match cell {
        Some((x, z)) => Vec4::new(x as f32, z as f32, 1.0, 0.0),
        None => Vec4::ZERO,
    }
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
    /// Unsigned (R8Unorm, the raw on-disk `watdisp.dat` byte): bilinear, same reason as `disp`. A precomputed
    /// ripple/sine-like pattern, not noise (`ground.wgsl`'s `water_height_offset` doc comment, confirmed by
    /// peterderivaz/populous): two counter-scrolling reads drive the sea's own vertex altitude and lighting.
    #[texture(10)]
    #[sampler(11)]
    pub water: Handle<Image>,
    /// The dev-mode grid's hovered cell (`cell_debug.rs`), for `ground.wgsl`'s highlight tint: x, y its own
    /// coordinates, z 1.0/0.0 on or off, w unused. `highlight_uniform` packs it.
    #[uniform(12)]
    pub highlight: Vec4,
    /// x: `terrain_mesh::CurveParams::height_scale`, `set_height_scale`'s own field - the sea's vertex-stage
    /// ripple displacement (`ground.wgsl`'s `vertex`) converts into the same render units the CPU-built mesh
    /// already uses. y, z, w: unused.
    #[uniform(13)]
    pub water_params: Vec4,
    /// The local player's explored cells (R8Unorm, 255 explored, 0 not: `fog_bytes`): bilinear, repeat-wrapped,
    /// grid-vertex-aligned like `channels`, so each corner takes its own cell's brightness and unexplored land fades
    /// to black over one cell (pop3-rev-analysis.md "Fog of war" "Drawing"). All white without fog; always map-sized,
    /// so a reveal rewrites it in place (`fog::sync_ground_fog`) instead of swapping in an image not uploaded yet.
    #[texture(14)]
    #[sampler(15)]
    pub fog: Handle<Image>,
}

impl Material for GroundMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://game_client/ground.wgsl".into()
    }

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
        highlight: Vec4::ZERO,
        water_params: Vec4::new(crate::terrain_mesh::BASE_HEIGHT_SCALE, 0.0, 0.0, 0.0),
        fog: images.add(fog_image(pop3_format::level::MAP_SIZE as u32, fog_bytes(None, pop3_format::level::MAP_SIZE))),
    }
}

/// One byte per cell of a `size` x `size` map, row by row: 255 where `explored` (None: no fog, everywhere), 0 elsewhere.
pub fn fog_bytes(explored: Option<&game_core::fog::Explored>, size: usize) -> Vec<u8> {
    (0..size * size).map(|i| if explored.is_none_or(|e| e.is_explored(((i % size) as i32, (i / size) as i32))) { 255 } else { 0 }).collect()
}

/// The fog texture of `bytes` (`fog_bytes`), `side` cells across.
pub fn fog_image(side: u32, bytes: Vec<u8>) -> Image {
    image((side, side), TextureFormat::R8Unorm, bytes, filterable_sampler())
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
    material.highlight = Vec4::ZERO;
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
    fn the_sea_is_drawn_raised_by_its_ripple_in_open_water_only() {
        let mut water = vec![0u8; DISP_SIZE * DISP_SIZE];
        let step = crate::terrain_texture::PX_PER_CELL * WATER_FREQUENCY as usize;
        water[(10 * step % DISP_SIZE) * DISP_SIZE + 10 * step % DISP_SIZE] = 200;
        let open = |_: i32, _: i32| false;
        assert_eq!(water_lift_at_corner(&water, 0, (10, 10), &open), 400.0 / 8.0, "both reads on the same texel at turn 0");
        assert_eq!(water_lift_at_corner(&water, 1, (10, 10), &open), 0.0, "scrolled off it");
        let coast = |x: i32, _: i32| x >= 12;
        assert_eq!(water_lift_at_corner(&water, 0, (10, 10), &coast), 0.0, "two cells from land: still flat");
        assert_eq!(water_lift(&water, 0, Vec2::new(10.5, 10.0), &open), 25.0, "half way to a flat corner");
        assert_eq!(water_lift(&[], 0, Vec2::ZERO, &open), 0.0, "no texture yet");
    }

    #[test]
    fn params_packs_size_grid_flag_and_px_per_cell() {
        assert_eq!(params(128, true), Vec4::new(128.0, 1.0, crate::terrain_texture::PX_PER_CELL as f32, 0.0));
        assert_eq!(params(128, false).y, 0.0);
    }

    #[test]
    fn fog_bytes_light_explored_cells_only_and_everything_without_fog() {
        assert_eq!(fog_bytes(None, 2), vec![255; 4]);
        let mut e = game_core::fog::Explored::new(4);
        e.explore((1, 2));
        let bytes = fog_bytes(Some(&e), 4);
        assert_eq!(bytes.len(), 16);
        assert_eq!(bytes.iter().filter(|&&b| b == 255).count(), 1);
        assert_eq!(bytes[2 * 4 + 1], 255);
    }

    fn test_material() -> GroundMaterial {
        GroundMaterial {
            params: params(128, true),
            channels: Handle::default(),
            altitude: Handle::default(),
            palette: Handle::default(),
            bigfade: Handle::default(),
            cliff: Handle::default(),
            disp: Handle::default(),
            water: Handle::default(),
            highlight: Vec4::ZERO,
            water_params: Vec4::ZERO,
            fog: Handle::default(),
        }
    }

    #[test]
    fn set_turn_wraps_every_256_and_leaves_the_rest_of_params_alone() {
        let mut material = test_material();
        set_turn(&mut material, 5);
        assert_eq!(material.params, Vec4::new(128.0, 1.0, crate::terrain_texture::PX_PER_CELL as f32, 5.0));
        set_turn(&mut material, 5 + 256);
        assert_eq!(material.params.w, 5.0, "wraps every 256 turns");
    }

    #[test]
    fn set_height_scale_sets_only_water_params_x() {
        let mut material = test_material();
        set_height_scale(&mut material, 0.5);
        assert_eq!(material.water_params, Vec4::new(0.5, 0.0, 0.0, 0.0));
    }

    #[test]
    fn highlight_uniform_packs_the_cell_and_an_on_off_flag() {
        assert_eq!(highlight_uniform(Some((12, 70))), Vec4::new(12.0, 70.0, 1.0, 0.0));
        assert_eq!(highlight_uniform(None), Vec4::ZERO);
    }

    #[test]
    fn altitude_bytes_roundtrip_through_half_precision() {
        let bytes = altitude_bytes(&[0.0, 500.0, 1022.0]);
        assert_eq!(bytes.len(), 6);
        let back: Vec<f32> = bytes.chunks_exact(2).map(|c| half::f16::from_le_bytes([c[0], c[1]]).to_f32()).collect();
        assert_eq!(back, vec![0.0, 500.0, 1022.0], "every altitude value is an integer well within f16's exact range");
    }
}
