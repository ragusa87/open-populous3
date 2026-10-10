//! Units on the map. Runs the simulation clock (`GameMap::tick` at `TICKS_PER_SECOND`), draws each
//! unit as a camera-facing sprite on the ground (its shadow under it, `shadow`), with a health bar
//! over the head of selected units.
//! Sprites are drawn pulled towards the camera along the eye-feet line (same picture on screen) so
//! the ground right around the feet never hides them.
//! Mouse selection and orders live in `selection`; Space looks at the player's shaman. The sprites
//! come from the original animations when allowed (`art`), else they are generated (`procedural`).

pub mod art;
mod dust;
mod procedural;
mod shadow;
pub mod sheets;
pub mod selection;
pub mod worship;

use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::grounded::{render_pos, Grounded};
use crate::world::{CurrentMap, LevelList, TerrainDirty};
use art::{frame_index, pose_for, sprite_dir, Frame, Pose, TribeArt};
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use crate::game_speed::GameSpeed;
use crate::sim_time::TICK_SECS;
use game_core::command::Command;
use game_core::schedule::Schedule;
use game_core::unit::{Unit, UnitKind};
use selection::Selection;
use pop3_format::WORLD_UNITS_PER_CELL;

/// The local player's tribe.
pub const PLAYER: u8 = 0;
/// Cells per sprite pixel: the ~34 px shaman stands about 0.39 cell tall, half a reincarnation
/// stone (400 units, 0.78 cell).
pub const PIXEL: f32 = 1.0 / 88.0;
/// Sprites are uploaded upscaled (Scale2x twice) and filtered linearly instead of shown as blocks.
const UPSCALE_STEPS: usize = 2;
const BAR_HEIGHT: f32 = 0.5;
const BAR_SIZE: Vec2 = Vec2::new(0.36, 0.045);
/// How far (cells) sprites are pulled towards the camera: ground rising less than this in front
/// of the feet does not cut them.
const PULL_TO_EYE: f32 = 0.6;
/// How far (cells) a unit up a lookout is pulled towards the camera, out of the tower's middle.
pub const PERCH_PULL: f32 = 0.7;

/// The local player's commands, waiting for the tick they apply on (`game_core::schedule`). Input issues
/// them here, never to the map; `run_ticks` applies them. A new map starts a new schedule.
#[derive(Resource)]
pub struct GameSchedule(pub Schedule);

impl Default for GameSchedule {
    fn default() -> Self {
        GameSchedule(Schedule::single(PLAYER, game_core::time::Tick::ZERO))
    }
}

impl GameSchedule {
    pub fn issue(&mut self, command: Command) {
        self.0.issue(command);
    }
}

/// Tick times run at most per frame: past that a slow frame drops time instead of catching up.
const MAX_CATCH_UP: u32 = 4;

/// Fixed-step simulation clock, plus what the views need to draw between two ticks.
#[derive(Resource, Default)]
pub struct SimClock {
    acc: f32,
    /// Unit positions and heights off the ground (`Motion::height`) before the last ticks run, to glide
    /// between them.
    prev: Vec<(u16, u16)>,
    prev_heights: Vec<Option<i32>>,
    /// Game seconds since start (scaled by the speed, stopped while paused), drives looping animations.
    pub anim_secs: f32,
    /// Ticks run this frame.
    pub ran: u32,
}

impl SimClock {
    /// Ticks to run for a frame of `dt` seconds: whole tick times elapsed (at most `MAX_CATCH_UP`), each
    /// running `speed.ticks_per_step()` ticks. Paused, time stands still and nothing runs.
    pub fn steps_due(&mut self, dt: f32, speed: GameSpeed) -> u32 {
        let per_step = speed.ticks_per_step();
        self.ran = 0;
        if per_step == 0 {
            return 0;
        }
        self.anim_secs += dt * per_step as f32;
        self.acc += dt;
        let due = (self.acc / TICK_SECS) as u32;
        self.acc -= due as f32 * TICK_SECS;
        if due > MAX_CATCH_UP {
            self.acc = 0.0;
        }
        self.ran = due.min(MAX_CATCH_UP) * per_step;
        self.ran
    }

    /// Fraction of the current tick elapsed, 0..1.
    pub fn alpha(&self) -> f32 {
        (self.acc / TICK_SECS).clamp(0.0, 1.0)
    }

    /// Drawn position of `units[i]` in cells: between its previous and current tick position.
    pub fn cell_pos(&self, i: usize, unit: &Unit) -> Vec2 {
        let prev = self.prev.get(i).copied().unwrap_or((unit.x, unit.z));
        glide(prev, (unit.x, unit.z), self.alpha())
    }

    /// Drawn height above the ground of `units[i]` in cells (see `drawn_lift`).
    pub fn lift(&self, i: usize, unit: &Unit, terrain: &game_core::terrain::Heightmap, height_scale: f32) -> f32 {
        let ground = terrain.height_at(unit.x as u32, unit.z as u32, WORLD_UNITS_PER_CELL);
        let jumped = self.prev.get(i).is_some_and(|&p| jumped(p, (unit.x, unit.z)));
        let prev = if jumped { unit.motion.height } else { self.prev_heights.get(i).copied().flatten() };
        drawn_lift(prev, unit.motion.height, ground, self.alpha(), height_scale)
    }
}

/// Where the player's shaman is drawn, in cells.
pub fn player_shaman_cell(map: &game_core::map::GameMap, clock: &SimClock) -> Option<Vec2> {
    map.units.iter().enumerate().find(|(_, u)| u.owner == PLAYER).map(|(i, u)| clock.cell_pos(i, u))
}

/// Moved more than a cell along an axis in one tick (teleport, reincarnation): drawn there at once.
fn jumped(a: (u16, u16), b: (u16, u16)) -> bool {
    let far = |a: u16, b: u16| game_core::unit::torus_delta(a, b).unsigned_abs() > WORLD_UNITS_PER_CELL;
    far(a.0, b.0) || far(a.1, b.1)
}

/// Position `alpha` of the way from `a` to `b` (world units, shortest way around the torus), in
/// cells. Jumps of more than a cell in one tick (teleport, reincarnation) are not glided: at `b`.
pub fn glide(a: (u16, u16), b: (u16, u16), alpha: f32) -> Vec2 {
    let alpha = if jumped(a, b) { 1.0 } else { alpha };
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
    /// The frame showing `unit` from a camera at `yaw`; praying towards `worship` when it
    /// worships its shaman (`worship::worship_facing`).
    pub fn frame_for(&self, unit: &Unit, worship: Option<u8>, yaw: f32, clock: &SimClock) -> Option<&FrameAsset> {
        let (pose, facing) = worship.map_or((pose_for(&unit.action, unit.carrying > 0, unit.motion.airborne()), unit.facing), |f| (Pose::Pray, f));
        let kind = UnitKind::ALL.iter().position(|&k| k == unit.kind)?;
        // Wildmen have no tribe (owner 255): one look for all.
        let tribe = if unit.kind == UnitKind::Wildman { 0 } else { unit.owner as usize };
        let frames = self.kinds.get(kind)?.get(tribe)?.get(pose as usize)?.get(sprite_dir(facing, yaw))?;
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

/// The simulation's ticks for this frame (`SimClock::ran`): what follows the map's time runs after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimStep;

/// Unit sprites take this frame's picture: halos and overlays drawn from them come after.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnitViews;

pub struct UnitsPlugin;

impl Plugin for UnitsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimClock>()
            .init_resource::<GameSchedule>()
            .init_resource::<UnitSprites>()
            .add_plugins((selection::SelectionPlugin, dust::DustPlugin, shadow::ShadowPlugin))
            .add_systems(Startup, load_sprites)
            .add_systems(Update, (selection::select_and_order.in_set(UnitInput), look_at_shaman, run_ticks.in_set(SimStep), respawn_views, animate_views.in_set(UnitViews)).chain().in_set(crate::menu::Gameplay))
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
    let mut upload = |f: &Frame| upload_frame(f, &mut images, &mut meshes, &mut mats);
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

/// Uploads a frame: pixel art (scale 1) upscaled with Scale2x and filtered linearly, edges bled so
/// they do not darken; an unlit quad anchored at the feet, cut out, or blended when the frame has
/// see-through pixels (alpha sprites, `art::alpha_picture_frame`).
pub fn upload_frame(f: &Frame, images: &mut Assets<Image>, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>) -> FrameAsset {
    let blended = art::has_partial_alpha(f);
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
        alpha_mode: if blended { AlphaMode::Blend } else { AlphaMode::Mask(0.5) },
        unlit: true,
        cull_mode: None,
        double_sided: true,
        ..default()
    });
    FrameAsset { image, mesh: meshes.add(sprite_quad(size, origin)), material, size, origin }
}

fn run_ticks(
    time: Res<Time>,
    speed: Res<GameSpeed>,
    mut clock: ResMut<SimClock>,
    mut schedule: ResMut<GameSchedule>,
    mut map: ResMut<CurrentMap>,
    mut dirty: ResMut<TerrainDirty>,
) {
    let due = clock.steps_due(time.delta_secs(), *speed);
    let (mut levelled, mut ran) = (false, 0);
    if due > 0 {
        let map = &mut map.bypass_change_detection().0;
        clock.prev = map.units.iter().map(|u| (u.x, u.z)).collect();
        clock.prev_heights = map.units.iter().map(|u| u.motion.height).collect();
        while ran < due {
            let now = map.now;
            schedule.0.close_local(now);
            if !schedule.0.ready(now) {
                break;
            }
            let commands = schedule.0.take(now);
            levelled |= !map.step(&commands).is_empty();
            ran += 1;
        }
    }
    clock.ran = ran;
    if levelled {
        dirty.0 = true;
        map.set_changed();
    }
}

/// Space looks at her, unless it turns a blueprint (`blueprint`).
fn look_at_shaman(keys: crate::keymap::Shortcuts, mut rig: ResMut<CameraRig>, map: Res<CurrentMap>, clock: Res<SimClock>, blueprint: Res<crate::blueprint::Blueprint>) {
    if keys.just_pressed(crate::keymap::Shortcut::LookAtShaman) && !blueprint.is_active() && let Some(cell) = player_shaman_cell(&map.0, &clock) {
        rig.fly_to(cell);
    }
}

#[derive(Component)]
pub struct UnitView(pub usize);
#[derive(Component)]
pub struct UnitSprite(pub usize);
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
    for (i, u) in map.0.units.iter().enumerate() {
        let hover = crate::hover::Hoverable { health: u.owner == PLAYER };
        commands.spawn((UnitView(i), crate::object_scale::Scaled(crate::object_scale::ObjectGroup::Units), hover, Grounded { at: Vec2::ZERO, half: 0.0 }, Transform::default(), Visibility::Hidden)).with_children(|v| {
            v.spawn((UnitSprite(i), Mesh3d::default(), MeshMaterial3d::<StandardMaterial>::default(), Transform::default()));
            v.spawn((HealthBar, Mesh3d(bar.clone()), MeshMaterial3d(back.clone()), Transform::from_xyz(0.0, BAR_HEIGHT, 0.0), Visibility::Hidden))
                .with_child((HealthFill(i), Mesh3d(fill.clone()), MeshMaterial3d(mats.add(flat(health_color(1.0)))), Transform::from_xyz(0.0, 0.0, 0.005)));
        });
    }
}

#[allow(clippy::too_many_arguments)]
/// A unit standing in a built building is not seen (walking in or out by the door it is, in a
/// building under construction, open, it is, and up a tower's lookout it is: `standing_in`).
pub fn hidden_inside(map: &game_core::map::GameMap, u: &game_core::unit::Unit) -> bool {
    standing_in(map, u).is_some_and(|b| crate::buildings::lookout(b.kind).is_none())
}

/// The built building a unit stands in (not walking in or out of it).
pub fn standing_in<'a>(map: &'a game_core::map::GameMap, u: &game_core::unit::Unit) -> Option<&'a game_core::building::Building> {
    use game_core::unit::Action;
    if matches!(u.action, Action::Walking { .. } | Action::Stranded { .. } | Action::Entering { .. }) {
        return None;
    }
    let b = &map.buildings[map.building_at_corner(u.inside?.site)?];
    (b.stage() == game_core::building::Stage::Built).then_some(b)
}

/// Where a unit up a lookout is drawn (map position, height in cells), if it is up one.
fn perched(map: &game_core::map::GameMap, heights: &crate::buildings::ModelHeights, u: &game_core::unit::Unit) -> Option<(Vec2, f32)> {
    let b = standing_in(map, u)?;
    crate::buildings::perch(b, *heights.0.get(&(b.x, b.z))?)
}

fn animate_views(
    map: Res<CurrentMap>,
    rig: Res<CameraRig>,
    clock: Res<SimClock>,
    sprites: Res<UnitSprites>,
    mut views: Query<(&UnitView, &mut Grounded, &mut Transform, &crate::hover::Hoverable), (Without<UnitSprite>, Without<HealthFill>)>,
    mut bodies: Query<(&UnitSprite, &mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>, &mut Visibility), (Without<HealthFill>, Without<HealthBar>)>,
    mut bars: Query<(&ChildOf, &mut Visibility), (With<HealthBar>, Without<UnitSprite>)>,
    mut fills: Query<(&HealthFill, &mut Transform, &MeshMaterial3d<StandardMaterial>), Without<UnitView>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    selection: Res<Selection>,
    hovered: Res<crate::hover::Hovered>,
    heights: Res<crate::buildings::ModelHeights>,
) {
    let units = &map.0.units;
    let facing_camera = Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch);
    for (view, mut ground, mut t, _) in &mut views {
        let Some(u) = units.get(view.0) else { continue };
        ground.at = perched(&map.0, &heights, u).map_or_else(|| clock.cell_pos(view.0, u), |p| p.0);
        t.rotation = facing_camera;
    }
    let shamans = worship::shamans(units);
    for (body, mut mesh, mut mat, mut vis) in &mut bodies {
        let hidden = units.get(body.0).is_some_and(|u| hidden_inside(&map.0, u));
        vis.set_if_neq(if hidden { Visibility::Hidden } else { Visibility::Inherited });
        let frame = units.get(body.0).and_then(|u| {
            let worship = worship::worship_facing(u, shamans.get(u.owner as usize).copied().flatten());
            sprites.frame_for(u, worship, rig.yaw, &clock)
        });
        if let Some(f) = frame && mesh.0 != f.mesh {
            mesh.0 = f.mesh.clone();
            mat.0 = f.material.clone();
        }
    }
    for (parent, mut vis) in &mut bars {
        let view = views.get(parent.parent()).ok();
        let index = view.as_ref().map(|(v, ..)| v.0);
        let unit = index.and_then(|i| units.get(i));
        let health_on_hover = view.is_some_and(|(.., h)| h.health);
        let under_mouse = health_on_hover && index.is_some_and(|i| hovered.0 == Some(crate::hover::HoverTarget::Unit(i)));
        let shown = unit.is_some_and(|u| u.is_alive() && (selection.contains(u.id) || under_mouse));
        *vis = if shown { Visibility::Inherited } else { Visibility::Hidden };
    }
    for (fill, mut t, mat) in &mut fills {
        let Some(u) = units.get(fill.0) else { continue };
        let f = u.health.current() as f32 / u.health.max() as f32;
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

/// Height above `ground` (cells) of a unit off the ground: its absolute height (terrain height units, drawn
/// with the terrain's `height_scale`) glided `alpha` of the way from the last tick's (`prev`, the ground when
/// it was on it); 0 on the ground.
pub fn drawn_lift(prev: Option<i32>, now: Option<i32>, ground: i32, alpha: f32, height_scale: f32) -> f32 {
    let Some(now) = now else { return 0.0 };
    let from = prev.unwrap_or(ground) as f32;
    ((from + (now as f32 - from) * alpha - ground as f32) * height_scale).max(0.0)
}

/// Pulls each unit's sprite and health bar towards the camera (see `toward_eye`), in the view's
/// own (camera-facing) space, and lifts them off the ground (`drawn_lift`).
fn pull_to_eye(
    map: Res<CurrentMap>,
    rig: Res<CameraRig>,
    params: Res<CurveParamsRes>,
    clock: Res<SimClock>,
    eye: Query<&Transform, (With<GameCamera>, Without<UnitView>)>,
    views: Query<(&UnitView, &Grounded, &Transform, &Children)>,
    mut parts: Query<(&mut Transform, Has<HealthBar>), (Or<(With<UnitSprite>, With<HealthBar>)>, Without<UnitView>, Without<GameCamera>)>,
    heights: Res<crate::buildings::ModelHeights>,
) {
    let Ok(eye) = eye.single() else { return };
    for (unit, ground, view, children) in &views {
        let Some(feet) = render_pos(&map.0.terrain, ground, rig.focus, &params.0) else { continue };
        let u = map.0.units.get(unit.0);
        // Up a lookout: on its platform, pulled further so the building around him never hides him.
        let up = u.and_then(|u| perched(&map.0, &heights, u)).map(|p| Vec3::Y * p.1);
        let (offset, scale) = toward_eye(feet + up.unwrap_or_default(), eye.translation, if up.is_some() { PERCH_PULL } else { PULL_TO_EYE });
        let lift = u.map_or(0.0, |u| clock.lift(unit.0, u, &map.0.terrain, params.0.height_scale));
        let local = view.rotation.inverse() * (offset + up.unwrap_or_default()) + Vec3::Y * lift * scale;
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
    use game_core::unit::Action;

    #[test]
    fn orders_issued_reach_the_map_on_the_next_tick_and_wait_while_paused() {
        use bevy::ecs::system::RunSystemOnce;
        use game_core::unit::Order;
        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<SimClock>()
            .init_resource::<GameSchedule>()
            .init_resource::<TerrainDirty>()
            .insert_resource(GameSpeed { paused: true, times: 1 })
            .insert_resource(CurrentMap(game_core::map::GameMap::sandbox_walk()));
        let tick_time = |app: &mut App| {
            app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(TICK_SECS * 1.01));
            app.world_mut().run_system_once(run_ticks).unwrap();
        };
        let shaman = &app.world().resource::<CurrentMap>().0.units[0];
        let (x, z) = (shaman.x, shaman.z + 2048);
        app.world_mut().resource_mut::<GameSchedule>().issue(Command::Order { player: PLAYER, order: Order::MoveTo { x, z } });
        tick_time(&mut app);
        assert_eq!(app.world().resource::<CurrentMap>().0.units[0].action, Action::Idle, "paused: waits");
        app.world_mut().resource_mut::<GameSpeed>().paused = false;
        tick_time(&mut app);
        let map = &app.world().resource::<CurrentMap>().0;
        assert_eq!((map.units[0].action, map.now.to_wire()), (Action::Walking { to: (x, z) }, 1));
        assert_eq!(app.world().resource::<SimClock>().ran, 1);
    }

    #[test]
    fn the_clock_runs_whole_tick_times_at_the_game_speed() {
        let normal = GameSpeed::default();
        let mut c = SimClock::default();
        assert_eq!(c.steps_due(TICK_SECS * 0.5, normal), 0);
        assert!((c.alpha() - 0.5).abs() < 1e-4);
        assert_eq!(c.steps_due(TICK_SECS * 0.6, normal), 1);
        assert_eq!(c.steps_due(TICK_SECS * 2.0, GameSpeed { paused: false, times: 4 }), 8, "two tick times, 4 ticks each");
        assert_eq!(c.ran, 8);
    }

    #[test]
    fn paused_time_stands_still_and_a_slow_frame_drops_its_backlog() {
        let mut c = SimClock::default();
        c.steps_due(TICK_SECS * 0.3, GameSpeed::default());
        let (alpha, anim) = (c.alpha(), c.anim_secs);
        assert_eq!(c.steps_due(5.0, GameSpeed { paused: true, times: 1 }), 0);
        assert_eq!((c.alpha(), c.anim_secs, c.ran), (alpha, anim, 0));
        assert_eq!(c.steps_due(5.0, GameSpeed::default()), MAX_CATCH_UP);
        assert_eq!(c.steps_due(TICK_SECS * 0.5, GameSpeed::default()), 0, "the backlog is gone");
    }

    #[test]
    fn units_inside_hide_unless_walking_through_the_door() {
        use game_core::unit::{Action, Inside, Unit, UnitKind};
        let map = game_core::map::GameMap::sandbox_buildings();
        let built = map.buildings.iter().find(|b| b.stage() == game_core::building::Stage::Built).unwrap();
        let open = map.buildings.iter().find(|b| matches!(b.stage(), game_core::building::Stage::UnderConstruction { .. })).unwrap();
        let mut u = Unit::new(1, 0, UnitKind::Brave, (0, 0));
        assert!(!hidden_inside(&map, &u));
        u.inside = Some(Inside { site: (built.x, built.z), door: (0, 0) });
        assert!(hidden_inside(&map, &u));
        u.action = Action::Entering { to: (1, 1) };
        assert!(!hidden_inside(&map, &u));
        (u.action, u.inside) = (Action::Hammering, Some(Inside { site: (open.x, open.z), door: (0, 0) }));
        assert!(!hidden_inside(&map, &u), "seen through the open frame");
    }

    #[test]
    fn drawn_at_the_unit_lift_like_the_terrain() {
        assert_eq!(drawn_lift(None, None, 100, 0.5, 0.01), 0.0, "on the ground");
        assert_eq!(drawn_lift(Some(400), Some(400), 100, 0.5, 0.01), 3.0, "as high as ground 300 above");
        assert_eq!(drawn_lift(None, Some(300), 100, 0.5, 0.01), 1.0, "half way up from the ground");
        assert_eq!(drawn_lift(Some(300), Some(500), 100, 0.25, 0.01), 2.5);
        assert_eq!(drawn_lift(Some(150), Some(90), 100, 1.0, 0.01), 0.0, "never under the ground");
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
