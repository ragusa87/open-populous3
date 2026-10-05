//! Populous-like sandbox. Usage: `game-client [levlXXXX.dat | levels_dir]`.

mod camera;
mod dev;
mod editor;
mod hud;
mod terrain_mesh;
mod terrain_texture;
mod world;

use bevy::prelude::*;
use bevy::window::{MonitorSelection, WindowMode};

fn main() {
    let level_arg = std::env::args().nth(1);
    let fullscreen = std::env::var("FULLSCREEN").is_ok_and(|v| v == "1");
    let mut app = App::new();
    if dev::headless() {
        app.add_plugins(dev::headless_plugins());
    } else {
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Open Populous".into(),
                resolution: (1280, 720).into(),
                mode: if fullscreen { WindowMode::BorderlessFullscreen(MonitorSelection::Current) } else { WindowMode::Windowed },
                ..default()
            }),
            ..default()
        }));
    }
    app
        .add_plugins((
            world::WorldPlugin { level_arg },
            camera::CameraPlugin,
            editor::EditorPlugin,
            hud::HudPlugin,
            dev::DevPlugin,
        ))
        .add_systems(Update, toggle_fullscreen)
        .run();
}

fn toggle_fullscreen(keys: Res<ButtonInput<KeyCode>>, mut windows: Query<&mut Window>) {
    if !keys.just_pressed(KeyCode::F11) {
        return;
    }
    for mut w in &mut windows {
        w.mode = match w.mode {
            WindowMode::Windowed => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
            _ => WindowMode::Windowed,
        };
    }
}

