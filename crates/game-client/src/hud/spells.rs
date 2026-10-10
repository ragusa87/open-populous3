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
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::command::Command;
use game_core::map::GameMap;
use game_core::spell::Spell;
use game_core::unit::Order;
use game_core::spell_book::{Availability, SpellBook, SpellKind, SpellSlot, MAX_CHARGES};

/// Mana given to every recharging spell per simulation tick.
const MANA_PER_TICK: u32 = 7;

#[derive(Resource)]
pub struct PlayerSpells(pub SpellBook);

#[derive(Resource, Default)]
pub struct SelectedSpell(pub Option<SpellKind>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileStyle {
    Empty,
    Unknown,
    Provided,
    Ready,
    Depleted,
}

/// Everything a tile shows, derived from a slot. Pure, so it is unit-tested.
#[derive(Clone, Debug, PartialEq)]
pub struct TileView {
    pub style: TileStyle,
    pub label: &'static str,
    pub pips: (u8, u8),
    pub progress: Option<f32>,
    pub badge: Option<String>,
}

pub fn tile_view(slot: &SpellSlot) -> TileView {
    let label = short_label(slot.kind);
    let base = TileView { style: TileStyle::Empty, label: "", pips: (0, 0), progress: None, badge: None };
    match slot.availability {
        Availability::Hidden => base,
        Availability::Discoverable => TileView { style: TileStyle::Unknown, label: "?", ..base },
        Availability::Provided { shots } => {
            TileView { style: TileStyle::Provided, label, badge: Some(format!("x{shots}")), ..base }
        }
        Availability::Unlimited => TileView { style: TileStyle::Ready, label, badge: Some("free".into()), ..base },
        Availability::Known => TileView {
            style: if slot.charges > 0 { TileStyle::Ready } else { TileStyle::Depleted },
            label,
            pips: (slot.charges, slot.kind.max_charges()),
            progress: slot.is_recharging().then(|| slot.recharge as f32 / slot.kind.cost() as f32),
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
    }
}

/// The spell to cast when `kind` is aimed at `at` (world units); None for spells not cast on a
/// spot (yet): those go with C.
pub fn ground_spell(kind: SpellKind, at: (u16, u16)) -> Option<Spell> {
    match kind {
        SpellKind::Teleport => Some(Spell::Teleport { to: at }),
        _ => None,
    }
}

/// Columns of the spell grid.
pub const COLUMNS: usize = 3;

/// The special spell with its own full-width tile under the grid, kept free even while hidden so
/// the panel never has to make room for it.
pub fn is_special(kind: SpellKind) -> bool {
    kind == SpellKind::Armageddon
}

/// Slot indices in drawing order: every other spell in book order, the special one last.
pub fn tile_order(slots: &[SpellSlot]) -> Vec<usize> {
    let (special, others): (Vec<usize>, Vec<usize>) = (0..slots.len()).partition(|&i| is_special(slots[i].kind));
    others.into_iter().chain(special).collect()
}

/// The player's spells on `map`: an original level's own loadout (`GameMap::spell_book`), else
/// the demo one (generated maps).
pub fn level_book(map: &GameMap) -> SpellBook {
    map.spell_book.clone().unwrap_or_else(demo_book)
}

/// Walk sandbox: the demo loadout plus Teleport, free.
pub fn sandbox_book() -> SpellBook {
    let mut b = demo_book();
    b.set(SpellKind::Teleport, Availability::Unlimited);
    b
}

/// Loadout showing every tile state, for maps without one of their own.
pub fn demo_book() -> SpellBook {
    let mut b = SpellBook::new();
    for k in [SpellKind::Blast, SpellKind::Convert, SpellKind::Swarm, SpellKind::LandBridge, SpellKind::Erosion] {
        b.set(k, Availability::Known);
    }
    b.set(SpellKind::Invisibility, Availability::Provided { shots: 3 });
    b.set(SpellKind::Volcano, Availability::Provided { shots: 1 });
    b.set(SpellKind::Flatten, Availability::Discoverable);
    b.set(SpellKind::Lightning, Availability::Discoverable);
    b.tick(100);
    b
}

#[derive(Component)]
struct Tile(usize);
#[derive(Component)]
struct TileLabel(usize);
#[derive(Component)]
struct TileBadge(usize);
#[derive(Component)]
struct Pip(usize, u8);
#[derive(Component)]
struct RechargeBar(usize);
#[derive(Component)]
struct RechargeFill(usize);
#[derive(Component)]
struct SpellInfo;

pub struct SpellsPlugin;

impl Plugin for SpellsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PlayerSpells(demo_book()))
            .init_resource::<SelectedSpell>()
            .add_systems(Startup, spawn_tab)
            .add_systems(
                Update,
                (
                    (recharge.after(crate::units::SimStep), tile_clicks, pause_clicks, cast_selected, aim_and_cast.after(crate::units::UnitInput)).chain().in_set(crate::menu::Gameplay),
                    update_tiles,
                    update_info,
                )
                    .chain(),
            );
    }
}

fn spawn_tab(mut commands: Commands, tabs: Query<(Entity, &TabContent)>, book: Res<PlayerSpells>) {
    if let Some((entity, _)) = tabs.iter().find(|(_, t)| t.0 == 0) {
        commands.entity(entity).with_children(|c| spawn_spells(c, &book.0.slots));
    }
}

fn spawn_spells(c: &mut ChildSpawnerCommands, slots: &[SpellSlot]) {
    c.spawn(Node {
        display: Display::Grid,
        grid_template_columns: RepeatedGridTrack::flex(COLUMNS as u16, 1.0),
        row_gap: px(4),
        column_gap: px(4),
        ..default()
    })
    .with_children(|grid| {
        for i in tile_order(slots) {
            // The special spell starts a row of its own, the whole width.
            let grid_column = if is_special(slots[i].kind) { GridPlacement::start_span(1, COLUMNS as u16) } else { GridPlacement::DEFAULT };
            grid.spawn((
                Tile(i),
                Button,
                Node {
                    grid_column,
                    height: px(58),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(px(3)),
                    border: UiRect::all(px(2)),
                    ..default()
                },
                BackgroundColor(Color::NONE),
                BorderColor::all(Color::NONE),
            ))
            .with_children(|tile| {
                tile.spawn(Node { column_gap: px(2), height: px(8), ..default() }).with_children(|pips| {
                    for n in 0..MAX_CHARGES {
                        pips.spawn((Pip(i, n), Node { width: px(8), height: px(8), ..default() }, BackgroundColor(Color::NONE)));
                    }
                });
                tile.spawn((TileLabel(i), Text::new(""), TextFont { font_size: FontSize::Px(12.0), ..default() }, TextColor(INK)));
                tile.spawn((TileBadge(i), Text::new(""), TextFont { font_size: FontSize::Px(11.0), ..default() }, TextColor(INK)));
                tile.spawn((RechargeBar(i), Node { width: percent(100), height: px(4), ..default() }, BackgroundColor(DARK_BROWN)))
                    .with_child((RechargeFill(i), Node { width: percent(0), height: percent(100), ..default() }, BackgroundColor(MANA_BLUE)));
            });
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

const MANA_BLUE: Color = Color::srgb(0.25, 0.55, 1.0);
const PAUSED_GREY: Color = Color::srgb(0.5, 0.5, 0.5);

fn style_colors(style: TileStyle) -> (Color, Color) {
    match style {
        TileStyle::Empty => (Color::srgba(0.0, 0.0, 0.0, 0.10), Color::NONE),
        TileStyle::Unknown => (Color::srgb(0.36, 0.24, 0.10), DARK_BROWN),
        TileStyle::Provided => (Color::srgb(0.66, 0.66, 0.64), Color::srgb(0.35, 0.35, 0.35)),
        TileStyle::Ready => (Color::srgb(1.0, 0.84, 0.48), DARK_BROWN),
        TileStyle::Depleted => (Color::srgb(0.62, 0.48, 0.28), DARK_BROWN),
    }
}

fn recharge(clock: Res<crate::units::SimClock>, mut book: ResMut<PlayerSpells>) {
    for _ in 0..clock.ran {
        book.0.tick(MANA_PER_TICK);
    }
}

fn tile_clicks(
    q: Query<(&Interaction, &Tile), Changed<Interaction>>,
    book: Res<PlayerSpells>,
    mut selected: ResMut<SelectedSpell>,
    mut blueprint: ResMut<crate::blueprint::Blueprint>,
) {
    for (interaction, tile) in &q {
        let slot = &book.0.slots[tile.0];
        if *interaction == Interaction::Pressed && slot.availability != Availability::Hidden {
            selected.0 = Some(slot.kind);
            blueprint.put_away();
        }
    }
}

/// Right click on a known spell's tile pauses or resumes its recharge.
fn pause_clicks(mouse: Res<ButtonInput<MouseButton>>, tiles: Query<(&Interaction, &Tile)>, mut book: ResMut<PlayerSpells>) {
    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }
    if let Some((_, tile)) = tiles.iter().find(|(i, _)| **i == Interaction::Hovered) {
        let kind = book.0.slots[tile.0].kind;
        book.0.toggle_pause(kind);
    }
}

/// Spells not cast on a spot: C makes the shaman jump (the spell effects come later).
fn cast_selected(
    keys: crate::keymap::Shortcuts,
    mut book: ResMut<PlayerSpells>,
    selected: Res<SelectedSpell>,
    mut schedule: ResMut<crate::units::GameSchedule>,
) {
    if let (true, Some(kind)) = (keys.just_pressed(crate::keymap::Shortcut::CastSelf), selected.0) && ground_spell(kind, (0, 0)).is_none() && book.0.cast(kind) {
        schedule.issue(Command::Order { player: PLAYER, order: Order::Cast });
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
    mut book: ResMut<PlayerSpells>,
    mut selected: ResMut<SelectedSpell>,
    mut look: ResMut<CursorLook>,
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
    let ground = cams.iter().next().and_then(|cam| {
        let ray = cam.0.viewport_to_world(cam.1, c).ok()?;
        pick_ground(&map.0.terrain, rig.focus, &params.0, ray.origin, *ray.direction)
    });
    let spell = ground.and_then(|cell| ground_spell(kind, world_units(cell)));
    let ready = book.0.slot(kind).is_some_and(|s| s.can_cast());
    let valid = spell.filter(|s| ready && map.0.can_cast(PLAYER, s));
    look.set_if_neq(CursorLook::Spell { kind, valid: valid.is_some() });
    if let (true, Some(spell)) = (mouse.just_pressed(MouseButton::Left), valid) && book.0.cast(kind) {
        schedule.issue(Command::Cast { player: PLAYER, spell });
        selected.0 = None;
        look.set_if_neq(CursorLook::Arrow);
    }
}


fn update_tiles(
    book: Res<PlayerSpells>,
    selected: Res<SelectedSpell>,
    mut tiles: Query<(&Tile, &Interaction, &mut BackgroundColor, &mut BorderColor)>,
    mut labels: Query<(&TileLabel, &mut Text, &mut TextFont), Without<TileBadge>>,
    mut badges: Query<(&TileBadge, &mut Text), Without<TileLabel>>,
    mut pips: Query<(&Pip, &mut BackgroundColor, &mut Node), (Without<Tile>, Without<RechargeFill>, Without<RechargeBar>)>,
    mut bars: Query<(&RechargeBar, &mut Node), (Without<Pip>, Without<RechargeFill>)>,
    mut fills: Query<(&RechargeFill, &mut Node, &mut BackgroundColor), (Without<Pip>, Without<RechargeBar>, Without<Tile>)>,
) {
    let views: Vec<TileView> = book.0.slots.iter().map(tile_view).collect();
    for (tile, interaction, mut bg, mut border) in &mut tiles {
        let v = &views[tile.0];
        let (fill, edge) = style_colors(v.style);
        bg.0 = fill;
        let is_selected = selected.0 == Some(book.0.slots[tile.0].kind) && v.style != TileStyle::Empty;
        *border = BorderColor::all(match (is_selected, interaction, v.style) {
            (true, _, _) => Color::WHITE,
            (_, _, TileStyle::Empty) => Color::NONE,
            (_, Interaction::Hovered, _) => Color::srgb(1.0, 0.95, 0.7),
            _ => edge,
        });
    }
    for (l, mut text, mut font) in &mut labels {
        let v = &views[l.0];
        text.0 = v.label.to_string();
        font.font_size = FontSize::Px(if v.style == TileStyle::Unknown { 22.0 } else { 12.0 });
    }
    for (b, mut text) in &mut badges {
        text.0 = views[b.0].badge.clone().unwrap_or_default();
    }
    for (pip, mut bg, mut node) in &mut pips {
        let (filled, max) = views[pip.0].pips;
        node.display = if pip.1 < max { Display::Flex } else { Display::None };
        bg.0 = if pip.1 < filled { MANA_BLUE } else { DARK_BROWN };
    }
    for (bar, mut node) in &mut bars {
        node.display = if views[bar.0].progress.is_some() { Display::Flex } else { Display::None };
    }
    for (fill, mut node, mut bg) in &mut fills {
        node.width = percent(views[fill.0].progress.unwrap_or(0.0) * 100.0);
        bg.0 = if book.0.slots[fill.0].paused { PAUSED_GREY } else { MANA_BLUE };
    }
}

fn update_info(
    book: Res<PlayerSpells>,
    selected: Res<SelectedSpell>,
    tiles: Query<(&Tile, &Interaction)>,
    mut info: Query<&mut Text, With<SpellInfo>>,
) {
    let hovered = tiles.iter().find(|(_, i)| **i == Interaction::Hovered).map(|(t, _)| &book.0.slots[t.0]);
    let selected = selected.0.and_then(|k| book.0.slot(k));
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
        let provided = tile_view(&slot(SpellKind::Volcano, Availability::Provided { shots: 2 }));
        assert_eq!((provided.style, provided.badge.as_deref(), provided.progress), (TileStyle::Provided, Some("x2"), None));
    }

    #[test]
    fn unlimited_is_ready_and_free() {
        let v = tile_view(&slot(SpellKind::Teleport, Availability::Unlimited));
        assert_eq!((v.style, v.label, v.badge.as_deref(), v.pips, v.progress), (TileStyle::Ready, "Teleport", Some("free"), (0, 0), None));
        assert!(describe(&slot(SpellKind::Teleport, Availability::Unlimited)).contains("Unlimited"));
    }

    #[test]
    fn known_shows_pips_and_progress() {
        let mut s = slot(SpellKind::Blast, Availability::Known);
        assert_eq!(tile_view(&s).style, TileStyle::Depleted);
        s.charges = 2;
        s.recharge = 20;
        let v = tile_view(&s);
        assert_eq!((v.style, v.pips, v.progress), (TileStyle::Ready, (2, 4), Some(0.5)));
        s.charges = 4;
        s.recharge = 0;
        assert_eq!(tile_view(&s).progress, None, "full spells hide the bar");
    }

    #[test]
    fn paused_tiles_say_so() {
        let mut s = slot(SpellKind::Blast, Availability::Known);
        s.charges = 1;
        s.paused = true;
        let v = tile_view(&s);
        assert_eq!((v.badge.as_deref(), v.progress), (Some("paused"), Some(0.0)), "the bar stays, frozen");
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
        assert_eq!(ground_spell(SpellKind::Teleport, (5, 6)), Some(Spell::Teleport { to: (5, 6) }));
        assert_eq!(ground_spell(SpellKind::Blast, (5, 6)), None);
        let teleport = sandbox_book().slot(SpellKind::Teleport).cloned().unwrap();
        assert!(teleport.can_cast() && teleport.availability == Availability::Unlimited);
        assert_eq!(demo_book().slot(SpellKind::Teleport).unwrap().availability, Availability::Hidden, "sandbox only");
    }

    #[test]
    fn original_levels_bring_their_spells() {
        let mut map = GameMap::generate(1);
        assert_eq!(level_book(&map), demo_book(), "generated: demo loadout");
        let mut own = SpellBook::new();
        own.set(SpellKind::Blast, Availability::Known);
        map.spell_book = Some(own.clone());
        assert_eq!(level_book(&map), own);
    }

    #[test]
    fn armageddon_has_the_last_tile_whatever_the_level() {
        let book = SpellBook::new();
        let order = tile_order(&book.slots);
        assert_eq!(order.len(), book.slots.len());
        assert_eq!(book.slots[*order.last().unwrap()].kind, SpellKind::Armageddon);
        assert_eq!(order.iter().filter(|&&i| is_special(book.slots[i].kind)).count(), 1);
    }

    #[test]
    fn demo_book_covers_every_state() {
        let styles: Vec<TileStyle> = demo_book().slots.iter().map(|s| tile_view(s).style).collect();
        for st in [TileStyle::Empty, TileStyle::Unknown, TileStyle::Provided, TileStyle::Ready] {
            assert!(styles.contains(&st), "{st:?} missing");
        }
    }
}
