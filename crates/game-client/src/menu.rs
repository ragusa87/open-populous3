//! Main menu shown before the game: New game (the level from the command line / PgUp-PgDn list),
//! Sandbox (test grounds: Walk, Units, Buildings, Worship, Spells, Vehicles, Bridge), Quit. Sub-pages open on Enter and go back with Esc/Backspace or
//! their Back entry; arrows (or W/S) move, Enter/Space picks, the mouse hovers and clicks.
//! Gameplay systems are in the `Gameplay` set, which runs while `Playing` or `Ended` (the match keeps running past
//! the win/lose dialog, `game_end.rs` — only input-driven systems need to exclude `Ended` themselves, e.g. via
//! `armageddon::input_locked`). Behind the menu the game camera is off: no world, units or HUD are drawn, the menu
//! has its own overlay camera.
//! Esc in the game pauses: the mouse is released and the pause menu (Resume, Main menu with a
//! confirmation) shows over the frozen game; Esc on it resumes. Its Dev mode entry switches `keymap::DevMode`; its
//! Shortcuts page lists the keys and mouse of the keymap in use (`keymap::shortcut_lines`).
//! The Audio page (from the main menu and the pause menu) sets the `audio_mix::Mixer` volumes (Enter steps up and
//! wraps, Left/Right step down/up) and its Mute all box; leaving it saves them (`user_settings`).
//! `POP3_START=menu|game|sandbox-walk|sandbox-units|sandbox-buildings|sandbox-worship|sandbox-spells|sandbox-vehicles|sandbox-bridge|sandbox-whirlwind` picks where to start (screenshots start in the game).

use crate::audio::{AudioOut, DeviceMuted, PlaySound, SoundCue};
use crate::audio_mix::{Channel, Level, Mixer, Slot};
use crate::camera::{GameCamera, OverlayCamera};
use crate::user_settings::SaveSettings;
use crate::game_speed::GameSpeed;
use crate::hud::spells::SelectedSpell;
use crate::units::selection::Selection;
use crate::virtual_cursor::VirtualCursor;
use crate::boot::{BeginGame, GameStep, LoadTarget};
use crate::world::{new_game, CurrentMap, LevelList, TerrainDirty};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{block_on, poll_once, AsyncComputeTaskPool, Task};
use game_core::map::GameMap;

const BACKDROP: Color = Color::srgb(0.06, 0.04, 0.02);
/// Over the frozen game while paused (the main menu is drawn on `BACKDROP`); reused by `game_end.rs`'s dialog.
pub(crate) const DIM: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
pub(crate) const TEXT: Color = Color::srgb(0.95, 0.85, 0.6);
pub(crate) const HIGHLIGHT: Color = Color::srgb(0.80, 0.56, 0.20);
pub(crate) const ITEM: Color = Color::srgba(0.30, 0.18, 0.06, 0.9);

#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppState {
    /// The loading screen (`boot`): at launch, and while a game's map is built.
    #[default]
    Loading,
    Menu,
    Playing,
    Paused,
    /// The match ended (`game_core::map::GameMap::outcome`); its dialog shows over the frozen game (`game_end.rs`).
    Ended,
}

/// Systems that only run in the game (input, simulation, HUD actions), not behind the menu.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Gameplay;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Main,
    Sandbox,
    Paused,
    /// The pause menu in a sandbox: Restart too.
    SandboxPaused,
    /// The shortcuts of the keymap in use (`keymap::shortcut_lines`).
    Shortcuts,
    Audio,
    ConfirmLeave,
}

/// What a game starts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    NewGame,
    SandboxWalk,
    SandboxUnits,
    SandboxBuildings,
    SandboxWorship,
    SandboxSpells,
    SandboxVehicles,
    SandboxBridge,
    SandboxWhirlwind,
}

impl Start {
    pub fn is_sandbox(self) -> bool {
        self != Start::NewGame
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Start(Start),
    /// The current sandbox again, as it started.
    Restart,
    Open(Page),
    Back,
    Resume,
    ToggleDev,
    CycleSpeed,
    Volume(Slot),
    ToggleMuteAll,
    Leave,
    Quit,
}

/// What picking an entry leads to outside the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Start(Start),
    Restart,
    Resume,
    ToggleDev,
    CycleSpeed,
    CycleVolume(Slot),
    /// A volume up or down by this many percent.
    StepVolume(Slot, i32),
    ToggleMuteAll,
    /// Leave the game for the main menu.
    Leave,
    Quit,
}

pub fn title(page: Page) -> &'static str {
    match page {
        Page::Main => "Open Populous",
        Page::Sandbox => "Sandbox",
        Page::Paused | Page::SandboxPaused => "Paused",
        Page::Shortcuts => "Shortcuts",
        Page::Audio => "Audio",
        Page::ConfirmLeave => "Leave this game?",
    }
}

pub fn items(page: Page) -> &'static [(&'static str, Action)] {
    match page {
        Page::Main => &[("New game", Action::Start(Start::NewGame)), ("Sandbox", Action::Open(Page::Sandbox)), ("Audio", Action::Open(Page::Audio)), ("Quit", Action::Quit)],
        Page::Sandbox => &[("Walk", Action::Start(Start::SandboxWalk)), ("Units", Action::Start(Start::SandboxUnits)), ("Buildings", Action::Start(Start::SandboxBuildings)), ("Worship", Action::Start(Start::SandboxWorship)), ("Spells", Action::Start(Start::SandboxSpells)), ("Vehicles", Action::Start(Start::SandboxVehicles)), ("Bridge", Action::Start(Start::SandboxBridge)), ("Whirlwind", Action::Start(Start::SandboxWhirlwind)), ("Back", Action::Back)],
        Page::Paused => &[("Resume", Action::Resume), ("Game speed", Action::CycleSpeed), ("Audio", Action::Open(Page::Audio)), ("Dev mode", Action::ToggleDev), ("Shortcuts", Action::Open(Page::Shortcuts)), ("Main menu", Action::Open(Page::ConfirmLeave))],
        Page::SandboxPaused => &[("Resume", Action::Resume), ("Restart", Action::Restart), ("Game speed", Action::CycleSpeed), ("Audio", Action::Open(Page::Audio)), ("Dev mode", Action::ToggleDev), ("Shortcuts", Action::Open(Page::Shortcuts)), ("Main menu", Action::Open(Page::ConfirmLeave))],
        Page::Shortcuts => &[("Back", Action::Back)],
        Page::Audio => &[
            ("Master", Action::Volume(Slot::Master)),
            ("Music", Action::Volume(Slot::Channel(Channel::Music))),
            ("Fx", Action::Volume(Slot::Channel(Channel::Fx))),
            ("Mute all", Action::ToggleMuteAll),
            ("Back", Action::Back),
        ],
        Page::ConfirmLeave => &[("No, keep playing", Action::Back), ("Yes, back to the main menu", Action::Leave)],
    }
}

/// The settings the entries show.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shown {
    pub dev: bool,
    pub speed: GameSpeed,
    pub mixer: Mixer,
}

/// An entry's text; a setting shows its value.
pub fn label(entry: (&str, Action), shown: &Shown) -> String {
    match entry {
        (name, Action::ToggleDev) => format!("{name}: {}", if shown.dev { "on" } else { "off" }),
        (name, Action::CycleSpeed) => format!("{name}: x{}", shown.speed.times),
        (name, Action::Volume(slot)) => format!("{name}: {}%", shown.mixer.level(slot).percent()),
        (name, Action::ToggleMuteAll) => format!("[{}] {name}", if shown.mixer.mute_all { "x" } else { " " }),
        (name, _) => name.to_string(),
    }
}

/// The Audio page's note when nothing can be heard whatever the settings.
pub fn device_note(device: bool, muted_by_flag: bool) -> Option<&'static str> {
    match (device, muted_by_flag) {
        (true, _) => None,
        (false, true) => Some("Audio device off (--mute)"),
        (false, false) => Some("Audio device off"),
    }
}

/// Open pages, root first, each with its highlighted entry.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct MenuNav {
    stack: Vec<(Page, usize)>,
}

impl Default for MenuNav {
    fn default() -> Self {
        MenuNav { stack: vec![(Page::Main, 0)] }
    }
}

impl MenuNav {
    /// Navigation starting on `page`, first entry highlighted.
    pub fn at(page: Page) -> Self {
        MenuNav { stack: vec![(page, 0)] }
    }

    pub fn page(&self) -> Page {
        self.stack.last().map_or(Page::Main, |s| s.0)
    }

    pub fn cursor(&self) -> usize {
        self.stack.last().map_or(0, |s| s.1)
    }

    pub fn set_cursor(&mut self, i: usize) {
        let n = items(self.page()).len();
        if let Some(top) = self.stack.last_mut() {
            top.1 = i.min(n - 1);
        }
    }

    /// Moves the highlight, wrapping at both ends.
    pub fn step(&mut self, delta: i32) {
        let n = items(self.page()).len() as i32;
        self.set_cursor((self.cursor() as i32 + delta).rem_euclid(n) as usize);
    }

    /// Picks the highlighted entry: opens/closes pages here, returns what the app must do.
    pub fn activate(&mut self) -> Option<Outcome> {
        match items(self.page())[self.cursor()].1 {
            Action::Start(s) => Some(Outcome::Start(s)),
            Action::Restart => Some(Outcome::Restart),
            Action::Resume => Some(Outcome::Resume),
            Action::ToggleDev => Some(Outcome::ToggleDev),
            Action::CycleSpeed => Some(Outcome::CycleSpeed),
            Action::Volume(slot) => Some(Outcome::CycleVolume(slot)),
            Action::ToggleMuteAll => Some(Outcome::ToggleMuteAll),
            Action::Leave => Some(Outcome::Leave),
            Action::Quit => Some(Outcome::Quit),
            Action::Open(p) => {
                self.stack.push((p, 0));
                None
            }
            Action::Back => {
                self.back();
                None
            }
        }
    }

    /// Left/Right on the highlighted entry: a volume steps down/up.
    pub fn nudge(&self, direction: i32) -> Option<Outcome> {
        match items(self.page())[self.cursor()].1 {
            Action::Volume(slot) => Some(Outcome::StepVolume(slot, direction.signum() * Level::STEP)),
            _ => None,
        }
    }

    /// Back to the parent page (the entry that opened it stays highlighted); false on the root.
    pub fn back(&mut self) -> bool {
        if self.stack.len() <= 1 {
            return false;
        }
        self.stack.pop();
        true
    }
}

/// `POP3_START` value (or capture mode: a screenshot, a video) to the game to start right away; None shows the menu.
pub fn start_from_env(start: Option<&str>, screenshot: bool) -> Option<Start> {
    match start {
        Some("menu") => None,
        Some("game") => Some(Start::NewGame),
        Some("sandbox-walk") => Some(Start::SandboxWalk),
        Some("sandbox-units") => Some(Start::SandboxUnits),
        Some("sandbox-buildings") => Some(Start::SandboxBuildings),
        Some("sandbox-worship") => Some(Start::SandboxWorship),
        Some("sandbox-spells") => Some(Start::SandboxSpells),
        Some("sandbox-vehicles") => Some(Start::SandboxVehicles),
        Some("sandbox-bridge") => Some(Start::SandboxBridge),
        Some("sandbox-whirlwind") => Some(Start::SandboxWhirlwind),
        _ if screenshot => Some(Start::NewGame),
        _ => None,
    }
}

#[derive(Component)]
struct MenuRoot;
#[derive(Component)]
struct MenuTitle;
#[derive(Component)]
struct MenuList;
#[derive(Component)]
struct MenuItem(usize);

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        let start = start_from_env(std::env::var("POP3_START").ok().as_deref(), crate::dev::Capture::from_env(|k| std::env::var(k).ok()).is_some());
        app.insert_state(AppState::Loading)
            .insert_resource(crate::boot::LoadTarget::Boot { then: start })
            .init_resource::<MenuNav>()
            .init_resource::<NewGameBuild>()
            .configure_sets(Update, Gameplay.run_if(in_state(AppState::Playing).or_else(in_state(AppState::Ended))))
            .add_systems(Startup, spawn_menu)
            .add_systems(Update, begin_game.after(crate::boot::LoadingSystems))
            .add_systems(Update, poll_new_game_build.before(crate::boot::LoadingSystems).run_if(in_state(AppState::Loading)))
            .add_systems(Update, pause_on_escape.before(crate::virtual_cursor::toggle_capture).in_set(Gameplay).run_if(not(in_state(AppState::Ended))))
            .add_systems(Update, menu_from_env.in_set(Gameplay).run_if(not(in_state(AppState::Ended))))
            .add_systems(
                Update,
                (menu_keys.before(crate::virtual_cursor::toggle_capture), menu_mouse, rebuild_items, item_visuals)
                    .chain()
                    .run_if(in_state(AppState::Menu).or_else(in_state(AppState::Paused))),
            )
            .add_systems(Update, save_on_leaving_audio)
            .add_systems(Update, show_state.run_if(state_changed::<AppState>));
    }
}

/// What is drawn in a state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// The game camera renders (world, units, HUD).
    pub game: bool,
    /// Color the overlay camera clears the window with; None draws over the game.
    pub clear: Option<Color>,
    /// Menu shown, over this backdrop.
    pub menu: Option<Color>,
}

pub fn view(state: AppState) -> View {
    match state {
        AppState::Loading => View { game: false, clear: Some(BACKDROP), menu: None },
        AppState::Menu => View { game: false, clear: Some(BACKDROP), menu: Some(Color::NONE) },
        AppState::Playing => View { game: true, clear: None, menu: None },
        AppState::Paused => View { game: true, clear: None, menu: Some(DIM) },
        // No menu overlay here: game_end.rs draws its own root, over the frozen game, like Paused's.
        AppState::Ended => View { game: true, clear: None, menu: None },
    }
}

fn show_state(
    state: Res<State<AppState>>,
    mut game: Query<&mut Camera, (With<GameCamera>, Without<OverlayCamera>)>,
    mut overlay: Query<&mut Camera, (With<OverlayCamera>, Without<GameCamera>)>,
    mut menu: Query<(&mut Visibility, &mut BackgroundColor), With<MenuRoot>>,
) {
    let v = view(*state.get());
    for mut cam in &mut game {
        cam.is_active = v.game;
    }
    for mut cam in &mut overlay {
        cam.clear_color = v.clear.map_or(ClearColorConfig::None, ClearColorConfig::Custom);
    }
    for (mut vis, mut bg) in &mut menu {
        *vis = if v.menu.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        bg.0 = v.menu.unwrap_or(Color::NONE);
    }
}

fn spawn_menu(mut commands: Commands, overlay: Single<Entity, With<OverlayCamera>>) {
    commands
        .spawn((
            MenuRoot,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(14),
                ..default()
            },
            BackgroundColor(DIM),
            GlobalZIndex(i32::MAX - 10),
            UiTargetCamera(*overlay),
            Visibility::Hidden,
        ))
        .with_children(|root| {
            root.spawn((MenuTitle, Text::new(""), TextFont { font_size: FontSize::Px(40.0), ..default() }, TextColor(TEXT)));
            root.spawn((MenuList, Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(8), margin: UiRect::top(px(12)), ..default() }));
            root.spawn((
                Text::new("Up/Down: move   Enter: select   Left/Right: change   Esc: back"),
                TextFont { font_size: FontSize::Px(13.0), ..default() },
                TextColor(TEXT.with_alpha(0.6)),
                Node { margin: UiRect::top(px(20)), ..default() },
            ));
        });
}

/// A game's map, building on a worker thread (`spawn_game_build`/`poll_new_game_build`): started once
/// `begin_game` sees a `BeginGame` message, taken once done. `GameMap::load_original`/`generate` and the
/// `sandbox_*` constructors are plain, deterministic, `Send` computation (`game-core` has no Bevy, no ECS
/// access), so - unlike `BootJob`'s GPU-touching `install` half - a game start's whole build fits on one
/// worker-thread task with nothing left to chunk into frame-sized pieces (TODO.md "Loading screen freezes").
#[derive(Resource, Default)]
struct NewGameBuild(Option<Task<GameMap>>);

/// Starts building the map of a game the loading pass wants (`boot::BeginGame`), on a worker thread so the
/// loading screen keeps rendering frames while it runs (`poll_new_game_build` picks up the result).
fn begin_game(mut begins: MessageReader<BeginGame>, mut build: ResMut<NewGameBuild>, levels: Res<LevelList>, mut cursor: ResMut<VirtualCursor>) {
    for BeginGame(start) in begins.read() {
        build.0 = Some(spawn_game_build(*start, levels.clone()));
        cursor.request = Some(true);
    }
}

/// The actual per-`Start` map construction (`GameSetup::begin`'s old match arm), off the main thread.
fn spawn_game_build(start: Start, levels: LevelList) -> Task<GameMap> {
    AsyncComputeTaskPool::get().spawn(async move {
        match start {
            Start::NewGame => levels.load_current(),
            Start::SandboxWalk => new_game(GameMap::sandbox_walk()),
            Start::SandboxUnits => new_game(GameMap::sandbox_units_with_spies()),
            Start::SandboxBuildings => new_game(GameMap::sandbox_buildings()),
            Start::SandboxWorship => new_game(GameMap::sandbox_worship()),
            Start::SandboxSpells => new_game(GameMap::sandbox_spells()),
            Start::SandboxVehicles => new_game(GameMap::sandbox_vehicles()),
            Start::SandboxBridge => new_game(GameMap::sandbox_bridge()),
            Start::SandboxWhirlwind => new_game(GameMap::sandbox_whirlwind()),
        }
    })
}

/// Once `NewGameBuild`'s task finishes, installs the map and moves `GameStep::Building` on to `Built` (`boot::next`
/// cannot do this transition itself: it is a pure function with no way to poll a `Task`).
fn poll_new_game_build(mut build: ResMut<NewGameBuild>, mut game: GameSetup) {
    let Some(mut task) = build.0.take() else { return };
    let Some(map) = block_on(poll_once(&mut task)) else {
        build.0 = Some(task);
        return;
    };
    let LoadTarget::Game { start, step: GameStep::Building } = *game.target else { return };
    game.finish_begin(start, map);
    *game.target = LoadTarget::Game { start, step: GameStep::Built };
}

/// What starting, pausing and leaving a game touch.
#[derive(SystemParam)]
pub(crate) struct GameSetup<'w> {
    levels: ResMut<'w, LevelList>,
    map: ResMut<'w, CurrentMap>,
    dirty: ResMut<'w, TerrainDirty>,
    selection: ResMut<'w, Selection>,
    cursor: ResMut<'w, VirtualCursor>,
    blueprint: ResMut<'w, crate::blueprint::Blueprint>,
    selected_spell: ResMut<'w, SelectedSpell>,
    grid: ResMut<'w, crate::world::ShowGrid>,
    dev: ResMut<'w, crate::keymap::DevMode>,
    speed: ResMut<'w, GameSpeed>,
    schedule: ResMut<'w, crate::units::GameSchedule>,
    clock: ResMut<'w, crate::units::SimClock>,
    target: ResMut<'w, crate::boot::LoadTarget>,
    mixer: ResMut<'w, Mixer>,
}

/// Leaves the current game for the main menu (the pause menu's "Main menu", and `game_end.rs`'s dismiss): resets
/// navigation, clears the selection, switches state. The one implementation both share.
pub(crate) fn leave_to_menu(game: &mut GameSetup, nav: &mut MenuNav, state: &mut NextState<AppState>) {
    *nav = MenuNav::default();
    game.selection.clear();
    state.set(AppState::Menu);
}

impl GameSetup<'_> {
    /// Installs a game's already-built `map` (`spawn_game_build`/`poll_new_game_build`); the camera frames it
    /// (`LevelList` change).
    fn finish_begin(&mut self, start: Start, map: GameMap) {
        self.map.0 = map;
        *self.schedule = crate::units::GameSchedule::default();
        self.clock.new_game();
        self.grid.0 = start != Start::NewGame;
        self.blueprint.put_away();
        self.selected_spell.0 = None;
        self.dirty.0 = true;
        self.selection.clear();
        self.levels.set_changed();
    }
}

/// The pause menu for the game `target` loaded: with Restart in a sandbox.
pub fn paused_page(target: &LoadTarget) -> Page {
    match target {
        LoadTarget::Game { start, .. } if start.is_sandbox() => Page::SandboxPaused,
        _ => Page::Paused,
    }
}

/// Esc in the game (not taken by an open HUD menu): pause and free the mouse.
pub(crate) fn pause_on_escape(
    target: Res<LoadTarget>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut nav: ResMut<MenuNav>,
    mut cursor: ResMut<VirtualCursor>,
    mut state: ResMut<NextState<AppState>>,
) {
    if keys.clear_just_pressed(KeyCode::Escape) {
        *nav = MenuNav::at(paused_page(&target));
        cursor.request = Some(false);
        state.set(AppState::Paused);
    }
}

/// Dev: `MENU=shortcuts|audio` pauses the game on that page once it runs, for shots.
fn menu_from_env(mut done: Local<bool>, mut nav: ResMut<MenuNav>, mut state: ResMut<NextState<AppState>>) {
    if std::mem::replace(&mut *done, true) {
        return;
    }
    let page = match std::env::var("MENU").ok().as_deref() {
        Some("shortcuts") => (Page::Shortcuts, 4),
        Some("audio") => (Page::Audio, 2),
        _ => return,
    };
    *nav = MenuNav { stack: vec![(Page::Paused, page.1), (page.0, 0)] };
    state.set(AppState::Paused);
}

/// Leaving the Audio page (whichever way) keeps its settings.
fn save_on_leaving_audio(nav: Res<MenuNav>, mut was_open: Local<bool>, mut save: MessageWriter<SaveSettings>) {
    let open = nav.page() == Page::Audio;
    if std::mem::replace(&mut *was_open, open) && !open {
        save.write(SaveSettings);
    }
}

fn apply(outcome: Option<Outcome>, game: &mut GameSetup, nav: &mut MenuNav, state: &mut NextState<AppState>, exit: &mut MessageWriter<AppExit>) {
    match outcome {
        Some(Outcome::Start(s)) => {
            *game.target = crate::boot::LoadTarget::game(s);
            state.set(AppState::Loading);
        }
        Some(Outcome::Restart) => {
            if let LoadTarget::Game { start, .. } = *game.target {
                *game.target = crate::boot::LoadTarget::game(start);
                state.set(AppState::Loading);
            }
        }
        Some(Outcome::Resume) => {
            game.cursor.request = Some(true);
            state.set(AppState::Playing);
        }
        Some(Outcome::ToggleDev) => {
            game.dev.0 = !game.dev.0;
        }
        Some(Outcome::CycleSpeed) => {
            game.speed.cycle();
        }
        Some(Outcome::CycleVolume(slot)) => {
            let level = game.mixer.level_mut(slot);
            *level = level.cycle_up();
        }
        Some(Outcome::StepVolume(slot, delta)) => {
            let level = game.mixer.level_mut(slot);
            *level = level.step(delta);
        }
        Some(Outcome::ToggleMuteAll) => {
            game.mixer.mute_all = !game.mixer.mute_all;
        }
        Some(Outcome::Leave) => leave_to_menu(game, nav, state),
        Some(Outcome::Quit) => {
            exit.write(AppExit::Success);
        }
        None => {}
    }
}

/// Esc on the root page: resumes a paused game (the main menu leaves it to free the mouse).
fn escape_outcome(page: Page) -> Option<Outcome> {
    matches!(page, Page::Paused | Page::SandboxPaused).then_some(Outcome::Resume)
}

fn menu_keys(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut nav: ResMut<MenuNav>,
    mut game: GameSetup,
    mut state: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
    mut sounds: MessageWriter<PlaySound>,
) {
    if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
        nav.step(-1);
    }
    if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
        nav.step(1);
    }
    let escape = keys.just_pressed(KeyCode::Escape);
    if escape || keys.just_pressed(KeyCode::Backspace) {
        let went_back = nav.back();
        let outcome = if went_back || !escape { None } else { escape_outcome(nav.page()) };
        if escape && (went_back || outcome.is_some()) {
            keys.clear_just_pressed(KeyCode::Escape);
        }
        apply(outcome, &mut game, &mut nav, &mut state, &mut exit);
    }
    for (keys_pressed, direction) in [([KeyCode::ArrowLeft, KeyCode::KeyA], -1), ([KeyCode::ArrowRight, KeyCode::KeyD], 1)] {
        if keys.any_just_pressed(keys_pressed) {
            let outcome = nav.nudge(direction);
            apply(outcome, &mut game, &mut nav, &mut state, &mut exit);
        }
    }
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) {
        click(&mut sounds);
        let outcome = nav.activate();
        apply(outcome, &mut game, &mut nav, &mut state, &mut exit);
    }
}

fn click(sounds: &mut MessageWriter<PlaySound>) {
    sounds.write(PlaySound::flat(SoundCue::Click));
}

fn menu_mouse(
    q: Query<(&Interaction, &MenuItem), Changed<Interaction>>,
    mut nav: ResMut<MenuNav>,
    mut game: GameSetup,
    mut state: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
    mut sounds: MessageWriter<PlaySound>,
) {
    for (interaction, item) in &q {
        match interaction {
            Interaction::Hovered => nav.set_cursor(item.0),
            Interaction::Pressed => {
                click(&mut sounds);
                nav.set_cursor(item.0);
                let outcome = nav.activate();
                apply(outcome, &mut game, &mut nav, &mut state, &mut exit);
                return;
            }
            Interaction::None => {}
        }
    }
}

/// Respawns the entries when the page changes.
fn rebuild_items(
    mut commands: Commands,
    nav: Res<MenuNav>,
    dev: Res<crate::keymap::DevMode>,
    speed: Res<GameSpeed>,
    mixer: Res<Mixer>,
    device: Option<Res<AudioOut>>,
    muted_by_flag: Res<DeviceMuted>,
    mut last: Local<Option<(Page, Shown)>>,
    lists: Query<Entity, With<MenuList>>,
    mut titles: Query<&mut Text, With<MenuTitle>>,
) {
    let page = nav.page();
    let shown = Shown { dev: dev.0, speed: *speed, mixer: *mixer };
    if *last == Some((page, shown)) {
        return;
    }
    *last = Some((page, shown));
    for mut t in &mut titles {
        t.0 = match page {
            Page::Shortcuts => format!("{} ({})", title(page), if dev.0 { "dev mode" } else { "original keys" }),
            _ => title(page).to_string(),
        };
    }
    for list in &lists {
        commands.entity(list).despawn_children().with_children(|l| {
            if page == Page::Shortcuts {
                spawn_shortcuts(l, dev.0);
            }
            if let Some(note) = device_note(device.is_some(), muted_by_flag.0).filter(|_| page == Page::Audio) {
                l.spawn((Text::new(note), TextFont { font_size: FontSize::Px(14.0), ..default() }, TextColor(TEXT.with_alpha(0.6))));
            }
            for (i, &entry) in items(page).iter().enumerate() {
                l.spawn((
                    MenuItem(i),
                    Button,
                    Node { width: px(260), padding: UiRect::axes(px(16), px(8)), justify_content: JustifyContent::Center, border: UiRect::all(px(2)), ..default() },
                    BackgroundColor(ITEM),
                    BorderColor::all(Color::NONE),
                ))
                .with_child((Text::new(label(entry, &shown)), TextFont { font_size: FontSize::Px(20.0), ..default() }, TextColor(TEXT)));
            }
        });
    }
}

/// The shortcuts page's table: what each does and its keys, in two pairs of columns.
fn spawn_shortcuts(list: &mut ChildSpawnerCommands, dev: bool) {
    let font = |size: f32| TextFont { font_size: FontSize::Px(size), ..default() };
    list.spawn((
        Node {
            display: Display::Grid,
            grid_template_columns: vec![RepeatedGridTrack::fr(1, 1.2), RepeatedGridTrack::fr(1, 1.0), RepeatedGridTrack::fr(1, 1.2), RepeatedGridTrack::fr(1, 1.0)],
            width: Val::Vw(90.0),
            max_width: px(1100),
            column_gap: px(14),
            row_gap: px(2),
            padding: UiRect::all(px(12)),
            margin: UiRect::bottom(px(8)),
            ..default()
        },
        BackgroundColor(ITEM),
    ))
    .with_children(|grid| {
        for (what, keys) in crate::keymap::shortcut_lines(dev) {
            grid.spawn((Text::new(what), font(12.0), TextColor(TEXT.with_alpha(0.75))));
            grid.spawn((Text::new(keys), font(12.0), TextColor(TEXT)));
        }
    });
}

fn item_visuals(nav: Res<MenuNav>, mut q: Query<(&MenuItem, &mut BorderColor)>) {
    for (item, mut border) in &mut q {
        *border = BorderColor::all(if item.0 == nav.cursor() { HIGHLIGHT } else { Color::NONE });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_wrap_and_sub_pages_go_back() {
        let mut nav = MenuNav::default();
        nav.step(-1);
        assert_eq!(nav.cursor(), 3, "up from the first entry wraps to Quit");
        nav.step(1);
        nav.step(1);
        assert_eq!(items(nav.page())[nav.cursor()].0, "Sandbox");
        assert_eq!(nav.activate(), None);
        assert_eq!((nav.page(), nav.cursor()), (Page::Sandbox, 0));
        assert!(nav.back());
        assert_eq!((nav.page(), nav.cursor()), (Page::Main, 1), "the entry that opened it stays highlighted");
        assert!(!nav.back(), "nothing above the main page");
    }

    #[test]
    fn entries_lead_to_games_or_quit() {
        let mut nav = MenuNav::default();
        assert_eq!(nav.activate(), Some(Outcome::Start(Start::NewGame)));
        nav.set_cursor(1);
        nav.activate();
        assert_eq!(nav.activate(), Some(Outcome::Start(Start::SandboxWalk)));
        nav.set_cursor(1);
        assert_eq!(nav.activate(), Some(Outcome::Start(Start::SandboxUnits)));
        nav.set_cursor(99);
        assert_eq!(nav.activate(), None, "Back entry, clamped cursor");
        assert_eq!(nav.page(), Page::Main);
        nav.set_cursor(3);
        assert_eq!(nav.activate(), Some(Outcome::Quit));
    }

    #[test]
    fn menu_draws_no_game_pause_draws_over_it() {
        assert_eq!(view(AppState::Menu), View { game: false, clear: Some(BACKDROP), menu: Some(Color::NONE) });
        assert_eq!(view(AppState::Playing), View { game: true, clear: None, menu: None }, "the overlay does not clear the game");
        assert_eq!(view(AppState::Paused), View { game: true, clear: None, menu: Some(DIM) }, "frozen game under a dimmed menu");
    }

    #[test]
    fn leaving_a_paused_game_asks_first() {
        let mut nav = MenuNav::at(Page::Paused);
        assert_eq!(nav.activate(), Some(Outcome::Resume), "Resume is first");
        nav.step(5);
        assert_eq!(nav.activate(), None, "Main menu opens the confirmation");
        assert_eq!((nav.page(), nav.cursor()), (Page::ConfirmLeave, 0));
        assert_eq!(nav.activate(), None, "No (highlighted first) goes back");
        assert_eq!((nav.page(), nav.cursor()), (Page::Paused, 5));
        nav.activate();
        nav.step(1);
        assert_eq!(nav.activate(), Some(Outcome::Leave));
    }

    #[test]
    fn dev_mode_toggles_from_the_pause_menu_and_shows_its_state() {
        let on = Shown { dev: true, speed: GameSpeed::default(), mixer: Mixer::default() };
        let off = Shown { dev: false, ..on };
        let mut nav = MenuNav::at(Page::Paused);
        nav.step(3);
        assert_eq!(nav.activate(), Some(Outcome::ToggleDev));
        assert_eq!(nav.page(), Page::Paused, "stays open to see the new state");
        let entry = items(Page::Paused)[3];
        assert_eq!((label(entry, &on), label(entry, &off)), ("Dev mode: on".to_string(), "Dev mode: off".to_string()));
        assert_eq!(label(items(Page::Paused)[0], &on), "Resume");
        nav.set_cursor(1);
        assert_eq!(nav.activate(), Some(Outcome::CycleSpeed));
        assert_eq!(label(items(Page::Paused)[1], &Shown { speed: GameSpeed { paused: false, times: 4 }, ..on }), "Game speed: x4");
    }

    #[test]
    fn the_audio_page_opens_from_the_main_and_the_pause_menus() {
        for (root, at) in [(Page::Main, 2), (Page::Paused, 2)] {
            let mut nav = MenuNav::at(root);
            nav.set_cursor(at);
            assert_eq!(nav.activate(), None);
            assert_eq!(nav.page(), Page::Audio);
            nav.set_cursor(4);
            assert_eq!(nav.activate(), None, "Back");
            assert_eq!((nav.page(), nav.cursor()), (root, at));
        }
    }

    #[test]
    fn audio_entries_change_and_show_the_mixer() {
        let mut nav = MenuNav::at(Page::Audio);
        let music = Slot::Channel(Channel::Music);
        nav.set_cursor(1);
        assert_eq!(nav.activate(), Some(Outcome::CycleVolume(music)));
        assert_eq!(nav.nudge(-1), Some(Outcome::StepVolume(music, -10)));
        assert_eq!(nav.nudge(1), Some(Outcome::StepVolume(music, 10)));
        nav.set_cursor(3);
        assert_eq!(nav.activate(), Some(Outcome::ToggleMuteAll));
        assert_eq!(nav.nudge(1), None, "only volumes move sideways");
        let mixer = Mixer { music: Level::new(60), mute_all: true, ..Mixer::default() };
        let shown = Shown { dev: false, speed: GameSpeed::default(), mixer };
        assert_eq!(label(items(Page::Audio)[1], &shown), "Music: 60%");
        assert_eq!(label(items(Page::Audio)[3], &shown), "[x] Mute all");
        assert_eq!(label(items(Page::Audio)[3], &Shown { mixer: Mixer::default(), ..shown }), "[ ] Mute all");
        assert_eq!(device_note(true, false), None);
        assert_eq!(device_note(false, true), Some("Audio device off (--mute)"));
    }

    #[test]
    fn the_pause_menu_opens_the_shortcuts_page_and_back() {
        let mut nav = MenuNav::at(Page::Paused);
        nav.step(4);
        assert_eq!(nav.activate(), None);
        assert_eq!(nav.page(), Page::Shortcuts);
        assert_eq!(nav.activate(), None, "Back");
        assert_eq!((nav.page(), nav.cursor()), (Page::Paused, 4));
    }

    #[test]
    fn escape_on_a_root_page() {
        assert_eq!(escape_outcome(Page::Paused), Some(Outcome::Resume));
        assert_eq!(escape_outcome(Page::SandboxPaused), Some(Outcome::Resume));
        assert_eq!(escape_outcome(Page::Main), None, "left to free the mouse");
    }

    #[test]
    fn start_picked_from_env() {
        assert_eq!(start_from_env(None, false), None);
        assert_eq!(start_from_env(None, true), Some(Start::NewGame), "screenshots skip the menu");
        assert_eq!(start_from_env(Some("menu"), true), None);
        assert_eq!(start_from_env(Some("sandbox-walk"), false), Some(Start::SandboxWalk));
        assert_eq!(start_from_env(Some("sandbox-units"), false), Some(Start::SandboxUnits));
        assert_eq!(start_from_env(Some("sandbox-buildings"), false), Some(Start::SandboxBuildings));
        assert_eq!(start_from_env(Some("sandbox-worship"), false), Some(Start::SandboxWorship));
        assert_eq!(start_from_env(Some("sandbox-spells"), false), Some(Start::SandboxSpells));
        assert_eq!(start_from_env(Some("sandbox-vehicles"), false), Some(Start::SandboxVehicles));
        assert_eq!(start_from_env(Some("sandbox-bridge"), false), Some(Start::SandboxBridge));
    }

    #[test]
    fn a_sandbox_pauses_with_restart_a_game_without() {
        use crate::boot::{GameStep, LoadTarget};
        let playing = |start| LoadTarget::Game { start, step: GameStep::Built };
        assert_eq!(paused_page(&playing(Start::SandboxWhirlwind)), Page::SandboxPaused);
        assert_eq!(paused_page(&playing(Start::SandboxWalk)), Page::SandboxPaused);
        assert_eq!(paused_page(&playing(Start::NewGame)), Page::Paused);
        assert!(!items(Page::Paused).iter().any(|e| e.1 == Action::Restart));
        let mut nav = MenuNav::at(Page::SandboxPaused);
        nav.set_cursor(1);
        assert_eq!(nav.activate(), Some(Outcome::Restart));
        assert_eq!(start_from_env(Some("sandbox-whirlwind"), false), Some(Start::SandboxWhirlwind));
    }
}
