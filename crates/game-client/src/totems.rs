//! Totems on the map (`GameMap::totems`): the original objects of their look, textured from the
//! level theme's atlas and seen from the front only like buildings, their flames animated; otherwise
//! (or with `--no-original`) a plain stone pillar. The stone totem is drawn layer by layer
//! (`rock_layers`, a stack of blocks without the original files): once it gave, its layers turn about
//! its centre axis (`layer_turn`). Redrawn when the map changes.

use crate::flame::{self, Flame, FlameFrames};
use crate::grounded::Grounded;
use crate::original_models::{atlas_image, object_mesh, solid_part, to_mesh, OriginalObjects};
use crate::world::{CurrentMap, LevelList};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::Face;
use game_core::totem::TotemKind;
use pop3_format::catalog::{PRAYER_TOTEM, STONE_HEAD, TOTEM, TOTEM_POLES, WINGED_DEATH_PERCHED, WINGED_DEATH_TOTEM};
use pop3_format::{Atlas, Object, Theme, WORLD_UNITS_PER_CELL};

/// Theme whose atlas textures totems on maps without one.
const DEFAULT_THEME: u8 = 0;
/// Footprint half size (cells): the totem rests on the lowest ground under it.
const HALF: f32 = 0.4;
/// The generated stand-in pillar (cells) and its colour.
const PILLAR: Vec3 = Vec3::new(0.45, 1.3, 0.45);
/// The generated stone totem: blocks stacked from the ground, each this high, narrowing upwards.
const BLOCKS: usize = 5;
const BLOCK_HIGH: f32 = 0.3;
const PILLAR_STONE: Color = Color::srgb(0.5, 0.48, 0.44);

/// The view of the totem at this index of `GameMap::totems`, and the height of its top (cells).
#[derive(Component)]
pub struct TotemView {
    pub index: usize,
    pub top: f32,
}

/// A rock layer of the stone totem, `0` the base (it never turns).
#[derive(Component)]
pub struct RockLayer(pub usize);

pub struct TotemsPlugin;

impl Plugin for TotemsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (respawn_totems, turn_rocks).chain());
    }
}

/// The stone totem's slabs, bottom first: its faces split by height, a face going to the slab whose
/// span holds its centre (a cap lying on a ring stays with the slab under it).
pub fn rock_layers(obj: &Object) -> Vec<Object> {
    let mut heights: Vec<i16> = obj.points.iter().map(|p| p[1]).collect();
    heights.sort_unstable();
    heights.dedup();
    let tops = &heights[1.min(heights.len())..];
    let mut layers: Vec<Object> = (0..tops.len().max(1)).map(|_| Object { points: obj.points.clone(), faces: Vec::new() }).collect();
    for face in &obj.faces {
        let centre = face.points.iter().map(|&i| obj.points[i as usize][1] as f32).sum::<f32>() / face.points.len() as f32;
        let layer = tops.iter().filter(|&&h| (h as f32) < centre).count().min(layers.len() - 1);
        layers[layer].faces.push(face.clone());
    }
    layers
}

/// How far layer `layer` has turned (radians) `secs` after the totem first gave: the base never,
/// the others each their own way, every other one backwards, the higher the faster.
pub fn layer_turn(layer: usize, secs: f32) -> f32 {
    if layer == 0 {
        return 0.0;
    }
    let way = if layer % 2 == 1 { 1.0 } else { -1.0 };
    way * (0.4 + 0.15 * layer as f32) * secs
}

/// The stone totems that gave turn their rock layers.
fn turn_rocks(time: Res<Time>, map: Res<CurrentMap>, views: Query<(&TotemView, &Children)>, mut layers: Query<(&RockLayer, &mut Transform)>, mut started: Local<std::collections::HashMap<usize, f32>>) {
    let now = time.elapsed_secs();
    for (view, children) in &views {
        if !map.0.totems.get(view.index).is_some_and(|t| t.given > 0) {
            started.remove(&view.index);
            continue;
        }
        let since = now - *started.entry(view.index).or_insert(now);
        for child in children.iter() {
            if let Ok((layer, mut t)) = layers.get_mut(child) {
                t.rotation = Quat::from_rotation_y(layer_turn(layer.0, since));
            }
        }
    }
}

/// The original objects drawing a totem of `kind`, at the same origin.
pub fn totem_objects(kind: TotemKind) -> Vec<usize> {
    match kind {
        TotemKind::Totem => vec![TOTEM],
        TotemKind::WingedDeath => vec![WINGED_DEATH_TOTEM, WINGED_DEATH_PERCHED],
        TotemKind::Prayer => vec![PRAYER_TOTEM],
        TotemKind::StoneHead => vec![STONE_HEAD],
        TotemKind::Pole(k) => vec![TOTEM_POLES[k as usize % TOTEM_POLES.len()]],
    }
}

/// The theme atlas as a front-faced material, None without original files.
fn original_material(levels: &LevelList, theme: u8, images: &mut Assets<Image>, mats: &mut Assets<StandardMaterial>) -> Option<Handle<StandardMaterial>> {
    if !levels.original {
        return None;
    }
    let load = || -> Result<_, pop3_format::LevelError> { Ok((Atlas::load(&levels.data_dir, theme)?, Theme::load(&levels.data_dir, theme)?)) };
    let (atlas, palette) = load().map_err(|e| warn!("totem atlas for theme {theme}: {e}")).ok()?;
    Some(mats.add(StandardMaterial {
        base_color_texture: Some(images.add(atlas_image(&atlas, &palette.palette))),
        alpha_mode: AlphaMode::Mask(0.5),
        perceptual_roughness: 0.95,
        cull_mode: Some(Face::Back),
        ..default()
    }))
}

#[allow(clippy::too_many_arguments)]
fn respawn_totems(
    mut commands: Commands,
    map: Res<CurrentMap>,
    levels: Res<LevelList>,
    objects: Res<OriginalObjects>,
    existing: Query<Entity, With<TotemView>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut flame_frames: ResMut<FlameFrames>,
) {
    if !map.is_changed() {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn();
    }
    if map.0.totems.is_empty() {
        return;
    }
    let theme = map.0.theme.unwrap_or(DEFAULT_THEME);
    let material = original_material(&levels, theme, &mut images, &mut mats);
    let bank = objects.0.as_ref().filter(|_| material.is_some());
    let pillar = (meshes.add(Cuboid::from_size(PILLAR)), mats.add(StandardMaterial { base_color: PILLAR_STONE, perceptual_roughness: 0.95, ..default() }));
    let cell = WORLD_UNITS_PER_CELL as f32;
    for (index, t) in map.0.totems.iter().enumerate() {
        let at = Vec2::new(t.x as f32, t.z as f32) / cell;
        let parts: Vec<_> = bank.map(|bank| totem_objects(t.kind).into_iter().filter_map(|i| bank.get(i)).collect()).unwrap_or_default();
        let top = parts.iter().flat_map(|o| o.points.iter().map(|p| p[1])).max().map_or(PILLAR.y, |y| y as f32 / cell);
        let mut view = commands.spawn((TotemView { index, top }, crate::hover::Hoverable::default(), Grounded { at, half: HALF }, Transform::default(), Visibility::Hidden));
        let stone = t.kind == TotemKind::Totem;
        let (Some(mat), false) = (material.clone(), parts.is_empty()) else {
            if stone {
                for k in 0..BLOCKS {
                    let side = PILLAR.x * 1.6 * (1.0 - 0.15 * k as f32);
                    let block = meshes.add(Cuboid::new(side, BLOCK_HIGH, side));
                    view.with_child((RockLayer(k), Mesh3d(block), MeshMaterial3d(pillar.1.clone()), Transform::from_xyz(0.0, BLOCK_HIGH * (k as f32 + 0.5), 0.0)));
                }
            } else {
                view.with_child((Mesh3d(pillar.0.clone()), MeshMaterial3d(pillar.1.clone()), Transform::from_xyz(0.0, PILLAR.y / 2.0, 0.0)));
            }
            continue;
        };
        let offset = (t.x as usize + t.z as usize) * 3 % flame::FRAMES;
        for obj in parts {
            if stone {
                for (k, layer) in rock_layers(&solid_part(obj)).iter().enumerate() {
                    view.with_child((RockLayer(k), Mesh3d(meshes.add(to_mesh(object_mesh(layer, 0)))), MeshMaterial3d(mat.clone()), Transform::default()));
                }
            } else {
                view.with_child((Mesh3d(meshes.add(to_mesh(object_mesh(&solid_part(obj), 0)))), MeshMaterial3d(mat.clone())));
            }
            let flames = flame::flame_mesh(obj);
            if !flames.indices.is_empty() {
                let frame = flame_frames.get(&levels, theme, &mut images, &mut mats)[offset].clone();
                view.with_child((Flame { offset }, Mesh3d(meshes.add(to_mesh(flames))), MeshMaterial3d(frame), NotShadowCaster, crate::hover::NoOutline));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_totem_look_has_its_objects() {
        for kind in TotemKind::ALL {
            assert!(!totem_objects(kind).is_empty(), "{kind:?}");
        }
        assert_eq!(totem_objects(TotemKind::WingedDeath), vec![WINGED_DEATH_TOTEM, WINGED_DEATH_PERCHED], "with its bird");
        assert_eq!(totem_objects(TotemKind::Pole(2)), vec![TOTEM_POLES[2]]);
    }

    #[test]
    fn the_stone_totem_splits_into_its_slabs() {
        use pop3_format::Face;
        let quad = |a: u16, b: u16, c: u16, d: u16| Face { tile: Some(1), colour: 0, points: vec![a, b, c, d], uv: vec![(0, 0); 4], flags: 6 };
        let ring = |y: i16| [[-100, y, -100], [100, y, -100], [100, y, 100], [-100, y, 100]];
        let points: Vec<[i16; 3]> = [ring(0), ring(160), ring(321)].concat();
        let faces = vec![quad(0, 1, 5, 4), quad(4, 5, 6, 7), quad(4, 5, 9, 8), quad(8, 9, 10, 11)];
        let layers = rock_layers(&Object { points, faces });
        let counts: Vec<usize> = layers.iter().map(|l| l.faces.len()).collect();
        assert_eq!(counts, vec![2, 2], "a side and the cap on its top ring each");
    }

    #[test]
    fn the_base_stays_the_layers_turn_every_other_one_backwards() {
        assert_eq!(layer_turn(0, 10.0), 0.0);
        assert!(layer_turn(1, 1.0) > 0.0 && layer_turn(2, 1.0) < 0.0);
        assert!(layer_turn(3, 1.0).abs() > layer_turn(1, 1.0).abs(), "higher, faster");
        assert_eq!(layer_turn(2, 0.0), 0.0, "from where it stood");
    }
}
