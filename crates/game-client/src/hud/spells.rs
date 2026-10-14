//! Spells tab: a grid of tiles mirroring the player's `SpellBook`.
//! Tile states: empty (hidden), "?" (discoverable), gray with uses left (provided),
//! gold with charge pips + recharge bar (known), gold marked "free" (unlimited, sandbox). Click selects,
//! right click pauses / resumes a known spell's recharge (it then takes no mana).
//! Spells cast on a spot (`ground_spell`, Teleport) are aimed with the mouse: the cursor shows the
//! spell, grayed where it cannot apply; left click casts and puts the spell away (back to the arrow
//! and the units' selection), right click puts it away without casting. The other
//! spells are cast with C (demo: uses a charge, the shaman jumps).

use super::panel::{TabContent, DARK_BROWN, INK, PANEL_WIDTH};
use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::grounded::pick_ground;
use crate::units::{world_units, PLAYER};
use crate::virtual_cursor::CursorLook;
use crate::world::{CurrentMap, LevelList};
use bevy::prelude::*;
use std::collections::HashMap;
use game_core::command::Command;
use game_core::map::GameMap;
use game_core::spell::Spell;
use game_core::spell_book::{Availability, SpellBook, SpellKind, SpellSlot, MAX_CHARGES};
use pop3_format::WORLD_UNITS_PER_CELL;

#[derive(Resource, Default)]
pub struct SelectedSpell(pub Option<SpellKind>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileStyle {
    /// Not in this level: a faint empty slot.
    Empty,
    /// To discover: "?".
    Unknown,
    /// A gift (uses found on the map, no recharge): light grey.
    Gift,
    /// The tribe's own spell: the panel's yellow.
    Own,
}

/// One of a tile's charge dots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dot {
    /// A charge ready to cast: blue.
    Loaded,
    /// A gifted use: red.
    Gift,
    /// Room for a charge, empty: a hollow.
    Empty,
}

/// Everything a tile shows, derived from a slot. Pure, so it is unit-tested.
#[derive(Clone, Debug, PartialEq)]
pub struct TileView {
    pub style: TileStyle,
    pub label: &'static str,
    /// Its charge dots, left to right: as many as the spell holds.
    pub dots: Vec<Dot>,
    /// The charge on its way: mana given and the cost of a charge (the bar's fill and finer layer).
    pub progress: Option<(u32, u32)>,
    pub badge: Option<String>,
}

/// `loaded` of `room` dots filled with `dot`, the rest hollow (at most `MAX_CHARGES` in all).
fn dots(dot: Dot, loaded: u8, room: u8) -> Vec<Dot> {
    let room = room.max(loaded).min(MAX_CHARGES);
    (0..room).map(|n| if n < loaded { dot } else { Dot::Empty }).collect()
}

pub fn tile_view(slot: &SpellSlot) -> TileView {
    let label = short_label(slot.kind);
    let room = slot.kind.max_charges();
    let base = TileView { style: TileStyle::Empty, label: "", dots: Vec::new(), progress: None, badge: None };
    match slot.availability {
        Availability::Hidden => base,
        Availability::Discoverable => TileView { style: TileStyle::Unknown, label: "?", ..base },
        Availability::Provided { shots } => TileView { style: TileStyle::Gift, label, dots: dots(Dot::Gift, shots, room), ..base },
        Availability::Unlimited => TileView { style: TileStyle::Own, label, dots: dots(Dot::Loaded, room, room), badge: Some("free".into()), ..base },
        Availability::Known => TileView {
            style: TileStyle::Own,
            label,
            dots: dots(Dot::Loaded, slot.charges, room),
            progress: slot.is_recharging().then(|| (slot.recharge, slot.kind.cost())),
            badge: slot.paused.then(|| "paused".into()),
        },
    }
}

/// One-line explanation shown under the grid for the hovered/selected spell.
pub fn describe(slot: &SpellSlot) -> String {
    let name = slot.kind.name();
    match slot.availability {
        Availability::Hidden => String::new(),
        Availability::Discoverable => "Unknown spell\nDiscover it (worship a totem) to use it.".into(),
        Availability::Provided { shots } => {
            format!("{name}\nProvided: {shots} use{} left, no recharge.", if shots == 1 { "" } else { "s" })
        }
        Availability::Unlimited => format!("{name}\nUnlimited uses."),
        Availability::Known => {
            let max = slot.kind.max_charges();
            let percent = 100.0 * slot.recharge as f32 / slot.kind.cost() as f32;
            let state = match (slot.paused, slot.is_recharging()) {
                (true, _) => format!("paused at {percent:.0}% (right click to resume)"),
                (false, true) => format!("recharging {percent:.0}% (right click to pause)"),
                (false, false) => "full".into(),
            };
            format!("{name}\n{}/{max} charges, {state}. Cost {} mana.", slot.charges, slot.kind.cost())
        }
    }
}

pub fn short_label(kind: SpellKind) -> &'static str {
    match kind {
        SpellKind::Blast => "Blast",
        SpellKind::Convert => "Convert",
        SpellKind::Swarm => "Swarm",
        SpellKind::Invisibility => "Invis",
        SpellKind::Hypnotism => "Hypno",
        SpellKind::Whirlwind => "Whirl",
        SpellKind::LandBridge => "Bridge",
        SpellKind::Lightning => "Bolt",
        SpellKind::Flatten => "Flatten",
        SpellKind::Erosion => "Erode",
        SpellKind::Swamp => "Swamp",
        SpellKind::Earthquake => "Quake",
        SpellKind::Firestorm => "Fire",
        SpellKind::AngelOfDeath => "Angel",
        SpellKind::Volcano => "Volcano",
        SpellKind::Armageddon => "Armageddon",
        SpellKind::GhostArmy => "Ghosts",
        SpellKind::MagicalShield => "Shield",
        SpellKind::Teleport => "Teleport",
        SpellKind::Bloodlust => "Blood",
    }
}

/// The spell to cast when `kind` is aimed at `at` from `origin` (world units: the shaman, or the view on a
/// god level, `cast_origin`); None for spells not cast on a spot (yet): those go with C.
pub fn ground_spell(kind: SpellKind, at: (u16, u16), origin: (u16, u16)) -> Option<Spell> {
    let cell = |(x, z): (u16, u16)| ((x as u32 / WORLD_UNITS_PER_CELL) as i32, (z as u32 / WORLD_UNITS_PER_CELL) as i32);
    match kind {
        SpellKind::Teleport => Some(Spell::Teleport { to: at }),
        SpellKind::Blast => Some(Spell::Blast { at }),
        SpellKind::Lightning => Some(Spell::Lightning { at }),
        SpellKind::Armageddon => Some(Spell::Armageddon { at }),
        SpellKind::Swamp => Some(Spell::Swamp { at }),
        SpellKind::LandBridge => Some(Spell::LandBridge { from: cell(origin), to: cell(at) }),
        SpellKind::Flatten => Some(Spell::Flatten { at: game_core::flatten::nearest_point(at) }),
        _ => None,
    }
}

/// Where a god's spells start (`GameMap::shaman_omni`): the bottom middle of the map view, right of the panel
/// (window pixels).
pub fn view_bottom_middle(window: Vec2) -> Vec2 {
    Vec2::new(PANEL_WIDTH + (window.x - PANEL_WIDTH) / 2.0, window.y - 1.0)
}

/// The panel's spells, row by row, each row centred, as on the original's panel.
pub const ROWS: [&[SpellKind]; 6] = {
    use SpellKind::*;
    [
        &[AngelOfDeath, Volcano],
        &[Earthquake, Erosion, Firestorm],
        &[Whirlwind, Swamp, Flatten],
        &[LandBridge, Lightning, Hypnotism],
        &[Swarm, Invisibility, MagicalShield],
        &[GhostArmy, Blast, Convert],
    ]
};

/// A hidden spell that takes another's place on the panel when the level gives it: Bloodlust, Ghost Army's.
pub const STAND_INS: [(SpellKind, SpellKind); 1] = [(SpellKind::Bloodlust, SpellKind::GhostArmy)];

/// Under the rows: Teleport, a sandbox spell, in a row shown only when the book has it.
pub const SANDBOX_ROW: [SpellKind; 1] = [SpellKind::Teleport];

/// The panel's icon of a spell (`hfx0-0.dat` 1057-1074, drawn in the panel's dark ink): Armageddon's scales at 1057,
/// models 2-17 in order from 1058 (Blast; Angel of Death, 13, is 1069), Teleport at 1074. None for the spells without
/// one (Magical Shield, Bloodlust): their name stands in.
pub fn panel_glyph(kind: SpellKind) -> Option<usize> {
    match kind {
        SpellKind::Armageddon => Some(1057),
        SpellKind::Teleport => Some(1074),
        SpellKind::MagicalShield | SpellKind::Bloodlust => None,
        k => Some(1056 + k.model() as usize),
    }
}

/// The panel's "?" of a spell to discover (`hfx0-0.dat`; 1056 is the same, smaller), drawn as it is.
pub const UNKNOWN_GLYPH: usize = 1055;

/// The panel's icon of a spell without the original files (`assets/spell-hud`, baked from
/// `assets/3d/spell-hud/*.svg`): None for the spells without one yet, which show their name instead
/// (as `panel_glyph` does for Magical Shield and Bloodlust).
pub fn generated_glyph_path(kind: SpellKind) -> Option<&'static str> {
    let name = match kind {
        SpellKind::Blast => "blast",
        SpellKind::Lightning => "lightning",
        SpellKind::Whirlwind => "whirlwind",
        SpellKind::Swarm => "swarm",
        SpellKind::Invisibility => "invisibility",
        SpellKind::Hypnotism => "hypnotism",
        SpellKind::Firestorm => "firestorm",
        SpellKind::GhostArmy => "ghostarmy",
        SpellKind::Erosion => "erosion",
        SpellKind::Swamp => "swamp",
        SpellKind::LandBridge => "land-bridge",
        SpellKind::AngelOfDeath => "angel-of-death",
        SpellKind::Earthquake => "earthquake",
        SpellKind::Flatten => "flatten",
        SpellKind::Convert | SpellKind::Volcano | SpellKind::Armageddon | SpellKind::MagicalShield | SpellKind::Teleport | SpellKind::Bloodlust => return None,
    };
    Some(name)
}

/// `generated_glyph_path`'s name as the asset path under `assets/`.
fn generated_glyph_asset(kind: SpellKind) -> Option<String> {
    generated_glyph_path(kind).map(|name| format!("spell-hud/{name}.png"))
}

/// A tile's width and height, pixels.
const TILE: f32 = 58.0;

/// The special spell with its own full-width tile under the rows, kept free even while hidden so
/// the panel never has to make room for it.
pub fn is_special(kind: SpellKind) -> bool {
    kind == SpellKind::Armageddon
}

/// The slot (index in `slots`) of `kind`.
fn slot_of(slots: &[SpellSlot], kind: SpellKind) -> Option<usize> {
    slots.iter().position(|s| s.kind == kind)
}

/// The player's spells on `map` (`GameMap::spell_books`).
pub fn player_book(map: &GameMap) -> &SpellBook {
    &map.spell_books[PLAYER as usize]
}

#[derive(Component)]
struct Tile(usize);
#[derive(Component)]
struct TileLabel(usize);
#[derive(Component)]
struct TileIcon(usize);
#[derive(Component)]
struct TileBadge(usize);
#[derive(Component)]
struct DotView(usize, u8);
/// The bar of slot `i`'s tile (`progress_bar`).
#[derive(Component)]
struct ChargeBar(usize);
#[derive(Component)]
struct SpellInfo;

/// The panel's icons without the original files (`generated_glyph_path`), loaded once: whichever of this
/// and `IconImages`'s original sprites applies to `LevelList.original` is the one with anything in it.
#[derive(Resource, Default)]
struct GeneratedSpellIcons(HashMap<SpellKind, Handle<Image>>);

fn load_generated_icons(levels: Res<LevelList>, assets: Res<AssetServer>, mut icons: ResMut<GeneratedSpellIcons>) {
    if levels.original {
        return;
    }
    icons.0 = SpellKind::ALL.iter().filter_map(|&k| Some((k, assets.load(generated_glyph_asset(k)?)))).collect();
}

pub struct SpellsPlugin;

impl Plugin for SpellsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedSpell>()
            .init_resource::<GeneratedSpellIcons>()
            .add_systems(Startup, (spawn_tab, load_generated_icons))
            .add_systems(
                Update,
                (
                    (tile_clicks.after(crate::units::SimStep), pause_clicks, cast_selected, aim_and_cast.after(crate::units::UnitInput).after(crate::hover::HoverSystems))
                        .chain()
                        .in_set(crate::menu::Gameplay)
                        .run_if(not(crate::armageddon::input_locked)),
                    (update_tiles.before(crate::progress_bar::DrawBars), show_given_rows),
                    update_info,
                )
                    .chain(),
            );
    }
}

fn spawn_tab(mut commands: Commands, tabs: Query<(Entity, &TabContent)>) {
    if let Some((entity, _)) = tabs.iter().find(|(_, t)| t.0 == 0) {
        commands.entity(entity).with_children(|c| spawn_spells(c, &SpellBook::new().slots));
    }
}

/// A row or tile of the panel shown only while the player's book has one of these spells (Teleport's row,
/// Bloodlust's tile).
#[derive(Component)]
struct GivenOnly(Vec<usize>);

/// A tile hidden while the book has this spell, which takes its place (Ghost Army's, for Bloodlust: `STAND_INS`).
#[derive(Component)]
struct UnlessGiven(usize);

fn spawn_spells(c: &mut ChildSpawnerCommands, slots: &[SpellSlot]) {
    let row = Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::Center, column_gap: px(4), ..default() };
    c.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(4), ..default() }).with_children(|rows| {
        for kinds in ROWS {
            rows.spawn(row.clone()).with_children(|r| {
                for &kind in kinds.iter() {
                    let Some(i) = slot_of(slots, kind) else { continue };
                    let stand_in = STAND_INS.iter().find(|s| s.1 == kind).and_then(|s| slot_of(slots, s.0));
                    if let Some(j) = stand_in {
                        spawn_tile(r, j, px(TILE)).insert(GivenOnly(vec![j]));
                    }
                    let mut tile = spawn_tile(r, i, px(TILE));
                    if let Some(j) = stand_in {
                        tile.insert(UnlessGiven(j));
                    }
                }
            });
        }
        let sandbox: Vec<usize> = SANDBOX_ROW.iter().filter_map(|&k| slot_of(slots, k)).collect();
        rows.spawn((row.clone(), GivenOnly(sandbox.clone()))).with_children(|r| {
            for &i in &sandbox {
                spawn_tile(r, i, px(TILE));
            }
        });
        // The special spell has a row of its own, the whole width.
        for i in (0..slots.len()).filter(|&i| is_special(slots[i].kind)) {
            spawn_tile(rows, i, percent(100));
        }
    });
    c.spawn((
        SpellInfo,
        Text::new(""),
        TextFont { font_size: FontSize::Px(12.0), ..default() },
        TextColor(INK),
        Node { min_height: px(48), ..default() },
    ));
}

/// The tile of slot `i`, `width` wide: its charge dots on top, its icon taking most of the room, its charge bar at
/// the bottom (`progress_bar`, as a training unit's).
fn spawn_tile<'a>(parent: &'a mut ChildSpawnerCommands, i: usize, width: Val) -> EntityCommands<'a> {
    let mut tile = parent.spawn((
        Tile(i),
        Button,
        Node {
            width,
            height: px(TILE),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            padding: UiRect::all(px(3)),
            border: UiRect::all(px(2)),
            ..default()
        },
        BackgroundColor(Color::NONE),
        BorderColor::all(Color::NONE),
    ));
    tile.with_children(|tile| {
        tile.spawn(Node { column_gap: px(3), height: px(DOT), ..default() }).with_children(|row| {
            for n in 0..MAX_CHARGES {
                let dot = Node { width: px(DOT), height: px(DOT), border: UiRect::all(px(1)), border_radius: BorderRadius::MAX, ..default() };
                row.spawn((DotView(i, n), dot, BackgroundColor(Color::NONE), BorderColor::all(Color::NONE)));
            }
        });
        tile.spawn(Node { flex_grow: 1.0, flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() })
            .with_children(|icon| {
                icon.spawn((TileIcon(i), ImageNode::default().with_color(INK), Node { display: Display::None, height: px(30), ..default() }));
                icon.spawn((TileLabel(i), Text::new(""), TextFont { font_size: FontSize::Px(13.0), ..default() }, TextColor(INK)));
                icon.spawn((TileBadge(i), Text::new(""), TextFont { font_size: FontSize::Px(10.0), ..default() }, TextColor(INK)));
            });
        let bar = crate::progress_bar::ProgressBar::horizontal(crate::progress_bar::FILL, true);
        crate::progress_bar::spawn(tile, bar, Node { width: percent(100), height: px(4), ..default() }).insert(ChargeBar(i));
    });
    tile
}

/// Rows and tiles shown only while the book has one of their spells (`GivenOnly`), tiles hidden while it has the one
/// taking their place (`UnlessGiven`).
fn show_given_rows(map: Res<CurrentMap>, mut parts: Query<(Option<&GivenOnly>, Option<&UnlessGiven>, &mut Node), Or<(With<GivenOnly>, With<UnlessGiven>)>>) {
    let book = player_book(&map.0);
    let given = |i: usize| book.slots.get(i).is_some_and(|s| s.availability != Availability::Hidden);
    for (only, unless, mut node) in &mut parts {
        let shown = only.is_none_or(|o| o.0.iter().any(|&i| given(i))) && unless.is_none_or(|u| !given(u.0));
        let display = if shown { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
}

/// A charge dot's size, pixels, and its colours: loaded, gifted, and a hollow's (a dark pit, a lit lower rim).
const DOT: f32 = 8.0;
const LOADED: Color = Color::srgb(0.25, 0.55, 1.0);
const GIFTED: Color = Color::srgb(0.85, 0.15, 0.1);
const HOLLOW: Color = Color::srgba(0.0, 0.0, 0.0, 0.3);
const HOLLOW_RIM: Color = Color::srgba(0.30, 0.18, 0.06, 0.6);

/// A dot's fill and rim.
fn dot_colors(dot: Dot) -> (Color, Color) {
    match dot {
        Dot::Loaded => (LOADED, DARK_BROWN),
        Dot::Gift => (GIFTED, DARK_BROWN),
        Dot::Empty => (HOLLOW, HOLLOW_RIM),
    }
}

fn style_colors(style: TileStyle) -> (Color, Color) {
    match style {
        TileStyle::Empty => (Color::srgba(0.0, 0.0, 0.0, 0.10), Color::NONE),
        TileStyle::Unknown => (Color::srgb(0.36, 0.24, 0.10), DARK_BROWN),
        TileStyle::Gift => (Color::srgb(0.80, 0.80, 0.78), Color::srgb(0.35, 0.35, 0.35)),
        TileStyle::Own => (super::panel::PARCHMENT, DARK_BROWN),
    }
}

fn tile_clicks(
    q: Query<(&Interaction, &Tile), Changed<Interaction>>,
    map: Res<CurrentMap>,
    mut selected: ResMut<SelectedSpell>,
    mut blueprint: ResMut<crate::blueprint::Blueprint>,
) {
    let book = player_book(&map.0);
    for (interaction, tile) in &q {
        let slot = &book.slots[tile.0];
        if *interaction == Interaction::Pressed && slot.availability != Availability::Hidden {
            selected.0 = Some(slot.kind);
            blueprint.put_away();
        }
    }
}

/// The command a right click on `slot`'s tile sends: pause or resume its recharge, only for a spell discovered and
/// recharging with mana (known).
pub fn pause_toggle(slot: &SpellSlot) -> Option<Command> {
    (slot.availability == Availability::Known).then_some(Command::PauseSpell { player: PLAYER, kind: slot.kind, paused: !slot.paused })
}

/// Right click on a spell's tile toggles its pause (`pause_toggle`).
fn pause_clicks(mouse: Res<ButtonInput<MouseButton>>, tiles: Query<(&Interaction, &Tile)>, map: Res<CurrentMap>, mut schedule: ResMut<crate::units::GameSchedule>) {
    let book = player_book(&map.0);
    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }
    if let Some(command) = tiles.iter().find(|(i, _)| **i == Interaction::Hovered).and_then(|(_, tile)| pause_toggle(&book.slots[tile.0])) {
        schedule.issue(command);
    }
}

/// Spells not cast on a spot: C makes the shaman jump (the spell effects come later).
fn cast_selected(
    keys: crate::keymap::Shortcuts,
    selected: Res<SelectedSpell>,
    mut schedule: ResMut<crate::units::GameSchedule>,
) {
    if let (true, Some(kind)) = (keys.just_pressed(crate::keymap::Shortcut::CastSelf), selected.0) && ground_spell(kind, (0, 0), (0, 0)).is_none() {
        schedule.issue(Command::CastSelf { player: PLAYER, kind });
    }
}

/// Spell aimed on the map: cursor look under the mouse, left click casts it there when it
/// applies, then puts the spell away; right click puts it away. The selection of units is kept.
#[allow(clippy::too_many_arguments)]
fn aim_and_cast(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    ui: Query<&Interaction>,
    cams: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    params: Res<CurveParamsRes>,
    rig: Res<CameraRig>,
    map: Res<CurrentMap>,
    mut schedule: ResMut<crate::units::GameSchedule>,
    mut selected: ResMut<SelectedSpell>,
    mut look: ResMut<CursorLook>,
    hovered: Res<crate::hover::Hovered>,
) {
    let cursor = windows.iter().next().and_then(Window::cursor_position);
    let over_ui = ui.iter().any(|i| *i != Interaction::None);
    let on_map = cursor.filter(|c| c.x > PANEL_WIDTH && !over_ui);
    let (Some(kind), Some(c)) = (selected.0, on_map) else {
        // Only its own look: a blueprint sets the cursor too.
        if matches!(*look, CursorLook::Spell { .. }) {
            look.set_if_neq(CursorLook::Arrow);
        }
        return;
    };
    if mouse.just_pressed(MouseButton::Right) {
        selected.0 = None;
        look.set_if_neq(CursorLook::Arrow);
        return;
    }
    let pick = |at: Vec2| {
        cams.iter().next().and_then(|cam| {
            let ray = cam.0.viewport_to_world(cam.1, at).ok()?;
            pick_ground(&map.0.terrain, rig.focus, &params.0, ray.origin, *ray.direction)
        })
    };
    let origin = if map.0.shaman_omni {
        let window = windows.iter().next().map(|w| w.size()).unwrap_or_default();
        world_units(pick(view_bottom_middle(window)).unwrap_or(rig.focus))
    } else {
        map.0.shaman_of(PLAYER).map_or((0, 0), |u| (u.x, u.z))
    };
    let target = crate::units::selection::hovered_site(&map.0, hovered.0).or_else(|| pick(c).map(world_units));
    let spell = target.and_then(|at| ground_spell(kind, at, origin));
    let ready = player_book(&map.0).slot(kind).is_some_and(|s| s.can_cast());
    let valid = spell.filter(|s| ready && map.0.can_cast(PLAYER, s));
    look.set_if_neq(CursorLook::Spell { kind, valid: valid.is_some() });
    if let (true, Some(spell)) = (mouse.just_pressed(MouseButton::Left), valid) {
        schedule.issue(Command::Cast { player: PLAYER, spell });
        selected.0 = None;
        look.set_if_neq(CursorLook::Arrow);
    }
}


fn update_tiles(
    map: Res<CurrentMap>,
    selected: Res<SelectedSpell>,
    mut tiles: Query<(&Tile, &Interaction, &mut BackgroundColor, &mut BorderColor), Without<DotView>>,
    mut labels: Query<(&TileLabel, &mut Text, &mut TextFont, &mut Node), (Without<TileBadge>, Without<TileIcon>, Without<DotView>)>,
    mut badges: Query<(&TileBadge, &mut Text), Without<TileLabel>>,
    mut glyphs: Query<(&TileIcon, &mut ImageNode, &mut Node), (Without<TileLabel>, Without<DotView>)>,
    (mut icons, mut images, generated): (ResMut<crate::tooltip::IconImages>, ResMut<Assets<Image>>, Res<GeneratedSpellIcons>),
    mut dot_views: Query<(&DotView, &mut BackgroundColor, &mut BorderColor, &mut Node), Without<Tile>>,
    mut bars: Query<(&ChargeBar, &mut crate::progress_bar::ProgressBar, &mut Visibility)>,
) {
    let book = player_book(&map.0);
    let views: Vec<TileView> = book.slots.iter().map(tile_view).collect();
    for (tile, interaction, mut bg, mut border) in &mut tiles {
        let v = &views[tile.0];
        let (fill, edge) = style_colors(v.style);
        bg.set_if_neq(BackgroundColor(fill));
        let is_selected = selected.0 == Some(book.slots[tile.0].kind) && v.style != TileStyle::Empty;
        border.set_if_neq(BorderColor::all(match (is_selected, interaction, v.style) {
            (true, _, _) => Color::WHITE,
            (_, _, TileStyle::Empty) => Color::NONE,
            (_, Interaction::Hovered, _) => Color::srgb(1.0, 0.95, 0.7),
            _ => edge,
        }));
    }
    // The original glyph when there is one, else the generated one (`GeneratedSpellIcons`) in its own
    // colours, else the spell's name. A known spell's original glyph in dark ink, a "?" as it is.
    let glyph = |i: usize, icons: &mut crate::tooltip::IconImages, images: &mut Assets<Image>, generated: &GeneratedSpellIcons| {
        match views[i].style {
            TileStyle::Own | TileStyle::Gift => {
                let kind = book.slots[i].kind;
                let original = panel_glyph(kind).and_then(|index| icons.sprite(index, images)).map(|g| (g.0, INK));
                original.or_else(|| generated.0.get(&kind).map(|h| (h.clone(), Color::WHITE)))
            }
            TileStyle::Unknown => icons.sprite(UNKNOWN_GLYPH, images).map(|g| (g.0, Color::WHITE)),
            TileStyle::Empty => None,
        }
    };
    for (g, mut image, mut node) in &mut glyphs {
        let shown = glyph(g.0, &mut icons, &mut images, &generated);
        let display = if shown.is_some() { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        if let Some((h, ink)) = shown.filter(|(h, ink)| image.image != *h || image.color != *ink) {
            (image.image, image.color) = (h, ink);
        }
    }
    for (l, mut text, mut font, mut node) in &mut labels {
        let v = &views[l.0];
        let display = if glyph(l.0, &mut icons, &mut images, &generated).is_some() { Display::None } else { Display::Flex };
        if node.display != display {
            node.display = display;
        }
        if text.0 != v.label {
            text.0 = v.label.to_string();
        }
        font.font_size = FontSize::Px(if v.style == TileStyle::Unknown { 22.0 } else { 13.0 });
    }
    for (b, mut text) in &mut badges {
        let badge = views[b.0].badge.clone().unwrap_or_default();
        if text.0 != badge {
            text.0 = badge;
        }
    }
    for (dot, mut bg, mut rim, mut node) in &mut dot_views {
        let shown = views[dot.0].dots.get(dot.1 as usize).copied();
        let display = if shown.is_some() { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        if let Some(d) = shown {
            let (fill, edge) = dot_colors(d);
            bg.set_if_neq(BackgroundColor(fill));
            rim.set_if_neq(BorderColor::all(edge));
        }
    }
    for (bar, mut widget, mut vis) in &mut bars {
        // Hidden, not removed, while no charge is on its way: the tile keeps its layout.
        let progress = views[bar.0].progress;
        vis.set_if_neq(if progress.is_some() { Visibility::Inherited } else { Visibility::Hidden });
        let (mana, cost) = progress.unwrap_or((0, 1));
        let shown = crate::progress_bar::ProgressBar { fill: mana as f32 / cost.max(1) as f32, fine: Some((mana, cost)), blocked: book.slots[bar.0].paused, ..*widget };
        if *widget != shown {
            *widget = shown;
        }
    }
}

fn update_info(
    map: Res<CurrentMap>,
    selected: Res<SelectedSpell>,
    tiles: Query<(&Tile, &Interaction)>,
    mut info: Query<&mut Text, With<SpellInfo>>,
) {
    let book = player_book(&map.0);
    let hovered = tiles.iter().find(|(_, i)| **i == Interaction::Hovered).map(|(t, _)| &book.slots[t.0]);
    let selected = selected.0.and_then(|k| book.slot(k));
    let text = hovered.or(selected).map(describe).unwrap_or_else(|| "Click a spell to select it.".into());
    for mut t in &mut info {
        t.0.clone_from(&text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(kind: SpellKind, a: Availability) -> SpellSlot {
        SpellSlot::new(kind, a)
    }

    #[test]
    fn views_per_state() {
        assert_eq!(tile_view(&slot(SpellKind::Blast, Availability::Hidden)).style, TileStyle::Empty);
        let unknown = tile_view(&slot(SpellKind::Blast, Availability::Discoverable));
        assert_eq!((unknown.style, unknown.label), (TileStyle::Unknown, "?"));
        let gift = tile_view(&slot(SpellKind::Blast, Availability::Provided { shots: 2 }));
        assert_eq!((gift.style, gift.progress), (TileStyle::Gift, None), "light grey, no recharge");
        assert_eq!(gift.dots, [Dot::Gift, Dot::Gift, Dot::Empty, Dot::Empty], "red gifted uses in the blast's room of 4");
        let volcano = tile_view(&slot(SpellKind::Volcano, Availability::Provided { shots: 1 }));
        assert_eq!(volcano.dots, [Dot::Gift], "a volcano holds one");
    }

    #[test]
    fn unlimited_is_ready_and_free() {
        let v = tile_view(&slot(SpellKind::Teleport, Availability::Unlimited));
        assert_eq!((v.style, v.label, v.badge.as_deref(), v.progress), (TileStyle::Own, "Teleport", Some("free"), None));
        assert!(v.dots.iter().all(|&d| d == Dot::Loaded));
        assert!(describe(&slot(SpellKind::Teleport, Availability::Unlimited)).contains("Unlimited"));
    }

    #[test]
    fn known_shows_its_charges_and_the_one_on_its_way() {
        let mut s = slot(SpellKind::Blast, Availability::Known);
        assert_eq!(tile_view(&s).dots, [Dot::Empty; 4], "4 hollows: no charge");
        s.charges = 2;
        s.recharge = 20;
        let v = tile_view(&s);
        assert_eq!((v.style, v.progress), (TileStyle::Own, Some((20, 40))));
        assert_eq!(v.dots, [Dot::Loaded, Dot::Loaded, Dot::Empty, Dot::Empty]);
        s.charges = 4;
        s.recharge = 0;
        assert_eq!(tile_view(&s).progress, None, "full spells hide the bar");
    }

    #[test]
    fn right_click_toggles_the_pause_of_known_spells_only() {
        let mut s = slot(SpellKind::Blast, Availability::Known);
        assert_eq!(pause_toggle(&s), Some(Command::PauseSpell { player: PLAYER, kind: SpellKind::Blast, paused: true }));
        s.paused = true;
        assert_eq!(pause_toggle(&s), Some(Command::PauseSpell { player: PLAYER, kind: SpellKind::Blast, paused: false }));
        for a in [Availability::Hidden, Availability::Discoverable, Availability::Provided { shots: 2 }, Availability::Unlimited] {
            assert_eq!(pause_toggle(&slot(SpellKind::Blast, a)), None, "{a:?}");
        }
    }

    #[test]
    fn paused_tiles_say_so() {
        let mut s = slot(SpellKind::Blast, Availability::Known);
        s.charges = 1;
        s.paused = true;
        let v = tile_view(&s);
        assert_eq!((v.badge.as_deref(), v.progress), (Some("paused"), Some((0, 40))), "the bar stays, frozen");
        assert!(describe(&s).contains("paused"));
        s.paused = false;
        assert!(describe(&s).contains("right click to pause"));
    }

    #[test]
    fn describe_mentions_state() {
        assert!(describe(&slot(SpellKind::Flatten, Availability::Discoverable)).contains("Discover"));
        assert!(describe(&slot(SpellKind::Volcano, Availability::Provided { shots: 1 })).contains("1 use left"));
    }

    #[test]
    fn only_spot_spells_are_aimed() {
        assert_eq!(ground_spell(SpellKind::Teleport, (5, 6), (0, 0)), Some(Spell::Teleport { to: (5, 6) }));
        assert_eq!(ground_spell(SpellKind::Blast, (5, 6), (0, 0)), Some(Spell::Blast { at: (5, 6) }));
        assert_eq!(ground_spell(SpellKind::LandBridge, (5 * 512 + 9, 6 * 512), (512, 1023)), Some(Spell::LandBridge { from: (1, 1), to: (5, 6) }));
        assert_eq!(ground_spell(SpellKind::Lightning, (700, 900), (0, 0)), Some(Spell::Lightning { at: (700, 900) }));
        assert_eq!(ground_spell(SpellKind::Flatten, (5 * 512 + 300, 6 * 512 + 9), (0, 0)), Some(Spell::Flatten { at: (6, 6) }), "the nearest height point");
        assert_eq!(ground_spell(SpellKind::Swarm, (5, 6), (0, 0)), None);
    }

    #[test]
    fn a_gods_spells_start_at_the_bottom_middle_of_the_view() {
        let at = view_bottom_middle(Vec2::new(PANEL_WIDTH + 800.0, 600.0));
        assert_eq!(at, Vec2::new(PANEL_WIDTH + 400.0, 599.0));
    }

    #[test]
    fn the_panels_glyphs_follow_the_spell_models() {
        assert_eq!(panel_glyph(SpellKind::Blast), Some(1058));
        assert_eq!(panel_glyph(SpellKind::AngelOfDeath), Some(1069));
        assert_eq!(panel_glyph(SpellKind::Convert), Some(1073));
        assert_eq!(panel_glyph(SpellKind::Armageddon), Some(1057));
        assert_eq!(panel_glyph(SpellKind::Teleport), Some(1074));
        assert_eq!(panel_glyph(SpellKind::MagicalShield), None);
        let glyphs: std::collections::BTreeSet<usize> = SpellKind::ALL.iter().filter_map(|&k| panel_glyph(k)).collect();
        assert_eq!(glyphs, (1057..=1074).collect(), "1057-1074, each once");
    }

    #[test]
    fn the_generated_glyphs_are_each_a_distinct_file() {
        let names: Vec<&str> = SpellKind::ALL.iter().filter_map(|&k| generated_glyph_path(k)).collect();
        assert_eq!(names.len(), std::collections::BTreeSet::from_iter(names.iter()).len(), "no two spells share a file");
        assert_eq!(generated_glyph_asset(SpellKind::Blast).as_deref(), Some("spell-hud/blast.png"));
        for kind in [SpellKind::Convert, SpellKind::Volcano, SpellKind::Armageddon, SpellKind::MagicalShield, SpellKind::Teleport, SpellKind::Bloodlust] {
            assert_eq!(generated_glyph_path(kind), None, "{kind:?}: no icon yet, its name stands in");
        }
    }

    #[test]
    fn every_spell_has_one_place_on_the_panel() {
        let placed: Vec<SpellKind> = ROWS.iter().flat_map(|r| r.iter().copied()).chain(SANDBOX_ROW).chain(STAND_INS.map(|s| s.0)).chain(SpellKind::ALL.into_iter().filter(|&k| is_special(k))).collect();
        for kind in SpellKind::ALL {
            assert_eq!(placed.iter().filter(|&&k| k == kind).count(), 1, "{kind:?}");
        }
        assert_eq!(ROWS.map(|r| r.len()), [2, 3, 3, 3, 3, 3]);
        assert_eq!(ROWS[5], [SpellKind::GhostArmy, SpellKind::Blast, SpellKind::Convert], "the last row");
        let book = SpellBook::new();
        assert!(SpellKind::ALL.iter().all(|&k| slot_of(&book.slots, k).is_some()));
    }

    #[test]
    fn demo_book_covers_every_state() {
        let styles: Vec<TileStyle> = SpellBook::demo().slots.iter().map(|s| tile_view(s).style).collect();
        for st in [TileStyle::Empty, TileStyle::Unknown, TileStyle::Gift, TileStyle::Own] {
            assert!(styles.contains(&st), "{st:?} missing");
        }
    }
}
