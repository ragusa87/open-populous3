//! Blast's own impact burst (`game_core::blast::explosion_effect`, `EffectKind::Explosion`; pop3-rev-analysis.md
//! "Blast's own cast chain": effect 7/0x26, "explosion 1" — a 30-frame alpha sprite, hfx 1099-1128, 9-turn life,
//! sound 0xB2 and the same dynamic ground-light call the flying shot itself carries). A small flash growing then
//! fading over its traced 9-turn life — kept modest on purpose, about the size of a firewarrior's bolt impact
//! (`firebolts.rs`'s own ball is 0.09), not the blast's own multi-cell damage radius (`blast::RADII`), which would
//! read as a crater rather than an explosion. Our own invented look (no sprite traced closely enough to follow).
//! The burn cell this same impact lights (a tree catching fire, or `ground_fire.rs`'s own standing flame) and the
//! plain smoke puff are drawn by their own existing views, nothing extra needed here.

use crate::grounded::Grounded;
use crate::sim_time::{seed, SimNow, TICK_SECS};
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::blast::EXPLOSION_TICKS;
use game_core::effect::{Effect, EffectKind};
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::time::Tick;
use pop3_format::WORLD_UNITS_PER_CELL;

const FLASH_COLOUR: Color = Color::srgb(1.0, 0.75, 0.35);
const SMOKE_COLOUR: Color = Color::srgb(0.22, 0.2, 0.19);
const FLASH_GLOW: f32 = 6.0;
/// The flash's own size (cells): small and fixed, see the module doc.
const FLASH_SIZE: (f32, f32) = (0.12, 0.5);
/// Its traced life, in seconds (`blast::EXPLOSION_TICKS`, 9 turns).
const TOTAL_SECS: f32 = EXPLOSION_TICKS.get() as f32 * TICK_SECS;

/// The flash's size (cells), opacity and colour-blend progress `secs` after it started: grows fast then slows
/// (same shape as `fireball_shot.rs`'s own burst) while fading from a bright flash towards smoke.
pub fn flash(secs: f32) -> (f32, f32, f32) {
    let p = (secs / TOTAL_SECS).clamp(0.0, 1.0);
    let size = FLASH_SIZE.0 + (FLASH_SIZE.1 - FLASH_SIZE.0) * p.sqrt();
    (size, (1.0 - p).powi(2), p)
}

#[derive(Resource)]
pub struct BlastImpactAssets {
    sphere: Handle<Mesh>,
}

/// One explosion's view: its flash, fading from a bright colour towards smoke as it grows then shrinks away.
#[derive(Component)]
pub struct BlastImpactView {
    flash: Entity,
    material: Handle<StandardMaterial>,
}

impl WorldView for BlastImpactView {
    type Model = (Id<Effect>, Effect);
    type Params = (
        Res<'static, CurrentMap>,
        Res<'static, BlastImpactAssets>,
        ResMut<'static, Assets<StandardMaterial>>,
        Query<'static, 'static, (&'static mut Transform, &'static mut Visibility)>,
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| e.kind == EffectKind::Explosion)
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((id, e): &(Id<Effect>, Effect), view: &mut EntityCommands, (_, assets, mats, ..): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let cell = WORLD_UNITS_PER_CELL as f32;
        let turn = Quat::from_rotation_y(seed(id.raw()) * std::f32::consts::TAU);
        view.insert((Grounded { at: Vec2::new(e.x as f32, e.z as f32) / cell, half: 0.0 }, Transform::from_rotation(turn), Visibility::Visible));
        let material = mats.add(StandardMaterial { base_color: FLASH_COLOUR, emissive: LinearRgba::from(FLASH_COLOUR) * FLASH_GLOW, alpha_mode: AlphaMode::Blend, unlit: true, cull_mode: None, ..default() });
        let mut flash = Entity::PLACEHOLDER;
        view.with_children(|v| {
            flash = v.spawn((Mesh3d(assets.sphere.clone()), MeshMaterial3d(material.clone()), NotShadowCaster, crate::hover::NoOutline, Transform::default(), Visibility::Visible)).id();
        });
        Some(BlastImpactView { flash, material })
    }

    fn update(&self, (_, e): &(Id<Effect>, Effect), now: SimNow, (map, _, mats, parts): &mut SystemParamItem<Self::Params>) {
        let start = Tick::from_wire(map.0.now.to_wire().wrapping_sub(e.left.done().get()));
        let secs = now.secs_since(start);
        let (size, opacity, colour_p) = flash(secs);
        if let Ok((mut t, mut vis)) = parts.get_mut(self.flash) {
            (t.scale, *vis) = (Vec3::splat(size), Visibility::Inherited);
            if let Some(mut m) = mats.get_mut(&self.material) {
                m.base_color = FLASH_COLOUR.mix(&SMOKE_COLOUR, colour_p).with_alpha(opacity);
            }
        }
    }
}

pub struct BlastImpactPlugin;

impl Plugin for BlastImpactPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets).add_systems(Update, sync_views::<BlastImpactView>.after(crate::units::run_ticks));
    }
}

fn load_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(BlastImpactAssets { sphere: meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap()) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flash_grows_then_fades_over_its_traced_life() {
        let (start, start_op, _) = flash(0.0);
        assert_eq!((start, start_op), (FLASH_SIZE.0, 1.0));
        let (mid, mid_op, _) = flash(TOTAL_SECS * 0.5);
        assert!(mid > start && mid_op < 1.0 && mid_op > 0.0);
        assert!(FLASH_SIZE.1 < 1.0, "a modest local burst, not the blast's own multi-cell reach");
        let (end, end_op, end_p) = flash(TOTAL_SECS);
        assert_eq!((end, end_op, end_p), (FLASH_SIZE.1, 0.0, 1.0), "grown to its full (still small) size, faded away, fully smoke-coloured");
    }
}
