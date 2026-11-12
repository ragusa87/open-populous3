//! A spy's thrown firebrand (`game_core::arson`, `EffectKind::Firebrand`), a world view of each: a small glowing ember
//! flying in a low arc from the spy to the building's door at the firebrand's own speed, gone once there. Our own
//! look (the original's sprite is not identified).

use crate::grounded::Grounded;
use crate::sim_time::SimNow;
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::arson::FIREBRAND_SPEED;
use game_core::effect::{Effect, EffectKind};
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::time::Tick;
use game_core::unit::torus_delta;
use pop3_format::WORLD_UNITS_PER_CELL;

const RADIUS: f32 = 0.07;
const GLOW: Color = Color::srgb(1.0, 0.55, 0.15);
/// The arc's top over the ground, render units.
const ARC: f32 = 0.6;

#[derive(Resource)]
pub struct FirebrandLook {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct FirebrandView {
    me: Entity,
    ember: Entity,
}

/// Where the ember is `turns` after the throw from `from` to `to` (world units), in cells, and how far along (0 to 1).
pub fn flight(from: (u16, u16), to: (u16, u16), turns: f32) -> (Vec2, f32) {
    let d = Vec2::new(torus_delta(from.0, to.0) as f32, torus_delta(from.1, to.1) as f32);
    let len = d.length();
    let p = if len == 0.0 { 1.0 } else { (turns.max(0.0) * FIREBRAND_SPEED as f32 / len).min(1.0) };
    ((Vec2::new(from.0 as f32, from.1 as f32) + d * p) / WORLD_UNITS_PER_CELL as f32, p)
}

/// Its height over the ground along the way: up and down again.
pub fn arc(p: f32) -> f32 {
    4.0 * ARC * p * (1.0 - p)
}

impl WorldView for FirebrandView {
    type Model = (Id<Effect>, Effect);
    type Params = (
        Res<'static, CurrentMap>,
        Res<'static, FirebrandLook>,
        Query<'static, 'static, &'static mut Grounded>,
        Query<'static, 'static, (&'static mut Transform, &'static mut Visibility), Without<Grounded>>,
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| matches!(e.kind, EffectKind::Firebrand { .. }))
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((_, e): &(Id<Effect>, Effect), view: &mut EntityCommands, (_, look, ..): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let me = view.id();
        view.insert((Grounded { at: Vec2::new(e.x as f32, e.z as f32) / WORLD_UNITS_PER_CELL as f32, half: 0.0 }, Transform::default(), Visibility::Visible));
        let mut ember = Entity::PLACEHOLDER;
        view.with_children(|v| {
            ember = v.spawn((Mesh3d(look.mesh.clone()), MeshMaterial3d(look.material.clone()), NotShadowCaster, crate::hover::NoOutline, Transform::default(), Visibility::Inherited)).id();
        });
        Some(FirebrandView { me, ember })
    }

    fn update(&self, (_, e): &(Id<Effect>, Effect), now: SimNow, (map, _, grounded, parts): &mut SystemParamItem<Self::Params>) {
        let EffectKind::Firebrand { to } = e.kind else { return };
        let start = Tick::from_wire(map.0.now.to_wire().wrapping_sub(e.left.done().get()));
        let turns = now.turns_since(start);
        let (at, p) = flight((e.x, e.z), to, turns);
        if let Ok(mut g) = grounded.get_mut(self.me) {
            g.at = at;
        }
        if let Ok((mut t, mut vis)) = parts.get_mut(self.ember) {
            t.translation.y = arc(p);
            vis.set_if_neq(if p < 1.0 { Visibility::Inherited } else { Visibility::Hidden });
        }
    }
}

pub struct FirebrandsPlugin;

impl Plugin for FirebrandsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_look).add_systems(Update, sync_views::<FirebrandView>.after(crate::units::run_ticks));
    }
}

fn load_look(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let material = mats.add(StandardMaterial { base_color: GLOW, emissive: LinearRgba::from(GLOW) * 4.0, unlit: true, ..default() });
    commands.insert_resource(FirebrandLook { mesh: meshes.add(Sphere::new(RADIUS)), material });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_flies_at_its_speed_and_lands_at_the_door() {
        let (at, p) = flight((1024, 1024), (1024 + 400, 1024), 1.0);
        assert_eq!(at, Vec2::new((1024.0 + 80.0) / 512.0, 2.0));
        assert!((p - 0.2).abs() < 1e-6);
        let (at, p) = flight((1024, 1024), (1024 + 400, 1024), 50.0);
        assert_eq!((at, p), (Vec2::new(1424.0 / 512.0, 2.0), 1.0));
        assert_eq!(arc(0.0), 0.0);
        assert!(arc(0.5) > arc(0.2) && arc(1.0) == 0.0);
    }
}
