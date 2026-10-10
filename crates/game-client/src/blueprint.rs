//! Blueprint (UX only): a building picked in the Build tab follows the mouse as a white footprint
//! draped on the ground, snapped to cell corners like the levels' buildings, with an arrow out of
//! its door side (local -z, the land side of the levels' boat huts); red where it cannot be built
//! (`game_core::placement`: sea, another building, a site, a tree with wood; all of it when the
//! ground is too steep or a boat hut is not on the shore). A boat hut turns itself to put its
//! jetty over the water when it can (`best_facing`). Space turns it a quarter turn, right click
//! puts it away, left click places it where it can stand (`Command::PlaceBuilding`), sends the
//! selected braves to build it (`GameMap::build_orders`) and puts it away. Meanwhile clicks do not
//! select or order units. See docs/specs/buildings.md "Blueprint".
//! A camp fire's blueprint is the cell under the mouse, red where it cannot be lit
//! (`campfire::can_place`); a left click lights it (`Command::PlaceCampfire`) and puts it away.

use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::grounded::{ground_point, pick_ground};
use crate::hud::PANEL_WIDTH;
use crate::units::{world_units, PLAYER};
use crate::virtual_cursor::CursorLook;
use crate::world::CurrentMap;
use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use game_core::building::{Building, BuildingKind};
use game_core::campfire;
use game_core::command::Command;
use game_core::map::GameMap;
use game_core::placement::{best_facing, blocked_at, can_place, shore_ok, too_steep};
use pop3_format::WORLD_UNITS_PER_CELL;

/// Height above the ground (render units), and grid lines per cell: the mark's grid follows the
/// terrain's cell lines so its triangles lie on the drawn ones (`grid_lines`).
const LIFT: f32 = 0.02;
const LINES_PER_CELL: f32 = 4.0;
/// Door arrow: half width at its base and length (cells), and its gap from the footprint edge.
const ARROW: (f32, f32, f32) = (0.3, 0.5, 0.1);
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 0.55];
const RED: [f32; 4] = [1.0, 0.15, 0.1, 0.6];

/// What a blueprint places.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Plan {
    Building(BuildingKind),
    Campfire,
}

/// What is being placed, if anything, and how it is turned (eighths of a turn, quarter steps).
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blueprint {
    pub plan: Option<Plan>,
    pub facing: u8,
}

impl Blueprint {
    /// Picks what to place; the turn is kept from the previous one.
    pub fn pick(&mut self, plan: Plan) {
        self.plan = Some(plan);
    }

    pub fn put_away(&mut self) {
        self.plan = None;
    }

    /// A quarter turn (Space).
    pub fn turn(&mut self) {
        self.facing = (self.facing + 2) % 8;
    }

    pub fn is_active(&self) -> bool {
        self.plan.is_some()
    }
}

/// The cell (map corner, cells) a camp fire would take with the mouse over map position `at`.
pub fn campfire_cell(at: Vec2) -> (i32, i32) {
    campfire::cell_at(world_units(at))
}

/// The building the blueprint stands for with the mouse over map position `cell`: on the nearest
/// cell corner, as the levels store buildings (`Building::centre` is half a cell off).
pub fn blueprint_at(kind: BuildingKind, facing: u8, cell: Vec2) -> Building {
    let corner = |c: f32| ((c.round() as i32).rem_euclid(128) as u32 * WORLD_UNITS_PER_CELL) as u16;
    Building::site(kind, PLAYER, corner(cell.x), corner(cell.y), facing)
}

/// The command placing a plan of `kind` with the mouse over map position `cell`, turned to fit the
/// shore like its blueprint; None where it cannot stand.
pub fn place_command(map: &GameMap, kind: BuildingKind, facing: u8, cell: Vec2) -> Option<Command> {
    let b = blueprint_at(kind, facing, cell);
    let b = Building { facing: best_facing(map, &b, facing), ..b };
    can_place(map, &b).then_some(Command::PlaceBuilding { player: PLAYER, kind, at: (b.x, b.z), facing: b.facing })
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

/// The door arrow in the building's own frame (cells): a triangle out of the -z side (the land
/// side of the levels' boat huts, their jetty is +z; other kinds unverified), pointing away.
pub fn door_arrow(kind: BuildingKind) -> [Vec2; 3] {
    let (min, max) = footprint_rect(kind);
    let x = (min.x + max.x) / 2.0;
    let base = min.y - ARROW.2;
    [Vec2::new(x - ARROW.0, base), Vec2::new(x + ARROW.0, base), Vec2::new(x, base - ARROW.1)]
}

/// The map rectangle (cells) a building's footprint covers: its turns are quarter turns, so it
/// stays aligned with the map axes.
pub fn world_rect(kind: BuildingKind, facing: u8, centre: Vec2) -> (Vec2, Vec2) {
    let (min, max) = footprint_rect(kind);
    let (a, b) = (turn_local(min, facing), turn_local(max, facing));
    (centre + a.min(b), centre + a.max(b))
}

/// Coordinates from `lo` to `hi` (both kept) through every multiple of 1 / `LINES_PER_CELL` in
/// between: with the terrain's cell lines among them, each grid quad lies in one terrain cell and its
/// diagonal on the cell's own split, so the mark lies exactly on the drawn triangles.
pub fn grid_lines(lo: f32, hi: f32) -> Vec<f32> {
    let first = (lo * LINES_PER_CELL).floor() as i32 + 1;
    let last = (hi * LINES_PER_CELL).ceil() as i32 - 1;
    let inner = (first..=last).map(|k| k as f32 / LINES_PER_CELL).filter(|&v| v - lo > 1e-4 && hi - v > 1e-4);
    std::iter::once(lo).chain(inner).chain(std::iter::once(hi)).collect()
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

/// The mark gets its mesh when first drawn (`put_mark`): Bevy 0.19 logs a "Use-after-free" error for
/// a mesh without vertices.
fn spawn_mark(mut commands: Commands, mut mats: ResMut<Assets<StandardMaterial>>) {
    commands.spawn((BlueprintMark, MeshMaterial3d(mats.add(mark_material())), NotShadowCaster, Transform::default(), Visibility::Hidden));
}

/// Draws `mark` into the mark's mesh, made the first time.
fn put_mark(commands: &mut Commands, mark_entity: Entity, mesh: Option<&Mesh3d>, meshes: &mut Assets<Mesh>, mark: MarkMesh) {
    match mesh {
        Some(mesh) => {
            if let Some(mut m) = meshes.get_mut(&mesh.0) {
                set_mark(&mut m, mark);
            }
        }
        None => {
            let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
            set_mark(&mut m, mark);
            commands.entity(mark_entity).insert(Mesh3d(meshes.add(m)));
        }
    }
}

/// The mouse over the map (not the panel or a button), if any.
fn cursor_on_map(windows: &Query<&Window>, ui: &Query<&Interaction>) -> Option<Vec2> {
    let over_ui = ui.iter().any(|i| *i != Interaction::None);
    windows.iter().next().and_then(Window::cursor_position).filter(|c| c.x > PANEL_WIDTH && !over_ui)
}

/// The map position (cells) under the mouse, if it is over the map.
fn ground_under_mouse(
    windows: &Query<&Window>,
    ui: &Query<&Interaction>,
    cams: &Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    map: &CurrentMap,
    rig: &CameraRig,
    params: &CurveParamsRes,
) -> Option<Vec2> {
    let c = cursor_on_map(windows, ui)?;
    let (cam, at) = cams.iter().next()?;
    let ray = cam.viewport_to_world(at, c).ok()?;
    pick_ground(&map.0.terrain, rig.focus, &params.0, ray.origin, *ray.direction)
}

/// Space turns it, right click on the map puts it away; left click on the map lights a camp fire or
/// places a building plan with the selected braves sent to it, and puts it away.
#[allow(clippy::too_many_arguments)]
fn blueprint_input(
    keys: crate::keymap::Shortcuts,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    ui: Query<&Interaction>,
    cams: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    rig: Res<CameraRig>,
    params: Res<CurveParamsRes>,
    map: Res<CurrentMap>,
    mut schedule: ResMut<crate::units::GameSchedule>,
    mut blueprint: ResMut<Blueprint>,
    selection: Res<crate::units::selection::Selection>,
) {
    if !blueprint.is_active() {
        return;
    }
    if keys.just_pressed(crate::keymap::Shortcut::TurnPlan) {
        blueprint.turn();
    }
    if mouse.just_pressed(MouseButton::Right) && cursor_on_map(&windows, &ui).is_some() {
        blueprint.put_away();
    }
    if let (Some(Plan::Building(kind)), true) = (blueprint.plan, mouse.just_pressed(MouseButton::Left)) {
        let Some(cell) = ground_under_mouse(&windows, &ui, &cams, &map, &rig, &params) else { return };
        let Some(place) = place_command(&map.0, kind, blueprint.facing, cell) else { return };
        let orders = map.0.place_orders(&place, &selection.units);
        schedule.issue(place);
        orders.into_iter().for_each(|c| schedule.issue(c));
        blueprint.put_away();
    }
    if blueprint.plan == Some(Plan::Campfire) && mouse.just_pressed(MouseButton::Left) {
        let Some(cell) = ground_under_mouse(&windows, &ui, &cams, &map, &rig, &params).map(campfire_cell) else { return };
        if campfire::can_place(&map.0, cell) {
            let at = world_units(Vec2::new(cell.0 as f32 + 0.5, cell.1 as f32 + 0.5));
            schedule.issue(Command::PlaceCampfire { player: PLAYER, at });
            blueprint.put_away();
        }
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
    mut mark: Query<(Entity, Option<&Mesh3d>, &mut Visibility), With<BlueprintMark>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut look: ResMut<CursorLook>,
    mut commands: Commands,
) {
    let Ok((mark_entity, mesh, mut vis)) = mark.single_mut() else { return };
    let playing = *state.get() == crate::menu::AppState::Playing;
    let under_mouse = || ground_under_mouse(&windows, &ui, &cams, &map, &rig, &params);
    let ground = blueprint.plan.filter(|_| playing).and_then(|plan| Some((plan, pinned.0.or_else(under_mouse)?)));
    let Some((plan, cell)) = ground else {
        *vis = Visibility::Hidden;
        if matches!(*look, CursorLook::Building { .. }) {
            look.set_if_neq(CursorLook::Arrow);
        }
        return;
    };
    let terrain = &map.0.terrain;
    // A map position (cells) -> render position on the ground.
    let on_ground = |at: Vec2| <[f32; 3]>::from(ground_point(terrain, rig.focus, &params.0, at) + Vec3::Y * LIFT);
    let kind = match plan {
        Plan::Building(kind) => kind,
        Plan::Campfire => {
            let fire = campfire_cell(cell);
            let colour = if campfire::can_place(&map.0, fire) { WHITE } else { RED };
            let lo = Vec2::new(fire.0 as f32, fire.1 as f32);
            let mark = grid_mark(lo, lo + Vec2::ONE, |at| (on_ground(at), colour));
            look.set_if_neq(CursorLook::Building { plan, valid: colour == WHITE });
            put_mark(&mut commands, mark_entity, mesh, &mut meshes, mark);
            *vis = Visibility::Inherited;
            return;
        }
    };
    let b = blueprint_at(kind, blueprint.facing, cell);
    let b = Building { facing: best_facing(&map.0, &b, blueprint.facing), ..b };
    let unit = WORLD_UNITS_PER_CELL as f32;
    let (cx, cz) = b.centre();
    let centre = Vec2::new(cx as f32, cz as f32) / unit;
    let all_red = too_steep(&map.0, &b) || !shore_ok(&map.0, &b);
    // A map position (cells) -> render position and colour.
    let vertex = |at: Vec2| (on_ground(at), if all_red || blocked_at(&map.0, world_units(at)).is_some() { RED } else { WHITE });
    let mark = mark_mesh(kind, b.facing, centre, vertex);
    look.set_if_neq(CursorLook::Building { plan, valid: !mark.colours.contains(&RED) });
    put_mark(&mut commands, mark_entity, mesh, &mut meshes, mark);
    *vis = Visibility::Inherited;
}

/// A footprint mark's triangles: the footprint as a grid through the terrain's cell lines (each
/// grid quad split like the terrain's cells) plus the door arrow, red when any vertex is. `vertex`
/// turns a map position (cells) into a render position and colour.
pub fn mark_mesh(kind: BuildingKind, facing: u8, centre: Vec2, vertex: impl Fn(Vec2) -> ([f32; 3], [f32; 4])) -> MarkMesh {
    let (lo, hi) = world_rect(kind, facing, centre);
    let mut m = grid_mark(lo, hi, &vertex);
    let first = m.positions.len() as u32;
    let arrow = if m.colours.contains(&RED) { RED } else { WHITE };
    for p in door_arrow(kind) {
        let (p, _) = vertex(centre + turn_local(p, facing));
        m.positions.push(p);
        m.colours.push(arrow);
    }
    m.indices.extend([first, first + 1, first + 2]);
    m
}

/// The map rectangle `lo`-`hi` (cells) as a grid through the terrain's cell lines, each grid quad
/// split like the terrain's cells. `vertex` as in `mark_mesh`.
pub fn grid_mark(lo: Vec2, hi: Vec2, vertex: impl Fn(Vec2) -> ([f32; 3], [f32; 4])) -> MarkMesh {
    let (xs, zs) = (grid_lines(lo.x, hi.x), grid_lines(lo.y, hi.y));
    let mut m = MarkMesh::default();
    for &z in &zs {
        for &x in &xs {
            let (p, c) = vertex(Vec2::new(x, z));
            m.positions.push(p);
            m.colours.push(c);
        }
    }
    let row = xs.len() as u32;
    for j in 0..zs.len() as u32 - 1 {
        for i in 0..row - 1 {
            // Split along (i + 1, j)-(i, j + 1), like the terrain's cells.
            let a = j * row + i;
            m.indices.extend([a, a + row, a + 1, a + 1, a + row, a + row + 1]);
        }
    }
    m
}

#[derive(Default)]
pub struct MarkMesh {
    pub positions: Vec<[f32; 3]>,
    pub colours: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

pub fn set_mark(m: &mut Mesh, mark: MarkMesh) {
    let normals = vec![[0.0, 1.0, 0.0]; mark.positions.len()];
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, mark.positions);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, mark.colours);
    m.insert_indices(Indices::U32(mark.indices));
}

/// The mark's material: vertex colours, see-through, drawn over the ground.
pub fn mark_material() -> StandardMaterial {
    StandardMaterial { alpha_mode: AlphaMode::Blend, unlit: true, cull_mode: None, depth_bias: 20.0, ..default() }
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
        assert!(arrow.iter().all(|p| p.y < min.y), "outside the door side (-z)");
        assert!(arrow[2].y < arrow[0].y, "points away");
    }

    #[test]
    fn a_camp_fire_takes_the_cell_under_the_mouse() {
        assert_eq!(campfire_cell(Vec2::new(10.9, 20.1)), (10, 20));
        assert_eq!(campfire_cell(Vec2::new(127.99, 0.0)), (127, 0));
        let m = grid_mark(Vec2::new(3.0, 4.0), Vec2::new(4.0, 5.0), |at| ([at.x, 0.0, at.y], WHITE));
        assert_eq!(m.positions.len(), 25, "a 4 x 4 grid over the cell");
        assert_eq!(m.indices.len(), 16 * 6);
    }

    #[test]
    fn a_plan_is_placed_only_where_it_can_stand() {
        let map = GameMap::sandbox_buildings();
        let free = Vec2::new(88.0, 64.0);
        let hut = BuildingKind::Hut { size: 1 };
        assert_eq!(place_command(&map, hut, 2, free), Some(Command::PlaceBuilding { player: PLAYER, kind: hut, at: (88 * 512, 64 * 512), facing: 2 }));
        assert_eq!(place_command(&map, hut, 0, Vec2::new(64.0, 64.0)), None, "on the site");
        assert_eq!(place_command(&map, hut, 0, Vec2::new(120.0, 64.0)), None, "sea");
    }

    #[test]
    fn grid_follows_the_cell_lines() {
        let lines = grid_lines(9.3, 10.6);
        assert_eq!(lines, vec![9.3, 9.5, 9.75, 10.0, 10.25, 10.5, 10.6]);
        assert_eq!(grid_lines(2.0, 2.5), vec![2.0, 2.25, 2.5], "ends on lines are not doubled");
    }

    #[test]
    fn turned_footprints_stay_on_the_map_axes() {
        let centre = Vec2::new(10.5, 20.5);
        let (lo, hi) = world_rect(BuildingKind::Temple, 0, centre);
        let (lo2, hi2) = world_rect(BuildingKind::Temple, 2, centre);
        assert!(((hi - lo).x - (hi2 - lo2).y).abs() < 1e-5 && ((hi - lo).y - (hi2 - lo2).x).abs() < 1e-5, "a quarter turn swaps the sides");
        assert!(lo.x < hi.x && lo2.y < hi2.y);
    }

    #[test]
    fn picking_turning_and_putting_away() {
        let mut b = Blueprint::default();
        assert!(!b.is_active());
        b.pick(Plan::Building(BuildingKind::Temple));
        b.turn();
        b.turn();
        b.turn();
        b.turn();
        assert_eq!((b.plan, b.facing), (Some(Plan::Building(BuildingKind::Temple)), 0));
        b.turn();
        b.pick(Plan::Campfire);
        assert_eq!(b.facing, 2, "the turn is kept");
        b.put_away();
        assert!(!b.is_active());
    }
}
