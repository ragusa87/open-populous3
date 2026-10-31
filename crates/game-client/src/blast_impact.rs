//! The simple blast's settling smoke (`game_core::blast`): dark puffs (`effects::BLAST`) bursting out from the
//! centre once `blast_fireball.rs`'s flash fades.

use crate::effects::{Puff, BLAST};
use crate::grounded::Grounded;
use crate::sim_time::SimNow;
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::effect::{Effect, EffectKind};
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::time::Tick;
use pop3_format::WORLD_UNITS_PER_CELL;

const COLOUR: Color = Color::srgba(0.2, 0.18, 0.16, 0.8);

/// Seconds into the puffs' own burst, `secs` after the blast started: they wait for `blast_fireball::FLASH_SECS`,
/// the flash, to fade first.
pub fn puff_secs(secs: f32) -> f32 {
    secs - crate::blast_fireball::FLASH_SECS
}

#[derive(Resource)]
pub struct BlastImpactBall(Handle<Mesh>);

/// One blast's view: its puffs of settling smoke, each with its own material as they fade on their own.
#[derive(Component)]
pub struct BlastImpactView {
    puffs: Vec<Entity>,
}

impl WorldView for BlastImpactView {
    type Model = (Id<Effect>, Effect);
    type Params = (Res<'static, CurrentMap>, Res<'static, BlastImpactBall>, ResMut<'static, Assets<StandardMaterial>>, Query<'static, 'static, &'static mut Puff>);

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| matches!(e.kind, EffectKind::SimpleBlast(_)))
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((_, e): &(Id<Effect>, Effect), view: &mut EntityCommands, (_, ball, mats, _): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let cell = WORLD_UNITS_PER_CELL as f32;
        view.insert((Grounded { at: Vec2::new(e.x as f32, e.z as f32) / cell, half: 0.0 }, Transform::default(), Visibility::Visible));
        let mut puffs = Vec::new();
        view.with_children(|v| {
            for k in 0..BLAST.count {
                let material = mats.add(StandardMaterial { base_color: COLOUR, alpha_mode: AlphaMode::Blend, unlit: true, cull_mode: None, ..default() });
                let puff = (Puff::new(BLAST, k, Vec3::ZERO), Mesh3d(ball.0.clone()), MeshMaterial3d(material), NotShadowCaster, crate::hover::NoOutline);
                puffs.push(v.spawn((puff, Transform::default(), Visibility::Hidden)).id());
            }
        });
        Some(BlastImpactView { puffs })
    }

    fn update(&self, (_, e): &(Id<Effect>, Effect), now: SimNow, (map, _, _, puffs): &mut SystemParamItem<Self::Params>) {
        let start = Tick::from_wire(map.0.now.to_wire().wrapping_sub(e.left.done().get()));
        let secs = puff_secs(now.secs_since(start));
        for &p in &self.puffs {
            if let Ok(mut puff) = puffs.get_mut(p) {
                (puff.secs, puff.on) = (secs, true);
            }
        }
    }
}

pub struct BlastImpactPlugin;

impl Plugin for BlastImpactPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_ball).add_systems(Update, sync_views::<BlastImpactView>.after(crate::units::run_ticks));
    }
}

fn load_ball(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(BlastImpactBall(meshes.add(Sphere::new(0.5).mesh().ico(1).unwrap())));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_smoke_waits_for_the_flash_to_fade_then_bursts_once() {
        assert!(BLAST.pose(0, puff_secs(0.0)).is_none(), "still flashing");
        let secs = crate::blast_fireball::FLASH_SECS + 0.1;
        assert!(BLAST.pose(0, puff_secs(secs)).is_some());
        let over = crate::blast_fireball::FLASH_SECS + BLAST.life + 0.01;
        assert!(BLAST.pose(0, puff_secs(over)).is_none(), "once");
    }
}
