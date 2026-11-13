//! A burning building's own companion fire (`game_core::building::Building::burning`, set by lightning), a
//! world view (`views::WorldView`) of each: one companion fire (`fire_flame`) standing at the building's centre
//! (user-observed; the traced spawn point is door-adjacent, pop3-rev-analysis.md) - independent of its model,
//! not parented to it (`buildings.rs` draws the building and its own permanent torches separately) - pop3-rev-analysis.md "Ground fire, ignited trees, and a burning building's
//! flame". Its own visual life is assumed to share the building's structural fire (`fire::BURN_TICKS`, not
//! separately re-armed by any traced call).

use crate::effects::Puff;
use crate::fire_flame::{CompanionFire, CompanionFireAssets};
use crate::flame::{self, Flame, FlameFrames};
use crate::grounded::Grounded;
use crate::sim_time::{seed, ticks_f32, SimNow, TICK_SECS};
use crate::views::{sync_views, WorldView};
use crate::world::{CurrentMap, LevelList};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use game_core::building::Building;
use game_core::fire::BURN_TICKS;
use game_core::map::GameMap;
use pop3_format::WORLD_UNITS_PER_CELL;

const DEFAULT_THEME: u8 = 0;

/// One burning building's view: its companion fire.
#[derive(Component)]
pub struct BurningBuildingView(CompanionFire);

impl WorldView for BurningBuildingView {
    type Model = Building;
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

    fn models(map: &GameMap) -> impl Iterator<Item = &Building> {
        map.buildings.iter().filter(|b| b.burning.is_some())
    }

    fn id(b: &Building) -> u32 {
        b.id.raw()
    }

    fn spawn(b: &Building, view: &mut EntityCommands, (map, levels, frames, assets, images, mats, ..): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let id = b.id.raw();
        let offset = (seed(id) * flame::FRAMES as f32) as usize % flame::FRAMES;
        let frame = frames.get(levels, map.0.theme.unwrap_or(DEFAULT_THEME), images, mats).get(offset)?.clone();
        let (x, z) = b.centre();
        let cell = WORLD_UNITS_PER_CELL as f32;
        let turn = Quat::from_rotation_y(seed(id.wrapping_add(1)) * std::f32::consts::TAU);
        view.insert((Grounded { at: Vec2::new(x as f32, z as f32) / cell, half: 0.0 }, Transform::from_rotation(turn), Visibility::Visible));
        Some(BurningBuildingView(CompanionFire::spawn(view, assets, frame, offset, levels.original, ticks_f32(BURN_TICKS) * TICK_SECS)))
    }

    fn update(&self, b: &Building, now: SimNow, (map, .., flames, puffs): &mut SystemParamItem<Self::Params>) {
        let Some(lit) = b.lit_at(map.0.now) else { return };
        self.0.update(now.secs_since(lit), ticks_f32(BURN_TICKS) * TICK_SECS, flames, puffs);
    }
}

pub struct BurningBuildingsPlugin;

impl Plugin for BurningBuildingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_views::<BurningBuildingView>.after(crate::buildings::RespawnBuildings));
    }
}
