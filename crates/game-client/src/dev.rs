//! Dev helpers driven by env vars, so screenshots can be taken without a window:
//! `SCREENSHOT=out.png [HEADLESS=1] [AERIAL=1] [SHOT_FRAME=90] [SHAMAN=walk|cast|teleport|worship] game-client [level]`.
//! `SHAMAN=worship` sends her to pray at the map's first vault of knowledge; `BRAVES=totem:N` sends
//! every brave of the player to pray at totem N.
//! `SHAMAN` gives the player's shaman an order at start, to check each pose; `BRAVES=cut|carry` sends
//! the player's braves to cut their nearest tree (and, for `carry`, bring the piece back where they stood).
//! `FOCUS=x,z` (cells), `DISTANCE=n`, `PITCH=deg`, `YAW=deg` place the camera for the shot;
//! `TAB=spells|build|stats` opens that panel tab; `BLUEPRINT=kind@x,z` (kind as in the Build tab,
//! e.g. `temple@64,70`) shows that blueprint at map position x,z (cells); `BUILD=kind@x,z` places that
//! plan with every brave of the player sent to it, then runs `BUILD_TICKS` ticks (0 by default).
//! `EDITOR_BRUSH=x,z:radius` turns on the world editor (`editor.rs`) and pins its brush ring at map
//! position x,z (cells) with that radius, for a headless shot of it.
//! `VAULT=progress:N|granted:T|spent` puts every pyramid of knowledge in that phase.
//! `EXPLODE=building:N` blows up building N (its index), as if `EXPLODE_TICKS` ticks ago (0 by default; with
//! `GAME_SPEED=0` the shot holds it there): the explosion is drawn, then the building removed the next frame.
//! `TRAIN=stored/cost[:short]` puts a brave of the player in each of their training units, making something with
//! that much mana stored (`:short`: short of mana).
//! `CREW=building:N` sends every brave of the player on foot into building N (its index), then runs `CREW_TICKS` ticks.
//! `VEHICLE=boat@x,z[:n]` (or `balloon`, several separated by `;`) puts a vehicle of the player at map position x,z
//! (cells) with n new braves aboard (none by default); `VEHICLE_TO=x,z` sends them there; then `VEHICLE_TICKS`
//! ticks run (0 by default).
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
use game_core::unit::{Order, Unit, UnitId};
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
    // SHOT_RESOLUTION=WxH overrides the hardcoded 1280x720, e.g. to compare a screenshot at the
    // original's own period resolution against our default.
    let (w, h) = std::env::var("SHOT_RESOLUTION")
        .ok()
        .and_then(|s| s.split_once('x').map(|(w, h)| (w.parse().ok(), h.parse().ok())).and_then(|(w, h)| Some((w?, h?))))
        .unwrap_or((1280, 720));
    let image = images.add(Image::new_target_texture(w, h, TextureFormat::Rgba8UnormSrgb, None));
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
        _ => Vec::new(),
    }
}

/// `ATTACK=nearest`: each of the player's people that may fight attacks the nearest enemy person it may attack.
pub fn attack_commands(how: &str, map: &game_core::map::GameMap) -> Vec<Command> {
    if how != "nearest" {
        return Vec::new();
    }
    let dist = |a: &game_core::unit::Unit, b: &game_core::unit::Unit| {
        let (dx, dz) = (game_core::unit::torus_delta(a.x, b.x) as i64, game_core::unit::torus_delta(a.z, b.z) as i64);
        dx * dx + dz * dz
    };
    map.units
        .iter()
        .filter(|u| u.owner == PLAYER && u.is_alive())
        .filter_map(|u| {
            let target = map.units.iter().filter(|t| game_core::attack::may_attack(u, t) || game_core::sermon::may_preach(u, t)).min_by_key(|t| dist(u, t))?;
            let order = game_core::unit::Order::Attack { target: game_core::unit::Target::Unit(target.id) };
            Some(Command::OrderUnit { player: PLAYER, unit: u.id, order })
        })
        .collect()
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

/// `EXPLODE=building:N`: building N's explosion, started `ago` ticks before now; None when it does not parse or
/// there is no such building.
pub fn explode_demo(map: &game_core::map::GameMap, what: &str, ago: u32) -> Option<crate::explosion::BuildingExploded> {
    let n: usize = what.strip_prefix("building:")?.trim().parse().ok()?;
    let start = game_core::time::Tick::from_wire(map.now.to_wire().wrapping_sub(ago));
    Some(crate::explosion::BuildingExploded { building: map.buildings.get(n)?.id, start })
}

/// `BLAST=x,z` (cells): another tribe's blast there, then `ticks` run at once (with `GAME_SPEED=0`, the shot
/// shows them as they were). False when `at` does not parse.
pub fn blast_demo(map: &mut game_core::map::GameMap, at: &str, ticks: u32) -> bool {
    let Some((x, z)) = at.split_once(',').and_then(|(x, z)| Some((x.trim().parse::<f32>().ok()?, z.trim().parse::<f32>().ok()?))) else { return false };
    let at = world_units(Vec2::new(x, z));
    map.apply(&Command::Cast { player: PLAYER.next(), spell: game_core::spell::Spell::Blast { at } });
    map.run(game_core::time::Ticks::new(ticks));
    true
}

/// `LIGHTNING=x,z` (cells): another tribe's lightning there, then `ticks` run at once (with `GAME_SPEED=0`, the shot
/// shows them as they were). False when `at` does not parse.
pub fn lightning_demo(map: &mut game_core::map::GameMap, at: &str, ticks: u32) -> bool {
    let Some((x, z)) = at.split_once(',').and_then(|(x, z)| Some((x.trim().parse::<f32>().ok()?, z.trim().parse::<f32>().ok()?))) else { return false };
    map.add_effect(game_core::lightning::effect(PLAYER.next(), world_units(Vec2::new(x, z))));
    map.run(game_core::time::Ticks::new(ticks));
    true
}

/// `SWAMP=x,z` (cells): another tribe's swamp there, then `ticks` run at once (with `GAME_SPEED=0`, before its
/// first kill check). False when `at` does not parse.
pub fn swamp_demo(map: &mut game_core::map::GameMap, at: &str, ticks: u32) -> bool {
    let Some((x, z)) = at.split_once(',').and_then(|(x, z)| Some((x.trim().parse::<f32>().ok()?, z.trim().parse::<f32>().ok()?))) else { return false };
    map.add_effect(game_core::swamp::effect(PLAYER.next(), world_units(Vec2::new(x, z))));
    map.run(game_core::time::Ticks::new(ticks));
    true
}

/// `CONVERT=x,z` (cells): the player's Convert effect there, with a wildman placed next to it to show the
/// conversion, then `ticks` run at once. False when `at` does not parse.
pub fn convert_demo(map: &mut game_core::map::GameMap, at: &str, ticks: u32) -> bool {
    let Some((x, z)) = at.split_once(',').and_then(|(x, z)| Some((x.trim().parse::<f32>().ok()?, z.trim().parse::<f32>().ok()?))) else { return false };
    let at = world_units(Vec2::new(x, z));
    map.spawn(game_core::tribes::Owner::Wild, game_core::unit::UnitKind::Wildman, (at.0.wrapping_add(50), at.1));
    map.add_effect(game_core::convert::effect(PLAYER, at));
    map.run(game_core::time::Ticks::new(ticks));
    true
}

/// `HYPNOTISM=x,z` (cells): the player's Hypnotism effect there, with an enemy placed next to it to show the
/// capture, then `ticks` run at once. False when `at` does not parse.
pub fn hypnotism_demo(map: &mut game_core::map::GameMap, at: &str, ticks: u32) -> bool {
    let Some((x, z)) = at.split_once(',').and_then(|(x, z)| Some((x.trim().parse::<f32>().ok()?, z.trim().parse::<f32>().ok()?))) else { return false };
    let at = world_units(Vec2::new(x, z));
    map.spawn(PLAYER.next(), game_core::unit::UnitKind::Brave, (at.0.wrapping_add(50), at.1));
    map.add_effect(game_core::hypnotism::effect(PLAYER, at));
    map.run(game_core::time::Ticks::new(ticks));
    true
}

/// `INVISIBLE=1`: a brave next to the player's own shaman is cast Invisible (`Command::Cast`, same path a real
/// cast takes - granted `Unlimited` here so it works regardless of the sandbox's own loadout), then enough ticks
/// run to clear the cast's own `CAST_RELEASE` delay and hold the brave invisible for inspection (units.md
/// "Statuses"). False when the player has no shaman (no sandbox/level loaded one).
pub fn invisibility_demo(map: &mut game_core::map::GameMap) -> bool {
    let Some(at) = map.units.iter().find(|u| u.owner == PLAYER && u.kind == game_core::unit::UnitKind::Shaman).map(|u| (u.x, u.z)) else { return false };
    map.spawn(PLAYER, game_core::unit::UnitKind::Brave, (at.0.wrapping_add(100), at.1));
    map.spell_books[PLAYER as usize].set(game_core::spell_book::SpellKind::Invisibility, game_core::spell_book::Availability::Unlimited);
    map.apply(&Command::Cast { player: PLAYER, spell: game_core::spell::Spell::Invisibility { at } });
    map.run(game_core::time::Ticks::new(20));
    true
}

/// `VOLCANO=x,z` (cells): the player's Volcano effect there, then `ticks` run at once (with `GAME_SPEED=0`: its
/// rumble/rise/destruction/lava/fireball phases over the run). False when `at` does not parse.
pub fn volcano_demo(map: &mut game_core::map::GameMap, at: &str, ticks: u32) -> bool {
    let Some((x, z)) = at.split_once(',').and_then(|(x, z)| Some((x.trim().parse::<f32>().ok()?, z.trim().parse::<f32>().ok()?))) else { return false };
    let at = world_units(Vec2::new(x, z));
    map.add_effect(game_core::volcano::effect(PLAYER, at, &map.terrain.clone()));
    map.run(game_core::time::Ticks::new(ticks));
    true
}

/// `AOD=x,z` (cells): another tribe's Angel of Death cast there, then `ticks` run at once (with `GAME_SPEED=0`:
/// its meteor fall, ground ripple and spawn blast over the run — the creature itself has no sprite yet,
/// TODO.md "Angel of Death"). False when `at` does not parse.
pub fn angel_of_death_demo(map: &mut game_core::map::GameMap, at: &str, ticks: u32) -> bool {
    let Some((x, z)) = at.split_once(',').and_then(|(x, z)| Some((x.trim().parse::<f32>().ok()?, z.trim().parse::<f32>().ok()?))) else { return false };
    let at = world_units(Vec2::new(x, z));
    map.add_effect(game_core::angel_of_death::cast(PLAYER.next(), at));
    map.run(game_core::time::Ticks::new(ticks));
    true
}

/// `FIRESTORM=x,z` (cells): another tribe's Firestorm effect there, then `ticks` run at once (with `GAME_SPEED=0`:
/// its fireballs landing over the run). False when `at` does not parse.
pub fn firestorm_demo(map: &mut game_core::map::GameMap, at: &str, ticks: u32) -> bool {
    let Some((x, z)) = at.split_once(',').and_then(|(x, z)| Some((x.trim().parse::<f32>().ok()?, z.trim().parse::<f32>().ok()?))) else { return false };
    let at = world_units(Vec2::new(x, z));
    map.add_effect(game_core::firestorm::effect(PLAYER.next(), at));
    map.run(game_core::time::Ticks::new(ticks));
    true
}

/// `EARTHQUAKE=x,z` (cells): another tribe's Earthquake effect there, then `ticks` run at once (with `GAME_SPEED=0`:
/// the shake, then its sink and lava fissure at turn 60). False when `at` does not parse.
pub fn earthquake_demo(map: &mut game_core::map::GameMap, at: &str, ticks: u32) -> bool {
    let Some((x, z)) = at.split_once(',').and_then(|(x, z)| Some((x.trim().parse::<f32>().ok()?, z.trim().parse::<f32>().ok()?))) else { return false };
    let at = world_units(Vec2::new(x, z));
    map.add_effect(game_core::earthquake::effect(PLAYER.next(), at));
    map.run(game_core::time::Ticks::new(ticks));
    true
}

/// `SWARM=x,z` (cells): another tribe's Swarm effect there, then `ticks` run at once (with `GAME_SPEED=0`: its
/// bees wandering, stinging and raiding over the run). False when `at` does not parse.
pub fn swarm_demo(map: &mut game_core::map::GameMap, at: &str, ticks: u32) -> bool {
    let Some((x, z)) = at.split_once(',').and_then(|(x, z)| Some((x.trim().parse::<f32>().ok()?, z.trim().parse::<f32>().ok()?))) else { return false };
    let at = world_units(Vec2::new(x, z));
    map.add_effect(game_core::swarm::effect(PLAYER.next(), at));
    map.run(game_core::time::Ticks::new(ticks));
    true
}

/// `ARMAGEDDON=x,z` (cells): the player casts Armageddon there, then `ticks` run at once — v1 settles almost
/// immediately (`docs/specs/outcome.md` "Armageddon"); with `GAME_SPEED=0`, a shot can catch the brief
/// camera-takeover/lock window. False when `at` does not parse. Grants itself one shot of Armageddon right before
/// casting (`SpellBook::unlimited` deliberately leaves it unavailable, same as Bloodlust - `GameMap::outcome`'s
/// one-shot latch would otherwise strand every other spell in the Spells sandbox once this demo settles it), so this
/// works against any map regardless of what its own book allows.
pub fn armageddon_demo(map: &mut game_core::map::GameMap, at: &str, ticks: u32) -> bool {
    let Some((x, z)) = at.split_once(',').and_then(|(x, z)| Some((x.trim().parse::<f32>().ok()?, z.trim().parse::<f32>().ok()?))) else { return false };
    let at = world_units(Vec2::new(x, z));
    map.spell_books[PLAYER as usize].set(game_core::spell_book::SpellKind::Armageddon, game_core::spell_book::Availability::Provided { shots: 1 });
    map.apply(&Command::Cast { player: PLAYER, spell: game_core::spell::Spell::Armageddon { at } });
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
    let flatten = game_core::flatten::effect(PLAYER.next(), at, &map.terrain);
    map.add_effect(flatten);
    map.run(game_core::time::Ticks::new(ticks));
    true
}

/// `ERODE=x,z` (height point): another tribe's erosion there (no cast jump), then `ticks` run at once. False when
/// `at` does not parse.
pub fn erode_demo(map: &mut game_core::map::GameMap, at: &str, ticks: u32) -> bool {
    let Some(at) = at.split_once(',').and_then(|(x, z)| Some((x.trim().parse::<i32>().ok()?, z.trim().parse::<i32>().ok()?))) else { return false };
    let erode = game_core::erode::effect(PLAYER.next(), at, &map.terrain);
    map.add_effect(erode);
    map.run(game_core::time::Ticks::new(ticks));
    true
}

/// Commands for a `BRAVES` demo: every brave of the player (not inside a building) cuts the tree nearest to it; `carry` then
/// brings the piece back where it stood.
pub fn brave_commands(name: &str, map: &game_core::map::GameMap) -> Vec<Command> {
    if let Some(totem) = name.strip_prefix("totem:").and_then(|n| n.parse().ok()) {
        let braves: Vec<UnitId> = map.units.iter().filter(|u| u.owner == PLAYER && u.kind == game_core::unit::UnitKind::Brave).map(|u| u.id).collect();
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

/// `x,z:radius` (`EDITOR_BRUSH`): a pinned brush position and radius, for a headless shot of the ring.
pub fn parse_editor_brush(v: &str) -> Option<(Vec2, i32)> {
    let (at, radius) = v.split_once(':')?;
    let (x, z) = at.split_once(',')?;
    Some((Vec2::new(x.trim().parse().ok()?, z.trim().parse().ok()?), radius.trim().parse().ok()?))
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

/// `stored/cost[:short]`: a training unit's production.
pub fn parse_train(v: &str) -> Option<game_core::production::Production> {
    let (amounts, short) = v.split_once(':').map_or((v, false), |(a, s)| (a, s == "short"));
    let (stored, cost) = amounts.split_once('/')?;
    Some(game_core::production::Production { stored: stored.trim().parse().ok()?, cost: cost.trim().parse().ok()?, short })
}

/// `TRAIN` demo: each of the player's training units gets `production` and one of their idle braves
/// inside, as its trainee.
pub fn train_demo(map: &mut game_core::map::GameMap, production: game_core::production::Production) {
    use game_core::building::BuildingKind;
    let units: Vec<usize> = (0..map.buildings.len()).filter(|&i| map.buildings[i].owner == PLAYER && matches!(map.buildings[i].kind, BuildingKind::WarriorTraining | BuildingKind::FirewarriorTraining | BuildingKind::SpyTraining)).collect();
    for i in units {
        map.buildings[i].production = Some(production);
        let (building, door) = (map.buildings[i].id, map.buildings[i].door());
        let Some(u) = map.units.iter_mut().find(|u| u.owner == PLAYER && u.kind == game_core::unit::UnitKind::Brave && u.is_alive() && u.inside.is_none() && u.action == game_core::unit::Action::Idle) else { return };
        u.inside = Some(game_core::unit::Inside { building, door });
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
    let braves: Vec<UnitId> = map.units.iter().filter(|u| u.owner == PLAYER && u.kind == game_core::unit::UnitKind::Brave && u.inside.is_none()).map(|u| u.id).collect();
    for c in map.build_orders(PLAYER, &braves, site) {
        map.apply(&c);
    }
    for _ in 0..ticks {
        map.tick();
    }
    true
}

/// `kind@x,z[:n]`: a vehicle kind by name (`boat`, `balloon`, any case), a map position in cells, and how many
/// people aboard.
pub fn parse_vehicle(v: &str) -> Option<(game_core::vehicle::VehicleKind, Vec2, u8)> {
    let (name, rest) = v.split_once('@')?;
    let kind = game_core::vehicle::VehicleKind::ALL.into_iter().find(|k| k.name().eq_ignore_ascii_case(name.trim()))?;
    let (at, n) = rest.split_once(':').map_or(Some((rest, 0)), |(a, n)| Some((a, n.trim().parse().ok()?)))?;
    let (x, z) = at.split_once(',')?;
    Some((kind, Vec2::new(x.trim().parse().ok()?, z.trim().parse().ok()?), n))
}

/// `VEHICLE` demo: a vehicle of the player at `at` (cells) with `aboard` new braves in it; then sent to `to`
/// (cells) for `ticks` ticks.
pub fn vehicle_demo(map: &mut game_core::map::GameMap, (kind, at, aboard): (game_core::vehicle::VehicleKind, Vec2, u8), to: Option<Vec2>, ticks: u32) {
    use game_core::vehicle::Vehicle;
    let world = |c: Vec2| ((c.x * WORLD as f32) as u16, (c.y * WORLD as f32) as u16);
    let at = world(at);
    let vehicle = map.add_vehicle(Vehicle::new(kind, PLAYER, at, game_core::angle::Angle::ZERO));
    for _ in 0..aboard.min(kind.seats()) {
        let u = map.spawn(PLAYER, game_core::unit::UnitKind::Brave, at);
        u.aboard = Some(vehicle);
        let id = u.id;
        if let Some(v) = map.vehicles.get_mut(vehicle) {
            v.seat(id, PLAYER);
        }
    }
    if let Some(to) = to {
        map.apply(&Command::MoveVehicle { player: PLAYER, vehicle, to: world(to) });
    }
    for _ in 0..ticks {
        map.tick();
    }
}

const WORLD: u32 = pop3_format::WORLD_UNITS_PER_CELL;

/// `CREW` demo: every brave of the player on foot is sent into building `building` (its index: a hut's people, a vehicle
/// hut's crew), then `ticks` ticks run.
pub fn crew_demo(map: &mut game_core::map::GameMap, building: usize, ticks: u32) {
    let braves: Vec<UnitId> = map.units.iter().filter(|u| u.owner == PLAYER && u.kind == game_core::unit::UnitKind::Brave && u.in_the_open()).map(|u| u.id).collect();
    for c in map.enter_orders(PLAYER, &braves, building) {
        map.apply(&c);
    }
    map.run(game_core::time::Ticks::new(ticks));
}

/// `x,z`: a map position in cells.
pub fn parse_cells(v: &str) -> Option<Vec2> {
    let (x, z) = v.split_once(',')?;
    Some(Vec2::new(x.trim().parse().ok()?, z.trim().parse().ok()?))
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
    (mut explode, mut exploded, mut editor, mut editor_pinned): (
        MessageWriter<crate::explosion::BuildingExploded>,
        Local<Option<game_core::building::BuildingId>>,
        ResMut<crate::editor::EditorState>,
        ResMut<crate::editor::PinnedAt>,
    ),
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
    if *frame == 2 && let Some(id) = exploded.take() {
        map.0.buildings.retain(|b| b.id != id);
    }
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
        if let Some((at, radius)) = std::env::var("EDITOR_BRUSH").ok().and_then(|v| parse_editor_brush(&v)) {
            editor.active = true;
            editor.brush_radius = crate::editor::clamp_radius(radius);
            editor_pinned.0 = Some(at);
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
        cmds.extend(attack_commands(&std::env::var("ATTACK").unwrap_or_default(), &map.0));
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
        if let Ok(at) = std::env::var("SWAMP") {
            let ticks = std::env::var("SWAMP_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            swamp_demo(&mut map.0, &at, ticks);
        }
        if let Ok(at) = std::env::var("CONVERT") {
            let ticks = std::env::var("CONVERT_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            convert_demo(&mut map.0, &at, ticks);
        }
        if let Ok(at) = std::env::var("HYPNOTISM") {
            let ticks = std::env::var("HYPNOTISM_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            hypnotism_demo(&mut map.0, &at, ticks);
        }
        if std::env::var("INVISIBLE").is_ok_and(|v| v == "1") {
            invisibility_demo(&mut map.0);
        }
        if let Ok(at) = std::env::var("VOLCANO") {
            let ticks = std::env::var("VOLCANO_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            volcano_demo(&mut map.0, &at, ticks);
        }
        if let Ok(at) = std::env::var("AOD") {
            let ticks = std::env::var("AOD_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            angel_of_death_demo(&mut map.0, &at, ticks);
        }
        if let Ok(at) = std::env::var("FIRESTORM") {
            let ticks = std::env::var("FIRESTORM_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            firestorm_demo(&mut map.0, &at, ticks);
        }
        if let Ok(at) = std::env::var("EARTHQUAKE") {
            let ticks = std::env::var("EARTHQUAKE_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            earthquake_demo(&mut map.0, &at, ticks);
        }
        if let Ok(at) = std::env::var("SWARM") {
            let ticks = std::env::var("SWARM_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            swarm_demo(&mut map.0, &at, ticks);
        }
        if let Ok(at) = std::env::var("ARMAGEDDON") {
            let ticks = std::env::var("ARMAGEDDON_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            armageddon_demo(&mut map.0, &at, ticks);
        }
        if let Ok(to) = std::env::var("BRIDGE") {
            let ticks = std::env::var("BRIDGE_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            dirty.0 |= bridge_demo(&mut map.0, &to, ticks);
        }
        if let Ok(at) = std::env::var("FLATTEN") {
            let ticks = std::env::var("FLATTEN_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            dirty.0 |= flatten_demo(&mut map.0, &at, ticks);
        }
        if let Ok(at) = std::env::var("ERODE") {
            let ticks = std::env::var("ERODE_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            dirty.0 |= erode_demo(&mut map.0, &at, ticks);
        }
        if let Some(phase) = std::env::var("VAULT").ok().and_then(|v| parse_vault(&v)) {
            map.0.buildings.iter_mut().filter_map(|b| b.vault.as_mut()).for_each(|v| v.phase = phase);
        }
        if let Some(e) = std::env::var("EXPLODE").ok().and_then(|v| explode_demo(&map.0, &v, std::env::var("EXPLODE_TICKS").ok().and_then(|t| t.parse().ok()).unwrap_or(0))) {
            *exploded = Some(e.building);
            explode.write(e);
        }
        if let Some(production) = std::env::var("TRAIN").ok().and_then(|v| parse_train(&v)) {
            train_demo(&mut map.0, production);
        }
        if let Some(building) = std::env::var("CREW").ok().and_then(|v| v.strip_prefix("building:").and_then(|n| n.trim().parse().ok())) {
            let ticks = std::env::var("CREW_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            crew_demo(&mut map.0, building, ticks);
        }
        if let Ok(vehicles) = std::env::var("VEHICLE") {
            let to = std::env::var("VEHICLE_TO").ok().and_then(|v| parse_cells(&v));
            for vehicle in vehicles.split(';').filter_map(parse_vehicle) {
                vehicle_demo(&mut map.0, vehicle, to, 0);
            }
            let ticks = std::env::var("VEHICLE_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            map.0.run(game_core::time::Ticks::new(ticks));
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
    fn vehicle_setups() {
        use game_core::vehicle::VehicleKind;
        assert_eq!(parse_vehicle("boat@64,115:3"), Some((VehicleKind::Boat, Vec2::new(64.0, 115.0), 3)));
        assert_eq!(parse_vehicle("Balloon@1.5,2"), Some((VehicleKind::Balloon, Vec2::new(1.5, 2.0), 0)));
        assert_eq!(parse_vehicle("cart@1,2"), None);
        assert_eq!(parse_vehicle("boat@1,2:x"), None);
        let mut map = game_core::map::GameMap::sandbox_buildings();
        vehicle_demo(&mut map, (VehicleKind::Boat, Vec2::new(64.5, 116.5), 9), parse_cells("80,116"), 30);
        let (_, v) = map.vehicles.entries().next().unwrap();
        assert_eq!(v.passengers.len(), 5, "its seats");
        assert!(v.x > 64 * 512 + 256 + 512, "on its way east");
    }

    #[test]
    fn train_puts_a_trainee_in_each_training_unit() {
        use game_core::production::Production;
        assert_eq!(parse_train("875/3500"), Some(Production { stored: 875, cost: 3500, short: false }));
        assert_eq!(parse_train("10/3500:short"), Some(Production { stored: 10, cost: 3500, short: true }));
        assert_eq!(parse_train("3500"), None);
        let mut map = game_core::map::GameMap::sandbox_buildings();
        train_demo(&mut map, Production { stored: 875, cost: 3500, short: false });
        let warrior = map.buildings.iter().position(|b| b.owner == PLAYER && b.kind == game_core::building::BuildingKind::WarriorTraining).unwrap();
        assert!(map.production(warrior).is_some());
        assert_eq!(map.people_inside(map.buildings[warrior].id), 1);
    }

    #[test]
    fn attack_nearest_sends_the_fighters_at_the_closest_enemy() {
        let map = game_core::map::GameMap::sandbox_units();
        let cmds = attack_commands("nearest", &map);
        let fighters = map.units.iter().filter(|u| u.owner == PLAYER && map.units.iter().any(|t| game_core::attack::may_attack(u, t))).count();
        assert!(fighters > 0 && cmds.len() == fighters);
        assert!(attack_commands("", &map).is_empty());
    }

    #[test]
    fn brave_demo_sends_every_brave_to_cut_then_carry_back() {
        let mut map = game_core::map::GameMap::sandbox_buildings();
        let wood = |m: &game_core::map::GameMap| m.wood.len() as u32 + m.units.iter().map(|u| u.carrying as u32).sum::<u32>();
        let before = wood(&map);
        let cmds = brave_commands("carry", &map);
        assert_eq!(cmds.len(), 2 * game_core::map::SANDBOX_FREE_BRAVES as usize, "a cut and a way back for each brave outside");
        for c in &cmds {
            map.apply(c);
        }
        for _ in 0..1500 {
            map.tick();
        }
        let cut = wood(&map) - before;
        assert!(cut >= 8, "pieces cut and carried back, as long as the trees near the crowd have wood to spare: {cut}");
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
    fn explode_demo_names_the_building_and_when() {
        let map = game_core::map::GameMap::sandbox_buildings();
        let e = explode_demo(&map, "building:3", 0).unwrap();
        assert_eq!((e.building, e.start), (map.buildings[3].id, map.now));
        assert!(explode_demo(&map, "building:99999", 0).is_none());
        assert!(explode_demo(&map, "tree:1", 0).is_none());
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
    fn convert_demo_places_a_wildman_and_converts_it() {
        let mut map = game_core::map::GameMap::sandbox_walk();
        assert!(convert_demo(&mut map, "10,10", game_core::convert::CHECK_EVERY.get()));
        let wildman = map.units.iter().find(|u| u.kind == game_core::unit::UnitKind::Brave && u.owner == PLAYER).unwrap();
        assert_eq!(wildman.owner, PLAYER);
        assert!(!convert_demo(&mut map, "here", 0));
    }

    #[test]
    fn hypnotism_demo_places_an_enemy_and_captures_it() {
        let mut map = game_core::map::GameMap::sandbox_walk();
        assert!(hypnotism_demo(&mut map, "10,10", 1));
        let captured = map.units.iter().find(|u| u.owner == PLAYER && u.statuses.hypnotized.is_on()).unwrap();
        assert_eq!(captured.statuses.hypnotized_from, Some(PLAYER.next()));
        assert!(!hypnotism_demo(&mut map, "here", 0));
    }

    #[test]
    fn volcano_demo_adds_the_controller_effect_and_shapes_the_ground_once_run() {
        let mut map = game_core::map::GameMap::sandbox_walk();
        assert!(volcano_demo(&mut map, "10,10", game_core::volcano::RUMBLE_TURNS + game_core::volcano::RISE_TURNS));
        assert_eq!(map.terrain.get(10, 10), 0, "the crater floor, fully risen");
        assert!(!volcano_demo(&mut map, "here", 0));
    }

    #[test]
    fn angel_of_death_demo_falls_ripples_and_spawns_the_creature_once_run() {
        let mut map = game_core::map::GameMap::sandbox_walk();
        assert!(angel_of_death_demo(&mut map, "10,10", game_core::angel_of_death::FALL_TICKS.get()));
        assert!(map.units.iter().any(|u| u.kind == game_core::unit::UnitKind::AngelOfDeath), "the creature appears");
        assert!(!angel_of_death_demo(&mut map, "here", 0));
    }

    #[test]
    fn firestorm_demo_adds_the_scheduler_effect_and_drops_a_fireball_once_run() {
        let mut map = game_core::map::GameMap::sandbox_walk();
        assert!(firestorm_demo(&mut map, "10,10", game_core::firestorm::FIREBALL_EVERY.get()));
        assert!(map.effects.entries().any(|(_, e)| matches!(e.kind, game_core::effect::EffectKind::PanickingBlast(_))));
        assert!(!firestorm_demo(&mut map, "here", 0));
    }

    #[test]
    fn earthquake_demo_adds_the_controller_effect_and_sinks_the_ground_once_run() {
        let mut map = game_core::map::GameMap::sandbox_walk();
        // The player's own site, at the island's centre: on land, unlike an arbitrary cell.
        let s = map.units.iter().position(|u| u.owner == PLAYER).unwrap();
        let cell = |v: u16| v as f32 / 512.0;
        let at = format!("{},{}", cell(map.units[s].x), cell(map.units[s].z));
        let before = map.terrain.get((map.units[s].x / 512) as i32, (map.units[s].z / 512) as i32);
        assert!(earthquake_demo(&mut map, &at, game_core::earthquake::SINK_AT + 1));
        assert!(map.terrain.get((map.units[s].x / 512) as i32, (map.units[s].z / 512) as i32) < before, "the centre sank");
        assert!(!earthquake_demo(&mut map, "here", 0));
    }

    #[test]
    fn armageddon_demo_casts_and_settles_within_the_run() {
        let mut map = game_core::map::GameMap::sandbox_spells(); // armageddon_demo grants itself a shot regardless
        // CAST_RELEASE (6 ticks) for the cast jump to release, then the match's own first-tick resolution.
        assert!(armageddon_demo(&mut map, "5,5", 10));
        assert!(map.outcome.is_some(), "v1 settles almost immediately once released");
        assert!(!armageddon_demo(&mut map, "nope", 0));
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
