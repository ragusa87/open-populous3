//! Dev-mode grid readout: while `ShowGrid` is on, the cell under the mouse gets a warm tint on the ground
//! (`ground.wgsl`'s `highlight` uniform) and its coordinates and height print bottom-left, in the same spirit as
//! `camera_debug.rs`'s own readout - so a bug report can name an exact cell instead of a screenshot and a guess.

use crate::blueprint::Blueprint;
use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::ground_material::{highlight_uniform, GroundMaterial};
use crate::grounded::pick_ground;
use crate::hud::PANEL_WIDTH;
use crate::keymap::Shortcuts;
use crate::world::{CurrentMap, ShowGrid, TerrainMaterial};
use bevy::prelude::*;
use game_core::map::GameMap;

/// The cell under the mouse (dev mode, grid on, cursor over the map, a ground point under it): map
/// coordinates, unwrapped (the disc's own, `terrain_mesh::build`'s doc comment) - `cell_line` wraps them for
/// display.
#[derive(Resource, Default, Clone, Copy, PartialEq)]
pub struct HoveredCell(pub Option<(i32, i32)>);

#[derive(Component)]
struct CellReadout;

pub struct CellDebugPlugin;

impl Plugin for CellDebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HoveredCell>()
            .add_systems(Startup, spawn_readout)
            .add_systems(Update, (track_hover, sync_highlight, update_readout).chain().in_set(crate::menu::Gameplay));
    }
}

/// The debug-overlay line for `cell`, its height read from `map` (`Heightmap::get` wraps it on both axes): None
/// without a hovered cell.
pub fn cell_line(cell: Option<(i32, i32)>, map: &GameMap) -> Option<String> {
    let (x, z) = cell?;
    let size = map.terrain.size() as i32;
    let (cx, cz) = (x.rem_euclid(size), z.rem_euclid(size));
    Some(format!("cell {cx}, {cz}  height {}", map.terrain.get(cx, cz)))
}

#[allow(clippy::too_many_arguments)]
fn track_hover(
    keys: Shortcuts,
    grid: Res<ShowGrid>,
    windows: Query<&Window>,
    ui: Query<&Interaction>,
    blueprint: Res<Blueprint>,
    cams: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    map: Res<CurrentMap>,
    rig: Res<CameraRig>,
    params: Res<CurveParamsRes>,
    mut hovered: ResMut<HoveredCell>,
) {
    if !keys.dev() || !grid.0 {
        hovered.set_if_neq(HoveredCell(None));
        return;
    }
    let cursor = windows.iter().next().and_then(Window::cursor_position);
    let over_ui = ui.iter().any(|i| *i != Interaction::None);
    let on_map = cursor.filter(|c| c.x > PANEL_WIDTH && !over_ui && !blueprint.is_active());
    let cell = on_map.zip(cams.iter().next()).and_then(|(c, cam)| {
        let ray = cam.0.viewport_to_world(cam.1, c).ok()?;
        let p = pick_ground(&map.0.terrain, rig.focus, &params.0, ray.origin, *ray.direction)?;
        Some((p.x.floor() as i32, p.y.floor() as i32))
    });
    hovered.set_if_neq(HoveredCell(cell));
}

fn sync_highlight(hovered: Res<HoveredCell>, material: Res<TerrainMaterial>, mut mats: ResMut<Assets<GroundMaterial>>) {
    if let Some(mut m) = mats.get_mut(&material.0) {
        let want = highlight_uniform(hovered.0);
        if m.highlight != want {
            m.highlight = want;
        }
    }
}

fn spawn_readout(mut commands: Commands) {
    commands.spawn((
        CellReadout,
        Text::new(""),
        TextFont { font_size: FontSize::Px(13.0), ..default() },
        Node { position_type: PositionType::Absolute, bottom: px(8), left: px(PANEL_WIDTH + 10.0), ..default() },
        Visibility::Hidden,
    ));
}

fn update_readout(hovered: Res<HoveredCell>, map: Res<CurrentMap>, mut q: Query<(&mut Text, &mut Visibility), With<CellReadout>>) {
    let text = cell_line(hovered.0, &map.0);
    let shown = if text.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    for (mut t, mut v) in &mut q {
        if let Some(s) = &text && t.0 != *s {
            t.0 = s.clone();
        }
        v.set_if_neq(shown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_line_wraps_negative_and_overflowing_coordinates_and_shows_the_height() {
        let mut map = GameMap::sandbox_walk();
        map.terrain.set(3, 5, 77);
        assert_eq!(cell_line(Some((3, 5)), &map).as_deref(), Some("cell 3, 5  height 77"));
        let size = map.terrain.size() as i32;
        assert_eq!(cell_line(Some((3 - size, 5 + size)), &map).as_deref(), Some("cell 3, 5  height 77"), "the disc's own unwrapped coordinates");
        assert_eq!(cell_line(None, &map), None);
    }
}
