//! Totems on the map (`GameMap::totems`): the original objects of their look, textured from the
//! level theme's atlas and seen from the front only like buildings, their flames animated; otherwise
//! (or with `--no-original`) a plain stone pillar. Redrawn when the map changes.

use crate::flame::{self, Flame, FlameFrames};
use crate::grounded::Grounded;
use crate::original_models::{atlas_image, object_mesh, solid_part, to_mesh, OriginalObjects};
use crate::world::{CurrentMap, LevelList};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::Face;
use game_core::totem::TotemKind;
use pop3_format::catalog::{PRAYER_TOTEM, STONE_HEAD, TOTEM, TOTEM_POLES, WINGED_DEATH_PERCHED, WINGED_DEATH_TOTEM};
use pop3_format::{Atlas, Theme, WORLD_UNITS_PER_CELL};

/// Theme whose atlas textures totems on maps without one.
const DEFAULT_THEME: u8 = 0;
/// Footprint half size (cells): the totem rests on the lowest ground under it.
const HALF: f32 = 0.4;
/// The generated stand-in pillar (cells) and its colour.
const PILLAR: Vec3 = Vec3::new(0.45, 1.3, 0.45);
const PILLAR_STONE: Color = Color::srgb(0.5, 0.48, 0.44);

#[derive(Component)]
struct TotemView;

pub struct TotemsPlugin;

impl Plugin for TotemsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, respawn_totems);
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
    for t in &map.0.totems {
        let at = Vec2::new(t.x as f32, t.z as f32) / cell;
        let parts: Vec<_> = bank.map(|bank| totem_objects(t.kind).into_iter().filter_map(|i| bank.get(i)).collect()).unwrap_or_default();
        let mut view = commands.spawn((TotemView, Grounded { at, half: HALF }, Transform::default(), Visibility::Hidden));
        let (Some(mat), false) = (material.clone(), parts.is_empty()) else {
            view.with_child((Mesh3d(pillar.0.clone()), MeshMaterial3d(pillar.1.clone()), Transform::from_xyz(0.0, PILLAR.y / 2.0, 0.0)));
            continue;
        };
        let offset = (t.x as usize + t.z as usize) * 3 % flame::FRAMES;
        for obj in parts {
            view.with_child((Mesh3d(meshes.add(to_mesh(object_mesh(&solid_part(obj), 0)))), MeshMaterial3d(mat.clone())));
            let flames = flame::flame_mesh(obj);
            if !flames.indices.is_empty() {
                let frame = flame_frames.get(&levels, theme, &mut images, &mut mats)[offset].clone();
                view.with_child((Flame { offset }, Mesh3d(meshes.add(to_mesh(flames))), MeshMaterial3d(frame), NotShadowCaster));
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
}
