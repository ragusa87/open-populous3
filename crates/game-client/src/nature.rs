//! Trees on the map as 3D models standing on the terrain, like the original's 3D trees (CC0
//! Quaternius Stylized Nature MegaKit, see assets/CREDITS.md): the tree's variant picks the model,
//! its size scales it, size 0 hides it. Each tree keeps its own turn so groves do not look copied.
//! Resting the cursor on a tree for `HOVER_SECS` shows how much wood it holds.

use crate::camera::GameCamera;
use crate::grounded::Grounded;
use crate::hud::PANEL_WIDTH;
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
/// Seconds the cursor rests on a tree before its wood shows.
pub const HOVER_SECS: f32 = 1.5;
/// A tree's hit box on screen: as wide as this fraction of its height, either side of the trunk.
const HIT_HALF_WIDTH: f32 = 0.3;

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

/// A tree as seen on screen: its index, trunk base and top (pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeOnScreen {
    pub index: usize,
    pub base: Vec2,
    pub top: Vec2,
}

/// The tree under the cursor: inside a box around its trunk (`HIT_HALF_WIDTH` of its height either
/// side); the one nearest the camera (lowest base on screen) when several overlap.
pub fn tree_at(cursor: Vec2, trees: &[TreeOnScreen]) -> Option<usize> {
    trees
        .iter()
        .filter(|t| {
            let height = t.base.distance(t.top);
            let (lo, hi) = (t.top.y.min(t.base.y), t.top.y.max(t.base.y));
            (lo..=hi).contains(&cursor.y) && (cursor.x - (t.base.x + t.top.x) / 2.0).abs() <= height * HIT_HALF_WIDTH
        })
        .max_by(|a, b| a.base.y.total_cmp(&b.base.y))
        .map(|t| t.index)
}

/// What the tooltip says about a tree of `size`.
pub fn wood_label(size: u8) -> String {
    format!("Tree: {size}/{MAX_SIZE} wood")
}

#[derive(Resource)]
struct TreeModels(Vec<Handle<WorldAsset>>);

#[derive(Component)]
struct TreeView(usize);

#[derive(Component)]
struct WoodTooltip;

/// The tree under the cursor and since when (seconds).
#[derive(Default)]
struct Hover {
    tree: Option<usize>,
    since: f32,
}
#[derive(Component)]
struct TreeModel(usize);

pub struct NaturePlugin;

impl Plugin for NaturePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (load_models, spawn_tooltip))
            .add_systems(Update, (respawn_trees, grow_trees).chain())
            .add_systems(Update, wood_tooltip.in_set(crate::menu::Gameplay));
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
            .spawn((TreeView(i), Grounded { at, half: TRUNK_HALF }, Transform::from_rotation(Quat::from_rotation_y(tree_yaw(tree.x, tree.z))), Visibility::Hidden))
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

fn spawn_tooltip(mut commands: Commands) {
    commands.spawn((
        WoodTooltip,
        Text::new(""),
        TextFont { font_size: FontSize::Px(14.0), ..default() },
        TextColor(Color::WHITE),
        TextShadow::default(),
        BackgroundColor(Color::srgba(0.1, 0.07, 0.03, 0.8)),
        Node { position_type: PositionType::Absolute, padding: UiRect::axes(px(6), px(3)), ..default() },
        GlobalZIndex(i32::MAX - 1),
        Pickable::IGNORE,
        Visibility::Hidden,
    ));
}

/// Shows the wood of the tree the cursor has rested on for `HOVER_SECS` (not over the panel, nor
/// while a spell is aimed).
#[allow(clippy::too_many_arguments)]
fn wood_tooltip(
    time: Res<Time>,
    map: Res<CurrentMap>,
    windows: Query<&Window>,
    ui: Query<&Interaction>,
    spell: Res<crate::hud::spells::SelectedSpell>,
    cams: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    views: Query<(&TreeView, &GlobalTransform, &Visibility)>,
    mut tooltip: Query<(&mut Text, &mut Node, &mut Visibility), (With<WoodTooltip>, Without<TreeView>)>,
    mut hover: Local<Hover>,
) {
    let Ok((mut text, mut node, mut vis)) = tooltip.single_mut() else { return };
    let cursor = windows.iter().next().and_then(Window::cursor_position);
    let over_ui = ui.iter().any(|i| *i != Interaction::None);
    let on_map = cursor.filter(|c| c.x > PANEL_WIDTH && !over_ui && spell.0.is_none());
    let tree = on_map.zip(cams.iter().next()).and_then(|(c, (cam, cam_t))| {
        let on_screen: Vec<TreeOnScreen> = views
            .iter()
            .filter(|(v, _, vis)| **vis != Visibility::Hidden && map.0.trees.get(v.0).is_some_and(|t| t.is_visible()))
            .filter_map(|(v, gt, _)| {
                let height = tree_scale(map.0.trees[v.0].size) * MODEL_HEIGHT;
                let base = cam.world_to_viewport(cam_t, gt.translation()).ok()?;
                let top = cam.world_to_viewport(cam_t, gt.translation() + Vec3::Y * height).ok()?;
                Some(TreeOnScreen { index: v.0, base, top })
            })
            .collect();
        tree_at(c, &on_screen)
    });
    let now = time.elapsed_secs();
    if tree != hover.tree {
        *hover = Hover { tree, since: now };
    }
    match (tree, on_map) {
        (Some(i), Some(c)) if now - hover.since >= HOVER_SECS => {
            text.0 = wood_label(map.0.trees[i].size);
            (node.left, node.top) = (px(c.x + 16.0), px(c.y + 18.0));
            vis.set_if_neq(Visibility::Inherited);
        }
        _ => {
            vis.set_if_neq(Visibility::Hidden);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_on_a_tree_picks_the_nearest_one() {
        let far = TreeOnScreen { index: 1, base: Vec2::new(100.0, 200.0), top: Vec2::new(100.0, 100.0) };
        let near = TreeOnScreen { index: 2, base: Vec2::new(110.0, 260.0), top: Vec2::new(110.0, 120.0) };
        assert_eq!(tree_at(Vec2::new(105.0, 150.0), &[far, near]), Some(2), "overlapping: the one in front");
        assert_eq!(tree_at(Vec2::new(100.0, 110.0), &[far, near]), Some(1), "only the far crown there");
        assert_eq!(tree_at(Vec2::new(140.0, 150.0), &[far]), None, "beside it");
        assert_eq!(tree_at(Vec2::new(100.0, 90.0), &[far]), None, "above it");
    }

    #[test]
    fn label_counts_the_wood() {
        assert_eq!(wood_label(3), "Tree: 3/4 wood");
    }

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
