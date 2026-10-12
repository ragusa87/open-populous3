//! Dev helpers driven by env vars, so screenshots can be taken without a window:
//! `SCREENSHOT=out.png [HEADLESS=1] [AERIAL=1] [SHOT_FRAME=90] [SHAMAN=walk|cast|drown|teleport|worship] game-client [level]`.
//! `SHAMAN=worship` sends her to pray at the map's first vault of knowledge; `BRAVES=totem:N` sends
//! every brave of the player to pray at totem N.
//! `SHAMAN` gives the player's shaman an order at start, to check each pose; `BRAVES=cut|carry` sends
//! the player's braves to cut their nearest tree (and, for `carry`, bring the piece back where they stood).
//! `FOCUS=x,z` (cells), `DISTANCE=n`, `PITCH=deg`, `YAW=deg` place the camera for the shot;
//! `TAB=spells|build|stats` opens that panel tab; `BLUEPRINT=kind@x,z` (kind as in the Build tab,
//! e.g. `temple@64,70`) shows that blueprint at map position x,z (cells); `BUILD=kind@x,z` places that
//! plan with every brave of the player sent to it, then runs `BUILD_TICKS` ticks (0 by default).
//! `VAULT=progress:N|granted:T|spent` puts every pyramid of knowledge in that phase.
//! `AIM=spell` (its name without spaces, any case, e.g. `blast`, `landbridge`) selects that spell as if picked in
//! the Spells tab.
//! `VIDEO=secs` instead of `SCREENSHOT`: the same setup, then `secs` seconds recorded from `SHOT_FRAME` straight
//! into `video::VIDEO_PATH` (`video.rs`). The clock steps exactly one video frame per rendered frame (none during
//! loading and the warm-up), so the video plays at true speed.
//! Frames (`SHOT_FRAME`, 10 by default) count from the end of loading (`boot::Loaded`), so captures never include
//! the boot; `SHOT_BOOT=1` counts them from launch instead, to shoot the loading screen.

use crate::camera::CameraRig;
use crate::units::{world_units, PLAYER};
use crate::world::CurrentMap;
use bevy::app::ScheduleRunnerPlugin;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::time::TimeUpdateStrategy;
use bevy::winit::WinitPlugin;
use game_core::command::Command;
use game_core::motion::{Motion, Velocity};
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
        .disable::<bevy::audio::AudioPlugin>()
}

pub struct DevPlugin;

impl Plugin for DevPlugin {
    fn build(&self, app: &mut App) {
        if headless() {
            app.add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 60.0)))
                .add_systems(PostStartup, render_to_image);
        }
        app.init_resource::<crate::video::Video>().add_systems(Update, screenshot);
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
        "cast" => order(Order::Cast),
        "teleport" => vec![Command::Cast { player: PLAYER, spell: Spell::Teleport { to: (shaman.x.wrapping_add(3 * 512), shaman.z) } }],
        "drown" => vec![Command::Cast { player: PLAYER, spell: Spell::Erode { at: shaman.cell() } }; 6],
        _ => Vec::new(),
    }
}

/// `TUMBLE=air|ground`: every unit of the player tumbles, in the air (a cell above the ground, falling: add
/// `GAME_SPEED=0` to hold it there) or on the ground (kept rolling, no friction yet).
pub fn tumble_demo(how: &str, map: &mut game_core::map::GameMap) {
    let terrain = &map.terrain;
    for u in map.units.iter_mut().filter(|u| u.owner == PLAYER && u.is_alive()) {
        let ground = terrain.height_at(u.x as u32, u.z as u32, pop3_format::WORLD_UNITS_PER_CELL);
        u.motion = match how {
            "air" => Motion::flying(Velocity::ZERO, ground + 256),
            "ground" => Motion { velocity: Velocity::new(1, 0, 0), height: None },
            _ => return,
        };
        u.action = game_core::unit::Action::Tumbling;
    }
}

/// `BLAST=x,z` (cells): another tribe's blast there, then `ticks` run at once (with `GAME_SPEED=0`, the shot
/// shows them as they were). False when `at` does not parse.
pub fn blast_demo(map: &mut game_core::map::GameMap, at: &str, ticks: u32) -> bool {
    let Some((x, z)) = at.split_once(',').and_then(|(x, z)| Some((x.trim().parse::<f32>().ok()?, z.trim().parse::<f32>().ok()?))) else { return false };
    let at = world_units(Vec2::new(x, z));
    map.apply(&Command::Cast { player: PLAYER + 1, spell: game_core::spell::Spell::Blast { at } });
    map.run(game_core::time::Ticks::new(ticks));
    true
}

/// `LIGHTNING=x,z` (cells): another tribe's lightning there, then `ticks` run at once (with `GAME_SPEED=0`, the shot
/// shows them as they were). False when `at` does not parse.
pub fn lightning_demo(map: &mut game_core::map::GameMap, at: &str, ticks: u32) -> bool {
    let Some((x, z)) = at.split_once(',').and_then(|(x, z)| Some((x.trim().parse::<f32>().ok()?, z.trim().parse::<f32>().ok()?))) else { return false };
    map.add_effect(game_core::lightning::effect(PLAYER + 1, world_units(Vec2::new(x, z))));
    map.run(game_core::time::Ticks::new(ticks));
    true
}

/// `BRIDGE=x,z` (cells): the player's land bridge from their shaman to there (no charge used, no cast jump), then
/// `ticks` run at once. False when `to` does not parse or the player has no shaman.
pub fn bridge_demo(map: &mut game_core::map::GameMap, to: &str, ticks: u32) -> bool {
    let Some(to) = to.split_once(',').and_then(|(x, z)| Some((x.trim().parse::<i32>().ok()?, z.trim().parse::<i32>().ok()?))) else { return false };
    let Some(from) = map.shaman_of(PLAYER).map(|u| u.cell()) else { return false };
    let bridge = game_core::land_bridge::effect(PLAYER, from, to, &map.terrain);
    map.add_effect(bridge);
    map.run(game_core::time::Ticks::new(ticks));
    true
}

/// `FLATTEN=x,z` (height point): another tribe's flatten there (no cast jump), then `ticks` run at once. False when
/// `at` does not parse.
pub fn flatten_demo(map: &mut game_core::map::GameMap, at: &str, ticks: u32) -> bool {
    let Some(at) = at.split_once(',').and_then(|(x, z)| Some((x.trim().parse::<i32>().ok()?, z.trim().parse::<i32>().ok()?))) else { return false };
    let flatten = game_core::flatten::effect(PLAYER + 1, at, &map.terrain);
    map.add_effect(flatten);
    map.run(game_core::time::Ticks::new(ticks));
    true
}

/// Commands for a `BRAVES` demo: every brave of the player (not inside a building) cuts the tree nearest to it; `carry` then
/// brings the piece back where it stood.
pub fn brave_commands(name: &str, map: &game_core::map::GameMap) -> Vec<Command> {
    if let Some(totem) = name.strip_prefix("totem:").and_then(|n| n.parse().ok()) {
        let braves: Vec<u32> = map.units.iter().filter(|u| u.owner == PLAYER && u.kind == game_core::unit::UnitKind::Brave).map(|u| u.id).collect();
        return map.totem_orders(PLAYER, &braves, totem);
    }
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

/// `progress:N` (the gauge, in `PrayTime` units), `granted:T` (ticks since the reward) or `spent`: a vault phase.
pub fn parse_vault(v: &str) -> Option<game_core::vault::VaultPhase> {
    use game_core::vault::VaultPhase;
    let (name, n) = v.split_once(':').unwrap_or((v, ""));
    match name {
        "progress" => Some(VaultPhase::Praying { progress: n.parse::<u32>().ok()? * game_core::gauge::STEP }),
        "granted" => Some(VaultPhase::Granted { ticks: n.parse().ok()? }),
        "spent" => Some(VaultPhase::Spent),
        _ => None,
    }
}

/// `BUILD` demo: places the plan `kind@x,z` (as `BLUEPRINT`) for the player, sends every brave of
/// theirs outside buildings to build it, then runs `ticks` ticks; whether it was placed.
pub fn build_demo(map: &mut game_core::map::GameMap, plan: &str, ticks: u32) -> bool {
    let Some((crate::blueprint::Plan::Building(kind), at)) = parse_blueprint(plan) else { return false };
    let Some(place) = crate::blueprint::place_command(map, kind, 0, at) else { return false };
    map.apply(&place);
    let Command::PlaceBuilding { at, .. } = place else { return false };
    let Some(site) = map.building_at_corner(at) else { return false };
    let braves: Vec<u32> = map.units.iter().filter(|u| u.owner == PLAYER && u.kind == game_core::unit::UnitKind::Brave && u.inside.is_none()).map(|u| u.id).collect();
    for c in map.build_orders(PLAYER, &braves, site) {
        map.apply(&c);
    }
    for _ in 0..ticks {
        map.tick();
    }
    true
}

/// A spell by its name without spaces, any case.
pub fn spell_by_name(name: &str) -> Option<game_core::spell_book::SpellKind> {
    game_core::spell_book::SpellKind::ALL.into_iter().find(|k| k.name().replace(' ', "").eq_ignore_ascii_case(name))
}

/// Panel tab by name (any case).
pub fn tab_index(name: &str) -> Option<usize> {
    crate::hud::TABS.iter().position(|t| t.eq_ignore_ascii_case(name))
}


/// What the run captures before it exits.
#[derive(Debug, PartialEq)]
pub enum Capture {
    /// One frame to this path (`SCREENSHOT`).
    Shot(String),
    /// This many frames into `video::VIDEO_PATH` (`VIDEO=secs`).
    Video { frames: u32 },
}

impl Capture {
    /// From the environment (`get` reads a variable); None when nothing is to be captured.
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Option<Capture> {
        if let Some(path) = get("SCREENSHOT") {
            return Some(Capture::Shot(path));
        }
        let secs: f32 = get("VIDEO")?.trim().parse().ok()?;
        Some(Capture::Video { frames: (secs * crate::video::FPS as f32).round().max(1.0) as u32 })
    }
}

/// The video frame (from 1) recorded at app frame `frame` when recording `frames` from `start`, None outside.
pub fn video_frame(frame: u32, start: u32, frames: u32) -> Option<u32> {
    (start..start + frames).contains(&frame).then(|| frame - start + 1)
}

#[allow(clippy::too_many_arguments)]
fn screenshot(
    mut commands: Commands,
    mut counted: Local<Option<u32>>,
    mut loaded: MessageReader<crate::boot::Loaded>,
    mut rig: ResMut<CameraRig>,
    mut map: ResMut<CurrentMap>,
    mut dirty: ResMut<crate::world::TerrainDirty>,
    mut tab: ResMut<crate::hud::ActiveTab>,
    mut blueprint: ResMut<crate::blueprint::Blueprint>,
    mut pinned: ResMut<crate::blueprint::PinnedAt>,
    mut aimed: ResMut<crate::hud::spells::SelectedSpell>,
    mut selection: ResMut<crate::units::selection::Selection>,
    target: Option<Res<OffscreenTarget>>,
    mut clock: ResMut<TimeUpdateStrategy>,
    mut video: ResMut<crate::video::Video>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(capture) = Capture::from_env(|k| std::env::var(k).ok()) else { return };
    let shot = std::env::var("SHOT_FRAME").ok().and_then(|v| v.parse().ok()).unwrap_or(10);
    let from_launch = std::env::var("SHOT_BOOT").is_ok_and(|v| v == "1");
    // The state set with `Loaded` takes effect the next frame: count from there, so no frame shows the loading screen.
    let just_loaded = loaded.read().count() > 0 && counted.is_none();
    if just_loaded || from_launch {
        counted.get_or_insert(0);
    }
    let Some(frame) = counted.as_mut().filter(|_| !just_loaded) else {
        if matches!(capture, Capture::Video { .. }) {
            *clock = TimeUpdateStrategy::ManualDuration(Duration::ZERO);
        }
        return;
    };
    *frame += 1;
    if *frame == 1 && std::env::var("AERIAL").is_ok() {
        rig.toggle_aerial();
    }
    if *frame == 1 {
        if let Some(i) = std::env::var("TAB").ok().and_then(|t| tab_index(&t)) {
            tab.0 = i;
        }
        if std::env::var("SELECT").is_ok_and(|v| v == "own") {
            selection.units = map.0.units.iter().filter(|u| u.owner == PLAYER && u.is_alive()).map(|u| u.id).collect();
        }
        if let Some(kind) = std::env::var("AIM").ok().and_then(|n| spell_by_name(&n)) {
            aimed.0 = Some(kind);
        }
        if let Some((kind, at)) = std::env::var("BLUEPRINT").ok().and_then(|v| parse_blueprint(&v)) {
            blueprint.pick(kind);
            pinned.0 = Some(at);
        }
        let demo = std::env::var("SHAMAN").unwrap_or_default();
        if demo == "teleport" {
            map.0.spell_books[PLAYER as usize].set(game_core::spell_book::SpellKind::Teleport, game_core::spell_book::Availability::Unlimited);
        }
        let mut cmds = map.0.shaman_of(PLAYER).map(|u| demo_commands(&demo, u)).unwrap_or_default();
        if let (Some(u), Some(vault), "worship") = (map.0.shaman_of(PLAYER), map.0.buildings.iter().position(|b| b.vault.is_some()), demo.as_str()) {
            cmds.extend(map.0.worship_orders(PLAYER, &[u.id], vault));
        }
        cmds.extend(brave_commands(&std::env::var("BRAVES").unwrap_or_default(), &map.0));
        for c in &cmds {
            dirty.0 |= map.0.apply(c).is_some();
        }
        if let Ok(how) = std::env::var("TUMBLE") {
            tumble_demo(&how, &mut map.0);
        }
        if let Ok(at) = std::env::var("BLAST") {
            let ticks = std::env::var("BLAST_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            blast_demo(&mut map.0, &at, ticks);
        }
        if let Ok(at) = std::env::var("LIGHTNING") {
            let ticks = std::env::var("LIGHTNING_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            lightning_demo(&mut map.0, &at, ticks);
        }
        if let Ok(to) = std::env::var("BRIDGE") {
            let ticks = std::env::var("BRIDGE_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            dirty.0 |= bridge_demo(&mut map.0, &to, ticks);
        }
        if let Ok(at) = std::env::var("FLATTEN") {
            let ticks = std::env::var("FLATTEN_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            dirty.0 |= flatten_demo(&mut map.0, &at, ticks);
        }
        if let Some(phase) = std::env::var("VAULT").ok().and_then(|v| parse_vault(&v)) {
            map.0.buildings.iter_mut().filter_map(|b| b.vault.as_mut()).for_each(|v| v.phase = phase);
        }
        if let Ok(plan) = std::env::var("BUILD") {
            let ticks = std::env::var("BUILD_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            dirty.0 |= build_demo(&mut map.0, &plan, ticks);
        }
    }
    if *frame < shot {
        ShotCamera::parse(|k| std::env::var(k).ok()).apply(&mut rig);
    }
    let grab = || match &target {
        Some(t) => Screenshot::image(t.0.clone()),
        None => Screenshot::primary_window(),
    };
    let last = match capture {
        Capture::Shot(path) => {
            if *frame == shot {
                commands.spawn(grab()).observe(save_to_disk(path));
            }
            shot
        }
        Capture::Video { frames } => {
            let step = if *frame >= shot { Duration::from_secs_f64(1.0 / crate::video::FPS as f64) } else { Duration::ZERO };
            *clock = TimeUpdateStrategy::ManualDuration(step);
            if let Some(n) = video_frame(*frame, shot, frames) {
                commands.spawn(grab()).observe(crate::video::to_video(n));
            }
            if *frame == shot + frames + 60 {
                video.finish();
            }
            shot + frames
        }
    };
    if *frame == last + 60 {
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
    fn vault_phase_from_env() {
        use game_core::vault::VaultPhase;
        assert_eq!(parse_vault("progress:95"), Some(VaultPhase::Praying { progress: 95 * game_core::gauge::STEP }), "in PrayTime units");
        assert_eq!(parse_vault("granted:3"), Some(VaultPhase::Granted { ticks: 3 }));
        assert_eq!(parse_vault("spent"), Some(VaultPhase::Spent));
        assert_eq!((parse_vault("granted"), parse_vault("open")), (None, None));
    }

    #[test]
    fn blast_demo_throws_the_player_up() {
        let mut map = game_core::map::GameMap::sandbox_walk();
        let s = map.units.iter().position(|u| u.owner == PLAYER).unwrap();
        let cell = |v: u16| v as f32 / 512.0;
        let at = format!("{},{}", cell(map.units[s].x), cell(map.units[s].z));
        assert!(blast_demo(&mut map, &at, 2));
        assert!(map.units[s].motion.airborne());
        assert!(!blast_demo(&mut map, "here", 0));
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
        assert_eq!((spell_by_name("landbridge"), spell_by_name("BLAST"), spell_by_name("nope")), (Some(game_core::spell_book::SpellKind::LandBridge), Some(game_core::spell_book::SpellKind::Blast), None));
    }

    #[test]
    fn capture_from_env() {
        let env = |vars: &'static [(&'static str, &'static str)]| move |k: &str| vars.iter().find(|v| v.0 == k).map(|v| v.1.to_string());
        assert_eq!(Capture::from_env(env(&[("SCREENSHOT", "a.png")])), Some(Capture::Shot("a.png".into())));
        assert_eq!(Capture::from_env(env(&[("VIDEO", "2.5")])), Some(Capture::Video { frames: 75 }));
        assert_eq!(Capture::from_env(env(&[("VIDEO", "soon")])), None);
        assert_eq!(Capture::from_env(env(&[])), None);
    }

    #[test]
    fn video_frames_from_the_start_frame() {
        assert_eq!((video_frame(89, 90, 3), video_frame(90, 90, 3), video_frame(92, 90, 3), video_frame(93, 90, 3)), (None, Some(1), Some(3), None));
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
