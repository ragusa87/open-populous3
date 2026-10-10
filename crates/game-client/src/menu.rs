//! Main menu shown before the game: New game (the level from the command line / PgUp-PgDn list),
//! Sandbox (test grounds: Walk, Units, Buildings, Worship), Quit. Sub-pages open on Enter and go back with Esc/Backspace or
//! their Back entry; arrows (or W/S) move, Enter/Space picks, the mouse hovers and clicks.
//! Gameplay systems are in the `Gameplay` set, which only runs while `Playing`. Behind the menu the
//! game camera is off: no world, units or HUD are drawn, the menu has its own overlay camera.
//! Esc in the game pauses: the mouse is released and the pause menu (Resume, Main menu with a
//! confirmation) shows over the frozen game; Esc on it resumes. Its Dev mode entry switches `keymap::DevMode`.
//! `POP3_START=menu|game|sandbox-walk|sandbox-units|sandbox-buildings|sandbox-worship` picks where to start (screenshots start in the game).

use crate::camera::{GameCamera, OverlayCamera};
use crate::hud::build::{level_builds, PlayerBuilds};
use crate::hud::spells::{level_book, sandbox_book, PlayerSpells, SelectedSpell};
use crate::units::selection::Selection;
use crate::virtual_cursor::VirtualCursor;
use crate::world::{CurrentMap, LevelList, TerrainDirty};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use game_core::map::GameMap;

const BACKDROP: Color = Color::srgb(0.06, 0.04, 0.02);
/// Over the frozen game while paused (the main menu is drawn on `BACKDROP`).
const DIM: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
const TEXT: Color = Color::srgb(0.95, 0.85, 0.6);
const HIGHLIGHT: Color = Color::srgb(0.80, 0.56, 0.20);
const ITEM: Color = Color::srgba(0.30, 0.18, 0.06, 0.9);

#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppState {
    #[default]
    Menu,
    Playing,
    Paused,
}

/// Systems that only run in the game (input, simulation, HUD actions), not behind the menu.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Gameplay;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Main,
    Sandbox,
    Paused,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Start(Start),
    Open(Page),
    Back,
    Resume,
    ToggleDev,
    Leave,
    Quit,
}

/// What picking an entry leads to outside the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Start(Start),
    Resume,
    ToggleDev,
    /// Leave the game for the main menu.
    Leave,
    Quit,
}

pub fn title(page: Page) -> &'static str {
    match page {
        Page::Main => "Open Populous",
        Page::Sandbox => "Sandbox",
        Page::Paused => "Paused",
        Page::ConfirmLeave => "Leave this game?",
    }
}

pub fn items(page: Page) -> &'static [(&'static str, Action)] {
    match page {
        Page::Main => &[("New game", Action::Start(Start::NewGame)), ("Sandbox", Action::Open(Page::Sandbox)), ("Quit", Action::Quit)],
        Page::Sandbox => &[("Walk", Action::Start(Start::SandboxWalk)), ("Units", Action::Start(Start::SandboxUnits)), ("Buildings", Action::Start(Start::SandboxBuildings)), ("Worship", Action::Start(Start::SandboxWorship)), ("Back", Action::Back)],
        Page::Paused => &[("Resume", Action::Resume), ("Dev mode", Action::ToggleDev), ("Main menu", Action::Open(Page::ConfirmLeave))],
        Page::ConfirmLeave => &[("No, keep playing", Action::Back), ("Yes, back to the main menu", Action::Leave)],
    }
}

/// An entry's text; a toggle shows its state.
pub fn label(entry: (&str, Action), dev: bool) -> String {
    match entry {
        (name, Action::ToggleDev) => format!("{name}: {}", if dev { "on" } else { "off" }),
        (name, _) => name.to_string(),
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
            Action::Resume => Some(Outcome::Resume),
            Action::ToggleDev => Some(Outcome::ToggleDev),
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

    /// Back to the parent page (the entry that opened it stays highlighted); false on the root.
    pub fn back(&mut self) -> bool {
        if self.stack.len() <= 1 {
            return false;
        }
        self.stack.pop();
        true
    }
}

/// `POP3_START` value (or screenshot mode) to the game to start right away; None shows the menu.
pub fn start_from_env(start: Option<&str>, screenshot: bool) -> Option<Start> {
    match start {
        Some("menu") => None,
        Some("game") => Some(Start::NewGame),
        Some("sandbox-walk") => Some(Start::SandboxWalk),
        Some("sandbox-units") => Some(Start::SandboxUnits),
        Some("sandbox-buildings") => Some(Start::SandboxBuildings),
        Some("sandbox-worship") => Some(Start::SandboxWorship),
        _ if screenshot => Some(Start::NewGame),
        _ => None,
    }
}

#[derive(Resource)]
struct InitialStart(Option<Start>);

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
        let start = start_from_env(std::env::var("POP3_START").ok().as_deref(), std::env::var("SCREENSHOT").is_ok());
        app.insert_state(if start.is_some() { AppState::Playing } else { AppState::Menu })
            .insert_resource(InitialStart(start))
            .init_resource::<MenuNav>()
            .configure_sets(Update, Gameplay.run_if(in_state(AppState::Playing)))
            .add_systems(Startup, (spawn_menu, initial_start))
            .add_systems(Update, pause_on_escape.before(crate::virtual_cursor::toggle_capture).in_set(Gameplay))
            .add_systems(
                Update,
                (menu_keys.before(crate::virtual_cursor::toggle_capture), menu_mouse, rebuild_items, item_visuals)
                    .chain()
                    .run_if(not(in_state(AppState::Playing))),
            )
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
        AppState::Menu => View { game: false, clear: Some(BACKDROP), menu: Some(Color::NONE) },
        AppState::Playing => View { game: true, clear: None, menu: None },
        AppState::Paused => View { game: true, clear: None, menu: Some(DIM) },
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
            root.spawn((MenuList, Node { flex_direction: FlexDirection::Column, row_gap: px(8), margin: UiRect::top(px(12)), ..default() }));
            root.spawn((
                Text::new("Up/Down: move   Enter: select   Esc: back"),
                TextFont { font_size: FontSize::Px(13.0), ..default() },
                TextColor(TEXT.with_alpha(0.6)),
                Node { margin: UiRect::top(px(20)), ..default() },
            ));
        });
}

fn initial_start(initial: Res<InitialStart>, mut game: GameSetup) {
    if let Some(start) = initial.0 {
        game.begin(start);
    }
}

/// What starting, pausing and leaving a game touch.
#[derive(SystemParam)]
struct GameSetup<'w> {
    levels: ResMut<'w, LevelList>,
    map: ResMut<'w, CurrentMap>,
    dirty: ResMut<'w, TerrainDirty>,
    selection: ResMut<'w, Selection>,
    cursor: ResMut<'w, VirtualCursor>,
    spells: ResMut<'w, PlayerSpells>,
    builds: ResMut<'w, PlayerBuilds>,
    blueprint: ResMut<'w, crate::blueprint::Blueprint>,
    selected_spell: ResMut<'w, SelectedSpell>,
    grid: ResMut<'w, crate::world::ShowGrid>,
    dev: ResMut<'w, crate::keymap::DevMode>,
}

impl GameSetup<'_> {
    /// Loads the map a game starts on; the camera frames it (`LevelList` change).
    fn begin(&mut self, start: Start) {
        (self.map.0, self.spells.0) = match start {
            Start::NewGame => {
                let map = self.levels.load_current();
                let book = level_book(&map);
                (map, book)
            }
            Start::SandboxWalk => (GameMap::sandbox_walk(), sandbox_book()),
            Start::SandboxUnits => (GameMap::sandbox_units(), sandbox_book()),
            Start::SandboxBuildings => (GameMap::sandbox_buildings(), sandbox_book()),
            Start::SandboxWorship => (GameMap::sandbox_worship(), sandbox_book()),
        };
        self.builds.0 = level_builds(&self.map.0);
        self.grid.0 = start != Start::NewGame;
        self.blueprint.put_away();
        self.selected_spell.0 = None;
        self.dirty.0 = true;
        self.selection.clear();
        self.levels.set_changed();
    }
}

/// Esc in the game (not taken by an open HUD menu): pause and free the mouse.
pub(crate) fn pause_on_escape(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut nav: ResMut<MenuNav>,
    mut cursor: ResMut<VirtualCursor>,
    mut state: ResMut<NextState<AppState>>,
) {
    if keys.clear_just_pressed(KeyCode::Escape) {
        *nav = MenuNav::at(Page::Paused);
        cursor.request = Some(false);
        state.set(AppState::Paused);
    }
}

fn apply(outcome: Option<Outcome>, game: &mut GameSetup, nav: &mut MenuNav, state: &mut NextState<AppState>, exit: &mut MessageWriter<AppExit>) {
    match outcome {
        Some(Outcome::Start(s)) => {
            game.begin(s);
            game.cursor.request = Some(true);
            state.set(AppState::Playing);
        }
        Some(Outcome::Resume) => {
            game.cursor.request = Some(true);
            state.set(AppState::Playing);
        }
        Some(Outcome::ToggleDev) => {
            game.dev.0 = !game.dev.0;
        }
        Some(Outcome::Leave) => {
            *nav = MenuNav::default();
            game.selection.clear();
            state.set(AppState::Menu);
        }
        Some(Outcome::Quit) => {
            exit.write(AppExit::Success);
        }
        None => {}
    }
}

/// Esc on the root page: resumes a paused game (the main menu leaves it to free the mouse).
fn escape_outcome(page: Page) -> Option<Outcome> {
    (page == Page::Paused).then_some(Outcome::Resume)
}

fn menu_keys(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut nav: ResMut<MenuNav>,
    mut game: GameSetup,
    mut state: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
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
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) {
        let outcome = nav.activate();
        apply(outcome, &mut game, &mut nav, &mut state, &mut exit);
    }
}

fn menu_mouse(
    q: Query<(&Interaction, &MenuItem), Changed<Interaction>>,
    mut nav: ResMut<MenuNav>,
    mut game: GameSetup,
    mut state: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, item) in &q {
        match interaction {
            Interaction::Hovered => nav.set_cursor(item.0),
            Interaction::Pressed => {
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
    mut shown: Local<Option<(Page, bool)>>,
    lists: Query<Entity, With<MenuList>>,
    mut titles: Query<&mut Text, With<MenuTitle>>,
) {
    let page = nav.page();
    if *shown == Some((page, dev.0)) {
        return;
    }
    *shown = Some((page, dev.0));
    for mut t in &mut titles {
        t.0 = title(page).to_string();
    }
    for list in &lists {
        commands.entity(list).despawn_children().with_children(|l| {
            for (i, &entry) in items(page).iter().enumerate() {
                l.spawn((
                    MenuItem(i),
                    Button,
                    Node { width: px(260), padding: UiRect::axes(px(16), px(8)), justify_content: JustifyContent::Center, border: UiRect::all(px(2)), ..default() },
                    BackgroundColor(ITEM),
                    BorderColor::all(Color::NONE),
                ))
                .with_child((Text::new(label(entry, dev.0)), TextFont { font_size: FontSize::Px(20.0), ..default() }, TextColor(TEXT)));
            }
        });
    }
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
        assert_eq!(nav.cursor(), 2, "up from the first entry wraps to Quit");
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
        nav.set_cursor(2);
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
        nav.step(2);
        assert_eq!(nav.activate(), None, "Main menu opens the confirmation");
        assert_eq!((nav.page(), nav.cursor()), (Page::ConfirmLeave, 0));
        assert_eq!(nav.activate(), None, "No (highlighted first) goes back");
        assert_eq!((nav.page(), nav.cursor()), (Page::Paused, 2));
        nav.activate();
        nav.step(1);
        assert_eq!(nav.activate(), Some(Outcome::Leave));
    }

    #[test]
    fn dev_mode_toggles_from_the_pause_menu_and_shows_its_state() {
        let mut nav = MenuNav::at(Page::Paused);
        nav.step(1);
        assert_eq!(nav.activate(), Some(Outcome::ToggleDev));
        assert_eq!(nav.page(), Page::Paused, "stays open to see the new state");
        let entry = items(Page::Paused)[1];
        assert_eq!((label(entry, true), label(entry, false)), ("Dev mode: on".to_string(), "Dev mode: off".to_string()));
        assert_eq!(label(items(Page::Paused)[0], true), "Resume");
    }

    #[test]
    fn escape_on_a_root_page() {
        assert_eq!(escape_outcome(Page::Paused), Some(Outcome::Resume));
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
    }
}
