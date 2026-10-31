//! A busy hut's chimney smoke (`effects::CHIMNEY`), the first world view (`views::WorldView`): its model
//! is a built building with room for people and someone inside; the view, a sibling of the building's
//! own placed and leaning like it, holds the puffs at the top of its roof (`Chimneys`, kept by
//! `buildings`). The smoke loops forever on simulation time, each hut at its own phase from its id.

use crate::buildings::{facing_yaw, RespawnBuildings, FOOTPRINT_HALF};
use crate::effects::{Puff, CHIMNEY};
use crate::grounded::{Grounded, Tilted};
use crate::object_scale::{ObjectGroup, Scaled};
use crate::sim_time::{seed, SimNow};
use crate::views::{sync_views, WorldView};
use bevy::light::NotShadowCaster;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use game_core::building::{Building, BuildingId, Stage};
use game_core::map::GameMap;
use game_core::time::{Tick, Ticks};
use pop3_format::WORLD_UNITS_PER_CELL;
use std::collections::HashMap;

/// One life of the smoke: its loop in whole ticks (`SimNow::secs_in`).
const LOOP: Ticks = Ticks::millis(2500);

/// The top of each built building's roof that could smoke, by building id, in its view's frame (cells).
#[derive(Resource, Default)]
pub struct Chimneys(pub HashMap<BuildingId, Vec3>);

#[derive(Resource)]
pub struct SmokeAssets {
    ball: Handle<Mesh>,
    smoke: Handle<StandardMaterial>,
}

/// The smoke of the building with id `id`: its puffs.
#[derive(Component)]
pub struct ChimneyView {
    id: BuildingId,
    puffs: Vec<Entity>,
}

/// Seconds into the smoke's loop at `now` of the building with id `id`.
pub fn smoke_secs(now: SimNow, id: u32) -> f32 {
    now.secs_in(Tick::ZERO, LOOP) + seed(id) * CHIMNEY.life
}

/// Whether `b` smokes: built, with room for people, someone inside.
pub fn smokes(b: &Building) -> bool {
    b.stage() == Stage::Built && b.kind.capacity() > 0 && b.inside > 0
}

impl WorldView for ChimneyView {
    type Model = Building;
    type Params = (Res<'static, Chimneys>, Res<'static, SmokeAssets>, Query<'static, 'static, &'static mut Puff>);

    fn models(map: &GameMap) -> impl Iterator<Item = &Building> {
        map.buildings.iter().filter(|b| smokes(b))
    }

    fn id(b: &Building) -> u32 {
        b.id.raw()
    }

    fn spawn(b: &Building, view: &mut EntityCommands, (chimneys, assets, _): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let top = *chimneys.0.get(&b.id)?;
        let (x, z) = b.centre();
        let cell = WORLD_UNITS_PER_CELL as f32;
        let yaw = facing_yaw(b.facing);
        view.insert((
            Grounded { at: Vec2::new(x as f32, z as f32) / cell, half: 0.0 },
            Tilted { half: FOOTPRINT_HALF, yaw },
            Scaled(ObjectGroup::Buildings),
            Transform::from_rotation(yaw.into()),
            Visibility::Hidden,
        ));
        let mut puffs = Vec::new();
        view.with_children(|v| {
            for k in 0..CHIMNEY.count {
                let puff = (Puff::new(CHIMNEY, k, top), Mesh3d(assets.ball.clone()), MeshMaterial3d(assets.smoke.clone()), NotShadowCaster, crate::hover::NoOutline);
                puffs.push(v.spawn((puff, Transform::from_translation(top), Visibility::Hidden)).id());
            }
        });
        Some(ChimneyView { id: b.id, puffs })
    }

    fn update(&self, _: &Building, now: SimNow, (chimneys, _, puffs): &mut SystemParamItem<Self::Params>) {
        let top = chimneys.0.get(&self.id).copied();
        for &e in &self.puffs {
            let Ok(mut p) = puffs.get_mut(e) else { continue };
            (p.secs, p.on) = (smoke_secs(now, self.id.raw()), top.is_some());
            if let Some(top) = top {
                p.origin = top;
            }
        }
    }
}

pub struct ChimneyPlugin;

impl Plugin for ChimneyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Chimneys>()
            .add_systems(Startup, load_assets)
            .add_systems(Update, sync_views::<ChimneyView>.after(RespawnBuildings));
    }
}

fn load_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(SmokeAssets {
        ball: meshes.add(Sphere::new(0.5).mesh().ico(1).unwrap()),
        smoke: mats.add(StandardMaterial { base_color: Color::srgba(0.75, 0.75, 0.75, 0.55), alpha_mode: AlphaMode::Blend, unlit: true, ..default() }),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::building::BuildingKind;
    use game_core::tribes::TribeId;

    #[test]
    fn busy_built_huts_smoke() {
        let hut = Building::new(BuildingKind::Hut { size: 1 }, TribeId::Blue, 0, 0, 0);
        assert!(!smokes(&hut), "nobody inside");
        assert!(smokes(&Building { inside: 1, ..hut.clone() }));
        assert!(!smokes(&Building { inside: 1, ..Building::site(BuildingKind::Hut { size: 1 }, TribeId::Blue, 0, 0, 0) }), "not built");
    }

    #[test]
    fn the_smoke_is_a_function_of_the_drawn_moment_and_the_id() {
        let at = |tick: u32, frac: f32| SimNow { tick: Tick::from_wire(tick), frac };
        assert_eq!(smoke_secs(at(100, 0.5), 9), smoke_secs(at(100, 0.5), 9), "paused: the same every frame");
        assert_eq!(smoke_secs(at(100, 0.5), 9), smoke_secs(at(100 + LOOP.get() * 50_000, 0.5), 9), "loops on whole ticks");
        assert_ne!(smoke_secs(at(100, 0.5), 9), smoke_secs(at(100, 0.5), 10), "each hut its own phase");
        let pose = |now| CHIMNEY.pose(0, smoke_secs(now, 9));
        assert_ne!(pose(at(100, 0.0)), pose(at(103, 0.0)), "rises");
    }
}
