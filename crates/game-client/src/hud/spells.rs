//! Spells tab: a grid of tiles mirroring the player's `SpellBook`.
//! Tile states: empty (hidden), "?" (discoverable), gray with uses left (provided),
//! gold with charge pips + recharge bar (known). Click selects, C casts (demo: uses a charge, the shaman jumps).

use super::panel::{TabContent, DARK_BROWN, INK};
use crate::units::PLAYER;
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::command::Command;
use game_core::unit::Order;
use game_core::spell_book::{Availability, SpellBook, SpellKind, SpellSlot, MAX_CHARGES};

/// Mana given to every recharging spell per tick, and tick length.
const MANA_PER_TICK: u32 = 8;
const TICK_SECS: f32 = 0.1;

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
        Availability::Known => TileView {
            style: if slot.charges > 0 { TileStyle::Ready } else { TileStyle::Depleted },
            label,
            pips: (slot.charges, slot.kind.max_charges()),
            progress: slot.is_recharging().then(|| slot.recharge as f32 / slot.kind.cost() as f32),
            badge: None,
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
        Availability::Known => {
            let max = slot.kind.max_charges();
            let state = if slot.is_recharging() {
                format!("recharging {:.0}%", 100.0 * slot.recharge as f32 / slot.kind.cost() as f32)
            } else {
                "full".into()
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
        SpellKind::Armageddon => "Armag.",
    }
}

/// Sandbox loadout showing every tile state until levels provide their own.
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
            .add_systems(Update, ((recharge, tile_clicks, cast_selected).in_set(crate::menu::Gameplay), update_tiles, update_info).chain());
    }
}

fn spawn_tab(mut commands: Commands, tabs: Query<(Entity, &TabContent)>, book: Res<PlayerSpells>) {
    for (entity, tab) in &tabs {
        commands.entity(entity).with_children(|c| match tab.0 {
            0 => spawn_spells(c, book.0.slots.len()),
            _ => {
                c.spawn((Text::new("Coming soon"), TextFont { font_size: FontSize::Px(13.0), ..default() }, TextColor(INK)));
            }
        });
    }
}

fn spawn_spells(c: &mut ChildSpawnerCommands, count: usize) {
    c.spawn(Node {
        display: Display::Grid,
        grid_template_columns: RepeatedGridTrack::flex(3, 1.0),
        row_gap: px(4),
        column_gap: px(4),
        ..default()
    })
    .with_children(|grid| {
        for i in 0..count {
            grid.spawn((
                Tile(i),
                Button,
                Node {
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

fn style_colors(style: TileStyle) -> (Color, Color) {
    match style {
        TileStyle::Empty => (Color::srgba(0.0, 0.0, 0.0, 0.10), Color::NONE),
        TileStyle::Unknown => (Color::srgb(0.36, 0.24, 0.10), DARK_BROWN),
        TileStyle::Provided => (Color::srgb(0.66, 0.66, 0.64), Color::srgb(0.35, 0.35, 0.35)),
        TileStyle::Ready => (Color::srgb(1.0, 0.84, 0.48), DARK_BROWN),
        TileStyle::Depleted => (Color::srgb(0.62, 0.48, 0.28), DARK_BROWN),
    }
}

fn recharge(time: Res<Time>, mut acc: Local<f32>, mut book: ResMut<PlayerSpells>) {
    *acc += time.delta_secs();
    while *acc >= TICK_SECS {
        *acc -= TICK_SECS;
        book.0.tick(MANA_PER_TICK);
    }
}

fn tile_clicks(
    q: Query<(&Interaction, &Tile), Changed<Interaction>>,
    book: Res<PlayerSpells>,
    mut selected: ResMut<SelectedSpell>,
) {
    for (interaction, tile) in &q {
        let slot = &book.0.slots[tile.0];
        if *interaction == Interaction::Pressed && slot.availability != Availability::Hidden {
            selected.0 = Some(slot.kind);
        }
    }
}

/// Casting makes the shaman jump (the spell effects come later).
fn cast_selected(
    keys: Res<ButtonInput<KeyCode>>,
    mut book: ResMut<PlayerSpells>,
    selected: Res<SelectedSpell>,
    mut map: ResMut<CurrentMap>,
) {
    if let (true, Some(kind)) = (keys.just_pressed(KeyCode::KeyC), selected.0) {
        if book.0.cast(kind) {
            map.bypass_change_detection().0.apply(&Command::Order { player: PLAYER, order: Order::Cast });
        }
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
    mut fills: Query<(&RechargeFill, &mut Node), (Without<Pip>, Without<RechargeBar>)>,
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
    for (fill, mut node) in &mut fills {
        node.width = percent(views[fill.0].progress.unwrap_or(0.0) * 100.0);
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
    fn describe_mentions_state() {
        assert!(describe(&slot(SpellKind::Flatten, Availability::Discoverable)).contains("Discover"));
        assert!(describe(&slot(SpellKind::Volcano, Availability::Provided { shots: 1 })).contains("1 use left"));
    }

    #[test]
    fn demo_book_covers_every_state() {
        let styles: Vec<TileStyle> = demo_book().slots.iter().map(|s| tile_view(s).style).collect();
        for st in [TileStyle::Empty, TileStyle::Unknown, TileStyle::Provided, TileStyle::Ready] {
            assert!(styles.contains(&st), "{st:?} missing");
        }
    }
}
