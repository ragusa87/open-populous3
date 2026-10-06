//! Dev helpers driven by env vars, so screenshots can be taken without a window:
//! `SCREENSHOT=out.png [HEADLESS=1] [AERIAL=1] [SHOT_FRAME=90] [SHAMAN=walk|pray|cast|drown] game-client [level]`.
//! `SHAMAN` gives the player's shaman an order at start, to check each pose.

use crate::camera::CameraRig;
use crate::units::PLAYER;
use crate::world::CurrentMap;
use bevy::app::ScheduleRunnerPlugin;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::winit::WinitPlugin;
use game_core::command::Command;
use game_core::spell::Spell;
use game_core::unit::{Order, Unit};
use std::time::Duration;

pub fn headless() -> bool {
    std::env::var("HEADLESS").is_ok_and(|v| v == "1")
}

/// DefaultPlugins without winit/window: the camera renders into an offscreen image.
pub fn headless_plugins() -> impl PluginGroup {
    DefaultPlugins
        .set(WindowPlugin { primary_window: None, exit_condition: bevy::window::ExitCondition::DontExit, ..default() })
        .disable::<WinitPlugin>()
}

pub struct DevPlugin;

impl Plugin for DevPlugin {
    fn build(&self, app: &mut App) {
        if headless() {
            app.add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 60.0)))
                .add_systems(PostStartup, render_to_image);
        }
        app.add_systems(Update, screenshot);
    }
}

#[derive(Resource)]
struct OffscreenTarget(Handle<Image>);

fn render_to_image(mut commands: Commands, mut images: ResMut<Assets<Image>>, cams: Query<Entity, With<Camera>>) {
    let image = images.add(Image::new_target_texture(1280, 720, TextureFormat::Rgba8UnormSrgb, None));
    for cam in &cams {
        commands.entity(cam).insert(RenderTarget::Image(image.clone().into()));
    }
    commands.insert_resource(OffscreenTarget(image));
}

/// Commands for a `SHAMAN` demo: walk 3 cells along +x, pray, cast, or sink her ground.
pub fn demo_commands(name: &str, shaman: &Unit) -> Vec<Command> {
    let order = |order| vec![Command::Order { player: PLAYER, order }];
    match name {
        "walk" => order(Order::MoveTo { x: shaman.x.wrapping_add(3 * 512), z: shaman.z }),
        "pray" => order(Order::Pray),
        "cast" => order(Order::Cast),
        "drown" => vec![Command::Cast { player: PLAYER, spell: Spell::Erode { at: shaman.cell() } }; 6],
        _ => Vec::new(),
    }
}

fn screenshot(
    mut commands: Commands,
    mut frame: Local<u32>,
    mut rig: ResMut<CameraRig>,
    mut map: ResMut<CurrentMap>,
    mut dirty: ResMut<crate::world::TerrainDirty>,
    target: Option<Res<OffscreenTarget>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(path) = std::env::var("SCREENSHOT") else { return };
    let shot = std::env::var("SHOT_FRAME").ok().and_then(|v| v.parse().ok()).unwrap_or(90);
    *frame += 1;
    if *frame == 1 && std::env::var("AERIAL").is_ok() {
        rig.toggle_aerial();
    }
    if *frame == 1 {
        let demo = std::env::var("SHAMAN").unwrap_or_default();
        let cmds = map.0.shaman_of(PLAYER).map(|u| demo_commands(&demo, u)).unwrap_or_default();
        for c in &cmds {
            dirty.0 |= map.0.apply(c).is_some();
        }
    }
    if *frame == shot {
        let shot = match target {
            Some(t) => Screenshot::image(t.0.clone()),
            None => Screenshot::primary_window(),
        };
        commands.spawn(shot).observe(save_to_disk(path));
    }
    if *frame == shot + 60 {
        exit.write(AppExit::Success);
    }
}
