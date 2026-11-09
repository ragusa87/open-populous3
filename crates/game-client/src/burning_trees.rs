//! Burning trees (`game_core::tree::Tree::burning`, set by lightning), a world view (`views::WorldView`) of
//! each: one companion fire (`fire_flame`) standing at the tree's own base - independent of it, not scaled by
//! its size, not parented to its trunk (pop3-rev-analysis.md "Ground fire, ignited trees, and a burning
//! building's flame"). Trees have no id: a tree is known by its cell (one per cell).

use crate::effects::Puff;
use crate::fire_flame::{CompanionFire, CompanionFireAssets};
use crate::flame::{self, Flame, FlameFrames};
use crate::grounded::Grounded;
use crate::sim_time::{seed, ticks_f32, SimNow, TICK_SECS};
use crate::views::{sync_views, WorldView};
use crate::world::{CurrentMap, LevelList};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use game_core::map::GameMap;
use game_core::time::Ticks;
use game_core::tree::Tree;
use pop3_format::WORLD_UNITS_PER_CELL;

/// How long an ignited tree's own companion fire actually shows: a FIXED length regardless of which spell lit
/// it (pop3-rev-analysis.md, confirmed) - decoupled from `Tree::BURN_TICKS` (still unconfirmed: how long the
/// tree itself takes to burn down to size 0; a burning tree shows no flame at all for whatever is left of that
/// time once this one runs out).
pub const VISUAL_TICKS: Ticks = Ticks::new(76);
const DEFAULT_THEME: u8 = 0;

/// The id of the tree in `cell` (one tree per cell).
pub fn tree_id((x, z): (i32, i32)) -> u32 {
    (z as u32) << 16 | x as u32
}

/// One burning tree's view: its companion fire.
#[derive(Component)]
pub struct BurningTreeView(CompanionFire);

impl WorldView for BurningTreeView {
    type Model = Tree;
    type Params = (
        Res<'static, CurrentMap>,
        Res<'static, LevelList>,
        ResMut<'static, FlameFrames>,
        Res<'static, CompanionFireAssets>,
        ResMut<'static, Assets<Image>>,
        ResMut<'static, Assets<StandardMaterial>>,
        Query<'static, 'static, &'static mut Transform, With<Flame>>,
        Query<'static, 'static, &'static mut Puff>,
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &Tree> {
        map.trees.iter().filter(|t| t.burning.is_some())
    }

    fn id(t: &Tree) -> u32 {
        tree_id(t.cell())
    }

    fn spawn(t: &Tree, view: &mut EntityCommands, (map, levels, frames, assets, images, mats, ..): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let id = tree_id(t.cell());
        let offset = (seed(id) * flame::FRAMES as f32) as usize % flame::FRAMES;
        let frame = frames.get(levels, map.0.theme.unwrap_or(DEFAULT_THEME), images, mats).get(offset)?.clone();
        let cell = WORLD_UNITS_PER_CELL as f32;
        let turn = Quat::from_rotation_y(seed(id.wrapping_add(1)) * std::f32::consts::TAU);
        view.insert((Grounded { at: Vec2::new(t.x as f32, t.z as f32) / cell, half: 0.0 }, Transform::from_rotation(turn), Visibility::Visible));
        Some(BurningTreeView(CompanionFire::spawn(view, assets, frame, offset)))
    }

    fn update(&self, t: &Tree, now: SimNow, (map, .., flames, puffs): &mut SystemParamItem<Self::Params>) {
        let Some(lit) = t.lit_at(map.0.now) else { return };
        self.0.update(now.secs_since(lit), ticks_f32(VISUAL_TICKS) * TICK_SECS, flames, puffs);
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
    fn one_id_per_cell() {
        assert_ne!(tree_id((1, 2)), tree_id((2, 1)));
    }
}
