//! Blueprint (UX only): a building picked in the Build tab follows the mouse as a white footprint
//! draped on the ground, snapped to cell corners like the levels' buildings, with an arrow out of
//! its door side; red where it cannot be built (`game_core::placement`: sea, another building, a
//! site, a tree with wood; all of it when the ground is too steep). Space turns it a quarter turn, right click
//! puts it away, left click will place it (`Command::PlaceBuilding`, not yet). Meanwhile clicks do
//! not select or order units. See docs/specs/buildings.md "Blueprint".

use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::grounded::{ground_y, pick_ground};
use crate::hud::PANEL_WIDTH;
use crate::units::{world_units, PLAYER};
use crate::world::CurrentMap;
use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use game_core::building::{Building, BuildingKind};
use game_core::placement::{blocked_at, too_steep};
use pop3_format::WORLD_UNITS_PER_CELL;

/// Height above the ground (render units) and grid steps per footprint side.
const LIFT: f32 = 0.03;
const STEPS: usize = 12;
/// Door arrow: half width at its base and length (cells), and its gap from the footprint edge.
const ARROW: (f32, f32, f32) = (0.3, 0.5, 0.1);
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 0.55];
const RED: [f32; 4] = [1.0, 0.15, 0.1, 0.6];

/// The building being placed, if any, and how it is turned (eighths of a turn, quarter steps).
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blueprint {
    pub kind: Option<BuildingKind>,
    pub facing: u8,
}

impl Blueprint {
    /// Picks a building; the turn is kept from the previous one.
    pub fn pick(&mut self, kind: BuildingKind) {
        self.kind = Some(kind);
    }

    pub fn put_away(&mut self) {
        self.kind = None;
    }

    /// A quarter turn (Space).
    pub fn turn(&mut self) {
        self.facing = (self.facing + 2) % 8;
    }

    pub fn is_active(&self) -> bool {
        self.kind.is_some()
    }
}

/// The building the blueprint stands for with the mouse over map position `cell`: on the nearest
/// cell corner, as the levels store buildings (`Building::centre` is half a cell off).
pub fn blueprint_at(kind: BuildingKind, facing: u8, cell: Vec2) -> Building {
    let corner = |c: f32| ((c.round() as i32).rem_euclid(128) as u32 * WORLD_UNITS_PER_CELL) as u16;
    Building { kind, owner: PLAYER, x: corner(cell.x), z: corner(cell.y), facing }
}

/// A point of the building's own frame (cells) turned by `facing` into map axes, the same way
/// `Heightmap::level_rect` and the building view turn it.
pub fn turn_local(local: Vec2, facing: u8) -> Vec2 {
    match (facing / 2) % 4 {
        0 => local,
        1 => Vec2::new(local.y, -local.x),
        2 => -local,
        _ => Vec2::new(-local.y, local.x),
    }
}

/// The footprint rectangle in the building's own frame (cells): min and max corners.
pub fn footprint_rect(kind: BuildingKind) -> (Vec2, Vec2) {
    let f = kind.footprint();
    let unit = WORLD_UNITS_PER_CELL as f32;
    let centre = Vec2::new(f.offset.0 as f32, f.offset.1 as f32) / unit;
    let half = Vec2::new(f.half.0 as f32, f.half.1 as f32) / unit;
    (centre - half, centre + half)
}

/// The door arrow in the building's own frame (cells): a triangle out of the +z side (door side,
/// unverified), pointing away from the building.
pub fn door_arrow(kind: BuildingKind) -> [Vec2; 3] {
    let (min, max) = footprint_rect(kind);
    let x = (min.x + max.x) / 2.0;
    let base = max.y + ARROW.2;
    [Vec2::new(x - ARROW.0, base), Vec2::new(x + ARROW.0, base), Vec2::new(x, base + ARROW.1)]
}

/// Dev only (`BLUEPRINT=kind@x,z` screenshots): the map position used instead of the mouse.
#[derive(Resource, Default)]
pub struct PinnedAt(pub Option<Vec2>);

#[derive(Component)]
struct BlueprintMark;

pub struct BlueprintPlugin;

impl Plugin for BlueprintPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Blueprint>()
            .init_resource::<PinnedAt>()
            .add_systems(Startup, spawn_mark)
            .add_systems(Update, blueprint_input.after(crate::units::UnitInput).in_set(crate::menu::Gameplay))
            .add_systems(PostUpdate, draw_blueprint)
            .add_systems(OnExit(crate::menu::AppState::Playing), hide_mark);
    }
}

fn spawn_mark(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    let material = StandardMaterial { alpha_mode: AlphaMode::Blend, unlit: true, cull_mode: None, depth_bias: 20.0, ..default() };
    commands.spawn((BlueprintMark, Mesh3d(meshes.add(mesh)), MeshMaterial3d(mats.add(material)), NotShadowCaster, Transform::default(), Visibility::Hidden));
}

/// The mouse over the map (not the panel or a button), if any.
fn cursor_on_map(windows: &Query<&Window>, ui: &Query<&Interaction>) -> Option<Vec2> {
    let over_ui = ui.iter().any(|i| *i != Interaction::None);
    windows.iter().next().and_then(Window::cursor_position).filter(|c| c.x > PANEL_WIDTH && !over_ui)
}

/// Space turns it, right click on the map puts it away; left click on the map is kept for placing.
fn blueprint_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    ui: Query<&Interaction>,
    mut blueprint: ResMut<Blueprint>,
) {
    if !blueprint.is_active() {
        return;
    }
    if keys.just_pressed(KeyCode::Space) {
        blueprint.turn();
    }
    if mouse.just_pressed(MouseButton::Right) && cursor_on_map(&windows, &ui).is_some() {
        blueprint.put_away();
    }
}

fn hide_mark(mut mark: Query<&mut Visibility, With<BlueprintMark>>) {
    for mut v in &mut mark {
        *v = Visibility::Hidden;
    }
}

/// Rebuilds the mark under the mouse every frame: the footprint as a grid draped on the drawn
/// ground (each vertex red where it cannot be built, all of it on steep ground), plus the door
/// arrow (red when any part is).
#[allow(clippy::too_many_arguments)]
fn draw_blueprint(
    blueprint: Res<Blueprint>,
    state: Res<State<crate::menu::AppState>>,
    windows: Query<&Window>,
    ui: Query<&Interaction>,
    cams: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    params: Res<CurveParamsRes>,
    rig: Res<CameraRig>,
    map: Res<CurrentMap>,
    pinned: Res<PinnedAt>,
    mut mark: Query<(&Mesh3d, &mut Visibility), With<BlueprintMark>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Ok((mesh, mut vis)) = mark.single_mut() else { return };
    let playing = *state.get() == crate::menu::AppState::Playing;
    let under_mouse = || {
        let c = cursor_on_map(&windows, &ui)?;
        let (cam, at) = cams.iter().next()?;
        let ray = cam.viewport_to_world(at, c).ok()?;
        pick_ground(&map.0.terrain, rig.focus, &params.0, ray.origin, *ray.direction)
    };
    let ground = blueprint.kind.filter(|_| playing).and_then(|kind| Some((kind, pinned.0.or_else(under_mouse)?)));
    let Some((kind, cell)) = ground else {
        *vis = Visibility::Hidden;
        return;
    };
    let b = blueprint_at(kind, blueprint.facing, cell);
    let unit = WORLD_UNITS_PER_CELL as f32;
    let (cx, cz) = b.centre();
    let centre = Vec2::new(cx as f32, cz as f32) / unit;
    let size = map.0.terrain.size() as f32;
    let wrap = |d: f32| (d + size / 2.0).rem_euclid(size) - size / 2.0;
    let terrain = &map.0.terrain;
    let steep = too_steep(&map.0, &b);
    // A point of the building's frame (cells) -> render position and colour.
    let vertex = |local: Vec2| {
        let at = centre + turn_local(local, b.facing);
        let (dx, dz) = (wrap(at.x - rig.focus.x), wrap(at.y - rig.focus.y));
        let colour = if steep || blocked_at(&map.0, world_units(at)).is_some() { RED } else { WHITE };
        ([dx, ground_y(terrain, rig.focus, &params.0, dx, dz) + LIFT, dz], colour)
    };
    let (min, max) = footprint_rect(kind);
    let mut positions = Vec::new();
    let mut colours = Vec::new();
    let mut indices = Vec::new();
    for j in 0..=STEPS {
        for i in 0..=STEPS {
            let t = Vec2::new(i as f32, j as f32) / STEPS as f32;
            let (p, c) = vertex(min + (max - min) * t);
            positions.push(p);
            colours.push(c);
        }
    }
    let row = STEPS as u32 + 1;
    for j in 0..STEPS as u32 {
        for i in 0..STEPS as u32 {
            let a = j * row + i;
            indices.extend([a, a + row, a + 1, a + 1, a + row, a + row + 1]);
        }
    }
    let first = positions.len() as u32;
    let arrow = if colours.contains(&RED) { RED } else { WHITE };
    for p in door_arrow(kind) {
        let (p, _) = vertex(p);
        positions.push(p);
        colours.push(arrow);
    }
    indices.extend([first, first + 1, first + 2]);
    if let Some(mut m) = meshes.get_mut(&mesh.0) {
        let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
        m.insert_indices(Indices::U32(indices));
    }
    *vis = Visibility::Inherited;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaps_to_the_nearest_corner_and_wraps() {
        let b = blueprint_at(BuildingKind::Hut { size: 1 }, 2, Vec2::new(10.4, 20.6));
        assert_eq!((b.x, b.z, b.facing, b.owner), (10 * 512, 21 * 512, 2, PLAYER));
        assert_eq!(blueprint_at(BuildingKind::Temple, 0, Vec2::new(127.7, 0.2)).x, 0, "past the edge");
    }

    #[test]
    fn turns_like_the_building_view() {
        assert_eq!(turn_local(Vec2::new(1.0, 2.0), 0), Vec2::new(1.0, 2.0));
        assert_eq!(turn_local(Vec2::new(0.0, 1.0), 2), Vec2::new(1.0, 0.0), "door side +z faces +x after a quarter turn");
        let q = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * Vec3::Z;
        assert!((q.x - 1.0).abs() < 1e-6 && q.z.abs() < 1e-6, "same as facing_yaw(2)");
        assert_eq!(turn_local(Vec2::new(1.0, 2.0), 4), Vec2::new(-1.0, -2.0));
        assert_eq!(turn_local(Vec2::new(1.0, 2.0), 8), Vec2::new(1.0, 2.0));
    }

    #[test]
    fn footprint_and_arrow_in_cells() {
        let (min, max) = footprint_rect(BuildingKind::Hut { size: 1 });
        assert!((max.x - 600.0 / 512.0).abs() < 1e-6 && (min.x + max.x).abs() < 1e-6);
        let arrow = door_arrow(BuildingKind::Hut { size: 1 });
        assert!(arrow.iter().all(|p| p.y > max.y), "outside the door side");
        assert!(arrow[2].y > arrow[0].y, "points away");
    }

    #[test]
    fn picking_turning_and_putting_away() {
        let mut b = Blueprint::default();
        assert!(!b.is_active());
        b.pick(BuildingKind::Temple);
        b.turn();
        b.turn();
        b.turn();
        b.turn();
        assert_eq!((b.kind, b.facing), (Some(BuildingKind::Temple), 0));
        b.turn();
        b.pick(BuildingKind::DrumTower);
        assert_eq!(b.facing, 2, "the turn is kept");
        b.put_away();
        assert!(!b.is_active());
    }
}
