//! Units on the map. Runs the simulation clock (`GameMap::tick` at `TICKS_PER_SECOND`), draws each
//! unit as a camera-facing sprite on the ground, with a health bar over the head of selected units.
//! Mouse selection and orders live in `selection`; Space looks at the player's shaman. The sprites
//! come from the original animations when allowed (`art`), else they are generated (`procedural`).

pub mod art;
mod procedural;
pub mod selection;

use crate::camera::CameraRig;
use crate::grounded::Grounded;
use crate::world::{CurrentMap, LevelList, TerrainDirty};
use art::{frame_index, pose_for, sprite_dir, Frame, TribeArt};
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use game_core::unit::{Unit, TICKS_PER_SECOND};
use selection::Selection;
use pop3_format::WORLD_UNITS_PER_CELL;

/// The local player's tribe.
pub const PLAYER: u8 = 0;
/// Cells per sprite pixel: the ~34 px shaman stands about 0.39 cell tall, half a reincarnation
/// stone (400 units, 0.78 cell).
pub const PIXEL: f32 = 1.0 / 88.0;
/// Sprites are uploaded upscaled (Scale2x twice) and filtered linearly instead of shown as blocks.
const UPSCALE_STEPS: usize = 2;
const TICK_SECS: f32 = 1.0 / TICKS_PER_SECOND as f32;
const BAR_HEIGHT: f32 = 0.5;
const BAR_SIZE: Vec2 = Vec2::new(0.36, 0.045);
/// Footprint half size: the sprite rests on the lowest ground under it.
const FOOT_HALF: f32 = 0.15;

/// Fixed-step simulation clock, plus what the views need to draw between two ticks.
#[derive(Resource, Default)]
pub struct SimClock {
    acc: f32,
    /// Unit positions before the last tick, to glide between ticks.
    prev: Vec<(u16, u16)>,
    /// Seconds since start, drives looping animations.
    pub anim_secs: f32,
}

impl SimClock {
    /// Fraction of the current tick elapsed, 0..1.
    pub fn alpha(&self) -> f32 {
        (self.acc / TICK_SECS).clamp(0.0, 1.0)
    }

    /// Drawn position of `units[i]` in cells: between its previous and current tick position.
    pub fn cell_pos(&self, i: usize, unit: &Unit) -> Vec2 {
        let prev = self.prev.get(i).copied().unwrap_or((unit.x, unit.z));
        glide(prev, (unit.x, unit.z), self.alpha())
    }
}

/// Where the player's shaman is drawn, in cells.
pub fn player_shaman_cell(map: &game_core::map::GameMap, clock: &SimClock) -> Option<Vec2> {
    map.units.iter().enumerate().find(|(_, u)| u.owner == PLAYER).map(|(i, u)| clock.cell_pos(i, u))
}

/// Position `alpha` of the way from `a` to `b` (world units, shortest way around the torus), in cells.
pub fn glide(a: (u16, u16), b: (u16, u16), alpha: f32) -> Vec2 {
    let lerp = |a: u16, b: u16| {
        let d = game_core::unit::torus_delta(a, b) as f32;
        (a as f32 + d * alpha).rem_euclid(65536.0) / WORLD_UNITS_PER_CELL as f32
    };
    Vec2::new(lerp(a.0, b.0), lerp(a.1, b.1))
}

/// Map position in cells to world units (wraps).
pub fn world_units(cell: Vec2) -> (u16, u16) {
    let w = |c: f32| (c * WORLD_UNITS_PER_CELL as f32).rem_euclid(65536.0) as u32 as u16;
    (w(cell.x), w(cell.y))
}

/// Health bar colour: green when full, through yellow, to red.
pub fn health_color(fraction: f32) -> Color {
    let f = fraction.clamp(0.0, 1.0);
    Color::srgb((2.0 - 2.0 * f).min(1.0), (2.0 * f).min(1.0), 0.1)
}

/// A drawable frame: image (for the HUD), quad anchored at the feet and its material.
#[derive(Clone)]
pub struct FrameAsset {
    pub image: Handle<Image>,
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
    pub size: UVec2,
    pub origin: UVec2,
}

/// Every tribe's shaman frames, uploaded once: `tribes[tribe].poses[pose][dir][frame]`.
#[derive(Resource, Default)]
pub struct ShamanSprites {
    tribes: Vec<Vec<Vec<Vec<FrameAsset>>>>,
}

impl ShamanSprites {
    /// The frame showing `unit` from a camera at `yaw`.
    pub fn frame_for(&self, unit: &Unit, yaw: f32, clock: &SimClock) -> Option<&FrameAsset> {
        let pose = pose_for(&unit.action);
        let frames = self.tribes.get(unit.owner as usize)?.get(pose as usize)?.get(sprite_dir(unit.facing, yaw))?;
        frames.get(frame_index(pose, frames.len(), &unit.action, clock.anim_secs, clock.alpha()))
    }
}

/// Quad in the sprite's pixel rectangle, feet at the local origin, facing +Z.
pub fn sprite_quad(width: u32, height: u32, origin: UVec2) -> Mesh {
    let (l, r) = (-(origin.x as f32) * PIXEL, (width as f32 - origin.x as f32) * PIXEL);
    let (t, b) = (origin.y as f32 * PIXEL, (origin.y as f32 - height as f32) * PIXEL);
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[l, t, 0.0], [r, t, 0.0], [r, b, 0.0], [l, b, 0.0]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
        .with_inserted_indices(Indices::U32(vec![0, 2, 1, 0, 3, 2]))
}

pub struct UnitsPlugin;

impl Plugin for UnitsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimClock>()
            .init_resource::<ShamanSprites>()
            .add_plugins(selection::SelectionPlugin)
            .add_systems(Startup, load_sprites)
            .add_systems(Update, (selection::select_and_order, look_at_shaman, run_ticks, respawn_views, animate_views).chain().in_set(crate::menu::Gameplay));
    }
}

fn load_sprites(
    levels: Res<LevelList>,
    mut sprites: ResMut<ShamanSprites>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let original = levels.original.then(|| art::original_art(&levels.data_dir).map_err(|e| warn!("original shaman sprites: {e}")).ok());
    let art: Vec<TribeArt> = original.flatten().unwrap_or_else(|| {
        info!("shaman: generated sprites");
        art::generated_art()
    });
    let mut upload = |f: &Frame| {
        let mut big = (0..UPSCALE_STEPS).fold(f.clone(), |g, _| art::scale2x(&g));
        art::bleed_edges(&mut big);
        let mut image = Image::new(
            Extent3d { width: big.width as u32, height: big.height as u32, depth_or_array_layers: 1 },
            TextureDimension::D2,
            big.rgba,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        image.sampler = ImageSampler::linear();
        let image = images.add(image);
        let (size, origin) = (UVec2::new(f.width as u32, f.height as u32), UVec2::new(f.origin.0 as u32, f.origin.1 as u32));
        let material = mats.add(StandardMaterial {
            base_color_texture: Some(image.clone()),
            alpha_mode: AlphaMode::Mask(0.5),
            unlit: true,
            cull_mode: None,
            double_sided: true,
            ..default()
        });
        FrameAsset { image, mesh: meshes.add(sprite_quad(size.x, size.y, origin)), material, size, origin }
    };
    sprites.tribes = art
        .iter()
        .map(|t| t.poses.iter().map(|dirs| dirs.iter().map(|frames| frames.iter().map(&mut upload).collect()).collect()).collect())
        .collect();
}

fn run_ticks(time: Res<Time>, mut clock: ResMut<SimClock>, mut map: ResMut<CurrentMap>, mut dirty: ResMut<TerrainDirty>) {
    clock.anim_secs += time.delta_secs();
    clock.acc += time.delta_secs();
    let mut levelled = false;
    while clock.acc >= TICK_SECS {
        clock.acc -= TICK_SECS;
        let map = map.bypass_change_detection();
        clock.prev = map.0.units.iter().map(|u| (u.x, u.z)).collect();
        levelled |= !map.0.tick().is_empty();
    }
    if levelled {
        dirty.0 = true;
        map.set_changed();
    }
}

fn look_at_shaman(keys: Res<ButtonInput<KeyCode>>, mut rig: ResMut<CameraRig>, map: Res<CurrentMap>, clock: Res<SimClock>) {
    if keys.just_pressed(KeyCode::Space) {
        if let Some(cell) = player_shaman_cell(&map.0, &clock) {
            rig.focus = cell;
        }
    }
}

#[derive(Component)]
struct UnitView(usize);
#[derive(Component)]
struct UnitSprite(usize);
#[derive(Component)]
struct HealthFill(usize);
#[derive(Component)]
struct HealthBar;

fn respawn_views(
    mut commands: Commands,
    map: Res<CurrentMap>,
    existing: Query<Entity, With<UnitView>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    if !map.is_changed() {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn();
    }
    let flat = |c: Color| StandardMaterial { base_color: c, unlit: true, cull_mode: None, ..default() };
    let back = mats.add(flat(Color::srgb(0.05, 0.05, 0.05)));
    let bar = meshes.add(Rectangle::from_size(BAR_SIZE));
    let fill = meshes.add(Rectangle::new(1.0, BAR_SIZE.y * 0.6));
    for (i, _) in map.0.units.iter().enumerate() {
        commands.spawn((UnitView(i), Grounded { at: Vec2::ZERO, half: FOOT_HALF }, Transform::default(), Visibility::Hidden)).with_children(|v| {
            v.spawn((UnitSprite(i), Mesh3d::default(), MeshMaterial3d::<StandardMaterial>::default(), Transform::default()));
            v.spawn((HealthBar, Mesh3d(bar.clone()), MeshMaterial3d(back.clone()), Transform::from_xyz(0.0, BAR_HEIGHT, 0.0), Visibility::Hidden))
                .with_child((HealthFill(i), Mesh3d(fill.clone()), MeshMaterial3d(mats.add(flat(health_color(1.0)))), Transform::from_xyz(0.0, 0.0, 0.005)));
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn animate_views(
    map: Res<CurrentMap>,
    rig: Res<CameraRig>,
    clock: Res<SimClock>,
    sprites: Res<ShamanSprites>,
    mut views: Query<(&UnitView, &mut Grounded, &mut Transform), (Without<UnitSprite>, Without<HealthFill>)>,
    mut bodies: Query<(&UnitSprite, &mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>), Without<HealthFill>>,
    mut bars: Query<(&ChildOf, &mut Visibility), With<HealthBar>>,
    mut fills: Query<(&HealthFill, &mut Transform, &MeshMaterial3d<StandardMaterial>), Without<UnitView>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    selection: Res<Selection>,
) {
    let units = &map.0.units;
    let facing_camera = Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch);
    for (view, mut ground, mut t) in &mut views {
        let Some(u) = units.get(view.0) else { continue };
        ground.at = clock.cell_pos(view.0, u);
        t.rotation = facing_camera;
    }
    for (body, mut mesh, mut mat) in &mut bodies {
        if let Some(f) = units.get(body.0).and_then(|u| sprites.frame_for(u, rig.yaw, &clock)) {
            if mesh.0 != f.mesh {
                mesh.0 = f.mesh.clone();
                mat.0 = f.material.clone();
            }
        }
    }
    for (parent, mut vis) in &mut bars {
        let unit = views.get(parent.parent()).ok().and_then(|(v, ..)| units.get(v.0));
        let shown = unit.is_some_and(|u| u.is_alive() && selection.contains(u.id));
        *vis = if shown { Visibility::Inherited } else { Visibility::Hidden };
    }
    for (fill, mut t, mat) in &mut fills {
        let Some(u) = units.get(fill.0) else { continue };
        let f = u.health as f32 / u.max_health() as f32;
        t.scale.x = (BAR_SIZE.x - 0.03) * f;
        t.translation.x = -(BAR_SIZE.x - 0.03) * (1.0 - f) / 2.0;
        if let Some(mut m) = mats.get_mut(&mat.0) {
            m.base_color = health_color(f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glides_between_ticks_across_the_edge() {
        let p = glide((65280, 512), (256, 512), 0.5);
        assert!(p.x.abs() < 1e-4 || (p.x - 128.0).abs() < 1e-4, "{p:?}");
        assert_eq!(p.y, 1.0);
        assert_eq!(glide((0, 0), (512, 1024), 1.0), Vec2::new(1.0, 2.0));
    }

    #[test]
    fn cells_to_world_units_wrap() {
        assert_eq!(world_units(Vec2::new(1.5, -0.5)), (768, 65280));
        assert_eq!(world_units(Vec2::new(128.0, 0.0)), (0, 0));
    }

    #[test]
    fn health_colours() {
        assert_eq!(health_color(1.0), Color::srgb(0.0, 1.0, 0.1));
        assert_eq!(health_color(0.5), Color::srgb(1.0, 1.0, 0.1));
        assert_eq!(health_color(0.0), Color::srgb(1.0, 0.0, 0.1));
    }

    #[test]
    fn quad_hangs_from_the_feet() {
        let m = sprite_quad(30, 40, UVec2::new(15, 38));
        let pos = m.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().unwrap();
        assert_eq!(pos[0], [-15.0 * PIXEL, 38.0 * PIXEL, 0.0]);
        assert!((pos[2][1] + 2.0 * PIXEL).abs() < 1e-6, "2 px under the feet");
    }
}
