//! Buildings on the map: the original 3D objects in their tribe's colours (textured from the
//! level's theme atlas) when the original files are allowed and the building is identified;
//! otherwise a box in the tribe colour with the building's name over it (no open-source models
//! yet). Turned by their facing.

use crate::camera::GameCamera;
use crate::grounded::Grounded;
use crate::original_models::{atlas_image, object_mesh, to_mesh, OriginalObjects};
use crate::sites::tribe_color;
use crate::world::{CurrentMap, LevelList};
use bevy::prelude::*;
use game_core::building::{Building, BuildingKind};
use pop3_format::catalog::{self, villager_hut, Building as Object};
use pop3_format::{Atlas, Theme, WORLD_UNITS_PER_CELL};

/// Footprint half size (cells): the building rests on the lowest ground under it.
const FOOTPRINT_HALF: f32 = 0.8;
/// Stand-in box (cells) when there is no model.
const BOX: Vec3 = Vec3::new(1.6, 0.8, 1.6);
/// Theme whose atlas textures buildings on maps without one.
const DEFAULT_THEME: u8 = 0;

/// The original object drawing a building of `owner`, None if not identified (neutral buildings
/// take the blue version).
pub fn building_object(kind: BuildingKind, owner: u8) -> Option<usize> {
    let tribe = if owner < catalog::TRIBES { owner } else { 0 };
    Some(match kind {
        BuildingKind::Hut { size } => villager_hut(0, tribe, size),
        BuildingKind::DrumTower => Object::DrumTower.object(tribe),
        BuildingKind::Temple => Object::PrayerHut.object(tribe),
        BuildingKind::SpyTraining => Object::SpyHut.object(tribe),
        BuildingKind::WarriorTraining => Object::WarriorTraining.object(tribe),
        BuildingKind::FirewarriorTraining => Object::FirewarriorTraining.object(tribe),
        BuildingKind::BoatHut => Object::BoatHut.object(tribe),
        BuildingKind::AirshipHut => Object::AirshipHut.object(tribe),
        BuildingKind::Vault => catalog::KNOWLEDGE_PYRAMID,
        BuildingKind::Prison => catalog::PRISON,
        BuildingKind::Reconversion | BuildingKind::WallPiece | BuildingKind::Gate | BuildingKind::GuardPost | BuildingKind::Other(_) => return None,
    })
}

/// Turn for a facing in eighths of a turn.
pub fn facing_yaw(facing: u8) -> f32 {
    facing as f32 * std::f32::consts::FRAC_PI_4
}

#[derive(Component)]
struct BuildingView;

/// The name over a stand-in box, following its building on screen.
#[derive(Component)]
struct BuildingLabel(Entity);

pub struct BuildingsPlugin;

impl Plugin for BuildingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (respawn_buildings, place_labels).chain());
    }
}

/// The theme atlas as a material, None without original files.
fn original_material(levels: &LevelList, theme: u8, images: &mut Assets<Image>, mats: &mut Assets<StandardMaterial>) -> Option<Handle<StandardMaterial>> {
    if !levels.original {
        return None;
    }
    let load = || -> Result<_, pop3_format::LevelError> { Ok((Atlas::load(&levels.data_dir, theme)?, Theme::load(&levels.data_dir, theme)?)) };
    let (atlas, palette) = load().map_err(|e| warn!("building atlas for theme {theme}: {e}")).ok()?;
    Some(mats.add(StandardMaterial {
        base_color_texture: Some(images.add(atlas_image(&atlas, &palette.palette))),
        alpha_mode: AlphaMode::Mask(0.5),
        perceptual_roughness: 0.95,
        double_sided: true,
        cull_mode: None,
        ..default()
    }))
}

#[allow(clippy::too_many_arguments)]
fn respawn_buildings(
    mut commands: Commands,
    map: Res<CurrentMap>,
    levels: Res<LevelList>,
    objects: Res<OriginalObjects>,
    existing: Query<Entity, Or<(With<BuildingView>, With<BuildingLabel>)>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    if !map.is_changed() {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn();
    }
    if map.0.buildings.is_empty() {
        return;
    }
    let material = original_material(&levels, map.0.theme.unwrap_or(DEFAULT_THEME), &mut images, &mut mats);
    let bank = objects.0.as_ref().filter(|_| material.is_some());
    let box_mesh = meshes.add(Cuboid::from_size(BOX));
    let cell = WORLD_UNITS_PER_CELL as f32;
    for b in &map.0.buildings {
        let Building { kind, owner, x, z, facing } = *b;
        let at = Vec2::new(x as f32 / cell, z as f32 / cell);
        let mut view = commands.spawn((BuildingView, Grounded { at, half: FOOTPRINT_HALF }, Transform::from_rotation(Quat::from_rotation_y(facing_yaw(facing))), Visibility::Hidden));
        let original = bank.zip(material.as_ref()).and_then(|(bank, mat)| Some((bank.get(building_object(kind, owner)?)?, mat)));
        match original {
            Some((obj, mat)) => {
                let tribe = if owner < catalog::TRIBES { owner } else { 0 };
                view.with_child((Mesh3d(meshes.add(to_mesh(object_mesh(obj, tribe)))), MeshMaterial3d(mat.clone())));
            }
            None => {
                let tint = StandardMaterial { base_color: tribe_color(owner), perceptual_roughness: 0.9, ..default() };
                view.with_child((Mesh3d(box_mesh.clone()), MeshMaterial3d(mats.add(tint)), Transform::from_xyz(0.0, BOX.y / 2.0, 0.0)));
                let id = view.id();
                commands.spawn((
                    BuildingLabel(id),
                    Text::new(kind.name()),
                    TextFont { font_size: FontSize::Px(12.0), ..default() },
                    TextColor(Color::WHITE),
                    TextShadow::default(),
                    Node { position_type: PositionType::Absolute, ..default() },
                    Pickable::IGNORE,
                    Visibility::Hidden,
                ));
            }
        }
    }
}

/// Each stand-in's name centred over its box on screen, hidden when the box is.
fn place_labels(
    cams: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    views: Query<(&GlobalTransform, &InheritedVisibility), With<BuildingView>>,
    mut labels: Query<(&BuildingLabel, &mut Node, &ComputedNode, &mut Visibility)>,
) {
    let Some((cam, cam_t)) = cams.iter().next() else { return };
    for (label, mut node, size, mut vis) in &mut labels {
        let spot = views.get(label.0).ok().filter(|(_, shown)| shown.get()).and_then(|(gt, _)| cam.world_to_viewport(cam_t, gt.translation() + Vec3::Y * (BOX.y + 0.3)).ok());
        match spot {
            Some(p) => {
                let half = size.size() * size.inverse_scale_factor() / 2.0;
                (node.left, node.top) = (px(p.x - half.x), px(p.y - half.y));
                vis.set_if_neq(Visibility::Inherited);
            }
            None => {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identified_buildings_have_their_tribes_object() {
        assert_eq!(building_object(BuildingKind::DrumTower, 2), Some(119));
        assert_eq!(building_object(BuildingKind::Hut { size: 3 }, 1), Some(villager_hut(0, 1, 3)));
        assert_eq!(building_object(BuildingKind::Vault, 255), Some(catalog::KNOWLEDGE_PYRAMID), "neutral");
        assert_eq!(building_object(BuildingKind::BoatHut, 255), Some(121), "neutral: the blue one");
        assert_eq!(building_object(BuildingKind::GuardPost, 0), None, "not identified: stand-in box");
    }

    #[test]
    fn facing_in_quarter_turns() {
        assert_eq!(facing_yaw(0), 0.0);
        assert!((facing_yaw(2) - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }
}
