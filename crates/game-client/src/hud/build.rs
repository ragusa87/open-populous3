//! Build tab: a 2 x 5 grid as the original's panel (`LAYOUT`): huts and towers, warrior and preacher (temple), spy and
//! firewarrior, boat and airship huts, the camp fire. A tile is the building's icon (the original's teal one when
//! allowed, its name otherwise), described on hover. Left click on an available one picks it as the blueprint
//! (`crate::blueprint`, white border while picked); with people selected and Shift held, sends them into the player's
//! buildings of that kind (`GameMap::send_into`); right click looks at the player's next building of that kind, one
//! after the other.

use super::panel::{TabContent, DARK_BROWN, INK};
use crate::blueprint::Plan;
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::build_book::{BuildAvailability, BuildBook, BuildSlot};
use game_core::building::BuildingKind;
use game_core::map::GameMap;
use game_core::send_in::Destination;

/// A cell of the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Building(BuildingKind),
    Campfire,
    Empty,
}

/// The grid, row by row, as the original's panel.
pub const LAYOUT: [[Cell; 2]; 5] = {
    use BuildingKind::*;
    [
        [Cell::Building(Hut { size: 1 }), Cell::Building(DrumTower)],
        [Cell::Building(WarriorTraining), Cell::Building(Temple)],
        [Cell::Building(SpyTraining), Cell::Building(FirewarriorTraining)],
        [Cell::Building(BoatHut), Cell::Building(AirshipHut)],
        [Cell::Campfire, Cell::Empty],
    ]
};

/// The panel's building icons in `hfx0-0.dat`, in the order of the cursor's (hut, drum tower, warrior, firewarrior,
/// temple, spy, boat hut, airship hut, camp fire): the icons themselves (white, drawn in dark ink), their shadows,
/// their highlights.
pub const ICONS: usize = 1075;
pub const SHADOWS: usize = 1037;
pub const HIGHLIGHTS: usize = 1046;
/// The "?" of plans to discover (`hfx0-0.dat`).
pub const UNKNOWN_ICON: usize = 1055;

/// A tile's picture (sprite indices in `hfx0-0.dat`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileIcon {
    Building { icon: usize, shadow: usize, highlight: usize },
    Unknown,
}

/// A tile of the tab: a building of the book, the camp fire, or an empty cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tile {
    Slot(BuildSlot),
    Campfire,
    Empty,
}

impl Tile {
    /// What a click on it picks, if it can be picked.
    pub fn plan(&self) -> Option<Plan> {
        match self {
            Tile::Slot(s) if s.availability == BuildAvailability::Available => Some(Plan::Building(s.kind)),
            Tile::Campfire => Some(Plan::Campfire),
            _ => None,
        }
    }

    /// Where a Shift click sends the selection (`GameMap::send_into`).
    pub fn destination(&self) -> Option<Destination> {
        match self {
            Tile::Slot(s) => Some(Destination::Kind(s.kind)),
            Tile::Campfire => Some(Destination::Campfire),
            Tile::Empty => None,
        }
    }

    pub fn view(&self) -> BuildTileView {
        match self {
            Tile::Slot(s) => build_tile_view(s),
            Tile::Campfire => BuildTileView { label: "Camp fire", shown: true, unknown: false },
            Tile::Empty => BuildTileView { label: "", shown: false, unknown: false },
        }
    }

    /// Its icon on the panel (`hfx0-0.dat`): the building's, in the cursor's order (`virtual_cursor::plan_sprite`,
    /// `POINT0-0.DAT` 58-66), from `ICONS` (drawn in dark ink), over its shadow (`SHADOWS`), its highlight on hover
    /// (`HIGHLIGHTS`); the "?" of plans to discover. None for an empty or hidden cell.
    pub fn icon(&self) -> Option<TileIcon> {
        if self.view().unknown {
            return Some(TileIcon::Unknown);
        }
        let k = self.plan().and_then(crate::virtual_cursor::plan_sprite)? - *pop3_format::sprites::POINTER_BUILDING_ICONS.start();
        Some(TileIcon::Building { icon: ICONS + k, shadow: SHADOWS + k, highlight: HIGHLIGHTS + k })
    }

    pub fn describe(&self) -> String {
        const USE: &str = "Right click: look at your next one. Shift+click with people selected: send them in.";
        match self {
            Tile::Slot(s) => describe_build(s),
            Tile::Campfire => format!("Camp fire\nLit at once, no wood, on flat free ground; left alone, it goes out. {USE}"),
            Tile::Empty => String::new(),
        }
    }
}

/// The tab's tiles in `LAYOUT` order: the book's buildings, the camp fire, the empty cell.
pub fn tiles(book: &BuildBook) -> Vec<Tile> {
    LAYOUT
        .iter()
        .flatten()
        .map(|cell| match *cell {
            Cell::Building(kind) => book.slot(kind).map_or(Tile::Empty, |s| Tile::Slot(*s)),
            Cell::Campfire => Tile::Campfire,
            Cell::Empty => Tile::Empty,
        })
        .collect()
}

/// What a tile shows: its label, whether it is drawn as a tile at all and whether it is "?".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildTileView {
    pub label: &'static str,
    pub shown: bool,
    pub unknown: bool,
}

pub fn panel_name(kind: BuildingKind) -> &'static str {
    match kind {
        BuildingKind::Hut { .. } => "Hut",
        BuildingKind::DrumTower => "Drum tower",
        BuildingKind::Temple => "Temple",
        BuildingKind::SpyTraining => "Spy hut",
        BuildingKind::WarriorTraining => "Warrior hut",
        BuildingKind::FirewarriorTraining => "Fire-warrior hut",
        BuildingKind::BoatHut => "Boat hut",
        BuildingKind::AirshipHut => "Airship hut",
        _ => "Building",
    }
}

pub fn build_tile_view(slot: &BuildSlot) -> BuildTileView {
    match slot.availability {
        BuildAvailability::Hidden => BuildTileView { label: "", shown: false, unknown: false },
        BuildAvailability::Discoverable => BuildTileView { label: "?", shown: true, unknown: true },
        BuildAvailability::Available => BuildTileView { label: panel_name(slot.kind), shown: true, unknown: false },
    }
}

pub fn describe_build(slot: &BuildSlot) -> String {
    match slot.availability {
        BuildAvailability::Hidden => String::new(),
        BuildAvailability::Discoverable => "Unknown building\nFind its plans to build it.".into(),
        BuildAvailability::Available => format!(
            "{}\nClick to place it: Space turns it, right click on the map puts it away. Right click: look at your next one. Shift+click with people selected: send them in.",
            panel_name(slot.kind)
        ),
    }
}

/// The player's buildings on `map` (`GameMap::build_books`).
pub fn player_builds(map: &GameMap) -> &BuildBook {
    &map.build_books[crate::units::PLAYER as usize]
}

/// Where the player's things a tile stands for are (built buildings of its kind, camp fires), world units, in id
/// order: what right clicks on it look at, one after the other.
pub fn own_places(map: &GameMap, tile: &Tile) -> Vec<(u16, u16)> {
    let player = crate::units::PLAYER;
    match tile.destination() {
        Some(Destination::Kind(kind)) => map
            .buildings
            .iter()
            .filter(|b| b.owner == player && b.stage() == game_core::building::Stage::Built && game_core::send_in::same_kind(b.kind, kind))
            .map(|b| b.centre())
            .collect(),
        Some(Destination::Campfire) => map.campfires.iter().filter(|f| f.owner == player).map(|f| f.centre()).collect(),
        None => Vec::new(),
    }
}

/// The next of `count` places to look at after `last` (the tile last clicked and its step), for tile `tile`.
pub fn next_step(last: Option<(usize, usize)>, tile: usize, count: usize) -> Option<usize> {
    (count > 0).then(|| match last {
        Some((t, step)) if t == tile => (step + 1) % count,
        _ => 0,
    })
}

#[derive(Component)]
struct BuildTile(usize);
#[derive(Component)]
struct BuildLabel(usize);
#[derive(Component)]
struct BuildIcon(usize);
#[derive(Component)]
struct BuildShadow(usize);
#[derive(Component)]
struct BuildInfo;


pub struct BuildPlugin;

impl Plugin for BuildPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_tab).add_systems(Update, ((tile_clicks, look_at_next).in_set(crate::menu::Gameplay), update_tiles, update_info).chain());
    }
}

fn spawn_tab(mut commands: Commands, tabs: Query<(Entity, &TabContent)>) {
    let Some((entity, _)) = tabs.iter().find(|(_, t)| t.0 == 1) else { return };
    commands.entity(entity).with_children(|c| {
        c.spawn(Node { display: Display::Grid, grid_template_columns: RepeatedGridTrack::flex(2, 1.0), row_gap: px(4), column_gap: px(4), ..default() })
            .with_children(|grid| {
                for i in 0..LAYOUT.len() * 2 {
                    grid.spawn((
                        BuildTile(i),
                        Button,
                        Node { height: px(58), justify_content: JustifyContent::Center, align_items: AlignItems::Center, padding: UiRect::all(px(3)), border: UiRect::all(px(2)), ..default() },
                        BackgroundColor(Color::NONE),
                        BorderColor::all(Color::NONE),
                    ))
                    .with_children(|tile| {
                        tile.spawn((BuildShadow(i), ImageNode::default(), Node { display: Display::None, position_type: PositionType::Absolute, height: px(44), ..default() }));
                        tile.spawn((BuildIcon(i), ImageNode::default(), Node { display: Display::None, height: px(44), ..default() }));
                        tile.spawn((BuildLabel(i), Text::new(""), TextFont { font_size: FontSize::Px(12.0), ..default() }, TextColor(INK), TextLayout::justify(Justify::Center)));
                    });
                }
            });
        c.spawn((BuildInfo, Text::new(""), TextFont { font_size: FontSize::Px(12.0), ..default() }, TextColor(INK), Node { min_height: px(48), ..default() }));
    });
}

/// A left click on an available building or the camp fire picks it as the blueprint (and puts any spell away); with
/// people selected and Shift held, sends them in instead (`GameMap::send_into`).
fn tile_clicks(
    q: Query<(&Interaction, &BuildTile), Changed<Interaction>>,
    map: Res<CurrentMap>,
    keys: crate::keymap::Shortcuts,
    selection: Res<crate::units::selection::Selection>,
    mut schedule: ResMut<crate::units::GameSchedule>,
    mut blueprint: ResMut<crate::blueprint::Blueprint>,
    mut spell: ResMut<crate::hud::spells::SelectedSpell>,
) {
    let tiles = tiles(player_builds(&map.0));
    for (interaction, tile) in &q {
        let Some(t) = tiles.get(tile.0).filter(|_| *interaction == Interaction::Pressed) else { continue };
        if keys.mods().shift && !selection.units.is_empty() {
            for c in t.destination().map_or(Vec::new(), |to| map.0.send_into(crate::units::PLAYER, &selection.units, to)) {
                schedule.issue(c);
            }
        } else if let Some(plan) = t.plan() {
            blueprint.pick(plan);
            spell.0 = None;
        }
    }
}

/// A right click on a tile flies the camera to the player's next building of its kind (or camp fire), one after the
/// other at each click.
fn look_at_next(
    mouse: Res<ButtonInput<MouseButton>>,
    q: Query<(&Interaction, &BuildTile)>,
    map: Res<CurrentMap>,
    mut rig: ResMut<crate::camera::CameraRig>,
    mut last: Local<Option<(usize, usize)>>,
) {
    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }
    let Some((_, tile)) = q.iter().find(|(i, _)| **i != Interaction::None) else { return };
    let all = tiles(player_builds(&map.0));
    let Some(t) = all.get(tile.0) else { return };
    let places = own_places(&map.0, t);
    let Some(step) = next_step(*last, tile.0, places.len()) else { return };
    *last = Some((tile.0, step));
    let (x, z) = places[step];
    rig.fly_to(Vec2::new(x as f32, z as f32) / pop3_format::WORLD_UNITS_PER_CELL as f32);
}

fn update_tiles(
    map: Res<CurrentMap>,
    blueprint: Res<crate::blueprint::Blueprint>,
    (mut icons, mut images): (ResMut<crate::tooltip::IconImages>, ResMut<Assets<Image>>),
    mut tiles: Query<(&BuildTile, &Interaction, &mut BackgroundColor, &mut BorderColor)>,
    mut labels: Query<(&BuildLabel, &mut Text, &mut TextFont, &mut Node), (Without<BuildIcon>, Without<BuildShadow>)>,
    mut pictures: Query<(&BuildIcon, &mut ImageNode, &mut Node), (Without<BuildLabel>, Without<BuildShadow>)>,
    mut shadows: Query<(&BuildShadow, &mut ImageNode, &mut Node), (Without<BuildLabel>, Without<BuildIcon>)>,
) {
    let all = self::tiles(player_builds(&map.0));
    let views: Vec<BuildTileView> = all.iter().map(Tile::view).collect();
    let hovered: Vec<usize> = tiles.iter().filter(|(_, i, ..)| **i != Interaction::None).map(|(t, ..)| t.0).collect();
    // What each tile draws: the icon (dark, or its highlight when hovered) and its shadow; the "?" alone.
    let mut drawn: Vec<Option<(Handle<Image>, Color, Option<Handle<Image>>)>> = Vec::new();
    for (i, tile) in all.iter().enumerate() {
        let mut sprite = |index: usize| icons.sprite(index, &mut images).map(|s| s.0);
        drawn.push(match tile.icon() {
            Some(TileIcon::Building { icon, shadow, highlight }) => match hovered.contains(&i) {
                true => sprite(highlight).map(|h| (h, Color::WHITE, sprite(shadow))),
                false => sprite(icon).map(|h| (h, INK, sprite(shadow))),
            },
            Some(TileIcon::Unknown) => sprite(UNKNOWN_ICON).map(|h| (h, Color::WHITE, None)),
            None => None,
        });
    }
    for (tile, interaction, mut bg, mut border) in &mut tiles {
        let Some(v) = views.get(tile.0) else { continue };
        let (fill, edge) = match (v.shown, v.unknown) {
            (false, _) => (Color::srgba(0.0, 0.0, 0.0, 0.10), Color::NONE),
            (true, true) => (Color::srgb(0.36, 0.24, 0.10), DARK_BROWN),
            (true, false) => (super::panel::PARCHMENT, DARK_BROWN),
        };
        bg.set_if_neq(BackgroundColor(fill));
        let picked = blueprint.plan.is_some() && blueprint.plan == all.get(tile.0).and_then(Tile::plan);
        border.set_if_neq(BorderColor::all(match (picked, v.shown && *interaction == Interaction::Hovered) {
            (true, _) => Color::WHITE,
            (_, true) => Color::srgb(1.0, 0.95, 0.7),
            _ => edge,
        }));
    }
    let show = |node: &mut Node, shown: bool| {
        let display = if shown { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    };
    for (pic, mut image, mut node) in &mut pictures {
        let d = drawn.get(pic.0).cloned().flatten();
        show(&mut node, d.is_some());
        if let Some((h, ink, _)) = d.filter(|(h, ink, _)| image.image != *h || image.color != *ink) {
            (image.image, image.color) = (h, ink);
        }
    }
    for (shadow, mut image, mut node) in &mut shadows {
        let d = drawn.get(shadow.0).cloned().flatten().and_then(|d| d.2);
        show(&mut node, d.is_some());
        if let Some(h) = d.filter(|h| image.image != *h) {
            image.image = h;
        }
    }
    for (label, mut text, mut font, mut node) in &mut labels {
        let Some(v) = views.get(label.0) else { continue };
        show(&mut node, drawn.get(label.0).is_none_or(|d| d.is_none()));
        if text.0 != v.label {
            text.0 = v.label.to_string();
        }
        font.font_size = FontSize::Px(if v.unknown { 22.0 } else { 11.0 });
    }
}

fn update_info(map: Res<CurrentMap>, tiles: Query<(&BuildTile, &Interaction)>, mut info: Query<&mut Text, With<BuildInfo>>) {
    let all = self::tiles(player_builds(&map.0));
    let hovered = tiles.iter().find(|(_, i)| **i == Interaction::Hovered).and_then(|(t, _)| all.get(t.0));
    let text = hovered.map(Tile::describe).filter(|s| !s.is_empty()).unwrap_or_else(|| "Hover a building to see it.".into());
    for mut t in &mut info {
        if t.0 != text {
            t.0.clone_from(&text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(kind: BuildingKind, availability: BuildAvailability) -> BuildSlot {
        BuildSlot { kind, availability }
    }

    #[test]
    fn tiles_per_state() {
        assert!(!build_tile_view(&slot(BuildingKind::Temple, BuildAvailability::Hidden)).shown);
        let unknown = build_tile_view(&slot(BuildingKind::Temple, BuildAvailability::Discoverable));
        assert_eq!((unknown.label, unknown.unknown), ("?", true));
        assert_eq!(build_tile_view(&slot(BuildingKind::Hut { size: 1 }, BuildAvailability::Available)).label, "Hut");
        assert!(describe_build(&slot(BuildingKind::BoatHut, BuildAvailability::Available)).starts_with("Boat hut"));
        assert!(describe_build(&slot(BuildingKind::BoatHut, BuildAvailability::Discoverable)).contains("plans"));
    }

    #[test]
    fn the_original_grid_two_by_five() {
        let all = tiles(&BuildBook::all(BuildAvailability::Available));
        assert_eq!(all.len(), 10);
        let kinds: Vec<Option<BuildingKind>> = all.iter().map(|t| match t { Tile::Slot(s) => Some(s.kind), _ => None }).collect();
        assert_eq!(kinds[..8], [Some(BuildingKind::Hut { size: 1 }), Some(BuildingKind::DrumTower), Some(BuildingKind::WarriorTraining), Some(BuildingKind::Temple), Some(BuildingKind::SpyTraining), Some(BuildingKind::FirewarriorTraining), Some(BuildingKind::BoatHut), Some(BuildingKind::AirshipHut)]);
        assert_eq!(all[8..], [Tile::Campfire, Tile::Empty]);
        assert_eq!((Tile::Campfire.plan(), Tile::Empty.plan()), (Some(Plan::Campfire), None));
        assert_eq!(Tile::Empty.destination(), None);
        assert_eq!(all[0].icon(), Some(TileIcon::Building { icon: 1075, shadow: 1037, highlight: 1046 }), "the hut's");
        assert_eq!(Tile::Campfire.icon(), Some(TileIcon::Building { icon: 1083, shadow: 1045, highlight: 1054 }), "the camp fire's, last");
        assert_eq!(Tile::Empty.icon(), None);
        assert_eq!(Tile::Slot(slot(BuildingKind::Temple, BuildAvailability::Discoverable)).icon(), Some(TileIcon::Unknown));
    }

    #[test]
    fn right_clicks_go_round_the_players_buildings_of_a_kind() {
        assert_eq!(next_step(None, 3, 2), Some(0));
        assert_eq!(next_step(Some((3, 0)), 3, 2), Some(1));
        assert_eq!(next_step(Some((3, 1)), 3, 2), Some(0), "back to the first");
        assert_eq!(next_step(Some((3, 1)), 4, 2), Some(0), "another tile starts over");
        assert_eq!(next_step(None, 3, 0), None, "none of them");
        let map = GameMap::sandbox_buildings();
        let huts = own_places(&map, &Tile::Slot(slot(BuildingKind::Hut { size: 1 }, BuildAvailability::Available)));
        assert!(huts.len() > 1, "every hut size");
        assert_eq!(own_places(&map, &Tile::Campfire).len(), 1);
    }
}
