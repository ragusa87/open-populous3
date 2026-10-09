//! Stats tab (docs/specs/ui-and-editor.md "Stats tab", "Population"): the tribe's huts by size, then its
//! units as a matrix of rows (selected, idle, housed, working, in a boat, in a balloon) by kind, read
//! from `game_core::headcount` each frame. A click on a number adds one more of those units to the
//! selection (`GameMap::next_unit`), Shift + click all of them; a click on a row's total does the same
//! across its kinds.

use super::panel::{TabContent, DARK_BROWN, INK};
use crate::tooltip::{Icon, IconImages};
use crate::units::selection::Selection;
use crate::units::PLAYER;
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::headcount::{kinds_of, Counts, Headcount, Housing, State, KINDS, SUPPLY};
use game_core::map::GameMap;
use game_core::unit::UnitKind;
use pop3_format::catalog::STATS_ROW_ICONS;

const TAB: usize = 2;
const TILE: Color = Color::srgb(1.0, 0.84, 0.48);
const SELECTED_TILE: Color = Color::srgb(1.0, 0.77, 0.25);
const SELECTED_TOTAL: Color = Color::srgb(0.47, 0.27, 0.08);
const LIGHT: Color = Color::srgb(1.0, 0.84, 0.48);
const HOVER: Color = Color::srgb(1.0, 0.96, 0.78);
/// Column widths in pixels: the row's label (an icon, else text) and its total.
const LABEL_ICON_W: f32 = 22.0;
const LABEL_TEXT_W: f32 = 64.0;
const TOTAL_W: f32 = 26.0;
const GAP: f32 = 2.0;

/// A row of the matrix: the selection, or the tribe's units doing a `State`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Selected,
    Doing(State),
}

/// The rows in order, `None` where a gap sets the groups apart.
pub const ROWS: [Option<Row>; 8] = [
    Some(Row::Selected),
    None,
    Some(Row::Doing(State::Idle)),
    Some(Row::Doing(State::Housed)),
    Some(Row::Doing(State::Working)),
    None,
    Some(Row::Doing(State::InBoat)),
    Some(Row::Doing(State::InBalloon)),
];

impl Row {
    pub fn label(self) -> &'static str {
        match self {
            Row::Selected => "Selected",
            Row::Doing(State::Idle) => "Idle",
            Row::Doing(State::Housed) => "Housed",
            Row::Doing(State::Working) => "Working",
            Row::Doing(State::InBoat) => "In boat",
            Row::Doing(State::InBalloon) => "In balloon",
        }
    }

    /// Its original label icon (`STATS_ROW_ICONS`).
    pub fn icon(self) -> usize {
        let k = match self {
            Row::Selected => 0,
            Row::Doing(State::Idle) => 1,
            Row::Doing(State::Housed) => 2,
            Row::Doing(State::Working) => 3,
            Row::Doing(State::InBoat) => 4,
            Row::Doing(State::InBalloon) => 5,
        };
        STATS_ROW_ICONS[k]
    }

    fn counts<'a>(self, count: &'a Headcount, selected: &'a Counts) -> &'a Counts {
        match self {
            Row::Selected => selected,
            Row::Doing(state) => count.row(state),
        }
    }
}

/// A number of the matrix: `kind`'s in `row`, or the row's total when None.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub row: Row,
    pub kind: Option<UnitKind>,
}

impl Cell {
    pub fn value(self, count: &Headcount, selected: &Counts) -> u16 {
        let counts = self.row.counts(count, selected);
        self.kind.map_or(counts.total(), |k| counts.get(k))
    }

    /// The units a click adds: the next one not selected yet, or with `all` every one of them.
    pub fn pick(self, map: &GameMap, tribe: u8, selected: &[u32], all: bool) -> Vec<u32> {
        let Row::Doing(state) = self.row else { return Vec::new() };
        if all {
            return map.units_in(tribe, state, self.kind).map(|u| u.id).filter(|id| !selected.contains(id)).collect();
        }
        map.next_unit(tribe, state, self.kind, selected).into_iter().collect()
    }

    /// What hovering it says.
    pub fn describe(self, map: &GameMap, tribe: u8, selected: &[u32]) -> String {
        let what = self.kind.map_or("units", |k| kind_plural(k));
        let Row::Doing(state) = self.row else { return format!("Selected {what}.") };
        let units: Vec<u32> = map.units_in(tribe, state, self.kind).map(|u| u.id).collect();
        let picked = units.iter().filter(|id| selected.contains(id)).count();
        let row = self.row.label().to_lowercase();
        format!("{row}: {} {what}, {picked} selected.\nClick: one more. Shift + click: all.", units.len())
    }
}

fn kind_plural(kind: UnitKind) -> &'static str {
    match kind {
        UnitKind::Brave => "braves",
        UnitKind::Warrior => "warriors",
        UnitKind::Firewarrior => "firewarriors",
        UnitKind::Preacher => "preachers",
        UnitKind::Spy => "spies",
        UnitKind::Shaman => "shamans",
        UnitKind::Wildman => "wildmen",
    }
}

/// The huts box's tiles: name, built huts of that size, the room they give.
pub fn hut_tiles(h: &Housing) -> [(&'static str, u16, u16); 3] {
    let names = ["Small", "Medium", "Large"];
    std::array::from_fn(|i| (names[i], h.huts[i], h.huts[i] * SUPPLY[i]))
}

#[derive(Component)]
struct CellText(Cell);
#[derive(Component)]
struct KindHeader(UnitKind);
#[derive(Component)]
struct RoomText;
#[derive(Component)]
struct HutText(usize);
#[derive(Component)]
struct StatsInfo;

pub struct StatsPlugin;

impl Plugin for StatsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, spawn_tab).add_systems(Update, (cell_clicks.in_set(crate::menu::Gameplay), update_tab).chain());
    }
}

fn text(s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(s), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color))
}

fn spawn_tab(mut commands: Commands, tabs: Query<(Entity, &TabContent)>, mut icons: ResMut<IconImages>, mut images: ResMut<Assets<Image>>) {
    let Some((entity, _)) = tabs.iter().find(|(_, t)| t.0 == TAB) else { return };
    let label_w = if icons.original() { LABEL_ICON_W } else { LABEL_TEXT_W };
    let row_h = if icons.original() { 25.0 } else { 22.0 };
    commands.entity(entity).with_children(|c| {
        c.spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(3), padding: UiRect::all(px(4)), border: UiRect::all(px(1)), ..default() }, BackgroundColor(TILE), BorderColor::all(DARK_BROWN)))
            .with_children(|b| {
                b.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|top| {
                    top.spawn(text("Huts", 13.0, INK));
                    top.spawn((RoomText, text("", 12.0, INK)));
                });
                b.spawn(Node { column_gap: px(4), ..default() }).with_children(|tiles| {
                    for i in 0..3 {
                        tiles
                            .spawn((Node { flex_grow: 1.0, flex_basis: px(0), justify_content: JustifyContent::Center, border: UiRect::all(px(1)), ..default() }, BorderColor::all(DARK_BROWN.with_alpha(0.5))))
                            .with_child((HutText(i), text("", 11.0, INK), TextLayout::justify(Justify::Center)));
                    }
                });
            });
        c.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(GAP), ..default() }).with_children(|m| {
            m.spawn(Node { column_gap: px(GAP), height: px(26), align_items: AlignItems::End, ..default() }).with_children(|r| {
                r.spawn(Node { width: px(label_w), ..default() });
                r.spawn((Node { width: px(TOTAL_W), justify_content: JustifyContent::Center, ..default() },)).with_child(text("All", 10.0, INK));
                for kind in KINDS {
                    let icon = icons.get(Icon::Unit { kind, tribe: PLAYER }, &mut images);
                    r.spawn((KindHeader(kind), Interaction::default(), Node { flex_grow: 1.0, flex_basis: px(0), justify_content: JustifyContent::Center, ..default() }))
                        .with_child((ImageNode::new(icon), Node { width: px(18), height: px(24), ..default() }));
                }
            });
            for row in ROWS {
                let Some(row) = row else {
                    m.spawn(Node { height: px(3), ..default() });
                    continue;
                };
                m.spawn(Node { column_gap: px(GAP), height: px(row_h), ..default() }).with_children(|r| {
                    let mut label = r.spawn(Node { width: px(label_w), align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..default() });
                    match icons.sprite(row.icon(), &mut images) {
                        Some((image, size)) => {
                            label.with_child((ImageNode::new(image), Node { width: px(size.x), height: px(size.y), ..default() }));
                        }
                        None => {
                            label.insert(Node { width: px(label_w), align_items: AlignItems::Center, ..default() });
                            label.with_child((text(row.label(), 10.0, INK), TextLayout::default().with_no_wrap()));
                        }
                    }
                    for kind in std::iter::once(None).chain(KINDS.map(Some)) {
                        let cell = Cell { row, kind };
                        let node = match kind {
                            None => Node { width: px(TOTAL_W), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(px(1)), ..default() },
                            Some(_) => Node { flex_grow: 1.0, flex_basis: px(0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(px(1)), ..default() },
                        };
                        r.spawn((cell, Button, node, BackgroundColor(TILE), BorderColor::all(DARK_BROWN))).with_child((CellText(cell), text("", 12.0, INK)));
                    }
                });
            }
        });
        c.spawn((StatsInfo, text("", 12.0, INK), Node { min_height: px(48), ..default() }));
    });
}

/// A click on a number adds the next of those units to the selection, Shift + click all of them.
fn cell_clicks(q: Query<(&Interaction, &Cell), Changed<Interaction>>, keys: Res<ButtonInput<KeyCode>>, map: Res<CurrentMap>, mut selection: ResMut<Selection>) {
    let all = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    for (interaction, cell) in &q {
        if *interaction == Interaction::Pressed {
            for id in cell.pick(&map.0, PLAYER, &selection.units, all) {
                selection.add(id);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_tab(
    active: Res<super::ActiveTab>,
    map: Res<CurrentMap>,
    selection: Res<Selection>,
    mut cells: Query<(&Cell, &Interaction, &mut BackgroundColor, &mut BorderColor)>,
    mut texts: Query<(&CellText, &mut Text, &mut TextColor), (Without<RoomText>, Without<HutText>, Without<StatsInfo>)>,
    headers: Query<(&KindHeader, &Interaction)>,
    mut room: Query<&mut Text, (With<RoomText>, Without<CellText>, Without<HutText>, Without<StatsInfo>)>,
    mut huts: Query<(&HutText, &mut Text), (Without<CellText>, Without<RoomText>, Without<StatsInfo>)>,
    mut info: Query<&mut Text, (With<StatsInfo>, Without<CellText>, Without<RoomText>, Without<HutText>)>,
) {
    if active.0 != TAB {
        return;
    }
    let count = map.0.headcount(PLAYER);
    let selected = kinds_of(&map.0, &selection.units);
    let housing = map.0.housing(PLAYER);
    for (cell, interaction, mut bg, mut border) in &mut cells {
        let n = cell.value(&count, &selected);
        let fill = match (cell.row, cell.kind) {
            (Row::Selected, None) => SELECTED_TOTAL,
            (Row::Selected, Some(_)) => SELECTED_TILE,
            (_, None) => DARK_BROWN,
            _ => TILE,
        };
        bg.set_if_neq(BackgroundColor(fill));
        let hot = cell.row != Row::Selected && n > 0 && *interaction != Interaction::None;
        border.set_if_neq(BorderColor::all(if hot { HOVER } else { DARK_BROWN }));
    }
    for (CellText(cell), mut t, mut color) in &mut texts {
        let n = cell.value(&count, &selected);
        let s = n.to_string();
        if t.0 != s {
            t.0 = s;
        }
        let base = if cell.kind.is_none() && cell.row != Row::Selected { LIGHT } else { INK };
        color.set_if_neq(TextColor(if n == 0 { base.with_alpha(0.35) } else { base }));
    }
    for mut t in &mut room {
        let s = format!("room {}", housing.room());
        if t.0 != s {
            t.0 = s;
        }
    }
    let tiles = hut_tiles(&housing);
    for (HutText(i), mut t) in &mut huts {
        let (name, n, room) = tiles[*i];
        let s = format!("{n}\n{name}\n+{room}");
        if t.0 != s {
            t.0 = s;
        }
    }
    let hovered_cell = cells.iter().find(|(c, i, _, _)| **i != Interaction::None && c.row != Row::Selected).map(|(c, ..)| *c);
    let hovered_kind = headers.iter().find(|(_, i)| **i != Interaction::None).map(|(k, _)| k.0);
    let s = match (hovered_cell, hovered_kind) {
        (Some(cell), _) => cell.describe(&map.0, PLAYER, &selection.units),
        (None, Some(kind)) => kind.name().to_string(),
        (None, None) => "Click a number to select those units, Shift + click for all of them.".into(),
    };
    for mut t in &mut info {
        if t.0 != s {
            t.0.clone_from(&s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::unit::{Action, Unit};

    fn map_with(units: &[(UnitKind, Action)]) -> GameMap {
        let mut map = GameMap::sandbox_units();
        map.units.clear();
        for (i, (kind, action)) in units.iter().enumerate() {
            let mut u = Unit::new(i as u32 + 1, PLAYER, *kind, (64 * 512, 64 * 512));
            u.action = *action;
            map.units.push(u);
        }
        map
    }

    #[test]
    fn every_row_has_a_label_and_its_own_icon() {
        let rows: Vec<Row> = ROWS.into_iter().flatten().collect();
        assert_eq!(rows.len(), 6);
        let mut icons: Vec<usize> = rows.iter().map(|r| r.icon()).collect();
        icons.dedup();
        assert_eq!(icons.len(), 6);
        assert!(rows.iter().all(|r| !r.label().is_empty()));
    }

    #[test]
    fn cells_read_their_row_and_the_selection() {
        let map = map_with(&[(UnitKind::Brave, Action::Idle), (UnitKind::Brave, Action::Idle), (UnitKind::Spy, Action::Hammering)]);
        let count = map.headcount(PLAYER);
        let selected = kinds_of(&map, &[2]);
        let idle_braves = Cell { row: Row::Doing(State::Idle), kind: Some(UnitKind::Brave) };
        assert_eq!(idle_braves.value(&count, &selected), 2);
        assert_eq!(Cell { row: Row::Doing(State::Working), kind: None }.value(&count, &selected), 1);
        assert_eq!(Cell { row: Row::Selected, kind: Some(UnitKind::Brave) }.value(&count, &selected), 1);
        assert!(idle_braves.describe(&map, PLAYER, &[2]).starts_with("idle: 2 braves, 1 selected."));
    }

    #[test]
    fn a_click_adds_the_next_unit_shift_all_and_the_selected_row_none() {
        let map = map_with(&[(UnitKind::Warrior, Action::Idle), (UnitKind::Brave, Action::Idle), (UnitKind::Brave, Action::Idle)]);
        let total = Cell { row: Row::Doing(State::Idle), kind: None };
        assert_eq!(total.pick(&map, PLAYER, &[], false), vec![2], "braves first");
        assert_eq!(total.pick(&map, PLAYER, &[2, 3], false), vec![1]);
        assert_eq!(total.pick(&map, PLAYER, &[2], true), vec![1, 3]);
        assert!(Cell { row: Row::Selected, kind: None }.pick(&map, PLAYER, &[], true).is_empty());
    }

    #[test]
    fn hut_tiles_give_the_room_per_size() {
        let tiles = hut_tiles(&Housing { huts: [3, 2, 1], population: 0 });
        assert_eq!(tiles, [("Small", 3, 9), ("Medium", 2, 10), ("Large", 1, 7)]);
    }
}
