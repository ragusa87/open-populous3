//! HUD: left control panel (shaman preview, tabs: spells, buildings, stats) and, in dev mode, the info line (the
//! level's name, the editor's brushes while it is on; the keys are on the pause menu's Shortcuts page).

pub mod build;
mod disguise;
mod mana;
mod panel;
mod population;
mod shaman;
pub mod spells;
mod stats;
mod view_menu;

use crate::editor::EditorState;
use crate::keymap::DevMode;
use crate::world::{CurrentMap, LevelList};
use bevy::prelude::*;

pub use panel::{ActiveTab, PANEL_WIDTH, TABS};

#[derive(Component)]
struct InfoText;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((panel::PanelPlugin, shaman::ShamanPreviewPlugin, spells::SpellsPlugin, build::BuildPlugin, view_menu::ViewMenuPlugin, population::PopulationPlugin, mana::ManaPlugin, stats::StatsPlugin, disguise::DisguisePlugin))
            .add_systems(Startup, spawn_info)
            .add_systems(Update, update_info);
    }
}

fn spawn_info(mut commands: Commands) {
    commands.spawn((
        InfoText,
        Text::new(""),
        TextFont { font_size: FontSize::Px(14.0), ..default() },
        Node { position_type: PositionType::Absolute, top: px(8), left: px(PANEL_WIDTH + 10.0), right: px(10), ..default() },
    ));
}

/// Where the map comes from, for the info line: none for a sandbox (its name says it).
fn level_label(name: &str, files: usize, index: usize) -> Option<String> {
    match files {
        _ if name.starts_with(game_core::map::SANDBOX_PREFIX) => None,
        0 => Some("generated".to_string()),
        n => Some(format!("level {}/{n}", index + 1)),
    }
}

fn update_info(
    map: Res<CurrentMap>,
    levels: Res<LevelList>,
    editor: Res<EditorState>,
    dev: Res<DevMode>,
    mut q: Query<(&mut Text, &mut Visibility), With<InfoText>>,
) {
    let shown = if dev.0 { Visibility::Inherited } else { Visibility::Hidden };
    let level = level_label(&map.0.name, levels.files.len(), levels.index);
    let mode = editor.active.then(|| {
        let brush = if editor.placing_tree { "O: click to plant a tree (right click/Esc cancels)".to_string() } else { format!("R raise  F lower  T flatten  M mark  B bridge  [ ] radius ({})  O tree", editor.brush_radius) };
        format!("EDITOR  {brush}")
    });
    let name = match level {
        Some(level) => format!("{} ({level})", map.0.name),
        None => map.0.name.clone(),
    };
    let s = format!("{name}\n{}", mode.unwrap_or_default());
    for (mut t, mut v) in &mut q {
        v.set_if_neq(shown);
        if t.0 != s {
            t.0.clone_from(&s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sandbox_shows_no_level_number() {
        assert_eq!(level_label("Sandbox: walk", 25, 0), None);
        assert_eq!(level_label("Level 3", 25, 2).as_deref(), Some("level 3/25"));
        assert_eq!(level_label("Generated", 0, 0).as_deref(), Some("generated"));
    }
}
