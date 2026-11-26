//! Populous-like sandbox. Usage: `game-client [--no-original] [--mute[=on|off]] [--debug-audio] [levlXXXX.dat | levels_dir]`.
//! `--no-original` (or `POP3_NO_ORIGINAL=1`) never reads the original game files.
//! `--mute` (`=on`) does not open the audio device for this run (not saved, unlike the Audio page's "Mute all");
//! `--debug-audio` logs every sound and music request: channel, position, clip, or why it was skipped.

mod angel_meteor;
mod angel_of_death;
mod armageddon;
mod audio;
mod audio_ambience;
mod audio_cues;
mod audio_load;
mod audio_drums;
mod audio_ids;
mod audio_mix;
mod audio_source;
mod audio_space;
mod audio_world;
mod blast_fireball;
mod blast_impact;
mod blueprint;
mod boot;
mod buildings;
mod burning_buildings;
mod campfire;
mod chimney;
mod debug_overlay;
mod explosion;
mod fireball_shot;
mod firebolts;
mod firebrands;
mod camera;
mod camera_debug;
mod cell_debug;
mod construction;
mod convert_ring;
mod cursor_debug;
mod dev;
mod edge_push;
mod editor;
mod effects;
mod fire_flame;
mod flame;
mod flyby;
mod fog;
mod game_end;
mod game_frame;
mod game_speed;
mod generated_buildings;
mod ground_material;
mod ground_shake;
mod grounded;
mod hover;
mod hud;
mod input_lock;
mod keymap;
mod lava;
mod levelling_ring;
mod lightning;
mod menu;
mod nature;
mod object_scale;
mod original_models;
mod procedural_theme;
mod progress_bar;
mod range_ring;
mod sim_time;
mod sites;
mod site_shot;
mod sky;
mod sparkles;
mod ground_fire;
mod burning_trees;
mod smoke;
mod splash;
mod swamp;
mod swarm;
mod terrain_mesh;
mod terrain_texture;
mod tooltip;
mod totems;
mod units;
mod user_settings;
mod video;
mod vault;
mod vehicles;
mod views;
mod volcano_rocks;
mod whirlwind;
mod virtual_cursor;
mod wood;
mod world;

use bevy::prelude::*;
use bevy::window::{MonitorSelection, WindowMode};

fn main() {
    let opts = Options::parse(std::env::args().skip(1), std::env::var("POP3_NO_ORIGINAL").is_ok_and(|v| v == "1"));
    for flag in &opts.unknown {
        eprintln!("unknown option {flag}, ignored");
    }
    let fullscreen = std::env::var("FULLSCREEN").is_ok_and(|v| v == "1");
    let mut app = App::new();
    if dev::headless() {
        app.add_plugins(dev::headless_plugins());
    } else {
        let plugins = DefaultPlugins.set(dev::asset_plugin()).set(WindowPlugin {
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
        });
        if opts.mute {
            app.add_plugins(plugins.disable::<bevy::audio::AudioPlugin>());
        } else {
            app.add_plugins(plugins);
        }
    }
    app
        .add_plugins((user_settings::UserSettingsPlugin, audio_cues::AudioCuesPlugin, audio_world::AudioWorldPlugin, audio_ambience::AudioAmbiencePlugin, audio::SoundPlugin { debug: opts.debug_audio, device_muted: opts.mute, original_dir: opts.use_original.then(pop3_format::install::find_install).flatten().map(|i| pop3_format::install::subdir(&i, "Sound")) }))
        .add_plugins((
            ((world::WorldPlugin { level_arg: opts.level_arg, use_original: opts.use_original },
            ground_material::GroundMaterialPlugin),
            camera::CameraPlugin,
            virtual_cursor::VirtualCursorPlugin,
            grounded::GroundedPlugin,
            sites::SitesPlugin,
            (sky::SkyPlugin, fog::FogPlugin),
            nature::NaturePlugin,
            buildings::BuildingsPlugin,
            campfire::CampfirePlugin,
            flame::FlamePlugin,
            vault::VaultPlugin,
            totems::TotemsPlugin,
            tooltip::TooltipPlugin,
            effects::EffectsPlugin,
            blueprint::BlueprintPlugin),
            units::UnitsPlugin,
            wood::WoodPlugin,
            hover::HoverPlugin,
            editor::EditorPlugin,
            hud::HudPlugin,
            (menu::MenuPlugin, boot::BootPlugin, game_end::GameEndPlugin),
            keymap::KeymapPlugin,
            game_speed::GameSpeedPlugin,
            dev::DevPlugin,
            cursor_debug::CursorDebugPlugin,
            (camera_debug::CameraDebugPlugin, cell_debug::CellDebugPlugin, debug_overlay::DebugOverlayPlugin, fire_flame::FireFlamePlugin, burning_buildings::BurningBuildingsPlugin),
            (object_scale::ObjectScalePlugin, angel_of_death::AngelOfDeathPlugin, angel_meteor::AngelMeteorPlugin),
            (chimney::ChimneyPlugin, fireball_shot::FireballShotPlugin, blast_fireball::BlastFireballPlugin, blast_impact::BlastImpactPlugin, splash::SplashPlugin, flyby::FlybyPlugin, site_shot::SiteShotPlugin),
            (range_ring::RangeRingPlugin, (sparkles::SparklesPlugin, convert_ring::ConvertRingPlugin, levelling_ring::LevellingRingPlugin), lightning::LightningPlugin, smoke::SmokePlugin, ground_fire::GroundFirePlugin, burning_trees::BurningTreesPlugin, (firebolts::FireboltsPlugin, firebrands::FirebrandsPlugin), explosion::ExplosionPlugin, vehicles::VehiclesPlugin, progress_bar::ProgressBarPlugin, armageddon::ArmageddonPlugin, swamp::SwampPlugin, swarm::SwarmPlugin, volcano_rocks::VolcanoRocksPlugin, (lava::LavaPlugin, whirlwind::WhirlwindPlugin)),
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


#[derive(Debug, PartialEq, Default)]
struct Options {
    level_arg: Option<String>,
    use_original: bool,
    /// `--mute[=on]`: no audio device this run.
    mute: bool,
    debug_audio: bool,
    unknown: Vec<String>,
}

impl Options {
    fn parse(args: impl IntoIterator<Item = String>, env_no_original: bool) -> Self {
        let (flags, rest): (Vec<String>, Vec<String>) = args.into_iter().partition(|a| a.starts_with("--"));
        let mut o = Options { level_arg: rest.into_iter().next(), use_original: !env_no_original, ..Options::default() };
        for flag in flags {
            match flag.as_str() {
                "--no-original" => o.use_original = false,
                "--mute" | "--mute=on" => o.mute = true,
                "--mute=off" => o.mute = false,
                "--debug-audio" => o.debug_audio = true,
                _ => o.unknown.push(flag),
            }
        }
        o
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
        assert_eq!(o, Options { level_arg: Some("x.dat".into()), use_original: false, ..Options::default() });
        assert!(Options::parse(args(&[]), false).use_original);
        assert!(!Options::parse(args(&[]), true).use_original, "env var disables too");
    }

    #[test]
    fn audio_flags() {
        assert!(!Options::parse(args(&[]), false).mute, "the device opens by default");
        assert!(Options::parse(args(&["--mute"]), false).mute);
        assert!(Options::parse(args(&["--mute=on"]), false).mute);
        assert!(!Options::parse(args(&["--mute", "--mute=off"]), false).mute, "the last one wins");
        let o = Options::parse(args(&["--debug-audio", "--mute=maybe", "l.dat"]), false);
        assert!(o.debug_audio && !o.mute);
        assert_eq!((o.unknown, o.level_arg), (vec!["--mute=maybe".to_string()], Some("l.dat".into())));
    }
}
