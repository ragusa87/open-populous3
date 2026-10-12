//! Sparkles on the ground a terrain spell is reshaping (land bridge, later flatten, volcano...): a world view
//! (`views::WorldView`) of each effect with height points still moving (`Effect::reshaping`), one tiny twinkling
//! star per point, facing the camera. Each star loops on simulation time at its own phase and spot near its point,
//! seeded from the effect's id and the point; it goes out once its point reached its height.

use crate::camera::{CameraRig, CurveParamsRes};
use crate::grounded::{render_pos, Grounded};
use crate::sim_time::{seed, SimNow, TICK_SECS};
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use game_core::effect::Effect;
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::spell::Cell;
use game_core::time::{Tick, Ticks};

/// One twinkle: the star grows, rises and shrinks away.
const LOOP: Ticks = Ticks::millis(750);
/// Largest size of a star (cells).
const SIZE: f32 = 0.22;
/// How high a star rises over a twinkle, from `LIFT` over the ground (render units).
const RISE: f32 = 0.35;
const LIFT: f32 = 0.08;
/// Farthest a star sits from its height point (cells).
const SPREAD: f32 = 0.4;
const TEXTURE: usize = 32;
const COLOUR: Color = Color::srgb(1.0, 0.92, 0.6);

#[derive(Resource)]
pub struct SparkleAssets {
    quad: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

/// The sparkles of the effect `id`; its stars are children of `entity`.
#[derive(Component)]
pub struct SparkleView {
    id: u32,
    entity: Entity,
}

#[derive(Component)]
pub struct Star;

/// A star's pose at a moment: where it sits from its height point (cells), its lift over the ground (render units)
/// and its size (cells).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Twinkle {
    pub offset: Vec2,
    pub rise: f32,
    pub size: f32,
}

/// The star of the effect `id` on height point `point` (wrapped on a map of `size` points), `secs` into the loop.
pub fn twinkle(id: u32, point: Cell, size: usize, secs: f32) -> Twinkle {
    let n = size as i32;
    let key = id.wrapping_mul(0x2545_F491) ^ (point.0.rem_euclid(n) as u32).wrapping_mul(73_856_093) ^ (point.1.rem_euclid(n) as u32).wrapping_mul(19_349_663);
    let p = (secs / (LOOP.get() as f32 * TICK_SECS) + seed(key)).fract();
    let offset = Vec2::new(seed(key ^ 0x51) - 0.5, seed(key ^ 0xA7) - 0.5) * 2.0 * SPREAD;
    let glow = (std::f32::consts::PI * p).sin();
    Twinkle { offset, rise: LIFT + RISE * p, size: SIZE * glow * glow }
}

/// A four-pointed glint, white and opaque in the middle, fading along its arms (RGBA, `size` x `size`).
pub fn star_texture(size: usize) -> Vec<u8> {
    let c = (size as f32 - 1.0) / 2.0;
    (0..size * size)
        .flat_map(|i| {
            let (x, y) = (((i % size) as f32 - c).abs() / c, ((i / size) as f32 - c).abs() / c);
            let core = (1.0 - (x * x + y * y).sqrt() * 2.5).max(0.0);
            let arms = (1.0 - x).max(0.0) * (1.0 - y * 8.0).max(0.0) + (1.0 - y).max(0.0) * (1.0 - x * 8.0).max(0.0);
            let a = (core + arms).min(1.0);
            [255, 255, 255, (a * 255.0) as u8]
        })
        .collect()
}

impl WorldView for SparkleView {
    type Model = (Id<Effect>, Effect);
    type Params = (
        Commands<'static, 'static>,
        Res<'static, CurrentMap>,
        Res<'static, CameraRig>,
        Res<'static, CurveParamsRes>,
        Res<'static, SparkleAssets>,
        Query<'static, 'static, &'static Children>,
        Query<'static, 'static, &'static mut Transform, With<Star>>,
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| !e.reshaping(&map.terrain).is_empty())
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((id, _): &(Id<Effect>, Effect), view: &mut EntityCommands, _: &mut SystemParamItem<Self::Params>) -> Option<Self> {
        view.insert((Transform::default(), Visibility::Visible));
        Some(SparkleView { id: id.raw(), entity: view.id() })
    }

    fn update(&self, (_, effect): &(Id<Effect>, Effect), now: SimNow, (commands, map, rig, params, assets, children, stars): &mut SystemParamItem<Self::Params>) {
        let terrain = &map.0.terrain;
        let points = effect.reshaping(terrain);
        let secs = now.secs_in(Tick::ZERO, LOOP);
        let facing = Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch);
        let have = children.get(self.entity).map(|c| c.to_vec()).unwrap_or_default();
        for (k, &star) in have.iter().enumerate() {
            let Ok(mut t) = stars.get_mut(star) else { continue };
            let pose = points.get(k).and_then(|&p| {
                let tw = twinkle(self.id, p, terrain.size(), secs);
                let at = Vec2::new(p.0 as f32, p.1 as f32) + tw.offset;
                render_pos(terrain, &Grounded { at, half: 0.0 }, rig.focus, &params.0).map(|ground| (ground + Vec3::Y * tw.rise, tw.size))
            });
            *t = match pose {
                Some((at, size)) => Transform { translation: at, rotation: facing, scale: Vec3::splat(size) },
                None => Transform::from_scale(Vec3::ZERO),
            };
        }
        for _ in have.len()..points.len() {
            commands.spawn((Star, Mesh3d(assets.quad.clone()), MeshMaterial3d(assets.material.clone()), Transform::from_scale(Vec3::ZERO), NotShadowCaster, crate::hover::NoOutline, ChildOf(self.entity)));
        }
    }
}

pub struct SparklesPlugin;

impl Plugin for SparklesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets).add_systems(Update, sync_views::<SparkleView>.after(crate::units::run_ticks));
    }
}

fn load_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let texture = Image::new(
        Extent3d { width: TEXTURE as u32, height: TEXTURE as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        star_texture(TEXTURE),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let material = mats.add(StandardMaterial {
        base_color: COLOUR,
        base_color_texture: Some(images.add(texture)),
        alpha_mode: AlphaMode::Add,
        unlit: true,
        cull_mode: None,
        ..default()
    });
    commands.insert_resource(SparkleAssets { quad: meshes.add(Rectangle::new(1.0, 1.0)), material });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_star_grows_rises_and_goes_out_over_its_loop_near_its_point() {
        let loop_secs = LOOP.get() as f32 * TICK_SECS;
        let poses: Vec<Twinkle> = (0..40).map(|i| twinkle(7, (3, 4), 128, i as f32 * loop_secs / 40.0)).collect();
        assert!(poses.iter().all(|t| t.offset.abs().max_element() <= SPREAD && t.size <= SIZE && t.rise >= LIFT));
        assert!(poses.iter().any(|t| t.size > SIZE * 0.9) && poses.iter().any(|t| t.size < SIZE * 0.1), "twinkles");
        assert_eq!(twinkle(7, (3, 4), 128, 0.2), twinkle(7, (3, 4), 128, 0.2 + loop_secs), "loops");
        assert_eq!(twinkle(7, (3, 4), 128, 0.2), twinkle(7, (131, -124), 128, 0.2), "a point the same across the map's edge");
        assert_ne!(twinkle(7, (3, 4), 128, 0.2), twinkle(7, (3, 5), 128, 0.2), "each point its own phase and spot");
        assert_ne!(twinkle(7, (3, 4), 128, 0.2), twinkle(8, (3, 4), 128, 0.2), "each effect its own");
    }

    #[test]
    fn the_star_is_bright_in_the_middle_along_its_arms_and_clear_in_the_corners() {
        let px = star_texture(TEXTURE);
        let alpha = |x: usize, y: usize| px[(y * TEXTURE + x) * 4 + 3];
        let mid = TEXTURE / 2;
        assert!(alpha(mid, mid) > 200 && alpha(mid, 3) > 0 && alpha(3, mid) > 0);
        assert_eq!((alpha(0, 0), alpha(3, 3)), (0, 0));
    }
}
