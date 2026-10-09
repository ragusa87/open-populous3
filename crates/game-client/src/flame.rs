//! Flames drawn on boards (the camp fire's, the huts' torches): the original tile
//! holds alpha pixels (`tint << 4 | strength`, docs/specs/objects.md "Blended faces"), turned into
//! RGBA through the theme's alpha table, the faint board around the flame left out. Animated by
//! warping the picture: the tip sways side to side with a wave climbing up the flame, the flame
//! stretches and flickers. Frames are built once (pure functions) and cycled on the GPU side by
//! swapping materials (`FlameFrames`, shared by every flame, `Flame` on each). Without the original
//! files, a generated flame goes through the same steps.

use crate::original_models::MeshData;
use crate::units::SimClock;
use crate::world::LevelList;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use pop3_format::blend::{AlphaTable, STRENGTHS, TINTS};
use pop3_format::catalog::FLAME_TILE;
use pop3_format::objects::{ATLAS_WIDTH, TILE};
use pop3_format::{Atlas, Object, Theme, WORLD_UNITS_PER_CELL};
use std::f32::consts::{PI, TAU};

/// Frames of the loop and how fast it plays.
pub const FRAMES: usize = 8;
pub const FPS: f32 = 10.0;
/// Alpha pixels weaker than this are the board around the flame (tint 0, strength 1-3): left out.
pub const MIN_STRENGTH: u8 = 4;
/// How far (pixels) the tip sways, how much (fraction) the flame stretches and dims through the loop.
const SWAY: f32 = 2.5;
const STRETCH: f32 = 0.12;
const FLICKER: f32 = 0.18;

/// The original theme 0 alpha table's tints that flames use (0 red, 1 orange, 2 white-hot, 5
/// yellow), measured, for the generated flame; the others unused.
const FALLBACK_TINTS: [[u8; 3]; TINTS] = {
    let mut t = [[0; 3]; TINTS];
    t[0] = [255, 75, 22];
    t[1] = [223, 155, 31];
    t[2] = [229, 220, 214];
    t[5] = [215, 197, 43];
    t
};

/// RGBA of every alpha pixel with the fallback tints, like `AlphaTable::rgba`.
pub fn fallback_alpha_rgba() -> [[u8; 4]; 256] {
    std::array::from_fn(|pixel| {
        let [r, g, b] = FALLBACK_TINTS[pixel / STRENGTHS];
        [r, g, b, (pixel % STRENGTHS * 255 / (STRENGTHS - 1)) as u8]
    })
}

/// A `TILE` x `TILE` tile of alpha pixels as RGBA (row by row, the flame's base at the bottom),
/// without the board (`MIN_STRENGTH`).
pub fn flame_rgba(tile: &[u8], alpha: &[[u8; 4]; 256]) -> Vec<u8> {
    tile.iter().flat_map(|&p| if (p as usize % STRENGTHS) < MIN_STRENGTH as usize { [0; 4] } else { alpha[p as usize] }).collect()
}

/// The tile at `(tx, ty)` of an atlas `width` pixels wide.
pub fn tile_pixels(atlas: &[u8], width: usize, (tx, ty): (usize, usize)) -> Vec<u8> {
    (0..TILE).flat_map(|y| atlas[(ty + y) * width + tx..][..TILE].iter().copied()).collect()
}

/// Frame `frame` of the loop out of a flame picture (`TILE` square RGBA): each pixel taken from
/// the source pixel the warp brings there (nearest), nothing from outside.
pub fn warp(rgba: &[u8], frame: usize) -> Vec<u8> {
    let phase = TAU * (frame % FRAMES) as f32 / FRAMES as f32;
    let last = (TILE - 1) as f32;
    let stretch = 1.0 + STRETCH * (2.0 * phase).sin();
    let dim = 1.0 - FLICKER * (0.5 + 0.5 * (3.0 * phase).sin());
    let mut out = vec![0u8; rgba.len()];
    for y in 0..TILE {
        // Height above the base: 0 at the bottom row, 1 at the top.
        let up = (last - y as f32) / last;
        let sy = last - (last - y as f32) / stretch;
        let shift = SWAY * up * up * (phase + up * PI).sin();
        for x in 0..TILE {
            let sx = x as f32 - shift;
            let (sx, sy) = (sx.round(), sy.round());
            if sx < 0.0 || sx > last || sy < 0.0 || sy > last {
                continue;
            }
            let src = &rgba[(sy as usize * TILE + sx as usize) * 4..][..4];
            let dst = &mut out[(y * TILE + x) * 4..][..4];
            dst[..3].copy_from_slice(&src[..3]);
            dst[3] = (src[3] as f32 * dim).round() as u8;
        }
    }
    out
}

/// Every frame of the loop.
pub fn frames(rgba: &[u8]) -> Vec<Vec<u8>> {
    (0..FRAMES).map(|k| warp(rgba, k)).collect()
}

/// The frame shown `secs` into the loop by a flame started `offset` frames in.
pub fn frame_at(secs: f32, offset: usize) -> usize {
    ((secs * FPS) as usize + offset) % FRAMES
}

/// A generated flame tile in alpha pixels, like the original's: a tongue of fire rising from the
/// bottom row, white-hot at its heart, yellow, orange, then red at its edges.
pub fn generated_tile() -> Vec<u8> {
    let alpha = |tint: u8, strength: u8| tint << 4 | strength;
    let last = (TILE - 1) as f32;
    let mut tile = vec![0u8; TILE * TILE];
    for y in 0..TILE {
        let up = (last - y as f32) / last;
        // Widest a third of the way up, a point at the top.
        let half = 8.0 * (1.0 - up).powf(0.8) * (up * 3.0).clamp(0.45, 1.0);
        for x in 0..TILE {
            let r = (x as f32 - last / 2.0).abs() / half.max(0.01);
            tile[y * TILE + x] = match r {
                r if r > 1.0 => 0,
                r if r > 0.8 => alpha(0, 6),
                r if r > 0.55 => alpha(1, 10),
                r if r > 0.3 || up > 0.55 => alpha(5, 13),
                _ => alpha(2, 15),
            };
        }
    }
    tile
}

/// The blended faces (`Face::is_alpha`) of `obj` in cells, each sampling its own tile as a whole
/// picture (UVs 0-1 within the tile, not the atlas): the flame's boards.
pub fn flame_mesh(obj: &Object) -> MeshData {
    let scale = 1.0 / WORLD_UNITS_PER_CELL as f32;
    let texel = |c: i32| (c as f32 / 65536.0).clamp(0.5, TILE as f32 - 0.5) / TILE as f32;
    let mut m = MeshData::default();
    for face in obj.faces.iter().filter(|f| f.is_alpha()) {
        let corner = |k: usize| {
            let p = obj.points[face.points[k] as usize];
            ([p[0] as f32 * scale, p[1] as f32 * scale, p[2] as f32 * scale], [texel(face.uv[k].0), texel(face.uv[k].1)])
        };
        for tri in [[0, 1, 2], [0, 2, 3]].iter().take(face.points.len() - 2) {
            let [a, b, c] = tri.map(|k| Vec3::from(corner(k).0));
            let n = (b - a).cross(c - a).normalize_or(Vec3::Y);
            for &k in tri {
                let (p, uv) = corner(k);
                m.indices.push(m.positions.len() as u32);
                m.positions.push(p);
                m.normals.push(n.into());
                m.uvs.push(uv);
            }
        }
    }
    m
}

/// Two boards crossed at right angles, `size` cells wide and high, standing on the ground, each
/// showing the whole tile on both sides: the generated camp fire's flame.
pub fn crossed_boards(size: f32) -> MeshData {
    crossed_boards_at(&[[0.0; 3]], size)
}

/// `crossed_boards` standing on each of `bases` (cells, in the model's frame).
pub fn crossed_boards_at(bases: &[[f32; 3]], size: f32) -> MeshData {
    let h = size / 2.0;
    let mut m = MeshData::default();
    for ([x, y, z], (ax, az)) in bases.iter().flat_map(|&b| [(b, (1.0, 0.0)), (b, (0.0, 1.0))]) {
        let corners = [[x - h * ax, y + size, z - h * az], [x + h * ax, y + size, z + h * az], [x + h * ax, y, z + h * az], [x - h * ax, y, z - h * az]];
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        for (order, normal) in [([0, 1, 2, 0, 2, 3], [az, 0.0, -ax]), ([0, 2, 1, 0, 3, 2], [-az, 0.0, ax])] {
            for k in order {
                m.indices.push(m.positions.len() as u32);
                m.positions.push(corners[k]);
                m.normals.push(normal);
                m.uvs.push(uvs[k]);
            }
        }
    }
    m
}

/// The flame picture (RGBA) of `theme`: the original tile through its alpha table, None without
/// the original files.
fn original_picture(levels: &LevelList, theme: u8) -> Option<Vec<u8>> {
    if !levels.original {
        return None;
    }
    let load = || -> Result<_, pop3_format::LevelError> {
        Ok((Atlas::load(&levels.data_dir, theme)?, Theme::load(&levels.data_dir, theme)?, AlphaTable::load(&levels.data_dir, theme)?))
    };
    let (atlas, palette, alpha) = load().map_err(|e| warn!("flame for theme {theme}: {e}")).ok()?;
    let tile = tile_pixels(&atlas.pixels, ATLAS_WIDTH, Atlas::tile_origin(FLAME_TILE));
    Some(flame_rgba(&tile, &alpha.rgba(&palette.palette)))
}

/// The frames as one unlit, blended material each (the original draws alpha pixels as their tint
/// over the background by their strength).
fn frame_materials(picture: &[u8], images: &mut Assets<Image>, mats: &mut Assets<StandardMaterial>) -> Vec<Handle<StandardMaterial>> {
    frames(picture)
        .into_iter()
        .map(|rgba| {
            let mut image = Image::new(
                Extent3d { width: TILE as u32, height: TILE as u32, depth_or_array_layers: 1 },
                TextureDimension::D2,
                rgba,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
            image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { mag_filter: ImageFilterMode::Nearest, min_filter: ImageFilterMode::Nearest, ..default() });
            mats.add(StandardMaterial { base_color_texture: Some(images.add(image)), alpha_mode: AlphaMode::Blend, unlit: true, ..default() })
        })
        .collect()
}

/// The flame loop's materials, made for one theme (and whether original files are read) the first
/// time a flame is shown.
#[derive(Resource, Default)]
pub struct FlameFrames {
    made_for: Option<(u8, bool)>,
    frames: Vec<Handle<StandardMaterial>>,
}

impl FlameFrames {
    pub fn get(&mut self, levels: &LevelList, theme: u8, images: &mut Assets<Image>, mats: &mut Assets<StandardMaterial>) -> &[Handle<StandardMaterial>] {
        if self.made_for != Some((theme, levels.original)) {
            let picture = original_picture(levels, theme).unwrap_or_else(|| flame_rgba(&generated_tile(), &fallback_alpha_rgba()));
            self.frames = frame_materials(&picture, images, mats);
            self.made_for = Some((theme, levels.original));
        }
        &self.frames
    }
}

/// A flame mesh, `offset` frames into the loop so flames do not flicker together.
#[derive(Component)]
pub struct Flame {
    pub offset: usize,
}

pub struct FlamePlugin;

impl Plugin for FlamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FlameFrames>().add_systems(Update, animate);
    }
}

fn animate(clock: Res<SimClock>, art: Res<FlameFrames>, mut flames: Query<(&Flame, &mut MeshMaterial3d<StandardMaterial>)>) {
    if art.frames.is_empty() {
        return;
    }
    for (f, mut material) in &mut flames {
        let frame = &art.frames[frame_at(clock.anim_secs, f.offset)];
        if material.0 != *frame {
            material.0 = frame.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pop3_format::Face;

    fn opaque(rgba: &[u8]) -> usize {
        rgba.chunks(4).filter(|p| p[3] > 0).count()
    }

    #[test]
    fn the_board_is_left_out_the_flame_kept() {
        let alpha = fallback_alpha_rgba();
        let tile = [0u8, 1, 3, 4, 47, 93];
        let rgba = flame_rgba(&tile, &alpha);
        let a: Vec<u8> = rgba.chunks(4).map(|p| p[3]).collect();
        assert_eq!(a[..3], [0, 0, 0], "nothing and the faint board");
        assert_eq!((a[3], a[4]), (68, 255), "strength 4 of 15, full");
        assert_eq!(&rgba[16..19], &[229, 220, 214], "tint 2: white-hot");
        assert_eq!(&rgba[20..23], &[215, 197, 43], "tint 5: yellow");
    }

    #[test]
    fn the_loop_moves_the_tip_more_than_the_base() {
        let picture = flame_rgba(&generated_tile(), &fallback_alpha_rgba());
        let frames = frames(&picture);
        assert_eq!(frames.len(), FRAMES);
        let row = |f: &[u8], y: usize| f[y * TILE * 4..][..TILE * 4].to_vec();
        let changes = |y: usize| frames.iter().filter(|f| row(f, y).chunks(4).map(|p| p[3] > 0).ne(row(&frames[0], y).chunks(4).map(|p| p[3] > 0))).count();
        assert_eq!(changes(TILE - 1), 0, "the base stays put");
        assert!(changes(TILE / 3) > 0, "the upper flame sways or stretches");
        for f in &frames {
            let o = opaque(f) as f32;
            assert!((o / opaque(&picture) as f32 - 1.0).abs() < 0.3, "about the same flame");
        }
        assert_ne!(frames[0], frames[FRAMES / 2]);
    }

    #[test]
    fn frames_cycle_with_time_and_offset() {
        assert_eq!(frame_at(0.0, 0), 0);
        assert_eq!(frame_at(1.0 / FPS + 0.001, 0), 1);
        assert_eq!(frame_at(FRAMES as f32 / FPS + 0.001, 3), 3, "loops");
    }

    #[test]
    fn generated_flame_rises_from_the_base_to_a_point() {
        let tile = generated_tile();
        let width = |y: usize| tile[y * TILE..][..TILE].iter().filter(|&&p| p != 0).count();
        assert!(width(TILE - 1) > 0, "touches the base");
        assert!(width(TILE * 2 / 3) > width(3), "narrows to the tip");
        assert_eq!(width(0), 0, "room above the tip");
        assert!(tile.iter().all(|&p| p == 0 || p as usize % STRENGTHS >= MIN_STRENGTH as usize), "no board pixels");
    }

    #[test]
    fn flame_mesh_keeps_only_blended_faces_with_tile_uvs() {
        let face = |flags, uv: i32| Face { tile: Some(92), colour: 0, points: vec![0, 1, 2, 3], uv: vec![(0, 0), (uv << 16, 0), (uv << 16, 31 << 16), (0, 31 << 16)], flags };
        let obj = Object { points: vec![[0, 0, 0], [512, 0, 0], [512, 512, 0], [0, 512, 0]], faces: vec![face(0, 31), face(pop3_format::objects::FACE_ALPHA, 15)] };
        let m = flame_mesh(&obj);
        assert_eq!(m.indices.len(), 6, "the blended face only");
        assert_eq!(m.positions[2], [1.0, 1.0, 0.0]);
        assert_eq!(m.uvs[0], [0.5 / 32.0, 0.5 / 32.0]);
        assert_eq!(m.uvs[1], [15.0 / 32.0, 0.5 / 32.0], "half the tile");
    }

    #[test]
    fn crossed_boards_stand_on_the_ground_seen_from_both_sides() {
        let m = crossed_boards(0.3);
        assert_eq!(m.indices.len(), 2 * 2 * 6);
        assert!(m.positions.iter().all(|p| (0.0..=0.3).contains(&p[1]) && p[0].abs() <= 0.15 && p[2].abs() <= 0.15));
        assert!(m.uvs.iter().all(|uv| (0.0..=1.0).contains(&uv[0]) && (0.0..=1.0).contains(&uv[1])));
    }

    #[test]
    fn crossed_boards_at_stand_on_each_base() {
        let m = crossed_boards_at(&[[1.0, 0.5, 0.0], [-1.0, 0.5, 0.0]], 0.4);
        assert_eq!(m.indices.len(), 2 * crossed_boards(0.4).indices.len());
        let (left, right): (Vec<[f32; 3]>, Vec<[f32; 3]>) = m.positions.iter().partition(|p| p[0] < 0.0);
        assert_eq!(left.len(), right.len());
        assert!(m.positions.iter().all(|p| (0.5..=0.9).contains(&p[1]) && (p[0].abs() - 1.0).abs() <= 0.2 + 1e-5));
    }

    #[test]
    fn tile_pixels_cut_the_tile_out_of_the_atlas() {
        let width = 64;
        let atlas: Vec<u8> = (0..width * TILE).map(|i| (i % width) as u8).collect();
        let t = tile_pixels(&atlas, width, (32, 0));
        assert_eq!((t.len(), t[0], t[TILE - 1], t[TILE]), (TILE * TILE, 32, 63, 32));
    }
}
