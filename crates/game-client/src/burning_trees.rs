//! Burning trees (`game_core::tree::Tree::burning`, set by lightning), a world view (`views::WorldView`) of each: flames
//! (`flame`'s boards) up its trunk and crown, as high as its size, going down as it burns (`buildings::fire_height`)
//! until it is burnt to nothing. Trees have no id: a tree is known by its cell (one per cell).

use crate::buildings::fire_height;
use crate::flame::{self, Flame, FlameFrames};
use crate::grounded::Grounded;
use crate::nature::{size_factor, FULL_HEIGHT};
use crate::original_models::to_mesh;
use crate::sim_time::{seed, ticks_f32, SimNow, TICK_SECS};
use crate::views::{sync_views, WorldView};
use crate::world::{CurrentMap, LevelList};
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::map::GameMap;
use game_core::tree::{Tree, BURN_TICKS};
use pop3_format::WORLD_UNITS_PER_CELL;

/// Where the flames stand on a full-size tree (cells: sideways, up as a share of its height) and their size.
const BASES: [[f32; 3]; 5] = [[0.0, 0.0, 0.0], [0.3, 0.3, 0.1], [-0.3, 0.45, -0.15], [0.1, 0.6, 0.3], [-0.1, 0.7, -0.3]];
const SIZE: f32 = 0.9;
const DEFAULT_THEME: u8 = 0;

/// The id of the tree in `cell` (one tree per cell).
pub fn tree_id((x, z): (i32, i32)) -> u32 {
    (z as u32) << 16 | x as u32
}

/// The flames' bases on a tree of `size`, its height `FULL_HEIGHT` at full size.
pub fn bases(size: u8) -> Vec<[f32; 3]> {
    let h = size_factor(size) * FULL_HEIGHT;
    BASES.iter().map(|&[x, y, z]| [x, y * h, z]).collect()
}

/// One burning tree's view: its flames.
#[derive(Component)]
pub struct BurningTreeView {
    flames: Entity,
}

impl WorldView for BurningTreeView {
    type Model = Tree;
    type Params = (
        Res<'static, CurrentMap>,
        Res<'static, LevelList>,
        ResMut<'static, FlameFrames>,
        ResMut<'static, Assets<Mesh>>,
        ResMut<'static, Assets<Image>>,
        ResMut<'static, Assets<StandardMaterial>>,
        Query<'static, 'static, &'static mut Transform, With<Flame>>,
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &Tree> {
        map.trees.iter().filter(|t| t.burning.is_some())
    }

    fn id(t: &Tree) -> u32 {
        tree_id(t.cell())
    }

    fn spawn(t: &Tree, view: &mut EntityCommands, (map, levels, frames, meshes, images, mats, _): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let offset = (seed(tree_id(t.cell())) * flame::FRAMES as f32) as usize % flame::FRAMES;
        let frame = frames.get(levels, map.0.theme.unwrap_or(DEFAULT_THEME), images, mats).get(offset)?.clone();
        let cell = WORLD_UNITS_PER_CELL as f32;
        view.insert((Grounded { at: Vec2::new(t.x as f32, t.z as f32) / cell, half: 0.0 }, crate::object_scale::Scaled(crate::object_scale::ObjectGroup::Trees), Transform::default(), Visibility::Visible));
        let boards = meshes.add(to_mesh(flame::crossed_boards_at(&bases(t.size), SIZE)));
        let mut flames = Entity::PLACEHOLDER;
        view.with_children(|v| {
            flames = v.spawn((Flame { offset }, Mesh3d(boards), MeshMaterial3d(frame), NotShadowCaster, crate::hover::NoOutline, Transform::default())).id();
        });
        Some(BurningTreeView { flames })
    }

    fn update(&self, t: &Tree, now: SimNow, (map, .., flames): &mut SystemParamItem<Self::Params>) {
        let Some(lit) = t.lit_at(map.0.now) else { return };
        if let Ok(mut tf) = flames.get_mut(self.flames) {
            tf.scale.y = fire_height(now.secs_since(lit) / (ticks_f32(BURN_TICKS) * TICK_SECS));
        }
    }
}

pub struct BurningTreesPlugin;

impl Plugin for BurningTreesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_views::<BurningTreeView>.after(crate::units::run_ticks));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_id_per_cell_and_flames_as_high_as_the_tree() {
        assert_ne!(tree_id((1, 2)), tree_id((2, 1)));
        let top = |size| bases(size).iter().map(|b| b[1]).fold(0.0, f32::max);
        assert!(top(1) < top(4) && top(4) < FULL_HEIGHT);
    }
}
