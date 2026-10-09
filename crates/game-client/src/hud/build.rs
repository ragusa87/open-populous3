//! Build tab: a grid of tiles mirroring the player's `BuildBook`, one per building of the original
//! panel, named (no icons yet), then the camp fire, always available. Tile states: empty (hidden),
//! "?" (plans to discover), named (available). Hover describes it, a click on an available one
//! picks it as the blueprint (`crate::blueprint`, white border while picked). Placing buildings
//! comes later, camp fires are lit at once (docs/specs/buildings.md).

use super::panel::{TabContent, DARK_BROWN, INK};
use crate::blueprint::Plan;
use bevy::prelude::*;
use game_core::build_book::{BuildAvailability, BuildBook, BuildSlot};
use crate::world::CurrentMap;
use game_core::building::{BuildingKind, Reward};
use game_core::map::GameMap;
use game_core::spell_book::SpellBook;

const COLUMNS: u16 = 3;

#[derive(Resource)]
pub struct PlayerBuilds(pub BuildBook);

/// A tile of the tab: a building of the book, or the camp fire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tile {
    Slot(BuildSlot),
    Campfire,
}

impl Tile {
    /// What a click on it picks, if it can be picked.
    pub fn plan(&self) -> Option<Plan> {
        match self {
            Tile::Slot(s) if s.availability == BuildAvailability::Available => Some(Plan::Building(s.kind)),
            Tile::Slot(_) => None,
            Tile::Campfire => Some(Plan::Campfire),
        }
    }

    pub fn view(&self) -> BuildTileView {
        match self {
            Tile::Slot(s) => build_tile_view(s),
            Tile::Campfire => BuildTileView { label: "Camp fire", shown: true, unknown: false },
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Tile::Slot(s) => describe_build(s),
            Tile::Campfire => "Camp fire\nLit at once, no wood, on flat free ground. Click it with people selected: they go round it. Shift+right click puts it out. Left alone, it goes out.".into(),
        }
    }
}

/// The tab's tiles: the book's buildings, then the camp fire.
pub fn tiles(book: &BuildBook) -> Vec<Tile> {
    book.slots.iter().copied().map(Tile::Slot).chain(std::iter::once(Tile::Campfire)).collect()
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
        BuildAvailability::Available => format!("{}\nClick to place it: Space turns it, right click puts it away.", panel_name(slot.kind)),
    }
}

/// The player's buildings on `map`: an original level's own (`GameMap::build_book`), else all of
/// them (generated maps and sandboxes).
pub fn level_builds(map: &GameMap) -> BuildBook {
    map.build_book.clone().unwrap_or_else(|| BuildBook::all(BuildAvailability::Available))
}

#[derive(Component)]
struct BuildTile(usize);
#[derive(Component)]
struct BuildLabel(usize);
#[derive(Component)]
struct BuildInfo;

pub struct BuildPlugin;

impl Plugin for BuildPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PlayerBuilds(BuildBook::all(BuildAvailability::Available)))
            .add_systems(Startup, spawn_tab)
            .add_systems(Update, ((tile_clicks.in_set(crate::menu::Gameplay), update_tiles).chain(), update_info, take_rewards.before(update_tiles)));
    }
}

/// Makes the player's rewards (`GameMap::granted`, from vaults of knowledge) available on the panels:
/// a spell becomes known, a building buildable. `seen` counts the rewards already taken; a new map
/// starts its list again.
pub fn apply_rewards(granted: &[(u8, Reward)], seen: &mut usize, spells: &mut SpellBook, builds: &mut BuildBook) {
    if granted.len() < *seen {
        *seen = 0;
    }
    for &(owner, reward) in &granted[*seen..] {
        if owner != crate::units::PLAYER {
            continue;
        }
        match reward {
            Reward::Spell(kind) => spells.set(kind, game_core::spell_book::Availability::Known),
            Reward::Building(kind) => builds.set(kind, BuildAvailability::Available),
        }
    }
    *seen = granted.len();
}

fn take_rewards(map: Res<CurrentMap>, mut seen: Local<usize>, mut spells: ResMut<crate::hud::spells::PlayerSpells>, mut builds: ResMut<PlayerBuilds>) {
    if map.0.granted.len() != *seen {
        apply_rewards(&map.0.granted, &mut seen, &mut spells.0, &mut builds.0);
    }
}

fn spawn_tab(mut commands: Commands, tabs: Query<(Entity, &TabContent)>, book: Res<PlayerBuilds>) {
    let Some((entity, _)) = tabs.iter().find(|(_, t)| t.0 == 1) else { return };
    commands.entity(entity).with_children(|c| {
        c.spawn(Node { display: Display::Grid, grid_template_columns: RepeatedGridTrack::flex(COLUMNS, 1.0), row_gap: px(4), column_gap: px(4), ..default() })
            .with_children(|grid| {
                for i in 0..tiles(&book.0).len() {
                    grid.spawn((
                        BuildTile(i),
                        Button,
                        Node { height: px(58), justify_content: JustifyContent::Center, align_items: AlignItems::Center, padding: UiRect::all(px(3)), border: UiRect::all(px(2)), ..default() },
                        BackgroundColor(Color::NONE),
                        BorderColor::all(Color::NONE),
                    ))
                    .with_child((BuildLabel(i), Text::new(""), TextFont { font_size: FontSize::Px(12.0), ..default() }, TextColor(INK), TextLayout::justify(Justify::Center)));
                }
            });
        c.spawn((BuildInfo, Text::new(""), TextFont { font_size: FontSize::Px(12.0), ..default() }, TextColor(INK), Node { min_height: px(48), ..default() }));
    });
}

/// A click on an available building or the camp fire picks it as the blueprint (and puts any
/// spell away).
fn tile_clicks(
    q: Query<(&Interaction, &BuildTile), Changed<Interaction>>,
    book: Res<PlayerBuilds>,
    mut blueprint: ResMut<crate::blueprint::Blueprint>,
    mut spell: ResMut<crate::hud::spells::SelectedSpell>,
) {
    let tiles = tiles(&book.0);
    for (interaction, tile) in &q {
        let Some(plan) = tiles.get(tile.0).and_then(Tile::plan) else { continue };
        if *interaction == Interaction::Pressed {
            blueprint.pick(plan);
            spell.0 = None;
        }
    }
}

fn update_tiles(
    book: Res<PlayerBuilds>,
    blueprint: Res<crate::blueprint::Blueprint>,
    mut tiles: Query<(&BuildTile, &Interaction, &mut BackgroundColor, &mut BorderColor)>,
    mut labels: Query<(&BuildLabel, &mut Text, &mut TextFont)>,
) {
    let all = self::tiles(&book.0);
    let views: Vec<BuildTileView> = all.iter().map(Tile::view).collect();
    for (tile, interaction, mut bg, mut border) in &mut tiles {
        let Some(v) = views.get(tile.0) else { continue };
        let (fill, edge) = match (v.shown, v.unknown) {
            (false, _) => (Color::srgba(0.0, 0.0, 0.0, 0.10), Color::NONE),
            (true, true) => (Color::srgb(0.36, 0.24, 0.10), DARK_BROWN),
            (true, false) => (Color::srgb(1.0, 0.84, 0.48), DARK_BROWN),
        };
        bg.0 = fill;
        let picked = blueprint.plan.is_some() && blueprint.plan == all.get(tile.0).and_then(Tile::plan);
        *border = BorderColor::all(match (picked, v.shown && *interaction == Interaction::Hovered) {
            (true, _) => Color::WHITE,
            (_, true) => Color::srgb(1.0, 0.95, 0.7),
            _ => edge,
        });
    }
    for (label, mut text, mut font) in &mut labels {
        let Some(v) = views.get(label.0) else { continue };
        text.0 = v.label.to_string();
        font.font_size = FontSize::Px(if v.unknown { 22.0 } else { 11.0 });
    }
}

fn update_info(book: Res<PlayerBuilds>, tiles: Query<(&BuildTile, &Interaction)>, mut info: Query<&mut Text, With<BuildInfo>>) {
    let all = self::tiles(&book.0);
    let hovered = tiles.iter().find(|(_, i)| **i == Interaction::Hovered).and_then(|(t, _)| all.get(t.0));
    let text = hovered.map(Tile::describe).filter(|s| !s.is_empty()).unwrap_or_else(|| "Hover a building to see it.".into());
    for mut t in &mut info {
        t.0.clone_from(&text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_players_rewards_reach_the_panels_once() {
        use game_core::spell_book::{Availability, SpellKind};
        let mut spells = SpellBook::new();
        let mut builds = BuildBook::all(BuildAvailability::Discoverable);
        let mut seen = 0;
        let granted = vec![(0, Reward::Building(BuildingKind::Temple)), (1, Reward::Spell(SpellKind::Swarm)), (0, Reward::Spell(SpellKind::Swarm))];
        apply_rewards(&granted[..2], &mut seen, &mut spells, &mut builds);
        assert_eq!(builds.slot(BuildingKind::Temple).unwrap().availability, BuildAvailability::Available);
        assert_ne!(spells.slots.iter().find(|s| s.kind == SpellKind::Swarm).unwrap().availability, Availability::Known, "another tribe's");
        assert_eq!(seen, 2);
        apply_rewards(&granted, &mut seen, &mut spells, &mut builds);
        assert_eq!(spells.slots.iter().find(|s| s.kind == SpellKind::Swarm).unwrap().availability, Availability::Known);
        apply_rewards(&[], &mut seen, &mut spells, &mut builds);
        assert_eq!(seen, 0, "a new map starts again");
    }

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
    fn the_camp_fire_comes_last_always_available() {
        let book = BuildBook::all(BuildAvailability::Hidden);
        let tiles = tiles(&book);
        assert_eq!(tiles.len(), book.slots.len() + 1);
        assert_eq!(tiles.last(), Some(&Tile::Campfire));
        assert_eq!(tiles[0].plan(), None, "hidden building");
        assert_eq!(Tile::Campfire.plan(), Some(Plan::Campfire));
        assert_eq!(Tile::Campfire.view().label, "Camp fire");
        assert!(Tile::Campfire.describe().starts_with("Camp fire"));
        let open = Tile::Slot(slot(BuildingKind::Temple, BuildAvailability::Available));
        assert_eq!(open.plan(), Some(Plan::Building(BuildingKind::Temple)));
    }

    #[test]
    fn original_levels_bring_their_buildings() {
        let mut map = GameMap::generate(1);
        assert!(level_builds(&map).slots.iter().all(|s| s.availability == BuildAvailability::Available), "generated: all");
        let own = BuildBook::all(BuildAvailability::Hidden);
        map.build_book = Some(own.clone());
        assert_eq!(level_builds(&map), own);
    }
}
