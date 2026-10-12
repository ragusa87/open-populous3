//! The lightning bolt (`game_core::lightning`), a world view (`views::WorldView`) of each strike: a jagged bolt from
//! `SKY` above the target down to the ground, turned towards the camera, flickering between `SHAPES` zigzags and
//! fading over the strike (`pose`), with a glow on the ground at its foot. The bolt is alpha-blended: an additive
//! material without a texture did not show. Our own look (the original's bolt sprites
//! are not identified); shapes seeded from the effect's id, poses from simulation time.

use crate::camera::CameraRig;
use crate::grounded::Grounded;
use crate::sim_time::{seed, SimNow, TICK_SECS};
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use game_core::effect::{Effect, EffectKind};
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::time::Tick;
use pop3_format::WORLD_UNITS_PER_CELL;

/// Height the bolt comes from (render units, one per cell).
const SKY: f32 = 8.0;
/// Corners of a zigzag, and how far it strays sideways (render units).
const SEGMENTS: usize = 10;
const STRAY: f32 = 0.45;
/// The bolt's core and its halo: thickness (render units), and the halo's brightness relative to the core.
const THICKNESS: f32 = 0.12;
const HALO_THICKNESS: f32 = 0.5;
const HALO_ALPHA: f32 = 0.45;
/// Zigzags the bolt flickers between, each shown `SHAPE_TICKS` ticks.
const SHAPES: usize = 3;
const SHAPE_TICKS: u32 = 2;
/// Seconds at full brightness before the fade.
const FULL_SECS: f32 = 0.15;
/// The ground glow: its size (render units) and how long it lasts.
const GLOW_SIZE: f32 = 1.6;
const GLOW_SECS: f32 = 0.35;
/// Over-bright (linear, above 1) so the tone mapping leaves it white-blue rather than grey.
const COLOUR: Color = Color::linear_rgb(3.0, 3.4, 4.5);
const TEXTURE: usize = 32;

/// The bolt's corners from the sky (`(0, SKY)`) to the ground (`(0, 0)`), sideways (x) and up (y), for shape `k` of
/// the strike `id`: each corner strays at most `STRAY`, the ends stay above the target.
pub fn zigzag(id: u32, k: usize) -> Vec<Vec2> {
    (0..=SEGMENTS)
        .map(|i| {
            let y = SKY * (1.0 - i as f32 / SEGMENTS as f32);
            let ends = i == 0 || i == SEGMENTS;
            let x = if ends { 0.0 } else { (seed(id.wrapping_mul(31).wrapping_add((k * 64 + i) as u32)) * 2.0 - 1.0) * STRAY };
            Vec2::new(x, y)
        })
        .collect()
}

/// The bolt at `secs` into its strike of `total` seconds: the zigzag shown and its brightness (0 to 1), None once
/// over. Full at first, then fading, every other shape dimmer (a flicker).
pub fn pose(secs: f32, total: f32) -> Option<(usize, f32)> {
    if !(0.0..total).contains(&secs) {
        return None;
    }
    let tick = (secs / TICK_SECS) as u32;
    let shape = (tick / SHAPE_TICKS) as usize % SHAPES;
    let fade = if secs < FULL_SECS { 1.0 } else { (1.0 - (secs - FULL_SECS) / (total - FULL_SECS)).max(0.0).powi(2) };
    let flicker = if shape % 2 == 0 { 1.0 } else { 0.5 };
    Some((shape, fade * flicker))
}

/// The ground glow's brightness at `secs` into the strike.
pub fn glow(secs: f32) -> f32 {
    (1.0 - secs / GLOW_SECS).clamp(0.0, 1.0)
}

/// A flat ribbon along `points` (x sideways, y up, z 0), `thickness` wide, its texture's u going across it (0 on one
/// side, 1 on the other) so `cross_texture` makes it soft-edged.
fn ribbon(points: &[Vec2], thickness: f32) -> Mesh {
    let (mut positions, mut uvs, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    for w in points.windows(2) {
        let side = (w[1] - w[0]).perp().normalize_or_zero() * thickness / 2.0;
        let base = positions.len() as u32;
        for (p, u) in [(w[0] - side, 0.0), (w[0] + side, 1.0), (w[1] + side, 1.0), (w[1] - side, 0.0)] {
            positions.push([p.x, p.y, 0.0]);
            uvs.push([u, 0.5]);
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let n = positions.len();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

/// Across a ribbon: opaque on its centre line, clear at its edges (RGBA, `size` x 1).
fn cross_texture(size: usize) -> Vec<u8> {
    (0..size)
        .flat_map(|i| {
            let d = ((i as f32 + 0.5) / size as f32 * 2.0 - 1.0).abs();
            [255, 255, 255, ((1.0 - d).powf(1.5) * 255.0) as u8]
        })
        .collect()
}

/// A soft round glow, white in the middle, clear at the edge (RGBA, `size` x `size`).
fn glow_texture(size: usize) -> Vec<u8> {
    let c = (size as f32 - 1.0) / 2.0;
    (0..size * size)
        .flat_map(|i| {
            let r = (((i % size) as f32 - c).powi(2) + ((i / size) as f32 - c).powi(2)).sqrt() / c;
            [255, 255, 255, ((1.0 - r).clamp(0.0, 1.0).powi(2) * 255.0) as u8]
        })
        .collect()
}

#[derive(Resource)]
pub struct LightningAssets {
    glow_quad: Handle<Mesh>,
    glow_texture: Handle<Image>,
    cross_texture: Handle<Image>,
}

/// One strike's view: its zigzags (core and halo meshes), the bolt, halo and glow entities and their own materials
/// (they fade on their own).
#[derive(Component)]
pub struct LightningView {
    shapes: Vec<(Handle<Mesh>, Handle<Mesh>)>,
    bolt: Entity,
    halo: Entity,
    glow: Entity,
    bolt_material: Handle<StandardMaterial>,
    halo_material: Handle<StandardMaterial>,
    glow_material: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct BoltPart;

impl WorldView for LightningView {
    type Model = (Id<Effect>, Effect);
    type Params = (
        Res<'static, CurrentMap>,
        Res<'static, CameraRig>,
        Res<'static, LightningAssets>,
        ResMut<'static, Assets<Mesh>>,
        ResMut<'static, Assets<StandardMaterial>>,
        Query<'static, 'static, (&'static mut Mesh3d, &'static mut Transform, &'static mut Visibility), With<BoltPart>>,
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| matches!(e.kind, EffectKind::Lightning(_)))
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((id, e): &(Id<Effect>, Effect), view: &mut EntityCommands, (_, _, assets, meshes, mats, _): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let id = id.raw();
        let shapes: Vec<(Handle<Mesh>, Handle<Mesh>)> = (0..SHAPES).map(|k| zigzag(id, k)).map(|z| (meshes.add(ribbon(&z, THICKNESS)), meshes.add(ribbon(&z, HALO_THICKNESS)))).collect();
        let blended = || StandardMaterial { base_color: COLOUR, base_color_texture: Some(assets.cross_texture.clone()), alpha_mode: AlphaMode::Blend, unlit: true, cull_mode: None, ..default() };
        let (bolt_material, halo_material) = (mats.add(blended()), mats.add(blended()));
        let glow_material = mats.add(StandardMaterial { base_color_texture: Some(assets.glow_texture.clone()), alpha_mode: AlphaMode::Add, unlit: true, cull_mode: None, ..default() });
        let cell = WORLD_UNITS_PER_CELL as f32;
        view.insert((Grounded { at: Vec2::new(e.x as f32, e.z as f32) / cell, half: 0.0 }, Transform::default(), Visibility::Visible));
        let (mut bolt, mut halo, mut glow) = (Entity::PLACEHOLDER, Entity::PLACEHOLDER, Entity::PLACEHOLDER);
        view.with_children(|v| {
            halo = v.spawn((BoltPart, Mesh3d(shapes[0].1.clone()), MeshMaterial3d(halo_material.clone()), Transform::default(), Visibility::Hidden, NotShadowCaster, crate::hover::NoOutline)).id();
            bolt = v.spawn((BoltPart, Mesh3d(shapes[0].0.clone()), MeshMaterial3d(bolt_material.clone()), Transform::default(), Visibility::Hidden, NotShadowCaster, crate::hover::NoOutline)).id();
            glow = v.spawn((BoltPart, Mesh3d(assets.glow_quad.clone()), MeshMaterial3d(glow_material.clone()), Transform::default(), Visibility::Hidden, NotShadowCaster, crate::hover::NoOutline)).id();
        });
        Some(LightningView { shapes, bolt, halo, glow, bolt_material, halo_material, glow_material })
    }

    fn update(&self, (_, e): &(Id<Effect>, Effect), now: SimNow, (map, rig, _, _, mats, parts): &mut SystemParamItem<Self::Params>) {
        let start = Tick::from_wire(map.0.now.to_wire().wrapping_sub(e.left.done().get()));
        let total = e.left.total().get() as f32 * TICK_SECS;
        let secs = now.secs_since(start);
        let facing = Quat::from_rotation_y(rig.yaw);
        let shown = pose(secs, total);
        for (part, halo) in [(self.bolt, false), (self.halo, true)] {
            let Ok((mut mesh, mut t, mut vis)) = parts.get_mut(part) else { continue };
            let Some((shape, alpha)) = shown else {
                vis.set_if_neq(Visibility::Hidden);
                continue;
            };
            let wanted = if halo { &self.shapes[shape].1 } else { &self.shapes[shape].0 };
            if mesh.0 != *wanted {
                mesh.0 = wanted.clone();
            }
            t.rotation = facing;
            vis.set_if_neq(Visibility::Inherited);
            let material = if halo { &self.halo_material } else { &self.bolt_material };
            if let Some(mut m) = mats.get_mut(material) {
                m.base_color = COLOUR.with_alpha(if halo { alpha * HALO_ALPHA } else { alpha });
            }
        }
        if let Ok((_, mut t, mut vis)) = parts.get_mut(self.glow) {
            let a = if secs >= 0.0 { glow(secs) } else { 0.0 };
            vis.set_if_neq(if a > 0.0 { Visibility::Inherited } else { Visibility::Hidden });
            *t = Transform::from_rotation(Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch)).with_translation(Vec3::Y * 0.2);
            if let Some(mut m) = mats.get_mut(&self.glow_material) {
                m.base_color = Color::WHITE.with_alpha(a);
            }
        }
    }
}

pub struct LightningPlugin;

impl Plugin for LightningPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets).add_systems(Update, sync_views::<LightningView>.after(crate::units::run_ticks));
    }
}

fn load_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>) {
    let texture = Image::new(
        Extent3d { width: TEXTURE as u32, height: TEXTURE as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        glow_texture(TEXTURE),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let cross = Image::new(
        Extent3d { width: TEXTURE as u32, height: 1, depth_or_array_layers: 1 },
        TextureDimension::D2,
        cross_texture(TEXTURE),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    commands.insert_resource(LightningAssets { glow_quad: meshes.add(Rectangle::new(GLOW_SIZE, GLOW_SIZE)), glow_texture: images.add(texture), cross_texture: images.add(cross) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zigzag_falls_from_the_sky_to_the_target_within_its_stray() {
        let z = zigzag(7, 1);
        assert_eq!((z.len(), z[0], *z.last().unwrap()), (SEGMENTS + 1, Vec2::new(0.0, SKY), Vec2::ZERO));
        assert!(z.iter().all(|p| p.x.abs() <= STRAY) && z.windows(2).all(|w| w[1].y < w[0].y), "always going down");
        assert_eq!(zigzag(7, 1), z, "the same strike looks the same");
        assert_ne!(zigzag(7, 2), z, "its shapes differ");
        assert_ne!(zigzag(8, 1), z, "so do strikes");
    }

    #[test]
    fn a_ribbon_is_soft_across_and_opaque_on_its_line() {
        let t = cross_texture(32);
        let alpha = |i: usize| t[i * 4 + 3];
        assert!(alpha(15) > 230 && alpha(0) < 10 && alpha(31) < 10);
    }

    #[test]
    fn the_bolt_flickers_then_fades_out_over_the_strike() {
        let total = 1.0;
        assert_eq!(pose(-0.01, total), None, "not struck yet");
        assert_eq!(pose(0.0, total), Some((0, 1.0)), "full at first");
        let (shape, alpha) = pose(2.0 * TICK_SECS + 0.001, total).unwrap();
        assert!(shape == 1 && alpha <= 0.5 && alpha > 0.4, "another shape, dimmer: {shape} {alpha}");
        let late = pose(0.9, total).unwrap().1;
        assert!(late < 0.05, "nearly gone: {late}");
        assert_eq!(pose(total, total), None);
        assert!((glow(0.0) - 1.0).abs() < 1e-6 && glow(GLOW_SECS) == 0.0);
    }
}
