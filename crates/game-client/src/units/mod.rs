//! Units on the map. Runs the simulation clock (`GameMap::tick` at `TICKS_PER_SECOND`), draws each
//! unit as a camera-facing sprite on the ground, with a health bar over the head of selected units.
//! Sprites are drawn pulled towards the camera along the eye-feet line (same picture on screen) so
//! the ground right around the feet never hides them.
//! Mouse selection and orders live in `selection`; Space looks at the player's shaman. The sprites
//! come from the original animations when allowed (`art`), else they are generated (`procedural`).

pub mod art;
mod dust;
mod procedural;
mod sheets;
pub mod selection;

use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::grounded::{render_pos, Grounded};
use crate::world::{CurrentMap, LevelList, TerrainDirty};
use art::{frame_index, pose_for, sprite_dir, Frame, TribeArt};
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use game_core::unit::{Action, Unit, UnitKind, LANDING_TICKS, TICKS_PER_SECOND};
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
/// How high (cells) she floats when she appears after a teleport.
const LANDING_HEIGHT: f32 = 0.2;
/// How far (cells) sprites are pulled towards the camera: ground rising less than this in front
/// of the feet does not cut them.
const PULL_TO_EYE: f32 = 0.6;

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

/// Position `alpha` of the way from `a` to `b` (world units, shortest way around the torus), in
/// cells. Jumps of more than a cell in one tick (teleport, reincarnation) are not glided: at `b`.
pub fn glide(a: (u16, u16), b: (u16, u16), alpha: f32) -> Vec2 {
    let far = |a: u16, b: u16| game_core::unit::torus_delta(a, b).unsigned_abs() > WORLD_UNITS_PER_CELL;
    let alpha = if far(a.0, b.0) || far(a.1, b.1) { 1.0 } else { alpha };
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
    /// Size and feet in base pixels (fractional for detailed art).
    pub size: Vec2,
    pub origin: Vec2,
}

/// Every kind's and tribe's frames, uploaded once: `kinds[kind][tribe][pose][dir][frame]`, kinds in
/// `UnitKind::ALL` order.
#[derive(Resource, Default)]
pub struct UnitSprites {
    kinds: Vec<Vec<Vec<Vec<Vec<FrameAsset>>>>>,
}

impl UnitSprites {
    /// The frame showing `unit` from a camera at `yaw`.
    pub fn frame_for(&self, unit: &Unit, yaw: f32, clock: &SimClock) -> Option<&FrameAsset> {
        let pose = pose_for(&unit.action);
        let kind = UnitKind::ALL.iter().position(|&k| k == unit.kind)?;
        let frames = self.kinds.get(kind)?.get(unit.owner as usize)?.get(pose as usize)?.get(sprite_dir(unit.facing, yaw))?;
        frames.get(frame_index(pose, frames.len(), &unit.action, clock.anim_secs, clock.alpha()))
    }
}

/// Quad in the sprite's rectangle (base pixels), feet at the local origin, facing +Z.
pub fn sprite_quad(size: Vec2, origin: Vec2) -> Mesh {
    let (l, r) = (-origin.x * PIXEL, (size.x - origin.x) * PIXEL);
    let (t, b) = (origin.y * PIXEL, (origin.y - size.y) * PIXEL);
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[l, t, 0.0], [r, t, 0.0], [r, b, 0.0], [l, b, 0.0]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
        .with_inserted_indices(Indices::U32(vec![0, 2, 1, 0, 3, 2]))
}

/// Mouse selection and orders to units: spell aiming runs after it (it may put the spell away).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnitInput;

pub struct UnitsPlugin;

impl Plugin for UnitsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimClock>()
            .init_resource::<UnitSprites>()
            .add_plugins((selection::SelectionPlugin, dust::DustPlugin))
            .add_systems(Startup, load_sprites)
            .add_systems(Update, (selection::select_and_order.in_set(UnitInput), look_at_shaman, run_ticks, respawn_views, animate_views).chain().in_set(crate::menu::Gameplay))
            .add_systems(PostUpdate, pull_to_eye.before(TransformSystems::Propagate));
    }
}

fn load_sprites(
    levels: Res<LevelList>,
    mut sprites: ResMut<UnitSprites>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    // Original animations when allowed, else the open-source sheets, else generated figures.
    let originals = levels.original.then(|| art::Originals::load(&levels.data_dir).map_err(|e| warn!("original unit sprites: {e}")).ok()).flatten();
    let art: Vec<Vec<TribeArt>> = UnitKind::ALL
        .iter()
        .map(|&kind| {
            let original = originals.as_ref().and_then(|o| o.art(kind).map_err(|e| warn!("original {kind:?} sprites: {e}")).ok());
            original.or_else(|| sheets::sheet_art(kind)).unwrap_or_else(|| {
                info!("{kind:?}: generated sprites");
                art::generated_art(kind)
            })
        })
        .collect();
    let mut upload = |f: &Frame| {
        let steps = if f.scale == 1 { UPSCALE_STEPS } else { 0 };
        let mut big = (0..steps).fold(f.clone(), |g, _| art::scale2x(&g));
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
        let s = f.scale as f32;
        let (size, origin) = (Vec2::new(f.width as f32, f.height as f32) / s, Vec2::new(f.origin.0 as f32, f.origin.1 as f32) / s);
        let material = mats.add(StandardMaterial {
            base_color_texture: Some(image.clone()),
            alpha_mode: AlphaMode::Mask(0.5),
            unlit: true,
            cull_mode: None,
            double_sided: true,
            ..default()
        });
        FrameAsset { image, mesh: meshes.add(sprite_quad(size, origin)), material, size, origin }
    };
    sprites.kinds = art
        .iter()
        .map(|tribes| {
            tribes
                .iter()
                .map(|t| t.poses.iter().map(|dirs| dirs.iter().map(|frames| frames.iter().map(&mut upload).collect()).collect()).collect())
                .collect()
        })
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
            rig.fly_to(cell);
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
        commands.spawn((UnitView(i), Grounded { at: Vec2::ZERO, half: 0.0 }, Transform::default(), Visibility::Hidden)).with_children(|v| {
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
    sprites: Res<UnitSprites>,
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

/// Offset (render space) and scale that slide a sprite standing at `feet` towards `eye` by
/// `pull` along their line, keeping its picture: every point moves along its own line of sight.
pub fn toward_eye(feet: Vec3, eye: Vec3, pull: f32) -> (Vec3, f32) {
    let dist = feet.distance(eye);
    if dist <= pull * 2.0 {
        return (Vec3::ZERO, 1.0);
    }
    ((eye - feet) / dist * pull, (dist - pull) / dist)
}

/// Height above the ground (cells) while landing after a teleport: from `LANDING_HEIGHT` down to
/// 0, slowing as she touches down. `alpha` is the fraction of the current tick.
pub fn landing_lift(action: &Action, alpha: f32) -> f32 {
    let Action::Landing { left } = *action else { return 0.0 };
    let t = ((left as f32 - alpha) / LANDING_TICKS as f32).clamp(0.0, 1.0);
    LANDING_HEIGHT * t * t
}

/// Pulls each unit's sprite and health bar towards the camera (see `toward_eye`), in the view's
/// own (camera-facing) space, and lifts them while landing (`landing_lift`).
fn pull_to_eye(
    map: Res<CurrentMap>,
    rig: Res<CameraRig>,
    params: Res<CurveParamsRes>,
    clock: Res<SimClock>,
    eye: Query<&Transform, (With<GameCamera>, Without<UnitView>)>,
    views: Query<(&UnitView, &Grounded, &Transform, &Children)>,
    mut parts: Query<(&mut Transform, Has<HealthBar>), (Or<(With<UnitSprite>, With<HealthBar>)>, Without<UnitView>, Without<GameCamera>)>,
) {
    let Ok(eye) = eye.single() else { return };
    for (unit, ground, view, children) in &views {
        let Some(feet) = render_pos(&map.0.terrain, ground, rig.focus, &params.0) else { continue };
        let (offset, scale) = toward_eye(feet, eye.translation, PULL_TO_EYE);
        let lift = map.0.units.get(unit.0).map_or(0.0, |u| landing_lift(&u.action, clock.alpha()));
        let local = view.rotation.inverse() * offset + Vec3::Y * lift * scale;
        for &child in children {
            if let Ok((mut t, bar)) = parts.get_mut(child) {
                t.translation = local + if bar { Vec3::Y * BAR_HEIGHT * scale } else { Vec3::ZERO };
                t.scale = Vec3::splat(scale);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_down_when_landing() {
        let landing = |left| Action::Landing { left };
        assert_eq!(landing_lift(&landing(LANDING_TICKS), 0.0), LANDING_HEIGHT, "appears up in the air");
        let (high, low) = (landing_lift(&landing(4), 0.0), landing_lift(&landing(2), 0.0));
        assert!(LANDING_HEIGHT > high && high > low && low > 0.0);
        assert!(high - low > low - landing_lift(&landing(1), 0.99), "slows down as she touches down");
        assert_eq!(landing_lift(&landing(1), 1.0), 0.0);
        assert_eq!(landing_lift(&Action::Idle, 0.5), 0.0);
    }

    #[test]
    fn pulled_towards_the_eye_looks_the_same() {
        let (feet, eye) = (Vec3::new(1.0, 2.0, 3.0), Vec3::new(4.0, 10.0, 15.0));
        let (offset, scale) = toward_eye(feet, eye, 0.6);
        assert!((offset.length() - 0.6).abs() < 1e-5);
        let dir = (eye - feet).normalize();
        assert!(offset.normalize().distance(dir) < 1e-5, "along the line of sight: the feet stay in place on screen");
        let head = feet + Vec3::Y * 0.4;
        let pulled_head = feet + offset + Vec3::Y * 0.4 * scale;
        let (a, b) = ((head - eye).normalize(), (pulled_head - eye).normalize());
        assert!(a.distance(b) < 1e-5, "the head too: same picture, just closer");
        assert_eq!(toward_eye(feet, feet + Vec3::Y, 0.6), (Vec3::ZERO, 1.0), "camera right on top: left alone");
    }

    #[test]
    fn glides_between_ticks_across_the_edge() {
        let p = glide((65280, 512), (256, 512), 0.5);
        assert!(p.x.abs() < 1e-4 || (p.x - 128.0).abs() < 1e-4, "{p:?}");
        assert_eq!(p.y, 1.0);
        assert_eq!(glide((0, 0), (512, 1024), 1.0), Vec2::new(1.0, 2.0));
        assert_eq!(glide((0, 0), (20 * 512, 0), 0.1), Vec2::new(20.0, 0.0), "teleported: no slide across the map");
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
        let m = sprite_quad(Vec2::new(30.0, 40.0), Vec2::new(15.0, 38.0));
        let pos = m.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().unwrap();
        assert_eq!(pos[0], [-15.0 * PIXEL, 38.0 * PIXEL, 0.0]);
        assert!((pos[2][1] + 2.0 * PIXEL).abs() < 1e-6, "2 px under the feet");
    }
}
