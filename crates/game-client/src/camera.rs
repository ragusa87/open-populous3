//! Orbit camera around a wrapping focus point. Arrows move, right-drag rotates,
//! wheel zooms, Enter toggles the aerial (planet) view.

use crate::terrain_mesh::{focus_height, CurveParams};
use crate::world::CurrentMap;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

const MAP: f32 = pop3_format::MAP_SIZE as f32;

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
            pitch: 0.7,
            distance: 30.0,
            aerial: false,
            saved: (0.7, 30.0),
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
            .add_systems(Update, (camera_input, apply_rig).chain());
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((Camera3d::default(), Transform::default()));
    commands.insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.06)));
    commands.insert_resource(GlobalAmbientLight { brightness: 250.0, ..default() });
    commands.spawn((
        DirectionalLight { illuminance: 6000.0, ..default() },
        Transform::from_xyz(30.0, 60.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn frame_new_map(map: Res<CurrentMap>, mut rig: ResMut<CameraRig>) {
    rig.look_at_cell(map.0.terrain.highest_cell());
}

fn camera_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    mut rig: ResMut<CameraRig>,
) {
    let speed = rig.distance.max(10.0) * 0.8 * time.delta_secs();
    let axis = |a: KeyCode, b: KeyCode| keys.pressed(a) as i32 as f32 - keys.pressed(b) as i32 as f32;
    let fwd = axis(KeyCode::ArrowUp, KeyCode::ArrowDown);
    let right = axis(KeyCode::ArrowRight, KeyCode::ArrowLeft);
    rig.move_by(fwd * speed, right * speed);
    rig.yaw += axis(KeyCode::KeyQ, KeyCode::KeyE) * 1.5 * time.delta_secs();

    if mouse.pressed(MouseButton::Right) {
        rig.yaw -= motion.delta.x * 0.005;
        rig.pitch = (rig.pitch + motion.delta.y * 0.005).clamp(0.15, 1.5);
    }
    if mouse.pressed(MouseButton::Middle) || mouse.pressed(MouseButton::Left) {
        let k = rig.distance * 0.002;
        rig.move_by(motion.delta.y * k, -motion.delta.x * k);
    }
    if scroll.delta.y != 0.0 {
        rig.distance = (rig.distance * (1.0 - scroll.delta.y * 0.1)).clamp(6.0, 220.0);
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
    fn aerial_toggle_restores_ground_view() {
        let mut rig = CameraRig { distance: 42.0, ..default() };
        rig.toggle_aerial();
        assert!(rig.aerial && rig.distance > 100.0);
        rig.toggle_aerial();
        assert_eq!(rig.distance, 42.0);
    }
}
