//! A spy's firebrand left burning on the building's wall (`game_core::arson`, `EffectKind::Firebrand`), a world view
//! of each: a small standing flame, the same look as a fire's own (`fire_flame`), at the middle of the wall beside
//! the spy, for the firebrand's life, settling away at its end. The building's own fire is `burning_buildings`.

use crate::fire_flame::{height, CompanionFireAssets, SETTLE_TICKS};
use crate::flame::{self, Flame, FlameFrames};
use crate::grounded::Grounded;
use crate::sim_time::{seed, ticks_f32, SimNow, TICK_SECS};
use crate::views::{sync_views, WorldView};
use crate::world::{CurrentMap, LevelList};
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::arson::FIREBRAND_TICKS;
use game_core::effect::{Effect, EffectKind};
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::time::Tick;
use pop3_format::WORLD_UNITS_PER_CELL;

/// Its size next to a fire's own flame: a small one. `[ours]`
const SCALE: f32 = 0.4;
const DEFAULT_THEME: u8 = 0;

#[derive(Component)]
pub struct FirebrandView {
    flame: Entity,
}

/// Its height (scale) `secs` into its life: full, then settling away over its last turns.
pub fn flame_scale(secs: f32) -> f32 {
    SCALE * height(secs, ticks_f32(FIREBRAND_TICKS) * TICK_SECS, ticks_f32(SETTLE_TICKS) * TICK_SECS)
}

impl WorldView for FirebrandView {
    type Model = (Id<Effect>, Effect);
    type Params = (
        Res<'static, CurrentMap>,
        Res<'static, LevelList>,
        ResMut<'static, FlameFrames>,
        Res<'static, CompanionFireAssets>,
        ResMut<'static, Assets<Image>>,
        ResMut<'static, Assets<StandardMaterial>>,
        Query<'static, 'static, &'static mut Transform, With<Flame>>,
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| matches!(e.kind, EffectKind::Firebrand { .. }))
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((id, e): &(Id<Effect>, Effect), view: &mut EntityCommands, (map, levels, frames, assets, images, mats, _): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let EffectKind::Firebrand { to } = e.kind else { return None };
        let offset = (seed(id.raw()) * flame::FRAMES as f32) as usize % flame::FRAMES;
        let frame = frames.get(levels, map.0.theme.unwrap_or(DEFAULT_THEME), images, mats).get(offset)?.clone();
        view.insert((Grounded { at: Vec2::new(to.0 as f32, to.1 as f32) / WORLD_UNITS_PER_CELL as f32, half: 0.0 }, Transform::default(), Visibility::Visible));
        let mut flame = Entity::PLACEHOLDER;
        view.with_children(|v| {
            flame = v.spawn((Flame { offset }, Mesh3d(assets.flame_board()), MeshMaterial3d(frame), NotShadowCaster, crate::hover::NoOutline, Transform::from_scale(Vec3::splat(SCALE)))).id();
        });
        Some(FirebrandView { flame })
    }

    fn update(&self, (_, e): &(Id<Effect>, Effect), now: SimNow, (map, .., flames): &mut SystemParamItem<Self::Params>) {
        let start = Tick::from_wire(map.0.now.to_wire().wrapping_sub(e.left.done().get()));
        if let Ok(mut t) = flames.get_mut(self.flame) {
            let s = flame_scale(now.secs_since(start));
            t.scale = Vec3::new(SCALE, s, SCALE);
        }
    }
}

pub struct FirebrandsPlugin;

impl Plugin for FirebrandsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_views::<FirebrandView>.after(crate::units::run_ticks));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_and_standing_until_it_settles_away_at_its_end() {
        let life = ticks_f32(FIREBRAND_TICKS) * TICK_SECS;
        assert_eq!(flame_scale(0.0), SCALE);
        assert_eq!(flame_scale(life * 0.5), SCALE);
        assert!(flame_scale(life - 0.1) < SCALE);
        assert_eq!(flame_scale(life + 1.0), 0.0);
    }
}
