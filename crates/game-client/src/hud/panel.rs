//! Left panel frame: shaman preview box (filled by `shaman.rs`), tab buttons, one content node per tab.

use bevy::prelude::*;

pub const PANEL_WIDTH: f32 = 204.0;
pub const TABS: [&str; 3] = ["Spells", "Build", "Stats"];

pub const PARCHMENT: Color = Color::srgb(0.80, 0.56, 0.20);
pub const DARK_BROWN: Color = Color::srgb(0.30, 0.18, 0.06);
pub const INK: Color = Color::srgb(0.18, 0.10, 0.03);

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub struct ActiveTab(pub usize);

#[derive(Component)]
struct TabButton(usize);

/// The box at the top of the panel showing the player's shaman.
#[derive(Component)]
pub struct ShamanPreview;

/// Content root of tab `n`; other plugins spawn their widgets inside it.
#[derive(Component)]
pub struct TabContent(pub usize);

pub struct PanelPlugin;

impl Plugin for PanelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveTab>()
            .add_systems(PreStartup, spawn_panel)
            .add_systems(Update, (tab_clicks, tab_visuals));
    }
}

fn spawn_panel(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                bottom: px(0),
                width: px(PANEL_WIDTH),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(6)),
                row_gap: px(6),
                border: UiRect::right(px(3)),
                ..default()
            },
            BackgroundColor(PARCHMENT),
            BorderColor::all(DARK_BROWN),
        ))
        .with_children(|panel| {
            panel.spawn((
                ShamanPreview,
                Button,
                Node { height: px(120), border: UiRect::all(px(2)), overflow: Overflow::clip(), ..default() },
                BackgroundColor(Color::srgb(0.08, 0.16, 0.30)),
                BorderColor::all(DARK_BROWN),
            ));
            panel.spawn(Node { column_gap: px(4), ..default() }).with_children(|row| {
                for (i, label) in TABS.iter().enumerate() {
                    row.spawn((
                        TabButton(i),
                        Button,
                        Node {
                            flex_grow: 1.0,
                            height: px(28),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border: UiRect::all(px(2)),
                            ..default()
                        },
                        BackgroundColor(PARCHMENT),
                        BorderColor::all(DARK_BROWN),
                    ))
                    .with_child((Text::new(*label), TextFont { font_size: FontSize::Px(13.0), ..default() }, TextColor(INK)));
                }
            });
            for i in 0..TABS.len() {
                panel.spawn((TabContent(i), Node { flex_direction: FlexDirection::Column, row_gap: px(6), ..default() }));
            }
        });
}

fn tab_clicks(q: Query<(&Interaction, &TabButton), Changed<Interaction>>, mut active: ResMut<ActiveTab>) {
    for (i, tab) in &q {
        if *i == Interaction::Pressed {
            active.0 = tab.0;
        }
    }
}

fn tab_visuals(
    active: Res<ActiveTab>,
    mut buttons: Query<(&TabButton, &mut BackgroundColor)>,
    mut contents: Query<(&TabContent, &mut Node)>,
) {
    for (tab, mut bg) in &mut buttons {
        bg.0 = if tab.0 == active.0 { Color::srgb(1.0, 0.82, 0.40) } else { Color::srgb(0.62, 0.42, 0.14) };
    }
    for (content, mut node) in &mut contents {
        node.display = if content.0 == active.0 { Display::Flex } else { Display::None };
    }
}
