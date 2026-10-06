//! F2 view menu: pick a camera/terrain preset (distance, tilt, relief, curvature), applied live.

use super::panel::{DARK_BROWN, INK, PANEL_WIDTH, PARCHMENT};
use crate::camera::{CameraRig, CurveParamsRes};
use crate::terrain_mesh::CurveParams;
use crate::world::TerrainDirty;
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewPreset {
    pub name: &'static str,
    /// Multiplier on the default terrain height scale.
    pub relief: f32,
    pub curvature: f32,
    pub distance: f32,
    pub pitch_deg: f32,
}

pub const PRESETS: [ViewPreset; 6] = [
    ViewPreset { name: "A  previous default", relief: 1.0, curvature: 0.012, distance: 20.0, pitch_deg: 3.0 },
    ViewPreset { name: "B  relief x2", relief: 2.0, curvature: 0.012, distance: 20.0, pitch_deg: 3.0 },
    ViewPreset { name: "C  closer", relief: 1.0, curvature: 0.012, distance: 10.0, pitch_deg: 3.0 },
    ViewPreset { name: "D  closer, relief x2", relief: 2.0, curvature: 0.012, distance: 10.0, pitch_deg: 3.0 },
    ViewPreset { name: "E  D + flatter planet", relief: 2.0, curvature: 0.006, distance: 10.0, pitch_deg: 3.0 },
    ViewPreset { name: "F  3/4 view", relief: 1.5, curvature: 0.006, distance: 10.0, pitch_deg: 12.0 },
];

impl ViewPreset {
    pub fn curve(&self, base: CurveParams) -> CurveParams {
        CurveParams { height_scale: CurveParams::default().height_scale * self.relief, curvature: self.curvature, ..base }
    }

    /// One line describing the preset values, for the menu and to report a choice.
    pub fn describe(&self) -> String {
        format!(
            "relief x{}  curvature {}  distance {}  tilt {}deg",
            self.relief, self.curvature, self.distance, self.pitch_deg
        )
    }
}

#[derive(Resource, Default)]
struct ViewMenu {
    open: bool,
    /// Index into `PRESETS`, None = the startup view (defaults / env overrides).
    active: Option<usize>,
}

#[derive(Component)]
struct MenuRoot;

#[derive(Component)]
struct PresetButton(usize);

#[derive(Component)]
struct CurrentLine;

pub struct ViewMenuPlugin;

impl Plugin for ViewMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ViewMenu>()
            .add_systems(Startup, spawn_menu)
            .add_systems(Update, (toggle_menu, preset_clicks, menu_visuals).chain());
    }
}

fn spawn_menu(mut commands: Commands) {
    let text = |s: &str, size: f32| (Text::new(s), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(INK));
    commands
        .spawn((
            MenuRoot,
            Node {
                position_type: PositionType::Absolute,
                top: px(84),
                left: px(PANEL_WIDTH + 10.0),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                padding: UiRect::all(px(8)),
                border: UiRect::all(px(3)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(PARCHMENT),
            BorderColor::all(DARK_BROWN),
            GlobalZIndex(10),
        ))
        .with_children(|menu| {
            menu.spawn(text("View presets (F2 closes)", 15.0));
            for (i, p) in PRESETS.iter().enumerate() {
                menu.spawn((
                    PresetButton(i),
                    Button,
                    Node { flex_direction: FlexDirection::Column, padding: UiRect::axes(px(8), px(4)), border: UiRect::all(px(2)), ..default() },
                    BackgroundColor(PARCHMENT),
                    BorderColor::all(DARK_BROWN),
                ))
                .with_children(|b| {
                    b.spawn(text(p.name, 14.0));
                    b.spawn(text(&p.describe(), 11.0));
                });
            }
            menu.spawn((CurrentLine, text("", 12.0)));
        });
}

fn toggle_menu(keys: Res<ButtonInput<KeyCode>>, mut menu: ResMut<ViewMenu>) {
    if keys.just_pressed(KeyCode::F2) {
        menu.open = !menu.open;
    }
}

fn preset_clicks(
    q: Query<(&Interaction, &PresetButton), Changed<Interaction>>,
    mut menu: ResMut<ViewMenu>,
    mut rig: ResMut<CameraRig>,
    mut params: ResMut<CurveParamsRes>,
    mut dirty: ResMut<TerrainDirty>,
) {
    for (interaction, button) in &q {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let p = PRESETS[button.0];
        params.0 = p.curve(params.0);
        rig.set_ground_view(p.pitch_deg.to_radians(), p.distance);
        dirty.0 = true;
        menu.active = Some(button.0);
        info!("view preset {}: {}", p.name, p.describe());
    }
}

fn menu_visuals(
    menu: Res<ViewMenu>,
    params: Res<CurveParamsRes>,
    rig: Res<CameraRig>,
    mut root: Query<&mut Node, With<MenuRoot>>,
    mut buttons: Query<(&PresetButton, &Interaction, &mut BackgroundColor)>,
    mut current: Query<&mut Text, With<CurrentLine>>,
) {
    for mut node in &mut root {
        node.display = if menu.open { Display::Flex } else { Display::None };
    }
    if !menu.open {
        return;
    }
    for (b, interaction, mut bg) in &mut buttons {
        bg.0 = match (menu.active == Some(b.0), interaction) {
            (true, _) => Color::srgb(1.0, 0.82, 0.40),
            (_, Interaction::Hovered) => Color::srgb(0.95, 0.72, 0.32),
            _ => PARCHMENT,
        };
    }
    let relief = params.0.height_scale / CurveParams::default().height_scale;
    let line = format!(
        "Now: relief x{relief:.2}  curvature {}  distance {:.1}  tilt {:.0}deg",
        params.0.curvature,
        rig.distance,
        rig.pitch.to_degrees()
    );
    for mut t in &mut current {
        if t.0 != line {
            t.0.clone_from(&line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_preset_is_the_previous_default() {
        let base = CurveParams::default();
        let a = PRESETS[0];
        assert_eq!(a.curve(base).height_scale, base.height_scale);
        assert_eq!(a.curve(base).curvature, base.curvature);
        assert!((a.pitch_deg.to_radians() - crate::camera::GROUND_VIEW.0).abs() < 1e-6);
        assert_eq!(a.distance, crate::camera::GROUND_VIEW.1);
    }

    #[test]
    fn presets_keep_the_draw_radius() {
        let base = CurveParams { radius: 7, ..CurveParams::default() };
        let f = PRESETS[5].curve(base);
        assert_eq!(f.radius, 7);
        assert_eq!(f.height_scale, CurveParams::default().height_scale * 1.5);
    }
}
