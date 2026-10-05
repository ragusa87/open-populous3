//! HUD: level name + help text, and the bottom control tabs (placeholders).

use crate::camera::CameraRig;
use crate::editor::EditorState;
use crate::world::{CurrentMap, LevelList};
use bevy::prelude::*;

pub const TABS: [&str; 3] = ["Spells", "Buildings", "Followers"];

#[derive(Component)]
struct InfoText;

#[derive(Component)]
struct Tab(usize);

#[derive(Resource, Default)]
pub struct ActiveTab(pub usize);

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveTab>()
            .add_systems(Startup, spawn_hud)
            .add_systems(Update, (update_info, tab_clicks, tab_colors));
    }
}

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        InfoText,
        Text::new(""),
        TextFont { font_size: FontSize::Px(16.0), ..default() },
        Node { position_type: PositionType::Absolute, top: px(8), left: px(8), ..default() },
    ));
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: px(8),
            left: px(8),
            column_gap: px(4),
            ..default()
        })
        .with_children(|bar| {
            for (i, label) in TABS.iter().enumerate() {
                bar.spawn((
                    Tab(i),
                    Button,
                    Node { padding: UiRect::axes(px(12), px(6)), ..default() },
                    BackgroundColor(Color::BLACK),
                ))
                .with_child((Text::new(*label), TextFont { font_size: FontSize::Px(14.0), ..default() }));
            }
        });
}

fn update_info(
    map: Res<CurrentMap>,
    levels: Res<LevelList>,
    rig: Res<CameraRig>,
    editor: Res<EditorState>,
    mut q: Query<&mut Text, With<InfoText>>,
) {
    let level = match levels.files.len() {
        0 => "no original levels found, generated map".to_string(),
        n => format!("level {}/{n}", levels.index + 1),
    };
    let mode = if editor.active { "EDITOR  R raise  F lower  T flatten  M mark  B bridge" } else { "" };
    let s = format!(
        "{} ({level})  focus {:.0},{:.0}{}\n\
         Mouse at screen edge scroll | Left/Right rotate, Up/Down move | middle-drag rotate | Enter aerial | PgUp/PgDn level | Tab editor | F11 fullscreen\n{mode}",
        map.0.name,
        rig.focus.x,
        rig.focus.y,
        if rig.aerial { "  [aerial]" } else { "" },
    );
    for mut t in &mut q {
        t.0.clone_from(&s);
    }
}

fn tab_clicks(q: Query<(&Interaction, &Tab), Changed<Interaction>>, mut active: ResMut<ActiveTab>) {
    for (i, tab) in &q {
        if *i == Interaction::Pressed {
            active.0 = tab.0;
        }
    }
}

fn tab_colors(active: Res<ActiveTab>, mut q: Query<(&Tab, &mut BackgroundColor)>) {
    for (tab, mut bg) in &mut q {
        bg.0 = if tab.0 == active.0 { Color::srgb(0.45, 0.30, 0.10) } else { Color::srgba(0.0, 0.0, 0.0, 0.6) };
    }
}
