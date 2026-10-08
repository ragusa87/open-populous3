//! Bakes the unit sprites (docs/specs/unit-art.md, "Baked atlases"): renders every kind's rigged CC0
//! character from `assets/3d/characters/` in each pose, direction and frame, seen from 30 deg above,
//! tribe materials in a magenta key, then writes `assets/units/<kind>.png` (the frames cropped and
//! packed) and `assets/units/<kind>.txt` (their index).
//!
//!   cargo run --release -p unit-baker [-- KIND]

mod atlas;
mod finish;
mod kinds;
mod shots;

use bevy::app::ScheduleRunnerPlugin;
use bevy::camera::{RenderTarget, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::gltf::{Gltf, GltfMaterialName};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use finish::{CELL, FEET};
use kinds::Kind;
use std::f32::consts::FRAC_PI_4;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Image pixels per base pixel, and base pixels from the feet to the top of the head.
const SCALE: f32 = 4.0;
const HEAD_PX: f32 = 34.0;
const ELEVATION_DEG: f32 = 30.0;
const DIRS: usize = 8;
const KEY: Color = Color::srgb(0.75, 0.0, 0.75);

struct Job {
    pose: &'static str,
    dir: usize,
    frame: usize,
}

/// Every pose, direction and frame of a kind, in index order.
fn jobs(kind: &Kind) -> Vec<Job> {
    kind.poses
        .iter()
        .flat_map(|&pose| (0..DIRS).flat_map(move |dir| (0..shots::shot(pose).frames).map(move |frame| Job { pose, dir, frame })))
        .collect()
}

/// The kind being rendered: its scene, camera and captures.
struct Current {
    kind: &'static Kind,
    model: Handle<Gltf>,
    jobs: Vec<Job>,
    captured: Vec<Option<finish::Cropped>>,
    next: usize,
    wait: u32,
    nodes: Vec<AnimationNodeIndex>,
    player: Option<Entity>,
    entities: Vec<Entity>,
}

#[derive(Resource)]
struct Bake {
    out: PathBuf,
    target: Handle<Image>,
    queue: Vec<&'static Kind>,
    current: Option<Current>,
    pending: bool,
}

fn main() {
    let arg = std::env::args().nth(1);
    let kinds = kinds::select(arg.as_deref()).unwrap_or_else(|e| {
        eprintln!("unit-baker [KIND]: {e}");
        std::process::exit(2)
    });
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let models = assets.join("3d/characters").canonicalize().expect("assets/3d/characters");
    let out = assets.join("units");
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
                .set(AssetPlugin { file_path: models.to_string_lossy().into_owned(), ..default() })
                .disable::<WinitPlugin>(),
        )
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 60.0)))
        .insert_resource(ClearColor(Color::NONE))
        .insert_resource(GlobalAmbientLight { brightness: 600.0, ..default() })
        .add_systems(Startup, move |mut commands: Commands, mut images: ResMut<Assets<Image>>| {
            let target = images.add(Image::new_target_texture(CELL.0 as u32, CELL.1 as u32, TextureFormat::Rgba8UnormSrgb, None));
            commands.spawn((DirectionalLight { illuminance: 6000.0, ..default() }, Transform::from_xyz(-3.0, 6.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y)));
            commands.insert_resource(Bake { out: out.clone(), target, queue: kinds.iter().rev().copied().collect(), current: None, pending: false });
        })
        .add_systems(Update, (next_kind, spawn_model, step).chain())
        .run();
}

/// Starts loading the next kind's model once the previous one is written; exits after the last.
fn next_kind(mut bake: ResMut<Bake>, assets: Res<AssetServer>, mut exit: MessageWriter<AppExit>) {
    if bake.current.is_some() {
        return;
    }
    let Some(kind) = bake.queue.pop() else {
        exit.write(AppExit::Success);
        return;
    };
    info!("baking {}", kind.name);
    let jobs = jobs(kind);
    bake.current = Some(Current {
        kind,
        model: assets.load(kind.model),
        captured: jobs.iter().map(|_| None).collect(),
        jobs,
        next: 0,
        wait: 0,
        nodes: Vec::new(),
        player: None,
        entities: Vec::new(),
    });
}

/// Once the glTF is loaded: tribe materials keyed, scene and camera spawned, clips in a graph.
fn spawn_model(mut commands: Commands, mut bake: ResMut<Bake>, gltfs: Res<Assets<Gltf>>, mut graphs: ResMut<Assets<AnimationGraph>>) {
    let target = bake.target.clone();
    let Some(current) = bake.current.as_mut().filter(|c| c.entities.is_empty()) else { return };
    let Some(gltf) = gltfs.get(&current.model) else { return };
    let kind = current.kind;
    let clip = |pose: &str| {
        let name = shots::shot(pose).clip;
        gltf.named_animations.get(name).unwrap_or_else(|| panic!("{}: no clip {name}", kind.model)).clone()
    };
    let (graph, nodes) = AnimationGraph::from_clips(kind.poses.iter().map(|p| clip(p)));
    current.nodes = nodes;
    let graph = graphs.add(graph);
    let skin = kind.skin.map(|hex| Color::Srgba(Srgba::hex(hex).expect("skin colour")));
    let root = commands
        .spawn(WorldAssetRoot(gltf.scenes[0].clone()))
        .observe(
            move |ready: On<WorldInstanceReady>,
                  children: Query<&Children>,
                  players: Query<(), With<AnimationPlayer>>,
                  meshes: Query<(&GltfMaterialName, &MeshMaterial3d<StandardMaterial>)>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  mut commands: Commands,
                  mut bake: ResMut<Bake>| {
                for e in children.iter_descendants(ready.entity) {
                    let Ok((name, mat)) = meshes.get(e) else { continue };
                    let colour = if kind.tribe.contains(&name.0.as_str()) { Some(KEY) } else if name.0 == "Skin" { skin } else { None };
                    let keyed = colour.and_then(|c| materials.get(&mat.0).map(|m| StandardMaterial { base_color: c, ..m.clone() }));
                    if let Some(keyed) = keyed {
                        commands.entity(e).insert(MeshMaterial3d(materials.add(keyed)));
                    }
                }
                let player = children.iter_descendants(ready.entity).find(|e| players.contains(*e));
                if let Some(p) = player {
                    commands.entity(p).insert(AnimationGraphHandle(graph.clone()));
                }
                if let Some(current) = bake.current.as_mut() {
                    current.player = player;
                }
            },
        )
        .id();
    // Orthographic camera from the front, `ELEVATION_DEG` above; feet land on `FEET`.
    let px_per_unit = SCALE * HEAD_PX / kind.head;
    let rot = Quat::from_rotation_x(-ELEVATION_DEG.to_radians());
    let (up, forward) = (rot * Vec3::Y, rot * -Vec3::Z);
    let centre_below_feet = (FEET.1 as f32 - CELL.1 as f32 / 2.0) / px_per_unit;
    let look_at = up * centre_below_feet + Vec3::X * (CELL.0 as f32 / 2.0 - FEET.0 as f32) / px_per_unit;
    let camera = commands
        .spawn((
            Camera3d::default(),
            Camera { clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
            RenderTarget::Image(target.into()),
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::Fixed { width: CELL.0 as f32 / px_per_unit, height: CELL.1 as f32 / px_per_unit },
                ..OrthographicProjection::default_3d()
            }),
            Tonemapping::None,
            Msaa::Off,
            Transform::from_translation(look_at - forward * 30.0).looking_to(forward, up),
        ))
        .id();
    current.entities = vec![root, camera];
}

/// One job at a time: pose and turn the model, let it settle, capture, then the next; after the
/// kind's last job its atlas is written and its scene removed.
fn step(
    mut commands: Commands,
    mut bake: ResMut<Bake>,
    clips: Res<Assets<AnimationClip>>,
    gltfs: Res<Assets<Gltf>>,
    mut players: Query<&mut AnimationPlayer>,
    mut transforms: Query<&mut Transform>,
) {
    if bake.pending {
        return;
    }
    let target = bake.target.clone();
    let out = bake.out.clone();
    let Some(current) = bake.current.as_mut() else { return };
    let (Some(player), Some(&root)) = (current.player, current.entities.first()) else { return };
    if current.next == current.jobs.len() {
        write_atlas(&out, current);
        for &e in &current.entities {
            commands.entity(e).despawn();
        }
        bake.current = None;
        return;
    }
    let job = &current.jobs[current.next];
    let shot = shots::shot(job.pose);
    if current.wait == 0 {
        let Some(duration) = gltfs.get(&current.model).and_then(|g| g.named_animations.get(shot.clip)).and_then(|h| clips.get(h)).map(AnimationClip::duration) else { return };
        let t = shots::frame_time(shot.timing, job.frame, shot.frames, duration, shot.pose);
        let node = current.nodes[current.kind.poses.iter().position(|p| *p == job.pose).expect("pose")];
        if let Ok(mut p) = players.get_mut(player) {
            p.stop_all();
            p.play(node).seek_to(t).pause();
        }
        if let Ok(mut tf) = transforms.get_mut(root) {
            *tf = Transform::from_rotation(Quat::from_rotation_y(job.dir as f32 * FRAC_PI_4)).with_translation(Vec3::Y * -shot.sink * current.kind.head);
        }
    }
    current.wait += 1;
    // The first pose waits longer: the scene's meshes and skin are still being set up.
    if current.wait < if current.next == 0 { 30 } else { 4 } {
        return;
    }
    current.wait = 0;
    let (index, sunk) = (current.next, shot.sink > 0.0);
    bake.pending = true;
    commands.spawn(Screenshot::image(target)).observe(move |shot: On<ScreenshotCaptured>, mut bake: ResMut<Bake>| {
        let mut rgba = shot.image.clone().try_into_dynamic().expect("captured image").to_rgba8().into_raw();
        finish::finish_cell(&mut rgba, sunk);
        if let Some(current) = bake.current.as_mut() {
            current.captured[index] = Some(finish::crop(&rgba));
            current.next += 1;
        }
        bake.pending = false;
    });
}

fn write_atlas(out: &Path, current: &Current) {
    let placed: Vec<atlas::Placed> = current
        .jobs
        .iter()
        .zip(&current.captured)
        .map(|(j, c)| atlas::Placed { pose: j.pose, dir: j.dir, frame: j.frame, image: c.as_ref().expect("every frame captured") })
        .collect();
    let (rgba, height, index) = atlas::build(&placed);
    let name = current.kind.name;
    std::fs::create_dir_all(out).expect("output folder");
    let png = out.join(format!("{name}.png"));
    std::fs::write(&png, atlas::encode_png(&rgba, atlas::WIDTH, height)).expect("atlas PNG");
    std::fs::write(out.join(format!("{name}.txt")), unit_atlas::write(&index)).expect("atlas index");
    info!("wrote {} ({} x {height})", png.display(), atlas::WIDTH);
}
