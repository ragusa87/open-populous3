//! The build/rebuild ritual's own shots in flight (`game_core::site::SiteShot`): "the shot's own visual
//! while in flight" (pop3-rev-analysis.md "Build/rebuild ritual") is, live-measured, not a straight bolt or
//! tether - each turn it hasn't yet landed it samples several small jittered points along its own path and
//! drops a "lightning elem" effect at each one, the identical falling-bolt-segment component Lightning's own
//! bolt is built from, reading as a rapid, crackling, scattered burst rather than a smooth beam. Reused here
//! rather than inventing a second lightning-ish look: `lightning::zigzag`/`ribbon`/`cross_texture`, shrunk
//! down and scattered along the shaman-to-slot line instead of falling from the sky to one fixed point.

use crate::camera::{CameraRig, CurveParamsRes};
use crate::grounded::{render_pos, Grounded};
use crate::lightning::{cross_texture, ribbon, zigzag, COLOUR, SKY};
use crate::sim_time::{seed, SimNow};
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
use game_core::tribes::TribeId;
use pop3_format::WORLD_UNITS_PER_CELL;

/// How many crackles show at once along the shot's own path.
const CRACKLES: usize = 3;
/// Shrinks `zigzag`'s own sky-to-ground shape (`SKY`, about 8 cells tall) down to a short crackle.
const CRACKLE_SCALE: f32 = 0.09;
/// In `zigzag`'s own pre-shrink units (it spans 0..`SKY`, about 8): scaled down by `CRACKLE_SCALE` along with
/// the rest of the mesh, so the final on-screen thickness is `CRACKLE_THICKNESS * CRACKLE_SCALE` - sized here
/// for that to land close to `lightning::THICKNESS`'s own already-tuned 0.12.
const CRACKLE_THICKNESS: f32 = 1.3;
/// How far a crackle strays off the straight shaman-to-target line, each axis (cells).
const JITTER: f32 = 0.3;
/// Ticks between picking a fresh jittered point and shape for a crackle: one a turn, same rhythm as the shot
/// itself re-aiming each sub-step (pop3-rev-analysis.md).
const CRACKLE_TICKS: u32 = 1;
const TEXTURE: usize = 16;

#[derive(Resource)]
pub struct SiteShotAssets {
    cross_texture: Handle<Image>,
}

/// One crackle: its own entity, the shapes it flickers between and its own material (so each one's
/// brightness is independent of the others).
struct Crackle {
    entity: Entity,
    shapes: [Handle<Mesh>; CRACKLES],
    material: Handle<StandardMaterial>,
}

/// One shot's view: `CRACKLES` short-lived bolt segments scattered along its path.
#[derive(Component)]
pub struct SiteShotView {
    crackles: Vec<Crackle>,
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

/// The shaman of `owner`'s own render-space position (ground plus her own lift), `None` if she's gone or
/// outside the drawn disc.
fn shaman_pos(map: &GameMap, owner: TribeId, rig: &CameraRig, params: &CurveParams) -> Option<Vec3> {
    let u = map.shaman_of(owner)?;
    let at = Vec2::new(u.x as f32, u.z as f32) / WORLD_UNITS_PER_CELL as f32;
    let ground = render_pos(&map.terrain, &Grounded { at, half: 0.0 }, rig.focus, params)?;
    Some(ground + Vec3::Y * lift(map, (u.x, u.z), map.altitude(u) as i32, params.height_scale))
}

/// The shot's own render-space position (ground plus its own lift), `None` outside the drawn disc.
fn shot_pos(map: &GameMap, s: &SiteShot, rig: &CameraRig, params: &CurveParams) -> Option<Vec3> {
    let ground = render_pos(&map.terrain, &Grounded { at: cells_at(s), half: 0.0 }, rig.focus, params)?;
    Some(ground + Vec3::Y * lift(map, s.at, s.height, params.height_scale))
}

/// One crackle's own jittered point along `a` (the shaman) to `b` (the shot), slot `slot` of `CRACKLES`,
/// fresh every `CRACKLE_TICKS`: spread evenly along the line, each nudged off it by up to `JITTER` on every
/// axis, reseeded from the shot's own id, the slot and the tick so every crackle moves independently.
pub fn crackle_point(a: Vec3, b: Vec3, id: u32, slot: usize, tick: u32) -> Vec3 {
    let at = (slot as f32 + 0.5) / CRACKLES as f32;
    let h = |salt: u32| seed(id.wrapping_mul(2654435761).wrapping_add((slot as u32).wrapping_mul(97)).wrapping_add(tick / CRACKLE_TICKS).wrapping_add(salt)) - 0.5;
    a.lerp(b, at) + Vec3::new(h(1), h(2), h(3)) * JITTER
}

/// A crackle's own brightness (0..1) at `tick`, each slot flickering out of phase with the others - the
/// same "every other shape dimmer" rule `lightning::pose` uses for its one bolt.
pub fn crackle_flicker(slot: usize, tick: u32) -> f32 {
    if (tick / CRACKLE_TICKS + slot as u32).is_multiple_of(2) { 1.0 } else { 0.5 }
}

impl WorldView for SiteShotView {
    type Model = (Id<SiteShot>, SiteShot);
    type Params = (Res<'static, CurrentMap>, Res<'static, CameraRig>, Res<'static, CurveParamsRes>, Res<'static, SiteShotAssets>, ResMut<'static, Assets<Mesh>>, ResMut<'static, Assets<StandardMaterial>>, Query<'static, 'static, (&'static mut Mesh3d, &'static mut Transform, &'static mut Visibility)>);

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<SiteShot>, SiteShot)> {
        map.site_shots.entries()
    }

    fn id((id, _): &(Id<SiteShot>, SiteShot)) -> u32 {
        id.raw()
    }

    fn spawn((id, _): &(Id<SiteShot>, SiteShot), view: &mut EntityCommands, (.., assets, meshes, mats, _): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let blended = StandardMaterial { base_color: COLOUR, base_color_texture: Some(assets.cross_texture.clone()), alpha_mode: AlphaMode::Blend, unlit: true, cull_mode: None, ..default() };
        view.insert((Transform::default(), Visibility::Visible));
        let mut crackles = Vec::with_capacity(CRACKLES);
        view.with_children(|v| {
            for slot in 0..CRACKLES {
                let shapes: [Handle<Mesh>; CRACKLES] = std::array::from_fn(|k| meshes.add(ribbon(&zigzag(id.raw().wrapping_add(slot as u32 * 677), k), CRACKLE_THICKNESS)));
                let material = mats.add(blended.clone());
                let entity = v.spawn((Mesh3d(shapes[0].clone()), MeshMaterial3d(material.clone()), Transform::from_scale(Vec3::ZERO), Visibility::Hidden, NotShadowCaster, crate::hover::NoOutline)).id();
                crackles.push(Crackle { entity, shapes, material });
            }
        });
        Some(SiteShotView { crackles })
    }

    fn update(&self, (id, s): &(Id<SiteShot>, SiteShot), _now: SimNow, (map, rig, params, _, _, mats, parts): &mut SystemParamItem<Self::Params>) {
        let (origin, bullet) = (shaman_pos(&map.0, s.owner, rig, &params.0), shot_pos(&map.0, s, rig, &params.0));
        let facing = Quat::from_rotation_y(rig.yaw);
        let tick = map.0.now.to_wire();
        for (slot, crackle) in self.crackles.iter().enumerate() {
            let Ok((mut mesh, mut t, mut vis)) = parts.get_mut(crackle.entity) else { continue };
            let Some((a, b)) = origin.zip(bullet) else {
                vis.set_if_neq(Visibility::Hidden);
                continue;
            };
            let shape = &crackle.shapes[(tick / CRACKLE_TICKS) as usize % CRACKLES];
            if mesh.0 != *shape {
                mesh.0 = shape.clone();
            }
            let point = crackle_point(a, b, id.raw(), slot, tick);
            *t = Transform { translation: point - facing * Vec3::Y * (SKY * CRACKLE_SCALE / 2.0), rotation: facing, scale: Vec3::splat(CRACKLE_SCALE) };
            vis.set_if_neq(Visibility::Inherited);
            if let Some(mut m) = mats.get_mut(&crackle.material) {
                m.base_color = COLOUR.with_alpha(crackle_flicker(slot, tick));
            }
        }
    }
}

pub struct SiteShotPlugin;

impl Plugin for SiteShotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets).add_systems(Update, sync_views::<SiteShotView>.after(crate::units::run_ticks));
    }
}

fn load_assets(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let texture = Image::new(
        Extent3d { width: TEXTURE as u32, height: 1, depth_or_array_layers: 1 },
        TextureDimension::D2,
        cross_texture(TEXTURE),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    commands.insert_resource(SiteShotAssets { cross_texture: images.add(texture) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crackles_spread_along_the_line_and_move_with_the_tick() {
        let (a, b) = (Vec3::ZERO, Vec3::new(3.0, 0.0, 0.0));
        let p0 = crackle_point(a, b, 7, 0, 0);
        let p1 = crackle_point(a, b, 7, 0, CRACKLE_TICKS);
        assert_ne!(p0, p1, "a fresh jitter every tick");
        let near = |p: Vec3, x: f32| (p.x - x).abs() <= JITTER;
        assert!(near(crackle_point(a, b, 7, 0, 0), 0.5) && near(crackle_point(a, b, 7, 1, 0), 1.5) && near(crackle_point(a, b, 7, 2, 0), 2.5), "spread evenly along the line");
    }

    #[test]
    fn crackles_flicker_out_of_phase_with_each_other() {
        assert_ne!(crackle_flicker(0, 0), crackle_flicker(1, 0));
        assert_eq!(crackle_flicker(0, 0), crackle_flicker(0, 2 * CRACKLE_TICKS));
    }

    #[test]
    fn lift_never_goes_below_the_ground() {
        let map = GameMap::sandbox_units();
        let ground = map.terrain.height_at(1024, 512, WORLD_UNITS_PER_CELL);
        assert_eq!(lift(&map, (1024, 512), ground + 80, 0.5), 80.0 * 0.5);
        assert_eq!(lift(&map, (1024, 512), ground - 10, 0.5), 0.0);
    }
}
