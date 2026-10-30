//! Units on the map. Runs the simulation clock (`GameMap::tick` at `TICKS_PER_SECOND`), draws each
//! unit as a camera-facing sprite on the ground (its shadow under it, `shadow`), with a health bar
//! over the head of selected units.
//! Sprites are drawn pulled towards the camera along the eye-feet line (same picture on screen) so
//! the ground right around the feet never hides them.
//! Mouse selection and orders live in `selection`; Space looks at the player's shaman. The sprites
//! come from the original animations when allowed (`art`), else they are generated (`procedural`).

pub mod art;
pub mod atlas;
mod dust;
pub mod markers;
mod procedural;
mod shadow;
pub mod sheets;
pub mod selection;
pub mod worship;

use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::grounded::{render_pos, Grounded};
use crate::world::{CurrentMap, LevelList, TerrainGlide, TerrainPatches};
use art::{frame_index, sprite_dir, Frame, Pose};
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use crate::game_speed::GameSpeed;
use crate::sim_time::{SimNow, TICK_SECS};
use game_core::command::Command;
use game_core::schedule::Schedule;
use game_core::unit::{Unit, UnitId, UnitKind};
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

/// Fixed-step simulation clock, plus what the views need to draw between two ticks. It runs on the
/// real frame time (it drives the simulation); the world is drawn at `SimNow`, derived from the map's tick.
#[derive(Resource, Default)]
pub struct SimClock {
    acc: f32,
    /// Unit ids, positions and heights off the ground (`Motion::height`) before the last ticks run, to glide
    /// between them.
    prev_ids: Vec<UnitId>,
    prev: Vec<(u16, u16)>,
    prev_heights: Vec<Option<i32>>,
    /// Ticks the last step ran (the game speed then), kept while paused: the drawn moment trails the map by
    /// one step (`SimNow::drawn`).
    step: u32,
    /// Ticks run this frame.
    pub ran: u32,
    /// Counts the games started (`new_game`): views of an earlier game are cleared (ids restart on a new map).
    pub game: u32,
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
        self.acc += dt;
        let due = (self.acc / TICK_SECS) as u32;
        self.acc -= due as f32 * TICK_SECS;
        if due > MAX_CATCH_UP {
            self.acc = 0.0;
        }
        self.ran = due.min(MAX_CATCH_UP) * per_step;
        if self.ran > 0 {
            self.step = per_step;
        }
        self.ran
    }

    /// The ticks due could not all run (lockstep waiting for a peer): hold the picture at the last tick
    /// run instead of going back, and try again next frame.
    pub fn stalled(&mut self) {
        self.acc = TICK_SECS;
    }

    /// A new game: nothing to glide from, and the views of the last game go.
    pub fn new_game(&mut self) {
        *self = SimClock { game: self.game.wrapping_add(1), ..SimClock::default() };
    }

    /// The moment to draw the world at, the map being at `now`.
    pub fn now(&self, now: game_core::time::Tick) -> SimNow {
        SimNow::drawn(now, self.step.max(1), self.alpha())
    }

    /// Fraction of the current tick elapsed, 0..1.
    pub fn alpha(&self) -> f32 {
        (self.acc / TICK_SECS).clamp(0.0, 1.0)
    }

    /// Drawn position of `units[i]` in cells: between its previous and current tick position.
    pub fn cell_pos(&self, i: usize, unit: &Unit) -> Vec2 {
        let prev = self.prev.get(i).copied().filter(|_| self.same(i, unit)).unwrap_or((unit.x, unit.z));
        glide(prev, (unit.x, unit.z), self.alpha())
    }

    /// `units[i]` was there before the last ticks (not shifted by a unit gone from the map).
    fn same(&self, i: usize, unit: &Unit) -> bool {
        self.prev_ids.get(i) == Some(&unit.id)
    }

    /// Drawn height above the ground of `units[i]` in cells (see `drawn_lift`).
    pub fn lift(&self, i: usize, unit: &Unit, terrain: &game_core::terrain::Heightmap, height_scale: f32) -> f32 {
        let ground = terrain.height_at(unit.x as u32, unit.z as u32, WORLD_UNITS_PER_CELL);
        let jumped = !self.same(i, unit) || self.prev.get(i).is_some_and(|&p| jumped(p, (unit.x, unit.z)));
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
    /// `still`: the vehicle it is aboard is not under way, so its pedalling holds its first frame.
    /// `celebrating`: the match has ended and this unit is on the winning side (`GameMap::on_winning_side`) — an
    /// otherwise-idle unit's victory bounce (`victory_bounce`) is shown arms-up (`Pose::Stranded`) instead of just
    /// standing there, our own stand-in: no real "happy" pose is traced in the original. A unit mid-action keeps its
    /// own pose unchanged. The shaman has
    /// no Stranded art at all (`sheets::plays`, she's "never stranded"); she gets her own cast pose instead (the
    /// closest thing to "excited" she has) — forced with no `Action::Casting` under it, so `frame_index` loops it
    /// by time instead of a one-shot countdown, same as Stranded already does for everyone else.
    pub fn frame_for(&self, unit: &Unit, worship: Option<u8>, still: bool, yaw: f32, clock: &SimClock, now: SimNow, celebrating: bool) -> Option<&FrameAsset> {
        let (mut pose, facing) = worship.map_or((art::pose_of(unit, now.tick), unit.facing), |f| (Pose::Pray, f));
        if celebrating && matches!(pose, Pose::Idle | Pose::CarryIdle) {
            pose = if sheets::plays(unit.kind, Pose::Stranded) {
                Pose::Stranded
            } else if sheets::plays(unit.kind, Pose::Cast) {
                Pose::Cast
            } else {
                pose
            };
        }
        let kind = UnitKind::ALL.iter().position(|&k| k == unit.kind)?;
        // Wildmen have no tribe (owner 255): one look for all.
        let tribe = if unit.kind == UnitKind::Wildman { 0 } else { unit.owner as usize };
        let frames = self.kinds.get(kind)?.get(tribe)?.get(pose as usize)?.get(sprite_dir(facing, yaw))?;
        let frame = if pose == Pose::Pedal && still { 0 } else { frame_index(pose, frames.len(), &unit.action, now, clock.alpha()) };
        frames.get(frame)
    }
}

/// Quad in the sprite's rectangle (base pixels), feet at the local origin, facing +Z, `uv`
/// (`[u_min, v_min, u_max, v_max]`) picking its sub-rect of a shared texture (a whole-texture frame
/// passes `[0, 0, 1, 1]`, see `sprite_quad`).
pub fn sprite_quad_uv(size: Vec2, origin: Vec2, uv: [f32; 4]) -> Mesh {
    let (l, r) = (-origin.x * PIXEL, (size.x - origin.x) * PIXEL);
    let (t, b) = (origin.y * PIXEL, (origin.y - size.y) * PIXEL);
    let [u0, v0, u1, v1] = uv;
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[l, t, 0.0], [r, t, 0.0], [r, b, 0.0], [l, b, 0.0]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[u0, v0], [u1, v0], [u1, v1], [u0, v1]])
        .with_inserted_indices(Indices::U32(vec![0, 2, 1, 0, 3, 2]))
}

/// Quad in the sprite's rectangle (base pixels), feet at the local origin, facing +Z, the whole texture as UV.
pub fn sprite_quad(size: Vec2, origin: Vec2) -> Mesh {
    sprite_quad_uv(size, origin, [0.0, 0.0, 1.0, 1.0])
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
            .init_resource::<SimNow>()
            .init_resource::<GameSchedule>()
            .init_resource::<UnitSprites>()
            .add_plugins((selection::SelectionPlugin, dust::DustPlugin, shadow::ShadowPlugin, markers::MarkersPlugin))
            .add_plugins(crate::boot::BootJobPlugin::<UnitSpritesJob>::default())
            .add_systems(
                Update,
                (
                    (selection::select_and_order.in_set(UnitInput), look_at_shaman).run_if(not(crate::armageddon::input_locked)),
                    run_ticks.in_set(SimStep),
                    respawn_views,
                    animate_views.in_set(UnitViews),
                )
                    .chain()
                    .in_set(crate::menu::Gameplay),
            )
            .add_systems(PostUpdate, pull_to_eye.before(TransformSystems::Propagate));
    }
}

/// The unit sprites, made at boot (`boot::BootJob`): each kind's art from the original files when allowed, else the
/// open-source sheets, else generated figures, every frame prepared off the main thread, then uploaded.
pub struct UnitSpritesJob;

/// Where the unit art comes from.
pub struct UnitArtSource {
    original: Option<std::path::PathBuf>,
}

/// Every kind's and tribe's frames, prepared AND already packed into their atlas buffers (`pack_tribe_atlases`):
/// `[kind][tribe]`. The packing itself (real pixel-copy work, `atlas::build_atlas`) runs here, in `run()` on the
/// boot job's background thread, same as `prepare_frame`'s own upscale/bleed - keeping it out of `install()`
/// (main thread, `&mut World`) matters: an earlier version of this packed inside `install()` and measured the
/// "Unit sprites installed" boot step at ~880 ms/call instead of ~23 ms, because the main thread was doing the
/// atlas's pixel copying synchronously instead of just registering already-built GPU resources.
pub type PreparedKinds = Vec<Vec<TribeAtlases>>;

impl crate::boot::BootJob for UnitSpritesJob {
    const LABEL: &'static str = "Unit sprites";
    type Input = UnitArtSource;
    type Output = PreparedKinds;

    fn input(world: &World) -> UnitArtSource {
        let levels = world.resource::<LevelList>();
        UnitArtSource { original: levels.original.then(|| levels.data_dir.clone()) }
    }

    fn steps(_: &UnitArtSource) -> u32 {
        1 + 2 * UnitKind::ALL.len() as u32
    }

    fn run(source: UnitArtSource, progress: crate::boot::Progress) -> PreparedKinds {
        let originals = source.original.and_then(|dir| art::Originals::load(&dir).map_err(|e| warn!("original unit sprites: {e}")).ok());
        progress.step();
        UnitKind::ALL
            .iter()
            .map(|&kind| {
                let original = originals.as_ref().and_then(|o| o.art(kind).map_err(|e| warn!("original {kind:?} sprites: {e}")).ok());
                let tribes = original.or_else(|| sheets::sheet_art(kind)).unwrap_or_else(|| {
                    info!("{kind:?}: generated sprites");
                    art::generated_art(kind)
                });
                progress.step();
                let prepared: Vec<Vec<Vec<Vec<PreparedFrame>>>> = tribes.iter().map(|t| t.poses.iter().map(|dirs| dirs.iter().map(|frames| frames.iter().map(prepare_frame).collect()).collect()).collect()).collect();
                let packed = prepared.into_iter().map(pack_tribe_atlases).collect();
                progress.step();
                packed
            })
            .collect()
    }

    fn install(kinds: PreparedKinds, world: &mut World) {
        world.resource_scope(|world, mut images: Mut<Assets<Image>>| {
            world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                    let kinds = kinds
                        .into_iter()
                        .map(|tribes| tribes.into_iter().map(|ta| upload_tribe_atlases(ta, &mut images, &mut meshes, &mut mats)).collect())
                        .collect();
                    world.resource_mut::<UnitSprites>().kinds = kinds;
                });
            });
        });
    }
}

/// Where one already-prepared frame landed once its tribe's frames were packed (`pack_tribe_atlases`): which of
/// `TribeAtlases::atlases` (0 opaque, 1 blended) and where in it, plus the quad geometry `upload_tribe_atlases`
/// still needs (unaffected by atlas packing).
struct FrameLookup {
    atlas: usize,
    rect: atlas::AtlasRect,
    size: Vec2,
    origin: Vec2,
}

/// One kind+tribe's frames packed into up to two shared atlases (TODO.md "Unit sprites", `atlas::build_atlas`)
/// instead of a distinct image per frame: a plain cutout frame (`Mask`) and a true-alpha one (`Blend`) can't share
/// a `StandardMaterial`'s single `alpha_mode`, so frames split into at most two atlases here (`[0]` opaque, `[1]`
/// blended) rather than one - still a large cut from "one image/material per frame". Pure pixel/index math, no
/// Bevy GPU types, so this can run on the boot job's background thread (`UnitSpritesJob::run`); `upload_tribe_atlases`
/// (main thread, `install`) turns the packed result into actual `Image`/`Material`/`Mesh` assets.
pub struct TribeAtlases {
    /// `[opaque, blended]`; `None` when this tribe has no frame of that kind.
    atlases: [Option<atlas::Atlas>; 2],
    /// `[pose][dir][frame]`.
    lookup: Vec<Vec<Vec<FrameLookup>>>,
}

fn pack_tribe_atlases(poses: Vec<Vec<Vec<PreparedFrame>>>) -> TribeAtlases {
    let shape: Vec<Vec<usize>> = poses.iter().map(|dirs| dirs.iter().map(|fs| fs.len()).collect()).collect();
    let mut flat = Vec::new();
    for (pose_i, dirs) in poses.into_iter().enumerate() {
        for (dir_i, fs) in dirs.into_iter().enumerate() {
            for (frame_i, pf) in fs.into_iter().enumerate() {
                flat.push((pose_i, dir_i, frame_i, pf));
            }
        }
    }
    let mut lookup: Vec<Vec<Vec<Option<FrameLookup>>>> = shape.iter().map(|dirs| dirs.iter().map(|&n| (0..n).map(|_| None).collect()).collect()).collect();
    let mut atlases: [Option<atlas::Atlas>; 2] = [None, None];
    let (blended, opaque): (Vec<_>, Vec<_>) = flat.into_iter().partition(|(_, _, _, pf)| pf.blended);
    for (atlas_i, group) in [opaque, blended].into_iter().enumerate() {
        if group.is_empty() {
            continue;
        }
        let frames: Vec<Frame> = group.iter().map(|(_, _, _, pf)| pf.pixels.clone()).collect();
        let packed = atlas::build_atlas(&frames);
        for ((pose_i, dir_i, frame_i, pf), rect) in group.into_iter().zip(packed.rects.iter()) {
            lookup[pose_i][dir_i][frame_i] = Some(FrameLookup { atlas: atlas_i, rect: *rect, size: pf.size, origin: pf.origin });
        }
        atlases[atlas_i] = Some(packed);
    }
    let lookup = lookup.into_iter().map(|dirs| dirs.into_iter().map(|fs| fs.into_iter().map(|f| f.expect("every frame placed by its blended bucket")).collect()).collect()).collect();
    TribeAtlases { atlases, lookup }
}

/// The main-thread half (`install`): turns one tribe's already-packed atlases into real `Image`/`StandardMaterial`
/// assets (cheap - no pixel copying left to do) and gives each frame its own small UV-mapped `Mesh`, so every
/// `FrameAsset` of the same (kind, tribe, alpha mode) shares one `.image`/`.material` handle - what lets Bevy
/// batch/instance a crowd of the same kind and tribe instead of giving each a distinct bind group.
fn upload_tribe_atlases(ta: TribeAtlases, images: &mut Assets<Image>, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>) -> Vec<Vec<Vec<FrameAsset>>> {
    let handles: Vec<Option<(Handle<Image>, Handle<StandardMaterial>, usize, usize)>> = ta
        .atlases
        .into_iter()
        .enumerate()
        .map(|(i, maybe_atlas)| {
            maybe_atlas.map(|atlas::Atlas { width, height, rgba, .. }| {
                let mut image = Image::new(Extent3d { width: width as u32, height: height as u32, depth_or_array_layers: 1 }, TextureDimension::D2, rgba, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
                image.sampler = ImageSampler::linear();
                let image = images.add(image);
                let material = mats.add(StandardMaterial {
                    base_color_texture: Some(image.clone()),
                    alpha_mode: if i == 1 { AlphaMode::Blend } else { AlphaMode::Mask(0.5) },
                    unlit: true,
                    cull_mode: None,
                    double_sided: true,
                    ..default()
                });
                (image, material, width, height)
            })
        })
        .collect();
    ta.lookup
        .into_iter()
        .map(|dirs| {
            dirs.into_iter()
                .map(|fs| {
                    fs.into_iter()
                        .map(|f| {
                            let (image, material, width, height) = handles[f.atlas].as_ref().expect("a frame referencing an atlas that wasn't packed");
                            let mesh = meshes.add(sprite_quad_uv(f.size, f.origin, f.rect.uv(*width, *height)));
                            FrameAsset { image: image.clone(), mesh, material: material.clone(), size: f.size, origin: f.origin }
                        })
                        .collect()
                })
                .collect()
        })
        .collect()
}

/// A frame ready to upload: upscaled and edge-bled (`prepare_frame`, the only way to make one), with its quad's size
/// and feet in base pixels and whether it is drawn blended.
pub struct PreparedFrame {
    pixels: Frame,
    size: Vec2,
    origin: Vec2,
    blended: bool,
}

/// The CPU half of an upload: pixel art (scale 1) upscaled with Scale2x, edges bled so linear filtering does not
/// darken them; blended when the frame has see-through pixels (alpha sprites, `art::alpha_picture_frame`).
pub fn prepare_frame(f: &Frame) -> PreparedFrame {
    let blended = art::has_partial_alpha(f);
    let steps = if f.scale == 1 { UPSCALE_STEPS } else { 0 };
    let mut big = (0..steps).fold(f.clone(), |g, _| art::scale2x(&g));
    art::bleed_edges(&mut big);
    let s = f.scale as f32;
    PreparedFrame { pixels: big, size: Vec2::new(f.width as f32, f.height as f32) / s, origin: Vec2::new(f.origin.0 as f32, f.origin.1 as f32) / s, blended }
}

/// The GPU half: the image, an unlit quad anchored at the feet, cut out or blended.
pub fn upload_prepared(p: PreparedFrame, images: &mut Assets<Image>, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>) -> FrameAsset {
    let mut image = Image::new(
        Extent3d { width: p.pixels.width as u32, height: p.pixels.height as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        p.pixels.rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::linear();
    let image = images.add(image);
    let material = mats.add(StandardMaterial {
        base_color_texture: Some(image.clone()),
        alpha_mode: if p.blended { AlphaMode::Blend } else { AlphaMode::Mask(0.5) },
        unlit: true,
        cull_mode: None,
        double_sided: true,
        ..default()
    });
    FrameAsset { image, mesh: meshes.add(sprite_quad(p.size, p.origin)), material, size: p.size, origin: p.origin }
}

/// Prepares and uploads a frame at once (`prepare_frame`, `upload_prepared`).
pub fn upload_frame(f: &Frame, images: &mut Assets<Image>, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>) -> FrameAsset {
    upload_prepared(prepare_frame(f), images, meshes, mats)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_ticks(
    time: Res<Time>,
    speed: Res<GameSpeed>,
    mut clock: ResMut<SimClock>,
    mut schedule: ResMut<GameSchedule>,
    mut map: ResMut<CurrentMap>,
    mut patches: ResMut<TerrainPatches>,
    mut glide: ResMut<TerrainGlide>,
    mut drawn: ResMut<SimNow>,
) {
    let due = clock.steps_due(time.delta_secs(), *speed);
    let (mut levelled, mut ran) = (Vec::new(), 0);
    if due > 0 {
        let map = &mut map.bypass_change_detection().0;
        clock.prev_ids = map.units.iter().map(|u| u.id).collect();
        clock.prev = map.units.iter().map(|u| (u.x, u.z)).collect();
        clock.prev_heights = map.units.iter().map(|u| u.motion.height).collect();
        glide.prev = Some(map.terrain.clone());
        while ran < due {
            let now = map.now;
            schedule.0.close_local(now);
            if !schedule.0.ready(now) {
                break;
            }
            let commands = schedule.0.take(now);
            levelled.extend(map.step(&commands));
            ran += 1;
        }
    }
    clock.ran = ran;
    if ran < due {
        clock.stalled();
    }
    drawn.set_if_neq(clock.now(map.0.now));
    if due > 0 {
        // Merged once here, not re-merged every reglide frame in `rebuild_terrain`: an ongoing effect (Volcano's
        // rise, lava flow) can hand back a few hundred adjacent small rects a tick, and this set gets walked by
        // `terrain_mesh::patch` on every one of the ~5 render frames the glide is in-flight for, not just once.
        glide.rects = crate::terrain_texture::merge_dirty_rects(&levelled);
    }
    if !levelled.is_empty() {
        patches.0.extend(levelled);
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
    // Views follow units by index: the dead leaving the map shift them all.
    if !map.is_changed() && existing.iter().count() == map.0.units.len() {
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
            v.spawn((UnitSprite(i), markers::BodyHeight::default(), Mesh3d::default(), MeshMaterial3d::<StandardMaterial>::default(), Transform::default()));
            for slot in 0..markers::SLOTS.len() {
                v.spawn((markers::MarkerSlot { slot, above: 0.0 }, Mesh3d::default(), MeshMaterial3d::<StandardMaterial>::default(), Transform::default(), Visibility::Hidden, bevy::light::NotShadowCaster, crate::hover::NoOutline));
            }
            v.spawn((HealthBar, Mesh3d(bar.clone()), MeshMaterial3d(back.clone()), Transform::from_xyz(0.0, BAR_HEIGHT, 0.0), Visibility::Hidden))
                .with_child((HealthFill(i), Mesh3d(fill.clone()), MeshMaterial3d(mats.add(flat(health_color(1.0)))), Transform::from_xyz(0.0, 0.0, 0.005)));
        });
    }
}

#[allow(clippy::too_many_arguments)]
/// A unit standing in a built building is not seen (one aboard a vehicle is, in it: `riding`) (walking in or out by the door it is, in a
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
    let b = map.building(u.inside?.building)?;
    (b.stage() == game_core::building::Stage::Built).then_some(b)
}

/// Where a unit up a lookout is drawn (map position, height in cells), if it is up one.
fn perched(map: &game_core::map::GameMap, heights: &crate::buildings::ModelHeights, u: &game_core::unit::Unit) -> Option<(Vec2, f32)> {
    let b = standing_in(map, u)?;
    crate::buildings::perch(b, *heights.0.get(&b.id)?)
}

/// How high a jump into or out of a vehicle goes over the straight line, cells.
const HOP: f32 = 0.3;

/// How long one little victory jump takes, seconds.
const VICTORY_HOP_SECS: f32 = 0.6;

/// A winning-side unit's small happy jump once the match has ended (`start`: `GameMap::outcome.at`) — our own
/// invented stand-in for a "victory lap", since no such animation is traced in the original's identified sprites.
/// Loops every `VICTORY_HOP_SECS` on the same arc a vehicle boarding jump uses (`HOP`), each unit's own phase
/// offset by its stable id (`sim_time::seed`, never its list index) so they don't all hop in lockstep. Zero before
/// `start`.
fn victory_bounce(now: SimNow, start: Option<game_core::time::Tick>, id: u32) -> f32 {
    let Some(start) = start else { return 0.0 };
    let period = game_core::time::Ticks::new((VICTORY_HOP_SECS / TICK_SECS).round() as u32);
    let t = (now.secs_in(start, period) / VICTORY_HOP_SECS + crate::sim_time::seed(id)).fract();
    HOP * 4.0 * t * (1.0 - t)
}

/// `victory_bounce`'s `start`, gated to the winning side (`GameMap::on_winning_side`): nobody on the losing side
/// gets a happy jump.
fn bounce_start(map: &game_core::map::GameMap, owner: u8) -> Option<game_core::time::Tick> {
    map.on_winning_side(owner).then(|| map.outcome.map(|o| o.at)).flatten()
}

/// How far through a timed jump `left` the drawn moment is, 0..1 (one tick behind, gliding with `alpha`).
fn jump_progress(left: game_core::time::Countdown, alpha: f32) -> f32 {
    let total = left.total().get().max(1) as f32;
    ((left.done().get() as f32 - 1.0 + alpha) / total).clamp(0.0, 1.0)
}

/// From `to` (cells) back to world point `from`, the short way around the torus, cells.
fn back_to(to: Vec2, from: (u16, u16)) -> Vec2 {
    let cell = WORLD_UNITS_PER_CELL as f32;
    let world = |c: f32| (c * cell) as i64 as u16;
    let d = |a: f32, b: u16| game_core::unit::torus_delta(world(a), b) as f32 / cell;
    Vec2::new(d(to.x, from.0), d(to.y, from.1))
}

/// Where a unit aboard a vehicle, or jumping into or out of one, is drawn: map position (cells) and height over
/// the ground (render units). Aboard, on its seat (`vehicles::seat_offset`) at the vehicle's deck; boarding, on an
/// arc from where it stood to its seat; getting off, on an arc down from the vehicle to where it stands. `at` is
/// where the unit glides (`SimClock::cell_pos`).
pub fn riding(map: &game_core::map::GameMap, u: &game_core::unit::Unit, at: Vec2, alpha: f32, height_scale: f32) -> Option<(Vec2, f32)> {
    use game_core::unit::Action;
    let arc = |t: f32| HOP * 4.0 * t * (1.0 - t);
    if let Action::Alighting { from, height, left } = u.action {
        let t = jump_progress(left, alpha);
        let ground = map.terrain.height_at(u.x as u32, u.z as u32, WORLD_UNITS_PER_CELL);
        let start = (height as i32 - ground).max(0) as f32 * height_scale;
        return Some((at + back_to(at, from) * (1.0 - t), start * (1.0 - t) + arc(t)));
    }
    let v = map.vehicle(u.aboard?)?;
    let k = v.passengers.iter().position(|&p| p == u.id).unwrap_or(0);
    let (seat, deck) = (at + crate::vehicles::seat_offset(v, k), crate::vehicles::deck(map, v, height_scale, alpha));
    Some(match u.action {
        Action::Boarding { from, left } => {
            let t = jump_progress(left, alpha);
            (seat + back_to(seat, from) * (1.0 - t), deck * t + arc(t))
        }
        _ => (seat, deck),
    })
}

fn animate_views(
    map: Res<CurrentMap>,
    rig: Res<CameraRig>,
    clock: Res<SimClock>,
    now: Res<SimNow>,
    sprites: Res<UnitSprites>,
    mut views: Query<(&UnitView, &mut Grounded, &mut Transform, &crate::hover::Hoverable), (Without<UnitSprite>, Without<HealthFill>)>,
    mut bodies: Query<(&UnitSprite, &mut markers::BodyHeight, &mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>, &mut Visibility), (Without<HealthFill>, Without<HealthBar>)>,
    mut bars: Query<(&ChildOf, &mut Visibility), (With<HealthBar>, Without<UnitSprite>)>,
    mut fills: Query<(&HealthFill, &mut Transform, &MeshMaterial3d<StandardMaterial>), Without<UnitView>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    selection: Res<Selection>,
    hovered: Res<crate::hover::Hovered>,
    heights: Res<crate::buildings::ModelHeights>,
    marker_art: Option<Res<markers::MarkerArt>>,
    params: Res<CurveParamsRes>,
) {
    let units = &map.0.units;
    let bars_on = !marker_art.is_some_and(|a| a.original);
    let facing_camera = Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch);
    for (view, mut ground, mut t, _) in &mut views {
        let Some(u) = units.get(view.0) else { continue };
        let at = clock.cell_pos(view.0, u);
        ground.at = perched(&map.0, &heights, u).or_else(|| riding(&map.0, u, at, clock.alpha(), params.0.height_scale)).map_or(at, |p| p.0);
        t.rotation = facing_camera;
    }
    let shamans = worship::shamans(units);
    for (body, mut height, mut mesh, mut mat, mut vis) in &mut bodies {
        let hidden = units.get(body.0).is_some_and(|u| hidden_inside(&map.0, u));
        vis.set_if_neq(if hidden { Visibility::Hidden } else { Visibility::Inherited });
        let frame = units.get(body.0).and_then(|u| {
            let worship = worship::worship_facing(u, shamans.get(u.owner as usize).copied().flatten());
            let still = !u.aboard.and_then(|v| map.0.vehicle(v)).is_some_and(crate::vehicles::under_way);
            sprites.frame_for(u, worship, still, rig.yaw, &clock, *now, map.0.on_winning_side(u.owner))
        });
        if let Some(f) = frame {
            height.0 = f.size.y * PIXEL;
        }
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
        let shown = bars_on && unit.is_some_and(|u| u.is_alive() && (selection.contains(u.id) || under_mouse));
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

/// Pulls each unit's sprite, health bar and markers towards the camera (see `toward_eye`), in the view's
/// own (camera-facing) space, and lifts them off the ground (`drawn_lift`).
fn pull_to_eye(
    map: Res<CurrentMap>,
    rig: Res<CameraRig>,
    params: Res<CurveParamsRes>,
    clock: Res<SimClock>,
    now: Res<SimNow>,
    eye: Query<&Transform, (With<GameCamera>, Without<UnitView>)>,
    views: Query<(&UnitView, &Grounded, &Transform, &Children)>,
    mut parts: Query<(&mut Transform, Has<HealthBar>, Option<&markers::MarkerSlot>), (Or<(With<UnitSprite>, With<HealthBar>, With<markers::MarkerSlot>)>, Without<UnitView>, Without<GameCamera>)>,
    heights: Res<crate::buildings::ModelHeights>,
    sizes: Res<crate::object_scale::ObjectScale>,
) {
    use crate::object_scale::ObjectGroup;
    let Ok(eye) = eye.single() else { return };
    for (unit, ground, view, children) in &views {
        let Some(feet) = render_pos(&map.0.terrain, ground, rig.focus, &params.0) else { continue };
        let u = map.0.units.get(unit.0);
        // Up a lookout: on its platform, pulled further so the building around him never hides him.
        let perch = u.and_then(|u| perched(&map.0, &heights, u)).map(|p| Vec3::Y * p.1);
        // Aboard: at the deck of a hull drawn at the buildings' scale, from a view drawn at the units' (its
        // children's offsets are scaled with it).
        let riding = u.and_then(|u| riding(&map.0, u, clock.cell_pos(unit.0, u), clock.alpha(), params.0.height_scale)).map(|p| Vec3::Y * p.1 * sizes.get(ObjectGroup::Buildings));
        let up = perch.or(riding);
        // Aboard, not pulled at all: its hull's near side hides it as it should (seen inside, not in front).
        let pull = if perch.is_some() { PERCH_PULL } else if riding.is_some() { 0.0 } else { PULL_TO_EYE };
        let (offset, scale) = toward_eye(feet + up.unwrap_or_default(), eye.translation, pull);
        let lift = u.map_or(0.0, |u| clock.lift(unit.0, u, &map.0.terrain, params.0.height_scale));
        let bounce = u.map_or(0.0, |u| victory_bounce(*now, bounce_start(&map.0, u.owner), u.id.raw()));
        let shown = perch.or(riding.map(|r| r / sizes.get(ObjectGroup::Units))).unwrap_or_default();
        let local = view.rotation.inverse() * (offset + shown) + Vec3::Y * (lift + bounce) * scale;
        for &child in children {
            if let Ok((mut t, bar, marker)) = parts.get_mut(child) {
                let above = if bar { BAR_HEIGHT } else { marker.map_or(0.0, |m| m.above) };
                let translation = local + Vec3::Y * above * scale;
                let scale = Vec3::splat(scale);
                if t.translation != translation {
                    t.translation = translation;
                }
                if t.scale != scale {
                    t.scale = scale;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::time::{Tick, Ticks};
    use game_core::unit::Action;

    #[test]
    fn people_aboard_stand_on_the_deck_and_jump_in_from_where_they_stood() {
        use game_core::vehicle::{Vehicle, VehicleKind};
        let mut map = game_core::map::GameMap::sandbox_vehicles();
        let at = (60 * 512 + 256, 60 * 512 + 256);
        let v = map.add_vehicle(Vehicle::new(VehicleKind::Balloon, 0, at, game_core::angle::Angle::ZERO));
        let id = map.spawn(0, UnitKind::Brave, at).id;
        map.unit_mut(id).unwrap().aboard = Some(v);
        map.vehicles.get_mut(v).unwrap().seat(id, 0);
        let cells = Vec2::new(60.5, 60.5);
        let deck = crate::vehicles::deck(&map, map.vehicle(v).unwrap(), 0.01, 0.0);
        let (pos, h) = riding(&map, map.unit(id).unwrap(), cells, 0.0, 0.01).unwrap();
        assert_eq!(h, deck, "on the basket's floor");
        assert!((pos - cells).length() < 0.1, "on its seat");
        let from = (58 * 512 + 256, 60 * 512 + 256);
        map.unit_mut(id).unwrap().action = Action::Boarding { from, left: game_core::time::Countdown::new(Ticks::new(7)) };
        let (pos, h) = riding(&map, map.unit(id).unwrap(), cells, 0.0, 0.01).unwrap();
        assert!((pos - Vec2::new(58.5, 60.5)).length() < 0.1 && h == 0.0, "the jump starts where it stood");
        assert_eq!(riding(&map, &map.units[0], cells, 0.0, 0.01), None, "on foot");
    }

    #[test]
    fn victory_bounce_is_zero_before_the_match_ends_then_loops_within_its_arc() {
        let at = |t: u32| SimNow { tick: Tick::from_wire(t), frac: 0.0 };
        assert_eq!(victory_bounce(at(100), None, 1), 0.0, "no match ended yet");
        let start = Tick::from_wire(100);
        let h = victory_bounce(at(130), Some(start), 1);
        assert!((0.0..=HOP).contains(&h), "{h} within the jump's own arc");
        let period_ticks = (VICTORY_HOP_SECS / TICK_SECS).round() as u32;
        assert_eq!(h, victory_bounce(at(130 + period_ticks), Some(start), 1), "loops every VICTORY_HOP_SECS");
        assert_ne!(
            victory_bounce(at(100), Some(start), 1),
            victory_bounce(at(100), Some(start), 2),
            "different units are out of phase (not guaranteed never equal by height, but true for these seeds)"
        );
    }

    #[test]
    fn bounce_start_is_none_for_the_losing_side() {
        let mut map = game_core::map::GameMap::generate(1);
        map.declare_outcome(Some(0), game_core::outcome::Reason::Eliminated);
        assert_eq!(bounce_start(&map, 0), Some(map.outcome.unwrap().at), "the winner celebrates");
        assert_eq!(bounce_start(&map, 1), None, "the loser doesn't");
    }

    #[test]
    fn a_jump_goes_through_its_turns_one_tick_behind() {
        let mut left = game_core::time::Countdown::new(Ticks::new(4));
        assert_eq!(jump_progress(left, 0.5), 0.0);
        left.tick();
        assert_eq!(jump_progress(left, 0.5), 0.125);
        (0..3).for_each(|_| _ = left.tick());
        assert_eq!(jump_progress(left, 1.0), 1.0);
    }

    #[test]
    fn no_glide_from_a_unit_gone_from_the_map() {
        let unit = |id, x| Unit::new(game_core::id::Id::from_raw(id), 0, UnitKind::Brave, (x, 0));
        let clock = SimClock { prev_ids: vec![game_core::id::Id::from_raw(1), game_core::id::Id::from_raw(2)], prev: vec![(0, 0), (512, 0)], acc: TICK_SECS / 2.0, ..default() };
        assert_eq!(clock.cell_pos(1, &unit(2, 768)), Vec2::new(1.25, 0.0), "glides");
        assert_eq!(clock.cell_pos(0, &unit(2, 768)), Vec2::new(1.5, 0.0), "unit 1 left: unit 2 moved to index 0");
    }

    #[test]
    fn orders_issued_reach_the_map_on_the_next_tick_and_wait_while_paused() {
        use bevy::ecs::system::RunSystemOnce;
        use game_core::unit::Order;
        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<SimClock>()
            .init_resource::<SimNow>()
            .init_resource::<GameSchedule>()
            .init_resource::<TerrainPatches>()
            .init_resource::<TerrainGlide>()
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

    /// Frames of `dt` seconds at `speed`, the map's tick following the ticks run: the drawn moment after each.
    fn frames(c: &mut SimClock, now: &mut Tick, dt: f32, speed: GameSpeed, n: usize) -> Vec<SimNow> {
        (0..n)
            .map(|_| {
                *now = *now + Ticks::new(c.steps_due(dt, speed));
                c.now(*now)
            })
            .collect()
    }

    #[test]
    fn the_drawn_moment_stops_while_paused_and_follows_the_speed() {
        let (mut c, mut now) = (SimClock::default(), Tick::ZERO);
        frames(&mut c, &mut now, TICK_SECS * 0.25, GameSpeed::default(), 5);
        let paused = frames(&mut c, &mut now, TICK_SECS * 0.25, GameSpeed { paused: true, times: 1 }, 6);
        assert!(paused.windows(2).all(|w| w[0] == w[1]), "frozen");
        let secs = |d: &SimNow| d.secs_since(Tick::ZERO);
        let gain = |c: &mut SimClock, now: &mut Tick, times| {
            let d = frames(c, now, TICK_SECS * 0.5, GameSpeed { paused: false, times }, 9);
            secs(&d[8]) - secs(&d[0])
        };
        let (mut c1, mut n1, mut c2, mut n2) = (SimClock::default(), Tick::ZERO, SimClock::default(), Tick::ZERO);
        gain(&mut c1, &mut n1, 1);
        gain(&mut c2, &mut n2, 2);
        let (one, two) = (gain(&mut c1, &mut n1, 1), gain(&mut c2, &mut n2, 2));
        assert!((two - 2.0 * one).abs() < 1e-4, "x2 runs twice as far per frame: {one} {two}");
    }

    #[test]
    fn the_drawn_moment_comes_from_the_tick_alone() {
        let (mut stepped, mut a) = (SimClock::default(), Tick::ZERO);
        frames(&mut stepped, &mut a, TICK_SECS, GameSpeed::default(), 4);
        let (mut caught_up, mut b) = (SimClock::default(), Tick::ZERO);
        frames(&mut caught_up, &mut b, TICK_SECS * 4.0, GameSpeed::default(), 1);
        assert_eq!((a, stepped.now(a)), (b, caught_up.now(b)), "four ticks in one frame land where four frames do");
        let walk = Action::Walking { to: (0, 0) };
        assert_eq!(frame_index(Pose::Walk, 8, &walk, stepped.now(a), 0.0), frame_index(Pose::Walk, 8, &walk, caught_up.now(b), 0.0));
        let mut played = SimClock::default();
        frames(&mut played, &mut a, TICK_SECS * 0.7, GameSpeed::default(), 37);
        let mut resynced = SimClock::default();
        resynced.new_game();
        resynced.steps_due(TICK_SECS * (1.0 + played.alpha()), GameSpeed::default());
        let at = Tick::from_wire(500);
        assert!((played.now(at).secs_since(Tick::ZERO) - resynced.now(at).secs_since(Tick::ZERO)).abs() < 1e-4, "a resync draws what the game drew");
    }

    #[test]
    fn a_stall_holds_the_picture() {
        let mut c = SimClock::default();
        c.steps_due(TICK_SECS * 0.9, GameSpeed::default());
        let at = Tick::from_wire(20);
        let before = c.now(at);
        assert_eq!(c.steps_due(TICK_SECS * 0.3, GameSpeed::default()), 1);
        c.stalled();
        assert!(c.now(at).secs_since(Tick::ZERO) >= before.secs_since(Tick::ZERO), "never back");
        assert_eq!(c.now(at), SimNow { tick: at, frac: 0.0 });
    }

    #[test]
    fn a_new_game_clears_the_clock_and_counts() {
        let mut c = SimClock::default();
        c.steps_due(TICK_SECS * 2.5, GameSpeed { paused: false, times: 4 });
        c.new_game();
        assert_eq!((c.game, c.alpha(), c.now(Tick::from_wire(1))), (1, 0.0, SimNow::default()));
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
        let (alpha, drawn) = (c.alpha(), c.now(Tick::ZERO));
        assert_eq!(c.steps_due(5.0, GameSpeed { paused: true, times: 1 }), 0);
        assert_eq!((c.alpha(), c.now(Tick::ZERO), c.ran), (alpha, drawn, 0));
        assert_eq!(c.steps_due(5.0, GameSpeed::default()), MAX_CATCH_UP);
        assert_eq!(c.steps_due(TICK_SECS * 0.5, GameSpeed::default()), 0, "the backlog is gone");
    }

    #[test]
    fn units_inside_hide_unless_walking_through_the_door() {
        use game_core::unit::{Action, Inside, Unit, UnitKind};
        let map = game_core::map::GameMap::sandbox_buildings();
        let built = map.buildings.iter().find(|b| b.stage() == game_core::building::Stage::Built).unwrap();
        let open = map.buildings.iter().find(|b| matches!(b.stage(), game_core::building::Stage::UnderConstruction { .. })).unwrap();
        let mut u = Unit::new(game_core::id::Id::from_raw(1), 0, UnitKind::Brave, (0, 0));
        assert!(!hidden_inside(&map, &u));
        u.inside = Some(Inside { building: built.id, door: (0, 0) });
        assert!(hidden_inside(&map, &u));
        u.action = Action::Entering { to: (1, 1) };
        assert!(!hidden_inside(&map, &u));
        (u.action, u.inside) = (Action::Hammering, Some(Inside { building: open.id, door: (0, 0) }));
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
