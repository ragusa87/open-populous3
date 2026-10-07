//! Build tab: a grid of tiles mirroring the player's `BuildBook`, one per building of the original
//! panel, named (no icons yet). Tile states: empty (hidden), "?" (plans to discover), named (available).
//! Hover describes it, a click on an available one picks it as the blueprint (`crate::blueprint`,
//! white border while picked). Placing buildings comes later (docs/specs/buildings.md).

use super::panel::{TabContent, DARK_BROWN, INK};
use bevy::prelude::*;
use game_core::build_book::{BuildAvailability, BuildBook, BuildSlot};
use game_core::building::BuildingKind;
use game_core::map::GameMap;

const COLUMNS: u16 = 3;

#[derive(Resource)]
pub struct PlayerBuilds(pub BuildBook);

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
            .add_systems(Update, ((tile_clicks.in_set(crate::menu::Gameplay), update_tiles).chain(), update_info));
    }
}

fn spawn_tab(mut commands: Commands, tabs: Query<(Entity, &TabContent)>, book: Res<PlayerBuilds>) {
    let Some((entity, _)) = tabs.iter().find(|(_, t)| t.0 == 1) else { return };
    commands.entity(entity).with_children(|c| {
        c.spawn(Node { display: Display::Grid, grid_template_columns: RepeatedGridTrack::flex(COLUMNS, 1.0), row_gap: px(4), column_gap: px(4), ..default() })
            .with_children(|grid| {
                for i in 0..book.0.slots.len() {
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

/// A click on an available building picks it as the blueprint (and puts any spell away).
fn tile_clicks(
    q: Query<(&Interaction, &BuildTile), Changed<Interaction>>,
    book: Res<PlayerBuilds>,
    mut blueprint: ResMut<crate::blueprint::Blueprint>,
    mut spell: ResMut<crate::hud::spells::SelectedSpell>,
) {
    for (interaction, tile) in &q {
        let Some(slot) = book.0.slots.get(tile.0) else { continue };
        if *interaction == Interaction::Pressed && slot.availability == BuildAvailability::Available {
            blueprint.pick(slot.kind);
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
    let views: Vec<BuildTileView> = book.0.slots.iter().map(build_tile_view).collect();
    for (tile, interaction, mut bg, mut border) in &mut tiles {
        let Some(v) = views.get(tile.0) else { continue };
        let (fill, edge) = match (v.shown, v.unknown) {
            (false, _) => (Color::srgba(0.0, 0.0, 0.0, 0.10), Color::NONE),
            (true, true) => (Color::srgb(0.36, 0.24, 0.10), DARK_BROWN),
            (true, false) => (Color::srgb(1.0, 0.84, 0.48), DARK_BROWN),
        };
        bg.0 = fill;
        let picked = blueprint.kind.is_some_and(|k| book.0.slots.get(tile.0).is_some_and(|s| s.kind == k));
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
    let hovered = tiles.iter().find(|(_, i)| **i == Interaction::Hovered).and_then(|(t, _)| book.0.slots.get(t.0));
    let text = hovered.map(describe_build).filter(|s| !s.is_empty()).unwrap_or_else(|| "Hover a building to see it.".into());
    for mut t in &mut info {
        t.0.clone_from(&text);
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
    fn original_levels_bring_their_buildings() {
        let mut map = GameMap::generate(1);
        assert!(level_builds(&map).slots.iter().all(|s| s.availability == BuildAvailability::Available), "generated: all");
        let own = BuildBook::all(BuildAvailability::Hidden);
        map.build_book = Some(own.clone());
        assert_eq!(level_builds(&map), own);
    }
}
