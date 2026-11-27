//! The building discovery cinematic (pop3-rev-analysis.md "Discovery cinematic: the reward flies to the panel",
//! "Building"): the building's model, one piece per face, bursts out from over the vault, flies to the middle of the
//! view growing, its pieces tumbling; they fly back into place, the whole building turns fast, slows and turns back,
//! then its faces peel off one by one and fly onto its tile on the build tab. Drawn by its own orthographic camera
//! over the game and its panel (`DISCOVERY_LAYER`), in logical pixels, unlit and two-sided.

use crate::camera::GameCamera;
use crate::discovery::frame_of;
use crate::hud::{ActiveTab, PANEL_WIDTH};
use crate::original_models::{object_mesh, solid_part, to_mesh, MeshData, OriginalObjects};
use crate::sim_time::SimNow;
use crate::units::PLAYER;
use crate::world::{CurrentMap, LevelList};
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, ScalingMode};
use bevy::prelude::*;
use game_core::building::{BuildingKind, Reward};
use game_core::discovery::Discovery;
use game_core::map::Lcg;
use game_core::time::Tick;
use game_core::tribes::Owner;
use std::f32::consts::TAU;

/// The stages, drawn frames [exe]: out, assembling, shown turning; then to the panel [ours: fixed, the exe's ends
/// about 10 frames after the last face leaves, up to 101].
const OUT: u32 = 19;
const ASSEMBLE: u32 = 23;
const SHOW: u32 = 31;
const TO_PANEL: u32 = 50;
pub const BUILDING_FRAMES: u32 = OUT + ASSEMBLE + SHOW + TO_PANEL;
/// The twinkle on the tile starts 16 frames into the flight to the panel [exe].
pub const TWINKLE_FROM: u32 = OUT + ASSEMBLE + SHOW + 16;
/// Into the flight to the panel: the whole building first, faces peeling until `PEEL_END`, each flying `FACE_FLIGHT`
/// [exe: from frame 12, 10 frames each; ours: spread evenly over the frames left].
const PEEL_FROM: u32 = 12;
const FACE_FLIGHT: u32 = 10;
const PEEL_END: u32 = TO_PANEL - FACE_FLIGHT;
/// How far the pieces fly out, model units (512 a cell) [exe].
const SPREAD: f32 = 700.0;
/// Scale at the start and in the middle (pixels per model unit × 256) [exe].
const SCALE_FROM: f32 = 8.0;
const SCALE_TO: f32 = 40.0;
/// Tilt at the start and in the middle, 2048ths of a turn [exe].
const TILT_FROM: f32 = 0x600 as f32;
const TILT_TO: f32 = 0x7A4 as f32;
/// Spin speeds, 2048ths of a turn a frame [exe].
const SPIN_OUT: f32 = 0x38 as f32;
const SPIN_SHOW: f32 = 182.0;
const SPIN_SHOW_DROP: f32 = 8.0;
const SPIN_PANEL_DROP: f32 = 6.0;
const SPIN_PANEL_MIN: f32 = -273.0;
/// The original's screen height, for its pixel sizes.
const ORIGINAL_HEIGHT: f32 = 480.0;
/// Pieces at most, for the generated models' many triangles (ours).
const MAX_PIECES: usize = 200;
/// The render layer of the cinematic and its camera.
pub const DISCOVERY_LAYER: usize = 3;

fn angle(units: f32) -> f32 {
    units / 2048.0 * TAU
}

/// One face of the model: its middle and the direction it flies out in (cells, model frame), its starting tumble and
/// its yaw rate (2048ths), its index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub centre: Vec3,
    pub out: Vec2,
    pub tumble: [f32; 3],
    pub yaw_rate: f32,
    pub index: u32,
}

impl Piece {
    pub fn new(centre: Vec3, first: Vec3, index: u32, seed: u32) -> Piece {
        let mut rng = Lcg(seed ^ index.wrapping_mul(0x9E37_79B9));
        let out = Vec2::new(first.x, first.z).normalize_or(Vec2::X);
        let tumble = [0; 3].map(|_| (rng.next_u32() % 2048) as f32);
        Piece { centre, out, tumble, yaw_rate: (rng.next_u32() % 34) as f32, index }
    }

    /// Where the piece is (cells, model frame, around the model's origin) and how it is turned about its own middle,
    /// `frame` frames in, until the building is whole.
    pub fn place(&self, frame: f32) -> (Vec3, Quat) {
        let i = self.index as i32;
        let steps = [2 * i.rem_euclid(16) + 2, 2 * (i - 7).rem_euclid(16) + 2, (i + 5).rem_euclid(16) + 1].map(|s| s as f32);
        let (radius, yaw, tumble) = if frame < OUT as f32 {
            let u = frame / OUT as f32;
            (SPREAD * u, self.yaw_rate * frame, [0, 1, 2].map(|k| self.tumble[k] + steps[k] * frame))
        } else {
            let out = [0, 1, 2].map(|k| (self.tumble[k] + steps[k] * OUT as f32).rem_euclid(2048.0));
            let u = ((frame - OUT as f32) / ASSEMBLE as f32).min(1.0);
            (SPREAD * (1.0 - u), self.yaw_rate * OUT as f32 * (1.0 - u), [0, 1, 2].map(|k| out[k] + (2048.0 - out[k]) * u))
        };
        let dir = Vec2::from_angle(angle(yaw)).rotate(self.out);
        let at = self.centre + Vec3::new(dir.x, 0.0, dir.y) * radius / 512.0;
        (at, Quat::from_euler(EulerRot::YXZ, angle(tumble[0]), angle(tumble[1]), angle(tumble[2])))
    }
}

/// The model's spin, 2048ths of a turn, `frame` frames in (between two frames, between their values).
pub fn spin(frame: f32) -> f32 {
    let mut total = 0.0;
    let mut speed;
    let whole = frame.floor() as u32;
    let mut last = 0.0;
    for k in 0..=whole {
        speed = if k < OUT + ASSEMBLE {
            SPIN_OUT
        } else if k < OUT + ASSEMBLE + SHOW {
            SPIN_SHOW - SPIN_SHOW_DROP * (k - OUT - ASSEMBLE) as f32
        } else {
            let end = SPIN_SHOW - SPIN_SHOW_DROP * SHOW as f32;
            (end - SPIN_PANEL_DROP * (k - OUT - ASSEMBLE - SHOW + 1) as f32).max(SPIN_PANEL_MIN)
        };
        if k < whole {
            total += speed;
        } else {
            last = speed;
        }
    }
    total + last * frame.fract()
}

/// The whole model's view `frame` frames in: its middle on screen (logical pixels), pixels per cell, tilt (radians).
pub fn view(frame: f32, start: Vec2, middle: Vec2, px: f32) -> (Vec2, f32, f32) {
    let u = (frame / OUT as f32).clamp(0.0, 1.0);
    let scale = SCALE_FROM + (SCALE_TO - SCALE_FROM) * u;
    (start.lerp(middle, u), 512.0 * scale / 256.0 * px, angle(2048.0 - (TILT_FROM + (TILT_TO - TILT_FROM) * u)))
}

/// The model's turn: tilted towards the camera, spun about its upright axis.
pub fn turn(tilt: f32, spin_units: f32) -> Quat {
    Quat::from_rotation_x(tilt) * Quat::from_rotation_y(angle(spin_units))
}

/// The frame (into the whole cinematic) piece `rank` of `n` leaves for the panel, in the order the turn brings them
/// past the front (ours: spread evenly).
pub fn release(rank: usize, n: usize) -> f32 {
    (OUT + ASSEMBLE + SHOW + PEEL_FROM) as f32 + (PEEL_END - PEEL_FROM) as f32 * rank as f32 / n.max(1) as f32
}

/// The order pieces leave in: by how far the stage-4 turn (backwards) has to carry each past the front from where
/// they are when the peeling starts.
pub fn release_ranks(pieces: &[Piece]) -> Vec<usize> {
    let at = angle(spin((OUT + ASSEMBLE + SHOW + PEEL_FROM) as f32));
    let ahead = |p: &Piece| {
        let a = Vec2::new(p.centre.x, p.centre.z);
        if a == Vec2::ZERO { 0.0 } else { (a.y.atan2(a.x) + at - TAU / 4.0).rem_euclid(TAU) }
    };
    let mut order: Vec<usize> = (0..pieces.len()).collect();
    order.sort_by(|&a, &b| ahead(&pieces[a]).total_cmp(&ahead(&pieces[b])));
    let mut ranks = vec![0; pieces.len()];
    for (rank, &i) in order.iter().enumerate() {
        ranks[i] = rank;
    }
    ranks
}

/// A piece on screen: middle (logical pixels, y down), depth (towards the camera), turn, pixels per cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drawn {
    pub at: Vec2,
    pub depth: f32,
    pub turn: Quat,
    pub scale: f32,
}

/// Piece `p` (leaving at `leaves`) `frame` frames in, from `start` to `middle` and onto `button`.
pub fn draw_piece(p: &Piece, leaves: f32, frame: f32, start: Vec2, middle: Vec2, button: Vec2, px: f32) -> Drawn {
    let on_model = |f: f32| {
        let (centre, scale, tilt) = view(f, start, middle, px);
        let model = turn(tilt, spin(f));
        let (at, tumble) = p.place(f);
        let r = model * at * scale;
        Drawn { at: centre + Vec2::new(r.x, -r.y), depth: r.z, turn: model * tumble, scale }
    };
    if frame < leaves {
        return on_model(frame);
    }
    let from = on_model(leaves);
    let u = ((frame - leaves) / FACE_FLIGHT as f32).min(1.0);
    let faster = Quat::from_rotation_z(u * TAU * 0.75) * Quat::from_rotation_x(u * TAU * 0.5);
    Drawn { at: from.at.lerp(button, u), depth: from.depth, turn: faster * from.turn, scale: from.scale * (1.0 - u) }
}

/// `mesh` cut into pieces of `sizes` triangles each, in order: each piece's mesh around its own middle, its middle,
/// its first corner.
pub fn cut(mesh: &MeshData, sizes: &[usize]) -> Vec<(MeshData, Vec3, Vec3)> {
    let mut out = Vec::new();
    let mut tri = 0;
    let tris = mesh.indices.len() / 3;
    for &n in sizes {
        let range = tri..(tri + n).min(tris);
        tri = range.end;
        if range.is_empty() {
            continue;
        }
        let corners: Vec<usize> = range.flat_map(|t| (0..3).map(move |k| t * 3 + k)).map(|i| mesh.indices[i] as usize).collect();
        let centre = corners.iter().map(|&i| Vec3::from(mesh.positions[i])).sum::<Vec3>() / corners.len() as f32;
        let mut piece = MeshData::default();
        for &i in &corners {
            piece.indices.push(piece.positions.len() as u32);
            piece.positions.push((Vec3::from(mesh.positions[i]) - centre).into());
            piece.normals.push(mesh.normals[i]);
            piece.uvs.push(mesh.uvs[i]);
            if !mesh.colors.is_empty() {
                piece.colors.push(mesh.colors[i]);
            }
        }
        out.push((piece, centre, Vec3::from(mesh.positions[corners[0]])));
    }
    out
}

/// Even groups of triangles, at most `MAX_PIECES` of them, for a model without faces of its own (the generated kit).
pub fn even_sizes(tris: usize) -> Vec<usize> {
    let per = tris.div_ceil(MAX_PIECES).max(1);
    vec![per; tris.div_ceil(per)]
}

/// The cinematic's camera: orthographic, one unit a logical pixel, drawn over the game and its panel, under the
/// menus and the cursor; off while no building flies.
#[derive(Component)]
pub struct DiscoveryCamera;

pub fn spawn_camera(commands: &mut Commands) {
    commands.spawn((
        DiscoveryCamera,
        Camera3d::default(),
        Camera { order: 1, clear_color: ClearColorConfig::None, is_active: false, ..default() },
        Projection::Orthographic(OrthographicProjection { scaling_mode: ScalingMode::WindowSize, near: -4000.0, far: 4000.0, ..OrthographicProjection::default_3d() }),
        Transform::default(),
        RenderLayers::layer(DISCOVERY_LAYER),
    ));
}

/// A building flight on screen: the discovery it shows, where it started, its model's cells per 512 units, its pieces (entity, piece, leaving frame)
/// and the cues already played.
#[derive(Component)]
struct BuildingFlight {
    granted: Tick,
    reward: Reward,
    start: Vec2,
    unit: f32,
    pieces: Vec<(Entity, Piece, f32)>,
    cued: u8,
}

pub struct DiscoveryBuildingPlugin;

impl Plugin for DiscoveryBuildingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (start_and_end, fly, wake_camera).chain().after(crate::units::SimStep));
    }
}

fn building_of(reward: Reward) -> Option<BuildingKind> {
    match reward {
        Reward::Building(k) => Some(k),
        _ => None,
    }
}

/// The building's model for the player: its pieces (meshes, middles and first corners, cells), its material (the
/// original's in the player's colours, else the generated kit's) and its cells per 512 model units (the exe scales
/// the raw model, not the building drawn 1.25 times bigger).
type Model = (Vec<(MeshData, Vec3, Vec3)>, Handle<StandardMaterial>, f32);

fn model(kind: BuildingKind, levels: &LevelList, objects: &OriginalObjects, theme: u8, images: &mut Assets<Image>, mats: &mut Assets<StandardMaterial>) -> Option<Model> {
    let owner = Owner::Tribe(PLAYER);
    let unlit = |texture: Handle<Image>, alpha| StandardMaterial { base_color_texture: Some(texture), unlit: true, cull_mode: None, alpha_mode: alpha, ..default() };
    if let Some(original) = crate::buildings::original_material(levels, theme, images, mats)
        && let Some(obj) = objects.0.as_ref().and_then(|bank| bank.get(crate::buildings::building_object(kind, owner)?))
    {
        let solid = solid_part(obj);
        let sizes: Vec<usize> = solid.faces.iter().map(|f| f.points.len().saturating_sub(2)).collect();
        let texture = mats.get(&original)?.base_color_texture.clone()?;
        return Some((cut(&object_mesh(&solid, PLAYER), &sizes), mats.add(unlit(texture, AlphaMode::Mask(0.5))), obj.cells_per_unit() * 512.0));
    }
    let body = crate::generated_buildings::body(kind, owner)?;
    let sizes = even_sizes(body.indices.len() / 3);
    Some((cut(&body, &sizes), mats.add(unlit(images.add(crate::generated_buildings::surface_image()), AlphaMode::Opaque)), 1.0))
}

#[allow(clippy::too_many_arguments)]
fn start_and_end(
    mut commands: Commands,
    map: Res<CurrentMap>,
    now: Res<SimNow>,
    flights: Query<(Entity, &BuildingFlight)>,
    starts: crate::discovery::StartPoints,
    (levels, objects): (Res<LevelList>, Res<OriginalObjects>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    let mine: Vec<&Discovery> = map.0.discoveries.iter().filter(|d| d.owner == PLAYER && frame_of(d, *now, BUILDING_FRAMES).is_some()).collect();
    for (e, f) in &flights {
        if !mine.iter().any(|d| d.granted == f.granted && d.reward == f.reward) {
            for &(piece, ..) in &f.pieces {
                commands.entity(piece).despawn();
            }
            commands.entity(e).despawn();
        }
    }
    for d in mine {
        if flights.iter().any(|(_, f)| d.granted == f.granted && d.reward == f.reward) {
            continue;
        }
        let Some(kind) = building_of(d.reward) else { continue };
        let Some(start) = starts.of(d) else { continue };
        let theme = map.0.theme.unwrap_or(crate::buildings::DEFAULT_THEME);
        let Some((parts, material, unit)) = model(kind, &levels, &objects, theme, &mut images, &mut mats) else { continue };
        let seed = d.granted.to_wire();
        let pieces: Vec<Piece> = parts.iter().enumerate().map(|(i, (_, centre, first))| Piece::new(*centre, *first, i as u32, seed)).collect();
        let ranks = release_ranks(&pieces);
        let n = pieces.len();
        let entities: Vec<(Entity, Piece, f32)> = parts.into_iter().zip(pieces).zip(ranks).map(|(((part, ..), piece), rank)| {
            let e = commands.spawn((Mesh3d(meshes.add(to_mesh(part))), MeshMaterial3d(material.clone()), Transform::from_scale(Vec3::ZERO), RenderLayers::layer(DISCOVERY_LAYER))).id();
            (e, piece, release(rank, n))
        }).collect();
        commands.spawn(BuildingFlight { granted: d.granted, reward: d.reward, start, unit, pieces: entities, cued: 0 });
    }
}

/// The camera draws only while a building flies (an idle one still costs its passes and slows the first frames).
fn wake_camera(flights: Query<(), With<BuildingFlight>>, mut camera: Query<&mut Camera, With<DiscoveryCamera>>) {
    let active = !flights.is_empty();
    for mut c in &mut camera {
        if c.is_active != active {
            c.is_active = active;
        }
    }
}

/// The cues of the building flight, by the frame they play at [exe]: 0xCC and 0xCB at the start, 0xCE from the
/// first stage (ours: never stopped, the exe stops it 24 frames into the flight to the panel), 0xCD as it leaves for
/// the panel.
fn cues(frame: f32) -> u8 {
    let mut fired = 0b011;
    if frame >= (OUT + ASSEMBLE + SHOW) as f32 {
        fired |= 0b100;
    }
    fired
}

fn fly(
    map: Res<CurrentMap>,
    now: Res<SimNow>,
    mut flights: Query<&mut BuildingFlight>,
    mut pieces: Query<&mut Transform>,
    tiles: crate::hud::build::BuildTiles,
    camera: Query<&Camera, With<GameCamera>>,
    mut tab: ResMut<ActiveTab>,
    mut sounds: MessageWriter<crate::audio::PlaySound>,
) {
    use crate::audio::{PlaySound, SoundCue};
    let Some(size) = camera.single().ok().and_then(|c| c.logical_viewport_size()) else { return };
    let middle = Vec2::new((PANEL_WIDTH + size.x) / 2.0, size.y / 2.0);
    for mut f in &mut flights {
        let Some(kind) = building_of(f.reward) else { continue };
        let Some(d) = map.0.discoveries.iter().find(|d| d.granted == f.granted && d.reward == f.reward) else { continue };
        let Some(frame) = frame_of(d, *now, BUILDING_FRAMES) else { continue };
        let fired = cues(frame) & !f.cued;
        for (bit, cue) in [(0b001, SoundCue::DiscoveryBuildingPop), (0b001, SoundCue::DiscoveryBuildingPath), (0b001, SoundCue::DiscoveryBuildingCircle), (0b100, SoundCue::DiscoveryBuildingEnd)] {
            if fired & bit != 0 {
                sounds.write(PlaySound::flat(cue));
            }
        }
        f.cued |= fired;
        if frame >= (OUT + ASSEMBLE + SHOW) as f32 {
            tab.set_if_neq(ActiveTab(1));
        }
        let button = tiles.centre(kind).unwrap_or(Vec2::new(PANEL_WIDTH / 2.0, size.y / 2.0));
        let px = size.y / ORIGINAL_HEIGHT / f.unit.max(0.01);
        for &(e, piece, leaves) in &f.pieces {
            let Ok(mut t) = pieces.get_mut(e) else { continue };
            let d = draw_piece(&piece, leaves, frame, f.start, middle, button, px);
            *t = Transform { translation: Vec3::new(d.at.x - size.x / 2.0, size.y / 2.0 - d.at.y, d.depth), rotation: d.turn, scale: Vec3::splat(d.scale) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: Vec2 = Vec2::new(900.0, 500.0);
    const MIDDLE: Vec2 = Vec2::new(700.0, 360.0);
    const BUTTON: Vec2 = Vec2::new(60.0, 300.0);

    fn piece(i: u32) -> Piece {
        Piece::new(Vec3::new(0.5, 0.3, -0.2), Vec3::new(0.6, 0.0, -0.3), i, 7)
    }

    #[test]
    fn the_pieces_fly_out_then_back_into_place_whole() {
        let p = piece(3);
        let (at, _) = p.place(OUT as f32);
        assert!((at - p.centre).length() > 1.3, "spread out 700 units");
        let (at, tumble) = p.place((OUT + ASSEMBLE) as f32);
        assert!((at - p.centre).length() < 1e-4);
        assert!(tumble.angle_between(Quat::IDENTITY) < 1e-3, "every tumble back to a whole turn");
    }

    #[test]
    fn it_grows_from_the_vault_to_the_middle() {
        let (at, scale, tilt) = view(0.0, START, MIDDLE, 1.5);
        assert_eq!(at, START);
        assert!((scale - 512.0 * 8.0 / 256.0 * 1.5).abs() < 1e-3);
        assert!((tilt - TAU / 4.0).abs() < 1e-4, "from above");
        let (at, scale, _) = view(OUT as f32, START, MIDDLE, 1.5);
        assert_eq!((at, scale), (MIDDLE, 512.0 * 40.0 / 256.0 * 1.5));
    }

    #[test]
    fn it_turns_fast_slows_and_turns_back() {
        let shown = (OUT + ASSEMBLE) as f32;
        assert_eq!(spin(shown), SPIN_OUT * shown);
        let peak = shown + 23.0;
        assert!(spin(peak) > spin(shown) && spin(peak) > spin(shown + SHOW as f32), "speed 182 down to -66 a frame");
        assert!(spin(BUILDING_FRAMES as f32) < spin((OUT + ASSEMBLE + SHOW) as f32));
        assert!((spin(10.5) - (spin(10.0) + spin(11.0)) / 2.0).abs() < 1e-3);
    }

    #[test]
    fn every_face_leaves_in_time_to_reach_the_tile_and_shrinks_to_nothing() {
        let pieces: Vec<Piece> = (0..40).map(|i| Piece::new(Vec3::new((i as f32).cos(), 0.2, (i as f32).sin()), Vec3::X, i, 1)).collect();
        let ranks = release_ranks(&pieces);
        let mut sorted = ranks.clone();
        sorted.sort();
        assert_eq!(sorted, (0..40).collect::<Vec<_>>());
        for (p, &rank) in pieces.iter().zip(&ranks) {
            let leaves = release(rank, pieces.len());
            assert!(leaves + FACE_FLIGHT as f32 <= BUILDING_FRAMES as f32);
            let end = draw_piece(p, leaves, BUILDING_FRAMES as f32, START, MIDDLE, BUTTON, 1.5);
            assert!(end.at.distance(BUTTON) < 1e-3 && end.scale == 0.0);
        }
    }

    #[test]
    fn a_model_is_cut_one_piece_per_face() {
        let mut m = MeshData::default();
        for k in 0..9 {
            m.indices.push(k);
            m.positions.push([k as f32, 0.0, 0.0]);
            m.normals.push([0.0, 1.0, 0.0]);
            m.uvs.push([0.0; 2]);
        }
        let parts = cut(&m, &[2, 1]);
        assert_eq!(parts.len(), 2);
        assert_eq!((parts[0].0.indices.len(), parts[1].0.indices.len()), (6, 3), "a quad, a triangle");
        assert_eq!(parts[0].1, Vec3::new(2.5, 0.0, 0.0));
        assert_eq!(parts[0].2, Vec3::ZERO, "its first corner");
        assert_eq!(even_sizes(1000).len(), 200);
        assert_eq!(even_sizes(7), vec![1; 7]);
    }
}
