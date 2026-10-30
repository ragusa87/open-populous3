//! World editor (dev mode only): Tab toggles edit mode. Brushes follow the mouse
//! (`grounded::pick_ground`), falling back to the camera focus when the cursor is off the terrain
//! or over the panel so a keyboard-only brush still works: R raise, F lower, T flatten, M mark, B
//! land bridge from the last mark. `[`/`]` shrink/grow the brush radius (shown as a ring on the
//! ground and on the info line); O toggles planting a tree under the mouse (left click plants one,
//! right click or Esc cancels). See docs/specs/ui-and-editor.md "World editor".

use crate::blueprint::{ground_under_mouse, mark_material, put_mark, MarkMesh};
use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::grounded::ground_point;
use crate::units::{world_units, GameSchedule, PLAYER};
use crate::world::{CurrentMap, TerrainDirty};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::command::Command;
use game_core::spell::Spell;
use std::f32::consts::TAU;

/// Default brush radius (cells), matching today's `Raise`/`Erode`/`Flatten` values (3/4/4).
const DEFAULT_RADIUS: i32 = 4;
const MIN_RADIUS: i32 = 1;
const MAX_RADIUS: i32 = 12;
/// `Heightmap::raise`'s amount for the raise/lower brushes, as `Spell::cast` used to hardcode them.
const RAISE_AMOUNT: i32 = 64;
const ERODE_AMOUNT: i32 = -120;
/// Height above the ground (render units) the ring is drawn at, and its colour (pale cyan).
const RING_LIFT: f32 = 0.02;
const RING_COLOUR: [f32; 4] = [0.55, 0.92, 1.0, 0.6];
const RING_SEGMENTS: usize = 48;
/// Half width of the ring line, cells.
const RING_HALF: f32 = 0.06;

#[derive(Resource)]
pub struct EditorState {
    pub active: bool,
    pub mark: Option<(i32, i32)>,
    pub brush_radius: i32,
    pub placing_tree: bool,
}

impl Default for EditorState {
    fn default() -> Self {
        EditorState { active: false, mark: None, brush_radius: DEFAULT_RADIUS, placing_tree: false }
    }
}

/// Dev only (`EDITOR_BRUSH=x,z:radius` screenshots): the map position used instead of the mouse.
#[derive(Resource, Default)]
pub struct PinnedAt(pub Option<Vec2>);

/// A brush radius clamped to a sane range (cells), pure so it can be tested.
pub fn clamp_radius(r: i32) -> i32 {
    r.clamp(MIN_RADIUS, MAX_RADIUS)
}

#[derive(Component)]
struct BrushRing;

pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EditorState>()
            .init_resource::<PinnedAt>()
            .add_systems(Startup, spawn_ring)
            .add_systems(Update, editor_input.in_set(crate::menu::Gameplay))
            .add_systems(PostUpdate, draw_brush_ring);
    }
}

/// Map pressed key to a spell at `at`: only `B` (land bridge) still goes through `Spell`; the
/// raise/lower/flatten brushes call `Heightmap` directly with the configurable radius instead
/// (`editor_input`), since `Spell::Flatten`/`Erode`'s own `.cast()` hardcodes it.
pub fn spell_for_key(key: KeyCode, at: (i32, i32), mark: Option<(i32, i32)>) -> Option<Spell> {
    match key {
        KeyCode::KeyB => mark.map(|from| Spell::LandBridge { from, to: at }),
        _ => None,
    }
}

/// The mark's mesh, made the first time it is drawn (Bevy 0.19 logs "Use-after-free" for one without vertices).
fn spawn_ring(mut commands: Commands, mut mats: ResMut<Assets<StandardMaterial>>) {
    commands.spawn((BrushRing, MeshMaterial3d(mats.add(mark_material())), NotShadowCaster, Transform::default(), Visibility::Hidden));
}

#[allow(clippy::too_many_arguments)]
fn editor_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    ui: Query<&Interaction>,
    cams: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    shortcuts: crate::keymap::Shortcuts,
    rig: Res<CameraRig>,
    params: Res<CurveParamsRes>,
    mut state: ResMut<EditorState>,
    mut map: ResMut<CurrentMap>,
    mut dirty: ResMut<TerrainDirty>,
    mut schedule: ResMut<GameSchedule>,
) {
    if !shortcuts.dev() && state.active {
        state.active = false;
        state.placing_tree = false;
    }
    if shortcuts.just_pressed(crate::keymap::Shortcut::Editor) {
        state.active = !state.active;
        state.placing_tree &= state.active;
    }
    if !state.active {
        return;
    }
    let picked = ground_under_mouse(&windows, &ui, &cams, &map, &rig, &params);
    let at_f = picked.unwrap_or(rig.focus);
    let at = (at_f.x.round() as i32, at_f.y.round() as i32);

    if keys.just_pressed(KeyCode::BracketLeft) {
        state.brush_radius = clamp_radius(state.brush_radius - 1);
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        state.brush_radius = clamp_radius(state.brush_radius + 1);
    }
    if keys.just_pressed(KeyCode::KeyO) {
        state.placing_tree = !state.placing_tree;
    }
    if state.placing_tree {
        if mouse.just_pressed(MouseButton::Left) && let Some(p) = picked {
            schedule.issue(Command::PlaceTree { player: PLAYER, at: world_units(p) });
        }
        if mouse.just_pressed(MouseButton::Right) || keys.just_pressed(KeyCode::Escape) {
            state.placing_tree = false;
        }
        return;
    }

    if keys.just_pressed(KeyCode::KeyM) {
        state.mark = Some(at);
    }
    let radius = state.brush_radius;
    if keys.just_pressed(KeyCode::KeyR) {
        map.0.terrain.raise(at, radius, RAISE_AMOUNT);
        dirty.0 = true;
    }
    if keys.just_pressed(KeyCode::KeyF) {
        map.0.terrain.raise(at, radius, ERODE_AMOUNT);
        dirty.0 = true;
    }
    if keys.just_pressed(KeyCode::KeyT) {
        map.0.terrain.flatten(at, radius);
        dirty.0 = true;
    }
    if keys.just_pressed(KeyCode::KeyB)
        && let Some(spell) = spell_for_key(KeyCode::KeyB, at, state.mark)
    {
        dirty.0 |= spell.cast(&mut map.0.terrain).is_some();
    }
}

/// A thin ring on the ground at `radius` (cells) around `centre`: the brush's footprint before it applies.
fn ring_mesh(centre: Vec2, radius: f32, on_ground: impl Fn(Vec2) -> [f32; 3]) -> MarkMesh {
    let mut m = MarkMesh::default();
    for k in 0..=RING_SEGMENTS {
        let dir = Vec2::from_angle(k as f32 / RING_SEGMENTS as f32 * TAU);
        m.positions.push(on_ground(centre + dir * (radius - RING_HALF)));
        m.positions.push(on_ground(centre + dir * (radius + RING_HALF)));
        m.colours.push(RING_COLOUR);
        m.colours.push(RING_COLOUR);
    }
    for k in 0..RING_SEGMENTS as u32 {
        let (a, b) = (k * 2, k * 2 + 1);
        m.indices.extend([a, b, a + 2, a + 2, b, b + 2]);
    }
    m
}

/// Rebuilds the brush ring under the mouse every frame, while active and not placing an object.
#[allow(clippy::too_many_arguments)]
fn draw_brush_ring(
    state: Res<EditorState>,
    windows: Query<&Window>,
    ui: Query<&Interaction>,
    cams: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    rig: Res<CameraRig>,
    params: Res<CurveParamsRes>,
    map: Res<CurrentMap>,
    pinned: Res<PinnedAt>,
    mut ring: Query<(Entity, Option<&Mesh3d>, &mut Visibility), With<BrushRing>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,
) {
    let Ok((entity, mesh, mut vis)) = ring.single_mut() else { return };
    let shown = state.active && !state.placing_tree;
    let centre = shown.then(|| pinned.0.or_else(|| ground_under_mouse(&windows, &ui, &cams, &map, &rig, &params))).flatten();
    let Some(centre) = centre else {
        *vis = Visibility::Hidden;
        return;
    };
    let terrain = &map.0.terrain;
    let on_ground = |at: Vec2| <[f32; 3]>::from(ground_point(terrain, rig.focus, &params.0, at) + Vec3::Y * RING_LIFT);
    let mark = ring_mesh(centre, state.brush_radius as f32, on_ground);
    put_mark(&mut commands, entity, mesh, &mut meshes, mark);
    *vis = Visibility::Inherited;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_needs_a_mark() {
        assert_eq!(spell_for_key(KeyCode::KeyB, (1, 1), None), None);
        assert!(matches!(spell_for_key(KeyCode::KeyB, (1, 1), Some((0, 0))), Some(Spell::LandBridge { .. })));
    }

    #[test]
    fn radius_is_clamped_to_a_sane_range() {
        assert_eq!(clamp_radius(0), MIN_RADIUS);
        assert_eq!(clamp_radius(4), 4);
        assert_eq!(clamp_radius(100), MAX_RADIUS);
    }

    #[test]
    fn a_ring_has_a_vertex_pair_per_segment_and_closes() {
        let flat = |at: Vec2| [at.x, 0.0, at.y];
        let m = ring_mesh(Vec2::ZERO, 4.0, flat);
        assert_eq!(m.positions.len(), (RING_SEGMENTS + 1) * 2);
        assert_eq!(m.indices.len(), RING_SEGMENTS * 6);
        assert!(m.indices.iter().all(|&i| (i as usize) < m.positions.len()));
        let (first, last) = (m.positions[0], m.positions[m.positions.len() - 2]);
        assert!((first[0] - last[0]).abs() < 1e-5 && (first[2] - last[2]).abs() < 1e-5, "closes the loop");
    }
}
