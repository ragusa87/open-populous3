//! Camera readout in the bottom-right corner (F3 hides it): focus, angle, tilt, distance, fov, eye, and the
//! `just shot` variables that reproduce the view.

use crate::camera::{CameraRig, GameCamera};
use crate::game_frame::GamePos;
use bevy::prelude::*;
use std::f32::consts::TAU;

pub struct CameraDebugPlugin;

impl Plugin for CameraDebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_readout).add_systems(Update, (toggle_readout, update_readout).chain());
    }
}

#[derive(Component)]
struct CameraReadout;

/// The game's angle (0..2048, as in the level header and the things) a camera yaw looks along.
pub fn game_angle(yaw: f32) -> u16 {
    ((-yaw).rem_euclid(TAU) * 2048.0 / TAU).round() as u16 % 2048
}

/// The readout for `rig`, its eye being at render point `eye` (relative to the focus: the world is drawn
/// around it).
pub fn readout(rig: &CameraRig, eye: Vec3) -> String {
    let eye = GamePos::from_render(eye).0 + Vec3::new(rig.focus.x, 0.0, rig.focus.y);
    let yaw = rig.yaw.to_degrees();
    let pitch = rig.pitch.to_degrees();
    format!(
        "camera{}  focus {:.1}, {:.1}  angle {}  yaw {yaw:.1}deg  tilt {pitch:.1}deg  distance {:.1}  fov {:.0}deg\n\
         eye {:.1}, {:.1}, {:.1}\n\
         FOCUS={:.1},{:.1} DISTANCE={:.1} PITCH={pitch:.1} YAW={yaw:.1}",
        if rig.aerial { " [aerial]" } else { "" },
        rig.focus.x,
        rig.focus.y,
        game_angle(rig.yaw),
        rig.distance,
        rig.fov.to_degrees(),
        eye.x.rem_euclid(pop3_format::MAP_SIZE as f32),
        eye.y,
        eye.z.rem_euclid(pop3_format::MAP_SIZE as f32),
        rig.focus.x,
        rig.focus.y,
        rig.distance,
    )
}

fn spawn_readout(mut commands: Commands) {
    commands.spawn((
        CameraReadout,
        Text::new(""),
        TextFont { font_size: FontSize::Px(13.0), ..default() },
        TextLayout::justify(Justify::Right),
        Node { position_type: PositionType::Absolute, bottom: px(8), right: px(10), ..default() },
    ));
}

fn toggle_readout(keys: Res<ButtonInput<KeyCode>>, mut q: Query<&mut Visibility, With<CameraReadout>>) {
    if keys.just_pressed(KeyCode::F3) {
        for mut v in &mut q {
            v.toggle_visible_hidden();
        }
    }
}

fn update_readout(
    rig: Res<CameraRig>,
    cam: Query<&Transform, With<GameCamera>>,
    mut q: Query<&mut Text, With<CameraReadout>>,
) {
    let Ok(eye) = cam.single() else { return };
    let s = readout(&rig, eye.translation);
    for mut t in &mut q {
        if t.0 != s {
            t.0 = s.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_angle_reads_back_the_level_angle() {
        for angle in [0u16, 1, 512, 1024, 1536, 2047] {
            assert_eq!(game_angle(crate::nature::angle_yaw(angle).render()), angle);
        }
        assert_eq!(game_angle(TAU), 0);
    }

    #[test]
    fn readout_shows_the_view_and_its_shot_variables() {
        let mut rig = CameraRig::default();
        (rig.focus, rig.yaw) = (Vec2::new(64.0, 70.5), 0.0);
        let text = readout(&rig, Vec3::new(0.0, 3.0, 14.0));
        assert!(text.contains("focus 64.0, 70.5"), "{text}");
        assert!(text.contains("angle 0"), "{text}");
        assert!(text.contains("eye 64.0, 3.0, 56.5"), "behind the focus, game frame z: {text}");
        assert!(text.contains("FOCUS=64.0,70.5 DISTANCE="), "{text}");
    }
}
