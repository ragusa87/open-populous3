//! Fires on the ground (`game_core::ground_fire`), a world view (`views::WorldView`) of each: a few flames (`flame`'s
//! boards, as the camp fire's) standing where it burns, sinking away over its last `DYING` seconds; posed from
//! simulation time.

use crate::flame::{self, Flame, FlameFrames};
use crate::grounded::Grounded;
use crate::original_models::to_mesh;
use crate::sim_time::{seed, SimNow, TICK_SECS};
use crate::views::{sync_views, WorldView};
use crate::world::{CurrentMap, LevelList};
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::effect::{Effect, EffectKind};
use game_core::id::Id;
use game_core::map::GameMap;
use pop3_format::WORLD_UNITS_PER_CELL;

/// The flames' size (cells) and where they stand round the fire's point.
const SIZE: f32 = 0.45;
const BASES: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [0.18, 0.0, 0.1], [-0.12, 0.0, -0.15]];
/// Seconds over which the flames sink away at the end.
const DYING: f32 = 1.0;
const DEFAULT_THEME: u8 = 0;

/// The flames' height (of their full height) with `secs_left` to burn.
pub fn height(secs_left: f32) -> f32 {
    (secs_left / DYING).clamp(0.0, 1.0)
}

#[derive(Resource)]
pub struct FireBoards(Handle<Mesh>);

/// One fire's view: its flames.
#[derive(Component)]
pub struct GroundFireView {
    flames: Entity,
}

impl WorldView for GroundFireView {
    type Model = (Id<Effect>, Effect);
    type Params = (
        Res<'static, CurrentMap>,
        Res<'static, LevelList>,
        Res<'static, FireBoards>,
        ResMut<'static, FlameFrames>,
        ResMut<'static, Assets<Image>>,
        ResMut<'static, Assets<StandardMaterial>>,
        Query<'static, 'static, &'static mut Transform, With<Flame>>,
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| e.kind == EffectKind::GroundFire)
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((id, e): &(Id<Effect>, Effect), view: &mut EntityCommands, (map, levels, boards, frames, images, mats, _): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let offset = (seed(id.raw()) * flame::FRAMES as f32) as usize % flame::FRAMES;
        let frame = frames.get(levels, map.0.theme.unwrap_or(DEFAULT_THEME), images, mats).get(offset)?.clone();
        let cell = WORLD_UNITS_PER_CELL as f32;
        let turn = Quat::from_rotation_y(seed(id.raw().wrapping_add(1)) * std::f32::consts::TAU);
        view.insert((Grounded { at: Vec2::new(e.x as f32, e.z as f32) / cell, half: 0.0 }, Transform::from_rotation(turn), Visibility::Visible));
        let mut flames = Entity::PLACEHOLDER;
        view.with_children(|v| {
            flames = v.spawn((Flame { offset }, Mesh3d(boards.0.clone()), MeshMaterial3d(frame), NotShadowCaster, crate::hover::NoOutline, Transform::default())).id();
        });
        Some(GroundFireView { flames })
    }

    fn update(&self, (_, e): &(Id<Effect>, Effect), now: SimNow, (map, .., flames): &mut SystemParamItem<Self::Params>) {
        let since = now.secs_since(game_core::time::Tick::from_wire(map.0.now.to_wire().wrapping_sub(e.left.done().get())));
        let left = e.left.total().get() as f32 * TICK_SECS - since;
        if let Ok(mut t) = flames.get_mut(self.flames) {
            t.scale.y = height(left).max(0.01);
        }
    }
}

pub struct GroundFirePlugin;

impl Plugin for GroundFirePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_boards).add_systems(Update, sync_views::<GroundFireView>.after(crate::units::run_ticks));
    }
}

fn load_boards(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(FireBoards(meshes.add(to_mesh(flame::crossed_boards_at(&BASES, SIZE)))));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_until_its_last_second_then_sinking_away() {
        assert_eq!(height(3.0), 1.0);
        assert!(height(0.5) < 1.0 && height(0.5) > 0.0);
        assert_eq!(height(-0.1), 0.0);
    }
}
