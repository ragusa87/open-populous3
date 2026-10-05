//! Orbit camera around a wrapping focus point. Arrows move, right-drag rotates,
//! wheel zooms, Enter toggles the aerial (planet) view.

use crate::terrain_mesh::{focus_height, CurveParams};
use crate::world::CurrentMap;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

const MAP: f32 = pop3_format::MAP_SIZE as f32;
/// (pitch, distance) of the default ground view: low and close, like the original.
pub const GROUND_VIEW: (f32, f32) = (0.32, 11.0);
const SKY: Color = Color::srgb(0.45, 0.65, 0.92);
const SPACE: Color = Color::srgb(0.02, 0.02, 0.06);

#[derive(Resource, Clone, Debug)]
pub struct CameraRig {
    /// Focus in cell coordinates, always kept in `[0, MAP)`.
    pub focus: Vec2,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub aerial: bool,
    /// Ground-level settings restored when leaving the aerial view.
    saved: (f32, f32),
}

impl Default for CameraRig {
    fn default() -> Self {
        CameraRig {
            focus: Vec2::splat(MAP / 2.0),
            yaw: 0.0,
            pitch: GROUND_VIEW.0,
            distance: GROUND_VIEW.1,
            aerial: false,
            saved: GROUND_VIEW,
        }
    }
}

impl CameraRig {
    /// Ground-plane forward (where "up arrow" goes) for the current yaw.
    pub fn forward(&self) -> Vec2 {
        Vec2::new(-self.yaw.sin(), -self.yaw.cos())
    }

    pub fn move_by(&mut self, forward: f32, right: f32) {
        let f = self.forward();
        let r = Vec2::new(-f.y, f.x);
        self.focus = (self.focus + f * forward + r * right).rem_euclid(Vec2::splat(MAP));
    }

    pub fn toggle_aerial(&mut self) {
        self.aerial = !self.aerial;
        if self.aerial {
            self.saved = (self.pitch, self.distance);
            (self.pitch, self.distance) = (1.35, 115.0);
        } else {
            (self.pitch, self.distance) = self.saved;
        }
    }

    pub fn look_at_cell(&mut self, cell: (i32, i32)) {
        self.focus = Vec2::new(cell.0 as f32, cell.1 as f32).rem_euclid(Vec2::splat(MAP));
    }

    /// Eye offset from the focus point in render space.
    pub fn eye_offset(&self) -> Vec3 {
        let horiz = self.pitch.cos() * self.distance;
        Vec3::new(self.yaw.sin() * horiz, self.pitch.sin() * self.distance, self.yaw.cos() * horiz)
    }
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraRig>()
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, frame_new_map.run_if(resource_changed::<crate::world::LevelList>))
            .add_systems(Update, (camera_input, apply_rig, sky_color).chain());
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((Camera3d::default(), Transform::default()));
    commands.insert_resource(ClearColor(SKY));
    commands.insert_resource(GlobalAmbientLight { brightness: 60.0, ..default() });
    commands.spawn((
        DirectionalLight { illuminance: 9000.0, ..default() },
        Transform::from_xyz(40.0, 22.0, 15.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn frame_new_map(map: Res<CurrentMap>, mut rig: ResMut<CameraRig>) {
    rig.look_at_cell(map.0.terrain.lowland_cell());
}

/// Screen-edge scrolling like the original: returns (forward, right) in -1..1
/// when the cursor is within `margin` pixels of a window border.
pub fn edge_scroll(cursor: Vec2, size: Vec2, margin: f32) -> Vec2 {
    let right = (cursor.x >= size.x - margin) as i32 - (cursor.x <= margin) as i32;
    let forward = (cursor.y <= margin) as i32 - (cursor.y >= size.y - margin) as i32;
    Vec2::new(forward as f32, right as f32)
}

fn camera_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    windows: Query<&Window>,
    time: Res<Time>,
    mut rig: ResMut<CameraRig>,
) {
    let dt = time.delta_secs();
    let axis = |a: KeyCode, b: KeyCode| keys.pressed(a) as i32 as f32 - keys.pressed(b) as i32 as f32;
    rig.yaw += axis(KeyCode::ArrowLeft, KeyCode::ArrowRight) * 1.8 * dt;
    rig.pitch = (rig.pitch + axis(KeyCode::ArrowUp, KeyCode::ArrowDown) * 0.8 * dt).clamp(0.08, 1.5);

    let edge = windows
        .iter()
        .find_map(|w| w.cursor_position().map(|c| edge_scroll(c, w.size(), 12.0)))
        .unwrap_or(Vec2::ZERO);
    let speed = rig.distance.max(10.0) * 0.8 * dt;
    rig.move_by(edge.x * speed, edge.y * speed);

    if mouse.pressed(MouseButton::Middle) {
        rig.yaw -= motion.delta.x * 0.005;
        rig.pitch = (rig.pitch + motion.delta.y * 0.005).clamp(0.08, 1.5);
    }
    if scroll.delta.y != 0.0 {
        rig.distance = (rig.distance * (1.0 - scroll.delta.y * 0.1)).clamp(3.0, 220.0);
    }
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
        rig.toggle_aerial();
    }
}

fn apply_rig(
    rig: Res<CameraRig>,
    map: Res<CurrentMap>,
    params: Res<CurveParamsRes>,
    mut cam: Query<&mut Transform, With<Camera3d>>,
) {
    let target = Vec3::Y * focus_height(&map.0.terrain, (rig.focus.x, rig.focus.y), &params.0);
    for mut t in &mut cam {
        let wanted = Transform::from_translation(target + rig.eye_offset()).looking_at(target, Vec3::Y);
        *t = Transform {
            translation: t.translation.lerp(wanted.translation, 0.25),
            rotation: t.rotation.slerp(wanted.rotation, 0.25),
            ..wanted
        };
    }
}

/// Blue sky near the ground, black space when zoomed out to the planet.
pub fn sky_for_distance(distance: f32) -> Color {
    let t = ((distance - 30.0) / 60.0).clamp(0.0, 1.0);
    SKY.mix(&SPACE, t)
}

fn sky_color(rig: Res<CameraRig>, mut clear: ResMut<ClearColor>) {
    clear.0 = sky_for_distance(rig.distance);
}

#[derive(Resource, Default)]
pub struct CurveParamsRes(pub CurveParams);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_wraps_around_the_torus() {
        let mut rig = CameraRig { focus: Vec2::new(1.0, 127.5), ..default() };
        rig.move_by(2.0, 0.0);
        assert!((rig.focus.y - 125.5).abs() < 1e-4);
        rig.move_by(-5.0, 0.0);
        assert!((rig.focus.y - 2.5).abs() < 1e-4);
    }

    #[test]
    fn edge_scroll_directions() {
        let size = Vec2::new(800.0, 600.0);
        assert_eq!(edge_scroll(Vec2::new(400.0, 300.0), size, 10.0), Vec2::ZERO);
        assert_eq!(edge_scroll(Vec2::new(400.0, 2.0), size, 10.0), Vec2::new(1.0, 0.0));
        assert_eq!(edge_scroll(Vec2::new(799.0, 599.0), size, 10.0), Vec2::new(-1.0, 1.0));
    }

    #[test]
    fn sky_fades_to_space() {
        assert_eq!(sky_for_distance(GROUND_VIEW.1), SKY);
        assert_eq!(sky_for_distance(200.0), SPACE);
    }

    #[test]
    fn aerial_toggle_restores_ground_view() {
        let mut rig = CameraRig { distance: 42.0, ..default() };
        rig.toggle_aerial();
        assert!(rig.aerial && rig.distance > 100.0);
        rig.toggle_aerial();
        assert_eq!(rig.distance, 42.0);
    }
}
