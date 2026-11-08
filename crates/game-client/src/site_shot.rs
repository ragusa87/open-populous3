//! The build/rebuild ritual's own shots in flight (`game_core::site::SiteShot`): "the shot's own visual
//! while in flight" (pop3-rev-analysis.md "Build/rebuild ritual") is, live-confirmed by screenshot, a sparse
//! scatter of a few small twinkles drifting near the shot as it crosses from the shaman to the target slot -
//! not a connecting beam or bolt of any kind, visually the same small four-point glint
//! `sparkles.rs`'s own terrain-reshaping stars use, just tribe-tinted and looping far faster (a whole shot's
//! flight is only a couple of ticks, not `sparkles.rs`'s own many-tick reshapes). Reused from there
//! (`twinkle_at`/`star_texture`/`mix`) rather than inventing a second small-sparkle look.

use crate::camera::{CameraRig, CurveParamsRes};
use crate::grounded::{render_pos, Grounded};
use crate::sim_time::{seed, SimNow};
use crate::sites::tribe_color;
use crate::sparkles::{mix, star_texture, twinkle_at};
use crate::terrain_mesh::CurveParams;
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::site::SiteShot;
use game_core::time::Ticks;
use pop3_format::WORLD_UNITS_PER_CELL;

/// How many twinkles drift near the shot at once.
const FIREFLIES: usize = 4;
/// A whole grow-rise-shrink cycle: our own guess, short enough to play out within a shot's brief flight
/// (1-2 ticks at the standard shot's own 1400 u/turn over a slot's few cells) rather than `sparkles.rs`'s own
/// many-tick `LOOP`.
const PERIOD: Ticks = Ticks::new(4);
/// How far a twinkle strays from the shot's own current position, each axis (cells).
const JITTER: f32 = 0.35;
const TEXTURE: usize = 24;

#[derive(Resource)]
pub struct SiteShotAssets {
    quad: Handle<Mesh>,
    texture: Handle<Image>,
}

/// One shot's view: `FIREFLIES` twinkles drifting near its own current position, each its own entity and
/// material so they can be tinted and faded independently.
#[derive(Component)]
pub struct SiteShotView {
    fireflies: [Entity; FIREFLIES],
}

/// A shot's own place, in cells.
fn cells_at(s: &SiteShot) -> Vec2 {
    Vec2::new(s.at.0 as f32, s.at.1 as f32) / WORLD_UNITS_PER_CELL as f32
}

/// How high over the ground under it something at `(x, z)`/`height` flies, in render units (`firebolts::lift`'s
/// own shape).
fn lift(map: &GameMap, (x, z): (u16, u16), height: i32, height_scale: f32) -> f32 {
    let ground = map.terrain.height_at(x as u32, z as u32, WORLD_UNITS_PER_CELL);
    (height - ground).max(0) as f32 * height_scale
}

/// The shot's own render-space position (ground plus its own lift), `None` outside the drawn disc.
fn shot_pos(map: &GameMap, s: &SiteShot, rig: &CameraRig, params: &CurveParams) -> Option<Vec3> {
    let ground = render_pos(&map.terrain, &Grounded { at: cells_at(s), half: 0.0 }, rig.focus, params)?;
    Some(ground + Vec3::Y * lift(map, s.at, s.height, params.height_scale))
}

/// One firefly's own jitter off the shot's current position, slot `slot` of `FIREFLIES`, fresh every tick:
/// reseeded from the shot's own id, the slot and the tick (`sparkles.rs`'s own `mix`) so each one drifts
/// independently rather than all sitting on top of each other.
pub fn firefly_jitter(id: u32, slot: usize, tick: u32) -> Vec3 {
    let h = |salt: u32| seed(mix(id ^ mix(slot as u32 ^ mix(tick ^ salt)))) - 0.5;
    Vec3::new(h(1), h(2), h(3)) * JITTER
}

impl WorldView for SiteShotView {
    type Model = (Id<SiteShot>, SiteShot);
    type Params = (Res<'static, CurrentMap>, Res<'static, CameraRig>, Res<'static, CurveParamsRes>, Res<'static, SiteShotAssets>, ResMut<'static, Assets<StandardMaterial>>, Query<'static, 'static, (&'static mut Transform, &'static mut Visibility)>);

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<SiteShot>, SiteShot)> {
        map.site_shots.entries()
    }

    fn id((id, _): &(Id<SiteShot>, SiteShot)) -> u32 {
        id.raw()
    }

    fn spawn((_, s): &(Id<SiteShot>, SiteShot), view: &mut EntityCommands, (.., assets, mats, _): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let material = StandardMaterial { base_color: tribe_color(s.owner), base_color_texture: Some(assets.texture.clone()), alpha_mode: AlphaMode::Add, unlit: true, cull_mode: None, ..default() };
        view.insert((Transform::default(), Visibility::Visible));
        let mut fireflies = [Entity::PLACEHOLDER; FIREFLIES];
        view.with_children(|v| {
            for firefly in &mut fireflies {
                *firefly = v.spawn((Mesh3d(assets.quad.clone()), MeshMaterial3d(mats.add(material.clone())), Transform::from_scale(Vec3::ZERO), Visibility::Hidden, NotShadowCaster, crate::hover::NoOutline)).id();
            }
        });
        Some(SiteShotView { fireflies })
    }

    fn update(&self, (id, s): &(Id<SiteShot>, SiteShot), now: SimNow, (map, rig, params, _, _, parts): &mut SystemParamItem<Self::Params>) {
        let at = shot_pos(&map.0, s, rig, &params.0);
        let facing = Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch);
        let tick = map.0.now.to_wire();
        for (slot, &entity) in self.fireflies.iter().enumerate() {
            let Ok((mut t, mut vis)) = parts.get_mut(entity) else { continue };
            let Some(at) = at else {
                vis.set_if_neq(Visibility::Hidden);
                continue;
            };
            let tw = twinkle_at(mix(id.raw() ^ mix(slot as u32)), PERIOD, now);
            *t = Transform { translation: at + firefly_jitter(id.raw(), slot, tick) + Vec3::Y * tw.rise, rotation: facing, scale: Vec3::splat(tw.size) };
            vis.set_if_neq(Visibility::Inherited);
        }
    }
}

pub struct SiteShotPlugin;

impl Plugin for SiteShotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets).add_systems(Update, sync_views::<SiteShotView>.after(crate::units::run_ticks));
    }
}

fn load_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>) {
    let texture = Image::new(
        Extent3d { width: TEXTURE as u32, height: TEXTURE as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        star_texture(TEXTURE),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    // A unit quad: `update`'s own `Transform.scale` (`twinkle_at`'s `size`, up to `SIZE`) sizes it, the same
    // split `sparkles.rs`'s own `SparkleAssets.quad` uses - building it pre-scaled here would double-shrink it.
    commands.insert_resource(SiteShotAssets { quad: meshes.add(Rectangle::new(1.0, 1.0)), texture: images.add(texture) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fireflies_jitter_independently_and_move_with_the_tick() {
        let j0 = firefly_jitter(7, 0, 0);
        let j1 = firefly_jitter(7, 0, 1);
        assert_ne!(j0, j1, "a fresh jitter every tick");
        assert_ne!(firefly_jitter(7, 0, 0), firefly_jitter(7, 1, 0), "slots drift independently");
        assert!(j0.x.abs() <= JITTER && j0.y.abs() <= JITTER && j0.z.abs() <= JITTER);
    }

    #[test]
    fn lift_never_goes_below_the_ground() {
        let map = GameMap::sandbox_units();
        let ground = map.terrain.height_at(1024, 512, WORLD_UNITS_PER_CELL);
        assert_eq!(lift(&map, (1024, 512), ground + 80, 0.5), 80.0 * 0.5);
        assert_eq!(lift(&map, (1024, 512), ground - 10, 0.5), 0.0);
    }
}
