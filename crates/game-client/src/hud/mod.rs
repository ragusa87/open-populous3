//! HUD: left control panel (tabs: spells, buildings, stats) and the info line.

mod panel;
mod spells;

use crate::camera::CameraRig;
use crate::editor::EditorState;
use crate::world::{CurrentMap, LevelList};
use bevy::prelude::*;

pub use panel::PANEL_WIDTH;

#[derive(Component)]
struct InfoText;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((panel::PanelPlugin, spells::SpellsPlugin))
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

fn update_info(
    map: Res<CurrentMap>,
    levels: Res<LevelList>,
    rig: Res<CameraRig>,
    editor: Res<EditorState>,
    mut q: Query<&mut Text, With<InfoText>>,
) {
    let level = match levels.files.len() {
        0 => "generated".to_string(),
        n => format!("level {}/{n}", levels.index + 1),
    };
    let mode = if editor.active { "EDITOR  R raise  F lower  T flatten  M mark  B bridge" } else { "" };
    let s = format!(
        "{} ({level})  focus {:.0},{:.0}  tilt {:.0}deg  distance {:.1}  fov {:.0}deg{}\n\
         Push mouse on window edges / Up-Down / WASD move | Left-Right rotate | Home/End tilt | Ctrl+PgUp/PgDn zoom | Shift+PgUp/PgDn fov | Enter aerial | PgUp/PgDn level | C cast selected spell | Tab editor | Esc free cursor | F11 fullscreen\n{mode}",
        map.0.name,
        rig.focus.x,
        rig.focus.y,
        rig.pitch.to_degrees(),
        rig.distance,
        rig.fov.to_degrees(),
        if rig.aerial { "  [aerial]" } else { "" },
    );
    for mut t in &mut q {
        t.0.clone_from(&s);
    }
}
