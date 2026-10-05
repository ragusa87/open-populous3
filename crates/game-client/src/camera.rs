//! Orbit camera around a wrapping focus point. Pushing the mouse against the window
//! border or Up-Down move, Left-Right and middle-drag rotate, Enter toggles the aerial view.
//! The cursor is confined to the window (Esc releases/re-confines it).

use crate::edge_push::EdgePush;
use crate::terrain_mesh::{focus_height, CurveParams};
use crate::world::CurrentMap;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};

const MAP: f32 = pop3_format::MAP_SIZE as f32;
/// (pitch, distance) of the default ground view: low and close, like the original.
pub const GROUND_VIEW: (f32, f32) = (0.32, 11.0);
/// Scroll speeds in camera-distances per second (mouse = at full push).
pub const MOUSE_SPEED: f32 = 2.2;
pub const KEY_SPEED: f32 = 3.0;
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
            .init_resource::<CursorConfined>()
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, frame_new_map.run_if(resource_changed::<crate::world::LevelList>))
            .add_systems(Update, (camera_input, apply_rig, sky_color).chain())
            .add_systems(Update, toggle_cursor_confine);
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

/// Whether the cursor should stay inside the window (Esc toggles).
#[derive(Resource)]
pub struct CursorConfined(pub bool);

impl Default for CursorConfined {
    fn default() -> Self {
        CursorConfined(true)
    }
}

/// Esc toggles confinement; regaining focus re-applies it (compositors drop the
/// constraint on focus loss, and a change made before the window exists is lost).
fn toggle_cursor_confine(
    keys: Res<ButtonInput<KeyCode>>,
    mut focus: MessageReader<bevy::window::WindowFocused>,
    mut confined: ResMut<CursorConfined>,
    mut cursors: Query<&mut CursorOptions>,
) {
    let refocused = focus.read().any(|f| f.focused);
    if keys.just_pressed(KeyCode::Escape) {
        confined.0 = !confined.0;
    } else if !refocused {
        return;
    }
    let mode = if confined.0 { CursorGrabMode::Confined } else { CursorGrabMode::None };
    for mut c in &mut cursors {
        c.grab_mode = mode;
        c.set_changed();
    }
}

fn frame_new_map(map: Res<CurrentMap>, mut rig: ResMut<CameraRig>) {
    rig.look_at_cell(map.0.terrain.lowland_cell());
}

fn camera_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    windows: Query<&Window>,
    mut push: Local<EdgePush>,
    time: Res<Time>,
    mut rig: ResMut<CameraRig>,
) {
    let dt = time.delta_secs();
    let axis = |a: KeyCode, b: KeyCode| keys.pressed(a) as i32 as f32 - keys.pressed(b) as i32 as f32;
    rig.yaw += axis(KeyCode::ArrowLeft, KeyCode::ArrowRight) * 1.8 * dt;

    let edge = windows
        .iter()
        .next()
        .map_or(Vec2::ZERO, |w| push.update(w.cursor_position(), w.size(), motion.delta, dt));
    let keys_fwd = axis(KeyCode::ArrowUp, KeyCode::ArrowDown) * KEY_SPEED;
    let forward = (edge.x * MOUSE_SPEED + keys_fwd).clamp(-KEY_SPEED, KEY_SPEED);
    let scale = rig.distance.max(10.0) * dt;
    rig.move_by(forward * scale, edge.y * MOUSE_SPEED * scale);

    if mouse.pressed(MouseButton::Middle) {
        rig.yaw -= motion.delta.x * 0.005;
        rig.pitch = (rig.pitch + motion.delta.y * 0.005).clamp(0.08, 1.5);
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
