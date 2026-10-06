//! Trees on the map as 3D models standing on the terrain, like the original's 3D trees (CC0
//! Quaternius Stylized Nature MegaKit, see assets/CREDITS.md): the tree's variant picks the model,
//! its size scales it, size 0 hides it. Each tree keeps its own turn so groves do not look copied.

use crate::grounded::Grounded;
use crate::world::CurrentMap;
use bevy::gltf::GltfAssetLabel;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use game_core::tree::{MAX_SIZE, VARIANTS};
use pop3_format::WORLD_UNITS_PER_CELL;

/// Models by variant (`assets/models/nature/<name>.gltf`).
const MODELS: [&str; VARIANTS as usize] =
    ["CommonTree_1", "CommonTree_2", "CommonTree_3", "CommonTree_4", "CommonTree_5", "Pine_1", "Pine_2", "Pine_3", "Pine_4", "Pine_5"];
/// The models are about this many units tall; a full-size tree stands `FULL_HEIGHT` cells.
const MODEL_HEIGHT: f32 = 7.0;
const FULL_HEIGHT: f32 = 1.6;
/// Footprint half size (cells): the trunk rests on the lowest ground under it.
const TRUNK_HALF: f32 = 0.15;

/// Model scale for a tree's size: 0 for none, then from 40% (size 1) to full (size 4).
pub fn tree_scale(size: u8) -> f32 {
    match size.min(MAX_SIZE) {
        0 => 0.0,
        s => (0.4 + 0.6 * (s - 1) as f32 / (MAX_SIZE - 1) as f32) * FULL_HEIGHT / MODEL_HEIGHT,
    }
}

/// A fixed turn per tree position (radians), scattered but the same every run.
pub fn tree_yaw(x: u16, z: u16) -> f32 {
    let h = (x as u32).wrapping_mul(2_654_435_761) ^ (z as u32).wrapping_mul(40_503);
    (h % 360) as f32 * std::f32::consts::PI / 180.0
}

#[derive(Resource)]
struct TreeModels(Vec<Handle<WorldAsset>>);

#[derive(Component)]
struct TreeView;
#[derive(Component)]
struct TreeModel(usize);

pub struct NaturePlugin;

impl Plugin for NaturePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_models).add_systems(Update, (respawn_trees, grow_trees).chain());
    }
}

fn load_models(mut commands: Commands, assets: Res<AssetServer>) {
    let scenes = MODELS.iter().map(|m| assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/nature/{m}.gltf")))).collect();
    commands.insert_resource(TreeModels(scenes));
}

/// One view per tree of a newly loaded map.
fn respawn_trees(mut commands: Commands, map: Res<CurrentMap>, models: Res<TreeModels>, existing: Query<Entity, With<TreeView>>) {
    if !map.is_changed() {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn();
    }
    let cell = WORLD_UNITS_PER_CELL as f32;
    for (i, tree) in map.0.trees.iter().enumerate() {
        let at = Vec2::new(tree.x as f32 / cell, tree.z as f32 / cell);
        commands
            .spawn((TreeView, Grounded { at, half: TRUNK_HALF }, Transform::from_rotation(Quat::from_rotation_y(tree_yaw(tree.x, tree.z))), Visibility::Hidden))
            .with_child((TreeModel(i), WorldAssetRoot(models.0[tree.variant as usize % MODELS.len()].clone()), Transform::from_scale(Vec3::splat(tree_scale(tree.size)))));
    }
}

/// Size changes (growth, cutting): scale, and hidden at size 0.
fn grow_trees(map: Res<CurrentMap>, mut models: Query<(&TreeModel, &mut Transform, &mut Visibility)>) {
    for (model, mut t, mut vis) in &mut models {
        let Some(tree) = map.0.trees.get(model.0) else { continue };
        let scale = tree_scale(tree.size);
        if t.scale.x != scale {
            t.scale = Vec3::splat(scale);
        }
        vis.set_if_neq(if tree.is_visible() { Visibility::Inherited } else { Visibility::Hidden });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bigger_with_size_hidden_at_zero() {
        assert_eq!(tree_scale(0), 0.0);
        assert!(tree_scale(1) < tree_scale(2) && tree_scale(3) < tree_scale(4));
        assert!((tree_scale(4) * MODEL_HEIGHT - FULL_HEIGHT).abs() < 1e-5, "full size is {FULL_HEIGHT} cells");
        assert_eq!(tree_scale(9), tree_scale(4));
    }

    #[test]
    fn each_tree_keeps_its_own_turn() {
        assert_eq!(tree_yaw(1000, 2000), tree_yaw(1000, 2000));
        assert_ne!(tree_yaw(1000, 2000), tree_yaw(1512, 2000));
        assert!((0.0..std::f32::consts::TAU).contains(&tree_yaw(5, 7)));
    }

    #[test]
    fn every_variant_has_a_bundled_model() {
        for m in MODELS {
            let path = format!("{}/../../assets/models/nature/{m}.gltf", env!("CARGO_MANIFEST_DIR"));
            assert!(std::path::Path::new(&path).exists(), "{path}");
        }
    }
}
