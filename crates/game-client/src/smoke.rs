//! Smoke effects (`game_core::smoke`), a world view (`views::WorldView`) of each: dark puffs (`effects::DAMAGE`) over
//! where it stands, one after another every `GAP`, each with its own material as they fade on their own; posed from
//! simulation time since the effect started.

use crate::effects::{Puff, DAMAGE};
use crate::grounded::Grounded;
use crate::sim_time::{seed, SimNow};
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

/// Seconds between two puffs.
const GAP: f32 = 0.12;
/// Height (cells) the puffs come out at: about a hut's roof.
const ORIGIN: f32 = 0.5;
const COLOUR: Color = Color::srgba(0.22, 0.2, 0.19, 0.75);

/// Seconds into its own puff of puff `k`, `secs` after the smoke started.
pub fn puff_secs(secs: f32, k: usize) -> f32 {
    secs - k as f32 * GAP
}

#[derive(Resource)]
pub struct SmokeBall(Handle<Mesh>);

/// One smoke's view: its puffs.
#[derive(Component)]
pub struct SmokeView {
    puffs: Vec<Entity>,
}

impl WorldView for SmokeView {
    type Model = (Id<Effect>, Effect);
    type Params = (Res<'static, CurrentMap>, Res<'static, SmokeBall>, ResMut<'static, Assets<StandardMaterial>>, Query<'static, 'static, &'static mut Puff>);

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| e.kind == EffectKind::Smoke)
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((id, e): &(Id<Effect>, Effect), view: &mut EntityCommands, (_, ball, mats, _): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let cell = WORLD_UNITS_PER_CELL as f32;
        let turn = Quat::from_rotation_y(seed(id.raw()) * std::f32::consts::TAU);
        view.insert((Grounded { at: Vec2::new(e.x as f32, e.z as f32) / cell, half: 0.0 }, Transform::from_rotation(turn), Visibility::Visible));
        let mut puffs = Vec::new();
        view.with_children(|v| {
            for k in 0..DAMAGE.count {
                let material = mats.add(StandardMaterial { base_color: COLOUR, alpha_mode: AlphaMode::Blend, unlit: true, ..default() });
                let puff = (Puff::new(DAMAGE, k, Vec3::Y * ORIGIN), Mesh3d(ball.0.clone()), MeshMaterial3d(material), NotShadowCaster, crate::hover::NoOutline);
                puffs.push(v.spawn((puff, Transform::default(), Visibility::Hidden)).id());
            }
        });
        Some(SmokeView { puffs })
    }

    fn update(&self, (_, e): &(Id<Effect>, Effect), now: SimNow, (map, _, _, puffs): &mut SystemParamItem<Self::Params>) {
        let start = Tick::from_wire(map.0.now.to_wire().wrapping_sub(e.left.done().get()));
        let secs = now.secs_since(start);
        for (k, &p) in self.puffs.iter().enumerate() {
            if let Ok(mut puff) = puffs.get_mut(p) {
                (puff.secs, puff.on) = (puff_secs(secs, k), true);
            }
        }
    }
}

pub struct SmokePlugin;

impl Plugin for SmokePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_ball).add_systems(Update, sync_views::<SmokeView>.after(crate::units::run_ticks));
    }
}

fn load_ball(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(SmokeBall(meshes.add(Sphere::new(0.5).mesh().ico(1).unwrap())));
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::smoke::DAMAGE_TICKS;

    #[test]
    fn the_puffs_come_one_after_another_all_gone_before_the_smoke_ends() {
        assert!(DAMAGE.pose(1, puff_secs(0.05, 1)).is_none(), "the second not out yet");
        assert!(DAMAGE.pose(1, puff_secs(GAP + 0.05, 1)).is_some());
        let end = DAMAGE_TICKS.get() as f32 * crate::sim_time::TICK_SECS;
        assert!((0..DAMAGE.count).all(|k| DAMAGE.pose(k, puff_secs(end, k)).is_none()));
    }
}
