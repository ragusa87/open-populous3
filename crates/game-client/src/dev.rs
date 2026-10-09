//! Dev helpers driven by env vars, so screenshots can be taken without a window:
//! `SCREENSHOT=out.png [HEADLESS=1] [AERIAL=1] [SHOT_FRAME=90] [SHAMAN=walk|pray|cast|drown|teleport] game-client [level]`.
//! `SHAMAN` gives the player's shaman an order at start, to check each pose; `BRAVES=cut|carry` sends
//! the player's braves to cut their nearest tree (and, for `carry`, bring the piece back where they stood).
//! `FOCUS=x,z` (cells), `DISTANCE=n`, `PITCH=deg`, `YAW=deg` place the camera for the shot;
//! `TAB=spells|build|stats` opens that panel tab; `BLUEPRINT=kind@x,z` (kind as in the Build tab,
//! e.g. `temple@64,70`) shows that blueprint at map position x,z (cells); `BUILD=kind@x,z` places that
//! plan with every brave of the player sent to it, then runs `BUILD_TICKS` ticks (0 by default).

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

/// Commands for a `BRAVES` demo: every brave of the player (not inside a building) cuts the tree nearest to it; `carry` then
/// brings the piece back where it stood.
pub fn brave_commands(name: &str, map: &game_core::map::GameMap) -> Vec<Command> {
    if !matches!(name, "cut" | "carry") {
        return Vec::new();
    }
    let d = |a: u16, b: u16| (game_core::unit::torus_delta(a, b) as i64).pow(2);
    let braves = map.units.iter().filter(|u| u.owner == PLAYER && u.kind == game_core::unit::UnitKind::Brave && u.inside.is_none());
    braves
        .flat_map(|u| {
            let tree = map.trees.iter().filter(|t| t.size > 0).min_by_key(|t| d(t.x, u.x) + d(t.z, u.z));
            let back = Command::QueueOrder { player: PLAYER, unit: u.id, order: Order::MoveTo { x: u.x, z: u.z } };
            tree.map(|t| Command::OrderUnit { player: PLAYER, unit: u.id, order: Order::CutTree { tree: (t.x, t.z) } })
                .into_iter()
                .chain((name == "carry").then_some(back))
        })
        .collect()
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

/// `kind@x,z`: a Build tab kind by its panel name, spaces and dashes optional (`firewarriorhut`,
/// `campfire`), and a map position in cells.
pub fn parse_blueprint(v: &str) -> Option<(crate::blueprint::Plan, Vec2)> {
    use crate::blueprint::Plan;
    let (name, at) = v.split_once('@')?;
    let squash = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    let building = game_core::build_book::BUILDABLE.into_iter().find(|&k| squash(crate::hud::build::panel_name(k)).starts_with(&squash(name))).map(Plan::Building);
    let plan = building.or_else(|| "campfire".starts_with(&squash(name)).then_some(Plan::Campfire).filter(|_| !name.is_empty()))?;
    let (x, z) = at.split_once(',')?;
    Some((plan, Vec2::new(x.trim().parse().ok()?, z.trim().parse().ok()?)))
}

/// `BUILD` demo: places the plan `kind@x,z` (as `BLUEPRINT`) for the player, sends every brave of
/// theirs to build it, then runs `ticks` ticks; whether it was placed.
pub fn build_demo(map: &mut game_core::map::GameMap, plan: &str, ticks: u32) -> bool {
    let Some((crate::blueprint::Plan::Building(kind), at)) = parse_blueprint(plan) else { return false };
    let Some(place) = crate::blueprint::place_command(map, kind, 0, at) else { return false };
    map.apply(&place);
    let Command::PlaceBuilding { at, .. } = place else { return false };
    let Some(site) = map.building_at_corner(at) else { return false };
    let braves: Vec<u32> = map.units.iter().filter(|u| u.owner == PLAYER && u.kind == game_core::unit::UnitKind::Brave).map(|u| u.id).collect();
    for c in map.build_orders(PLAYER, &braves, site) {
        map.apply(&c);
    }
    for _ in 0..ticks {
        map.tick();
    }
    true
}

/// Panel tab by name (any case).
pub fn tab_index(name: &str) -> Option<usize> {
    crate::hud::TABS.iter().position(|t| t.eq_ignore_ascii_case(name))
}

#[allow(clippy::too_many_arguments)]
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
        let mut cmds = map.0.shaman_of(PLAYER).map(|u| demo_commands(&demo, u)).unwrap_or_default();
        cmds.extend(brave_commands(&std::env::var("BRAVES").unwrap_or_default(), &map.0));
        for c in &cmds {
            dirty.0 |= map.0.apply(c).is_some();
        }
        if let Ok(plan) = std::env::var("BUILD") {
            let ticks = std::env::var("BUILD_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            dirty.0 |= build_demo(&mut map.0, &plan, ticks);
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
    fn brave_demo_sends_every_brave_to_cut_then_carry_back() {
        let mut map = game_core::map::GameMap::sandbox_buildings();
        let before: u32 = map.trees.iter().map(|t| t.size as u32).sum();
        let cmds = brave_commands("carry", &map);
        assert_eq!(cmds.len(), 16, "a cut and a way back for each of the 8 braves outside");
        for c in &cmds {
            map.apply(c);
        }
        for _ in 0..600 {
            map.tick();
        }
        let after: u32 = map.trees.iter().map(|t| t.size as u32).sum();
        assert_eq!(before - after, 8);
        assert!(brave_commands("dance", &map).is_empty());
    }

    #[test]
    fn blueprint_by_name_and_position() {
        use game_core::building::BuildingKind;
        use crate::blueprint::Plan;
        assert_eq!(parse_blueprint("temple@64,70.5"), Some((Plan::Building(BuildingKind::Temple), Vec2::new(64.0, 70.5))));
        assert_eq!(parse_blueprint("Fire-warrior@1,2").map(|b| b.0), Some(Plan::Building(BuildingKind::FirewarriorTraining)));
        assert_eq!(parse_blueprint("camp fire@1,2").map(|b| b.0), Some(Plan::Campfire));
        assert_eq!(parse_blueprint("hut"), None);
    }

    #[test]
    fn build_demo_places_and_runs() {
        let mut map = game_core::map::GameMap::sandbox_buildings();
        let n = map.buildings.len();
        assert!(build_demo(&mut map, "hut@88,64", 50));
        assert!(map.units.iter().any(|u| u.work.is_some()));
        assert_eq!(map.buildings.len(), n + 1);
        assert!(!build_demo(&mut map, "hut@64,64", 0), "on the site");
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
