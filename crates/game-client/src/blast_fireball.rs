//! The simple blast's own flash (`game_core::blast`): a bright disc at its centre, growing to match each of its
//! 3 damage turns' radius (`blast::RADII`) before fading out — otherwise the blast's push/damage would have
//! nothing to look like an explosion. `blast_impact.rs` picks up once this fades, with the settling smoke.

use crate::grounded::Grounded;
use crate::sim_time::{SimNow, TICK_SECS};
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::blast::RADII;
use game_core::effect::{Effect, EffectKind};
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::time::Tick;
use pop3_format::WORLD_UNITS_PER_CELL;

/// As long as the blast's own 3 damage turns (`blast::RADII`), one tick each.
pub const FLASH_SECS: f32 = RADII.len() as f32 * TICK_SECS;

const COLOUR: Color = Color::srgb(1.0, 0.85, 0.45);
const GLOW: f32 = 6.0;
/// How thin the flash disc sits over the ground (cells).
const THICKNESS: f32 = 0.12;

/// The flash's radius (cells, the current turn's `blast::RADII`) and opacity `secs` after the blast started;
/// `None` once faded.
pub fn flash(secs: f32) -> Option<(f32, f32)> {
    (0.0..FLASH_SECS).contains(&secs).then(|| {
        let turn = ((secs / TICK_SECS) as usize).min(RADII.len() - 1);
        let radius = RADII[turn] as f32 / WORLD_UNITS_PER_CELL as f32;
        let p = secs / FLASH_SECS;
        (radius, (1.0 - p) * (1.0 - p))
    })
}

#[derive(Resource)]
pub struct BlastFireballAssets {
    sphere: Handle<Mesh>,
}

#[derive(Component)]
pub struct BlastFireballPart;

#[derive(Component)]
pub struct BlastFireballView {
    part: Entity,
    material: Handle<StandardMaterial>,
}

impl WorldView for BlastFireballView {
    type Model = (Id<Effect>, Effect);
    type Params =
        (Res<'static, CurrentMap>, Res<'static, BlastFireballAssets>, ResMut<'static, Assets<StandardMaterial>>, Query<'static, 'static, (&'static mut Transform, &'static mut Visibility), With<BlastFireballPart>>);

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| matches!(e.kind, EffectKind::SimpleBlast(_)))
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((_, e): &(Id<Effect>, Effect), view: &mut EntityCommands, (_, assets, mats, _): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let cell = WORLD_UNITS_PER_CELL as f32;
        view.insert((Grounded { at: Vec2::new(e.x as f32, e.z as f32) / cell, half: 0.0 }, Transform::default(), Visibility::Visible));
        let material = mats.add(StandardMaterial { base_color: COLOUR, emissive: LinearRgba::from(COLOUR) * GLOW, alpha_mode: AlphaMode::Blend, unlit: true, cull_mode: None, ..default() });
        let mut part = Entity::PLACEHOLDER;
        view.with_children(|v| {
            part = v.spawn((BlastFireballPart, Mesh3d(assets.sphere.clone()), MeshMaterial3d(material.clone()), NotShadowCaster, crate::hover::NoOutline, Transform::default(), Visibility::Hidden)).id();
        });
        Some(BlastFireballView { part, material })
    }

    fn update(&self, (_, e): &(Id<Effect>, Effect), now: SimNow, (map, _, mats, parts): &mut SystemParamItem<Self::Params>) {
        let start = Tick::from_wire(map.0.now.to_wire().wrapping_sub(e.left.done().get()));
        let secs = now.secs_since(start);
        let Ok((mut t, mut vis)) = parts.get_mut(self.part) else { return };
        match flash(secs) {
            Some((radius, opacity)) => {
                (t.translation, t.scale, *vis) = (Vec3::Y * THICKNESS, Vec3::new(radius, THICKNESS, radius), Visibility::Inherited);
                if let Some(mut m) = mats.get_mut(&self.material) {
                    m.base_color.set_alpha(opacity);
                }
            }
            None => *vis = Visibility::Hidden,
        }
    }
}

pub struct BlastFireballPlugin;

impl Plugin for BlastFireballPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets).add_systems(Update, sync_views::<BlastFireballView>.after(crate::units::run_ticks));
    }
}

fn load_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(BlastFireballAssets { sphere: meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap()) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flash_grows_through_each_turns_radius_then_fades_once() {
        assert_eq!(flash(-0.01), None, "not out yet");
        let (r0, o0) = flash(0.0).unwrap();
        assert_eq!(r0 as u32 * WORLD_UNITS_PER_CELL, RADII[0]);
        let (r1, _) = flash(TICK_SECS * 1.5).unwrap();
        assert_eq!(r1 as u32 * WORLD_UNITS_PER_CELL, RADII[1]);
        let (r2, o2) = flash(FLASH_SECS - 0.001).unwrap();
        assert_eq!(r2 as u32 * WORLD_UNITS_PER_CELL, RADII[2]);
        assert!(o2 < o0, "fades out: {o0} -> {o2}");
        assert_eq!(flash(FLASH_SECS), None, "faded");
    }
}
