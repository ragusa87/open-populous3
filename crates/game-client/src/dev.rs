//! Dev helpers driven by env vars, so screenshots can be taken without a window:
//! `SCREENSHOT=out.png [HEADLESS=1] [AERIAL=1] game-client [level]`.

use crate::camera::CameraRig;
use bevy::app::ScheduleRunnerPlugin;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::winit::WinitPlugin;
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

fn render_to_image(mut commands: Commands, mut images: ResMut<Assets<Image>>, cams: Query<Entity, With<Camera3d>>) {
    let image = images.add(Image::new_target_texture(1280, 720, TextureFormat::Rgba8UnormSrgb, None));
    for cam in &cams {
        commands.entity(cam).insert((RenderTarget::Image(image.clone().into()), IsDefaultUiCamera));
    }
    commands.insert_resource(OffscreenTarget(image));
}

fn screenshot(
    mut commands: Commands,
    mut frame: Local<u32>,
    mut rig: ResMut<CameraRig>,
    target: Option<Res<OffscreenTarget>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(path) = std::env::var("SCREENSHOT") else { return };
    *frame += 1;
    if *frame == 1 && std::env::var("AERIAL").is_ok() {
        rig.toggle_aerial();
    }
    if *frame == 90 {
        let shot = match target {
            Some(t) => Screenshot::image(t.0.clone()),
            None => Screenshot::primary_window(),
        };
        commands.spawn(shot).observe(save_to_disk(path));
    }
    if *frame == 150 {
        exit.write(AppExit::Success);
    }
}
