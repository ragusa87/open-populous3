//! Dev helpers driven by env vars, so screenshots can be taken without a window:
//! `SCREENSHOT=out.png [HEADLESS=1] [AERIAL=1] [SHOT_FRAME=90] [SHAMAN=walk|pray|cast|drown|teleport] game-client [level]`.
//! `SHAMAN` gives the player's shaman an order at start, to check each pose.
//! `FOCUS=x,z` (cells), `DISTANCE=n`, `PITCH=deg`, `YAW=deg` place the camera for the shot;
//! `TAB=spells|build|stats` opens that panel tab; `BLUEPRINT=kind@x,z` (kind as in the Build tab,
//! e.g. `temple@64,70`) shows that blueprint at map position x,z (cells).

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

/// Assets (models) come from the repository's `assets/` folder.
pub fn asset_plugin() -> AssetPlugin {
    AssetPlugin { file_path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets").into(), ..default() }
}

/// DefaultPlugins without winit/window: the camera renders into an offscreen image.
pub fn headless_plugins() -> impl PluginGroup {
    DefaultPlugins
        .set(asset_plugin())
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

/// Commands for a `SHAMAN` demo: walk 3 cells along +x, pray, cast, sink her ground, or teleport
/// 3 cells along +x.
pub fn demo_commands(name: &str, shaman: &Unit) -> Vec<Command> {
    let order = |order| vec![Command::Order { player: PLAYER, order }];
    match name {
        "walk" => order(Order::MoveTo { x: shaman.x.wrapping_add(3 * 512), z: shaman.z }),
        "pray" => order(Order::Pray),
        "cast" => order(Order::Cast),
        "teleport" => vec![Command::Cast { player: PLAYER, spell: Spell::Teleport { to: (shaman.x.wrapping_add(3 * 512), shaman.z) } }],
        "drown" => vec![Command::Cast { player: PLAYER, spell: Spell::Erode { at: shaman.cell() } }; 6],
        _ => Vec::new(),
    }
}

/// Camera overrides from `FOCUS=x,z`, `DISTANCE`, `PITCH` and `YAW` (degrees); bad values are ignored.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShotCamera {
    pub focus: Option<Vec2>,
    pub distance: Option<f32>,
    pub pitch: Option<f32>,
    pub yaw: Option<f32>,
}

impl ShotCamera {
    pub fn parse(get: impl Fn(&str) -> Option<String>) -> Self {
        let num = |k: &str| get(k).and_then(|v| v.trim().parse::<f32>().ok());
        let focus = get("FOCUS").and_then(|v| {
            let (x, z) = v.split_once(',')?;
            Some(Vec2::new(x.trim().parse().ok()?, z.trim().parse().ok()?))
        });
        ShotCamera { focus, distance: num("DISTANCE"), pitch: num("PITCH").map(f32::to_radians), yaw: num("YAW").map(f32::to_radians) }
    }

    fn apply(&self, rig: &mut CameraRig) {
        if let Some(f) = self.focus {
            rig.focus = f;
        }
        rig.distance = self.distance.unwrap_or(rig.distance);
        rig.pitch = self.pitch.unwrap_or(rig.pitch);
        rig.yaw = self.yaw.unwrap_or(rig.yaw);
    }
}

/// `kind@x,z`: a Build tab kind by its panel name, spaces and dashes optional (`firewarriorhut`),
/// and a map position in cells.
pub fn parse_blueprint(v: &str) -> Option<(game_core::building::BuildingKind, Vec2)> {
    let (name, at) = v.split_once('@')?;
    let squash = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    let kind = game_core::build_book::BUILDABLE.into_iter().find(|&k| squash(crate::hud::build::panel_name(k)).starts_with(&squash(name)))?;
    let (x, z) = at.split_once(',')?;
    Some((kind, Vec2::new(x.trim().parse().ok()?, z.trim().parse().ok()?)))
}

/// Panel tab by name (any case).
pub fn tab_index(name: &str) -> Option<usize> {
    crate::hud::TABS.iter().position(|t| t.eq_ignore_ascii_case(name))
}

fn screenshot(
    mut commands: Commands,
    mut frame: Local<u32>,
    mut rig: ResMut<CameraRig>,
    mut map: ResMut<CurrentMap>,
    mut dirty: ResMut<crate::world::TerrainDirty>,
    mut tab: ResMut<crate::hud::ActiveTab>,
    mut blueprint: ResMut<crate::blueprint::Blueprint>,
    mut pinned: ResMut<crate::blueprint::PinnedAt>,
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
        if let Some(i) = std::env::var("TAB").ok().and_then(|t| tab_index(&t)) {
            tab.0 = i;
        }
        if let Some((kind, at)) = std::env::var("BLUEPRINT").ok().and_then(|v| parse_blueprint(&v)) {
            blueprint.pick(kind);
            pinned.0 = Some(at);
        }
        let demo = std::env::var("SHAMAN").unwrap_or_default();
        let cmds = map.0.shaman_of(PLAYER).map(|u| demo_commands(&demo, u)).unwrap_or_default();
        for c in &cmds {
            dirty.0 |= map.0.apply(c).is_some();
        }
    }
    if *frame < shot {
        ShotCamera::parse(|k| std::env::var(k).ok()).apply(&mut rig);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blueprint_by_name_and_position() {
        use game_core::building::BuildingKind;
        assert_eq!(parse_blueprint("temple@64,70.5"), Some((BuildingKind::Temple, Vec2::new(64.0, 70.5))));
        assert_eq!(parse_blueprint("Fire-warrior@1,2").map(|b| b.0), Some(BuildingKind::FirewarriorTraining));
        assert_eq!(parse_blueprint("hut"), None);
    }

    #[test]
    fn tabs_by_name() {
        assert_eq!((tab_index("build"), tab_index("Spells"), tab_index("nope")), (Some(1), Some(0), None));
    }

    #[test]
    fn shot_camera_from_env() {
        let env = |k: &str| match k {
            "FOCUS" => Some("22.5, 101".to_string()),
            "PITCH" => Some("90".to_string()),
            "DISTANCE" => Some("oops".to_string()),
            _ => None,
        };
        let c = ShotCamera::parse(env);
        assert_eq!(c.focus, Some(Vec2::new(22.5, 101.0)));
        assert!((c.pitch.unwrap() - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert_eq!((c.distance, c.yaw), (None, None));
        assert_eq!(ShotCamera::parse(|_| None), ShotCamera::default());
    }
}
