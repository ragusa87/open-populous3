//! Populous-like sandbox. Usage: `game-client [--no-original] [levlXXXX.dat | levels_dir]`.
//! `--no-original` (or `POP3_NO_ORIGINAL=1`) never reads the original game files.

mod blueprint;
mod buildings;
mod campfire;
mod camera;
mod construction;
mod cursor_debug;
mod dev;
mod edge_push;
mod editor;
mod flame;
mod generated_buildings;
mod grounded;
mod hud;
mod menu;
mod nature;
mod original_models;
mod procedural_theme;
mod sites;
mod sky;
mod terrain_mesh;
mod terrain_texture;
mod units;
mod virtual_cursor;
mod wood;
mod world;

use bevy::prelude::*;
use bevy::window::{MonitorSelection, WindowMode};

fn main() {
    let opts = Options::parse(std::env::args().skip(1), std::env::var("POP3_NO_ORIGINAL").is_ok_and(|v| v == "1"));
    let fullscreen = std::env::var("FULLSCREEN").is_ok_and(|v| v == "1");
    let mut app = App::new();
    if dev::headless() {
        app.add_plugins(dev::headless_plugins());
    } else {
        app.add_plugins(DefaultPlugins.set(dev::asset_plugin()).set(WindowPlugin {
            primary_window: Some(Window {
                title: "Open Populous".into(),
                resolution: (1280, 720).into(),
                mode: if fullscreen { WindowMode::BorderlessFullscreen(MonitorSelection::Current) } else { WindowMode::Windowed },
                ..default()
            }),
            primary_cursor_options: Some(bevy::window::CursorOptions {
                grab_mode: bevy::window::CursorGrabMode::Locked,
                visible: false,
                ..default()
            }),
            ..default()
        }));
    }
    app
        .add_plugins((
            (world::WorldPlugin { level_arg: opts.level_arg, use_original: opts.use_original },
            camera::CameraPlugin,
            virtual_cursor::VirtualCursorPlugin,
            grounded::GroundedPlugin,
            sites::SitesPlugin,
            sky::SkyPlugin,
            nature::NaturePlugin,
            buildings::BuildingsPlugin,
            campfire::CampfirePlugin,
            blueprint::BlueprintPlugin),
            units::UnitsPlugin,
            wood::WoodPlugin,
            editor::EditorPlugin,
            hud::HudPlugin,
            menu::MenuPlugin,
            dev::DevPlugin,
            cursor_debug::CursorDebugPlugin,
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


#[derive(Debug, PartialEq)]
struct Options {
    level_arg: Option<String>,
    use_original: bool,
}

impl Options {
    fn parse(args: impl IntoIterator<Item = String>, env_no_original: bool) -> Self {
        let (flags, rest): (Vec<String>, Vec<String>) = args.into_iter().partition(|a| a.starts_with("--"));
        Options {
            level_arg: rest.into_iter().next(),
            use_original: !env_no_original && !flags.iter().any(|f| f == "--no-original"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_flag_anywhere() {
        let o = Options::parse(args(&["x.dat", "--no-original"]), false);
        assert_eq!(o, Options { level_arg: Some("x.dat".into()), use_original: false });
        assert!(Options::parse(args(&[]), false).use_original);
        assert!(!Options::parse(args(&[]), true).use_original, "env var disables too");
    }
}
