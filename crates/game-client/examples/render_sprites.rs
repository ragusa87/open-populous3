//! Renders a rigged glTF character into unit sprite sheets (docs/specs/unit-art.md, "Rendered sheets"):
//! one PNG per pose, 8 rows (directions 0-7, see `art::sprite_dir`) of 320 x 288 cells at 4 pixels per
//! base pixel, feet at (160, 256), seen from 30 deg above. Tribe-coloured materials are rendered magenta
//! (the game swaps the hue per tribe), a dark outline is added, edges are hard.
//!
//!   cargo run -p game-client --release --example render_sprites -- MODEL.gltf OUT_DIR [--head 3.1]
//!       [--tribe Clothes,Hat] [--skin d29a6e]
//!
//! --head: model units from the feet to the top of the head (34 base px).
//! --tribe: comma-separated material names drawn in the tribe colour.
//! --skin: sRGB hex colour for the material named "Skin" ("none" keeps the model's; the Quaternius
//! characters ship a near-black skin).

use bevy::app::ScheduleRunnerPlugin;
use bevy::asset::RenderAssetUsages;
use bevy::camera::{RenderTarget, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::gltf::{Gltf, GltfMaterialName};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use bevy::winit::WinitPlugin;
use std::f32::consts::FRAC_PI_4;
use std::path::PathBuf;
use std::time::Duration;

const CELL: (u32, u32) = (320, 288);
const FEET: (f32, f32) = (160.0, 256.0);
/// Image pixels per base pixel, and base pixels from the feet to the top of the head.
const SCALE: f32 = 4.0;
const HEAD_PX: f32 = 34.0;
const ELEVATION_DEG: f32 = 30.0;
const DIRS: usize = 8;
const KEY: Color = Color::srgb(0.75, 0.0, 0.75);
const OUTLINE: [u8; 4] = [30, 18, 10, 255];
const WATER: [u8; 4] = [170, 210, 255, 255];

/// A pose sheet: which clip, how many frames, looped or played once, and how deep it is sunk.
struct PoseShot {
    pose: &'static str,
    clip: &'static str,
    frames: usize,
    /// Frame times: `Loop` spreads them over the clip, `Once` from start to end, `End` is the last
    /// moment only (a held pose), `Span` loops over that fraction of the clip; a `fall` repeats its
    /// last frame (held while dead).
    timing: Timing,
    /// Fraction of the head height under the water line (drowning).
    sink: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum Timing {
    Loop,
    Once,
    End,
    Span(f32, f32),
}

const SHOTS: &[PoseShot] = &[
    PoseShot { pose: "idle", clip: "Idle", frames: 8, timing: Timing::Loop, sink: 0.0 },
    PoseShot { pose: "walk", clip: "Walk", frames: 12, timing: Timing::Loop, sink: 0.0 },
    PoseShot { pose: "pray", clip: "SitDown", frames: 1, timing: Timing::End, sink: 0.0 },
    PoseShot { pose: "cast", clip: "Jump", frames: 12, timing: Timing::Once, sink: 0.0 },
    PoseShot { pose: "fall", clip: "Death", frames: 8, timing: Timing::Once, sink: 0.0 },
    PoseShot { pose: "drown", clip: "RecieveHit", frames: 4, timing: Timing::Loop, sink: 0.45 },
    PoseShot { pose: "stranded", clip: "Victory", frames: 4, timing: Timing::Span(0.22, 0.67), sink: 0.0 },
];

/// Time of frame `k` of `n` in a clip lasting `duration`.
fn frame_time(timing: Timing, k: usize, n: usize, duration: f32, pose: &str) -> f32 {
    match timing {
        Timing::Loop => duration * k as f32 / n as f32,
        Timing::End => duration,
        Timing::Span(from, to) => duration * (from + (to - from) * k as f32 / n as f32),
        Timing::Once if pose == "fall" => duration * (k as f32 / (n - 2) as f32).min(1.0),
        Timing::Once => duration * k as f32 / (n - 1).max(1) as f32,
    }
}

struct Job {
    shot: usize,
    dir: usize,
    frame: usize,
}

#[derive(Resource)]
struct Render {
    model: Handle<Gltf>,
    out: PathBuf,
    head: f32,
    tribe_materials: Vec<String>,
    skin: Option<Color>,
    target: Handle<Image>,
    jobs: Vec<Job>,
    next: usize,
    wait: u32,
    nodes: Vec<AnimationNodeIndex>,
    player: Option<Entity>,
    root: Option<Entity>,
    captured: Vec<Option<Vec<u8>>>,
    pending: bool,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let option = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let model = PathBuf::from(args.get(1).expect("usage: render_sprites MODEL.gltf OUT_DIR [--head 3.1] [--tribe Clothes,Hat] [--skin d29a6e]"));
    let out = PathBuf::from(args.get(2).expect("OUT_DIR"));
    let head: f32 = option("--head").and_then(|v| v.parse().ok()).unwrap_or(3.1);
    let tribe_materials: Vec<String> = option("--tribe").unwrap_or_else(|| "Clothes,Hat".into()).split(',').map(String::from).collect();
    let skin = match option("--skin").as_deref() {
        Some("none") => None,
        Some(hex) => Some(Color::Srgba(Srgba::hex(hex).expect("--skin: hex colour"))),
        None => Some(Color::Srgba(Srgba::hex("d29a6e").unwrap())),
    };
    let dir = model.parent().map(|p| p.canonicalize().expect("model folder")).unwrap_or_default();
    let file = model.file_name().expect("model file").to_string_lossy().into_owned();
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin { primary_window: None, exit_condition: bevy::window::ExitCondition::DontExit, ..default() })
                .set(AssetPlugin { file_path: dir.to_string_lossy().into_owned(), ..default() })
                .disable::<WinitPlugin>(),
        )
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 60.0)))
        .insert_resource(ClearColor(Color::NONE))
        .insert_resource(GlobalAmbientLight { brightness: 600.0, ..default() })
        .add_systems(Startup, move |mut commands: Commands, assets: Res<AssetServer>, mut images: ResMut<Assets<Image>>| {
            let target = images.add(Image::new_target_texture(CELL.0, CELL.1, TextureFormat::Rgba8UnormSrgb, None));
            let jobs = SHOTS.iter().enumerate().flat_map(|(shot, s)| (0..DIRS).flat_map(move |dir| (0..s.frames).map(move |frame| Job { shot, dir, frame }))).collect::<Vec<_>>();
            let captured = jobs.iter().map(|_| None).collect();
            commands.insert_resource(Render {
                model: assets.load(file.clone()),
                out: out.clone(),
                head,
                tribe_materials: tribe_materials.clone(),
                skin,
                target,
                jobs,
                next: 0,
                wait: 0,
                nodes: Vec::new(),
                player: None,
                root: None,
                captured,
                pending: false,
            });
        })
        .add_systems(Update, (spawn_model, step).chain())
        .run();
}

/// Once the glTF is loaded: tribe materials keyed, scene and camera spawned, clips in a graph.
fn spawn_model(
    mut commands: Commands,
    mut render: ResMut<Render>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut done: Local<bool>,
) {
    let Some(gltf) = gltfs.get(&render.model).filter(|_| !*done) else { return };
    *done = true;
    let clips: Vec<Handle<AnimationClip>> = SHOTS.iter().map(|s| gltf.named_animations.get(s.clip).unwrap_or_else(|| panic!("no clip {}", s.clip)).clone()).collect();
    let (graph, nodes) = AnimationGraph::from_clips(clips);
    render.nodes = nodes;
    let graph = graphs.add(graph);
    let root = commands
        .spawn(WorldAssetRoot(gltf.scenes[0].clone()))
        .observe(
            move |ready: On<WorldInstanceReady>,
                  children: Query<&Children>,
                  players: Query<(), With<AnimationPlayer>>,
                  meshes: Query<(&GltfMaterialName, &MeshMaterial3d<StandardMaterial>)>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  mut commands: Commands,
                  mut render: ResMut<Render>| {
            for e in children.iter_descendants(ready.entity) {
                if let Ok((name, mat)) = meshes.get(e) {
                    let colour = if render.tribe_materials.iter().any(|n| *n == name.0) {
                        Some(KEY)
                    } else if name.0 == "Skin" {
                        render.skin
                    } else {
                        None
                    };
                    if let Some(colour) = colour {
                        let keyed = materials.get(&mat.0).map(|m| StandardMaterial { base_color: colour, ..m.clone() });
                        if let Some(keyed) = keyed {
                            commands.entity(e).insert(MeshMaterial3d(materials.add(keyed)));
                        }
                    }
                }
            }
            let player = children.iter_descendants(ready.entity).find(|e| players.contains(*e));
            if let Some(p) = player {
                commands.entity(p).insert(AnimationGraphHandle(graph.clone()));
            }
            render.player = player;
        },
        )
        .id();
    render.root = Some(root);
    // Orthographic camera from the front, `ELEVATION_DEG` above; feet land on `FEET`.
    let px_per_unit = SCALE * HEAD_PX / render.head;
    let rot = Quat::from_rotation_x(-ELEVATION_DEG.to_radians());
    let up = rot * Vec3::Y;
    let forward = rot * -Vec3::Z;
    let centre_below_feet = (FEET.1 - CELL.1 as f32 / 2.0) / px_per_unit;
    let look_at = up * centre_below_feet + Vec3::X * (CELL.0 as f32 / 2.0 - FEET.0) / px_per_unit;
    commands.spawn((
        Camera3d::default(),
        Camera { clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
        RenderTarget::Image(render.target.clone().into()),
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::Fixed { width: CELL.0 as f32 / px_per_unit, height: CELL.1 as f32 / px_per_unit },
            ..OrthographicProjection::default_3d()
        }),
        Tonemapping::None,
        Msaa::Off,
        Transform::from_translation(look_at - forward * 30.0).looking_to(forward, up),
    ));
    commands.spawn((DirectionalLight { illuminance: 6000.0, ..default() }, Transform::from_xyz(-3.0, 6.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y)));
}

/// One job at a time: pose and turn the model, let it settle, capture, then the next; at the end
/// the sheets are written and the app exits.
fn step(
    mut commands: Commands,
    mut render: ResMut<Render>,
    clips: Res<Assets<AnimationClip>>,
    gltfs: Res<Assets<Gltf>>,
    mut players: Query<&mut AnimationPlayer>,
    mut roots: Query<&mut Transform>,
    mut exit: MessageWriter<AppExit>,
) {
    let (Some(player), Some(root)) = (render.player, render.root) else { return };
    if render.pending {
        return;
    }
    if render.next == render.jobs.len() {
        write_sheets(&render);
        exit.write(AppExit::Success);
        return;
    }
    let job = &render.jobs[render.next];
    let shot = &SHOTS[job.shot];
    if render.wait == 0 {
        let Some(gltf) = gltfs.get(&render.model) else { return };
        let Some(duration) = gltf.named_animations.get(shot.clip).and_then(|h| clips.get(h)).map(AnimationClip::duration) else { return };
        let t = frame_time(shot.timing, job.frame, shot.frames, duration, shot.pose);
        let node = render.nodes[job.shot];
        if let Ok(mut p) = players.get_mut(player) {
            p.stop_all();
            p.play(node).seek_to(t).pause();
        }
        if let Ok(mut tf) = roots.get_mut(root) {
            *tf = Transform::from_rotation(Quat::from_rotation_y(job.dir as f32 * FRAC_PI_4)).with_translation(Vec3::Y * -shot.sink * render.head);
        }
    }
    render.wait += 1;
    // The first pose waits longer: the scene's meshes and skin are still being set up.
    if render.wait < if render.next == 0 { 30 } else { 4 } {
        return;
    }
    render.wait = 0;
    render.pending = true;
    let index = render.next;
    commands.spawn(Screenshot::image(render.target.clone())).observe(move |shot: On<ScreenshotCaptured>, mut render: ResMut<Render>| {
        let rgba = shot.image.clone().try_into_dynamic().map(|d| d.to_rgba8().into_raw()).ok();
        render.captured[index] = rgba;
        render.next += 1;
        render.pending = false;
    });
}

/// Hard alpha, a dark outline 2 px wide, and for sunk poses nothing under the water line but ripples.
fn finish_cell(rgba: &mut [u8], sunk: bool) {
    let (w, h) = (CELL.0 as usize, CELL.1 as usize);
    for px in rgba.chunks_exact_mut(4) {
        if px[3] < 128 {
            px.copy_from_slice(&[0; 4]);
        } else {
            px[3] = 255;
        }
    }
    let waterline = FEET.1 as usize;
    if sunk {
        for px in rgba[waterline * w * 4..].chunks_exact_mut(4) {
            px.copy_from_slice(&[0; 4]);
        }
    }
    let src = rgba.to_vec();
    let opaque = |x: i32, y: i32| x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && src[(y as usize * w + x as usize) * 4 + 3] != 0;
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if !opaque(x, y) && (-2..=2).any(|dy| (-2..=2).any(|dx| dx * dx + dy * dy <= 5 && opaque(x + dx, y + dy))) {
                rgba[(y as usize * w + x as usize) * 4..][..4].copy_from_slice(&OUTLINE);
            }
        }
    }
    if sunk {
        for x in (FEET.0 as usize - 70)..(FEET.0 as usize + 70) {
            if (x / 10) % 3 != 2 {
                for y in waterline.saturating_sub(2)..waterline + 2 {
                    rgba[(y * w + x) * 4..][..4].copy_from_slice(&WATER);
                }
            }
        }
    }
}

fn write_sheets(render: &Render) {
    std::fs::create_dir_all(&render.out).expect("output folder");
    let (cw, ch) = (CELL.0 as usize, CELL.1 as usize);
    for (s, shot) in SHOTS.iter().enumerate() {
        let (w, h) = (cw * shot.frames, ch * DIRS);
        let mut sheet = vec![0u8; w * h * 4];
        for (job, cell) in render.jobs.iter().zip(&render.captured).filter(|(j, _)| j.shot == s) {
            let Some(cell) = cell else { continue };
            let mut cell = cell.clone();
            finish_cell(&mut cell, shot.sink > 0.0);
            for y in 0..ch {
                let dst = ((job.dir * ch + y) * w + job.frame * cw) * 4;
                sheet[dst..dst + cw * 4].copy_from_slice(&cell[y * cw * 4..(y + 1) * cw * 4]);
            }
        }
        let image = Image::new(
            Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
            TextureDimension::D2,
            sheet,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        let path = render.out.join(format!("{}.png", shot.pose));
        match image.try_into_dynamic().map(|d| d.save(&path)) {
            Ok(Ok(())) => info!("wrote {}", path.display()),
            Ok(Err(e)) => error!("{}: {e}", path.display()),
            Err(e) => error!("{}: {e:?}", path.display()),
        }
    }
}
