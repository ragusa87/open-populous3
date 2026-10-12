//! Sparkles on the ground a terrain spell is reshaping (land bridge, later flatten, volcano...): a world view
//! (`views::WorldView`) of each effect with height points still moving (`Effect::reshaping`), one tiny twinkling
//! star per point, facing the camera. Each star loops on simulation time at its own phase, each twinkle at a new
//! random spot of the cell round its point, seeded from the effect's id and the point; it goes out once its point
//! reached its height.

use crate::camera::{CameraRig, CurveParamsRes};
use crate::grounded::{render_pos, Grounded};
use crate::sim_time::SimNow;
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
use game_core::time::Ticks;

/// One twinkle: the star grows, rises and shrinks away.
const LOOP: Ticks = Ticks::millis(750);
/// Largest size of a star (cells).
const SIZE: f32 = 0.22;
/// How high a star rises over a twinkle, from `LIFT` over the ground (render units).
const RISE: f32 = 0.35;
const LIFT: f32 = 0.08;
/// Farthest a star sits from its height point (cells): the whole cell around it, so neighbours' stars mingle.
const SPREAD: f32 = 0.5;
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

/// The two multipliers of MurmurHash3's 32-bit finaliser (`fmix32`): odd numbers its author found by search to
/// spread every input bit over every output bit. Any value is fine as long as it is odd and bit-scrambled.
const MURMUR3_MIX_1: u32 = 0x85EB_CA6B;
const MURMUR3_MIX_2: u32 = 0xC2B2_AE35;

/// A well-mixed hash of `x` (MurmurHash3's `fmix32`): nearby inputs give unrelated outputs.
fn mix(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(MURMUR3_MIX_1);
    x ^= x >> 13;
    x = x.wrapping_mul(MURMUR3_MIX_2);
    x ^ (x >> 16)
}

/// 0..1 from a hash.
fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// The star of the effect `id` on height point `point` (wrapped on a map of `size` points) at `now`: each twinkle
/// at its own phase, and at a new random spot of the cell around the point.
pub fn twinkle(id: u32, point: Cell, size: usize, now: SimNow) -> Twinkle {
    let n = size as i32;
    let key = mix(id ^ mix(point.0.rem_euclid(n) as u32 ^ mix(point.1.rem_euclid(n) as u32)));
    let period = LOOP.get();
    let t = now.tick.to_wire().wrapping_add(key % period);
    let (cycle, p) = (t / period, ((t % period) as f32 + now.frac) / period as f32);
    let spot = mix(key ^ cycle);
    let offset = Vec2::new(unit(spot) - 0.5, unit(mix(spot)) - 0.5) * 2.0 * SPREAD;
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
        let facing = Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch);
        let have = children.get(self.entity).map(|c| c.to_vec()).unwrap_or_default();
        for (k, &star) in have.iter().enumerate() {
            let Ok(mut t) = stars.get_mut(star) else { continue };
            let pose = points.get(k).and_then(|&p| {
                let tw = twinkle(self.id, p, terrain.size(), now);
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
    use game_core::time::Tick;

    #[test]
    fn a_star_twinkles_at_a_new_spot_of_its_cell_each_time() {
        let at = |tick: u32, frac: f32| SimNow { tick: Tick::from_wire(tick), frac };
        let period = LOOP.get();
        let poses: Vec<Twinkle> = (0..period * 4).map(|t| twinkle(7, (3, 4), 128, at(t, 0.5))).collect();
        assert!(poses.iter().all(|t| t.offset.abs().max_element() <= SPREAD && t.size <= SIZE && t.rise >= LIFT));
        assert!(poses.iter().any(|t| t.size > SIZE * 0.8) && poses.iter().any(|t| t.size < SIZE * 0.2), "twinkles");
        let spots: std::collections::BTreeSet<_> = poses.iter().map(|t| (t.offset.x.to_bits(), t.offset.y.to_bits())).collect();
        assert!(spots.len() >= 4, "a new spot each twinkle: {}", spots.len());
        assert_eq!(twinkle(7, (3, 4), 128, at(20, 0.2)), twinkle(7, (131, -124), 128, at(20, 0.2)), "a point the same across the map's edge");
        assert_ne!(twinkle(7, (3, 4), 128, at(20, 0.2)), twinkle(7, (3, 5), 128, at(20, 0.2)), "each point its own phase and spot");
        assert_ne!(twinkle(7, (3, 4), 128, at(20, 0.2)), twinkle(8, (3, 4), 128, at(20, 0.2)), "each effect its own");
    }

    #[test]
    fn neighbours_stars_are_not_lined_up() {
        let at = SimNow { tick: Tick::from_wire(100), frac: 0.0 };
        let xs: Vec<f32> = (0..20).map(|z| twinkle(7, (10, z), 128, at).offset.x).collect();
        let mean = xs.iter().sum::<f32>() / xs.len() as f32;
        let spread = xs.iter().map(|x| (x - mean).abs()).sum::<f32>() / xs.len() as f32;
        assert!(spread > 0.15, "a column of points gives scattered stars: {xs:?}");
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
