//! Camera readout in the bottom-right corner, in dev mode (F3 hides it): focus, angle, tilt, distance, fov, eye,
//! and the `just shot` variables that reproduce the view. The same readout goes to stdout on one line each time
//! the camera settles after a change (wheel, keys, drag, scrolling), to note views while tuning.

use crate::camera::{CameraRig, GameCamera};
use crate::game_frame::GamePos;
use crate::keymap::{DevMode, Shortcut, Shortcuts};
use bevy::prelude::*;
use std::f32::consts::TAU;

pub struct CameraDebugPlugin;

impl Plugin for CameraDebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_readout).add_systems(Update, (toggle_readout, update_readout, log_settled).chain());
    }
}

#[derive(Component)]
struct CameraReadout;

/// Seconds the camera stays still before its readout is printed.
const SETTLE_SECS: f32 = 0.4;

/// Prints a readout once it has stayed the same for `SETTLE_SECS`, and only once.
#[derive(Default)]
pub struct SettledLog {
    printed: Option<String>,
    pending: Option<(String, f32)>,
}

impl SettledLog {
    /// The line to print at `now` (seconds) for the current readout, if it just settled.
    pub fn update(&mut self, current: &str, now: f32) -> Option<String> {
        match &self.pending {
            Some((text, _)) if text == current => {}
            _ => {
                self.pending = Some((current.to_string(), now));
                return None;
            }
        }
        let (text, since) = self.pending.as_ref()?;
        if now - since < SETTLE_SECS || self.printed.as_deref() == Some(text) {
            return None;
        }
        self.printed = Some(text.clone());
        Some(text.replace('\n', "  |  "))
    }
}

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

fn toggle_readout(keys: Shortcuts, mut hidden: Local<bool>, mut q: Query<&mut Visibility, With<CameraReadout>>) {
    if keys.just_pressed(Shortcut::CameraReadout) {
        *hidden = !*hidden;
    }
    let shown = if keys.dev() && !*hidden { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut q {
        v.set_if_neq(shown);
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

fn log_settled(time: Res<Time>, dev: Res<DevMode>, rig: Res<CameraRig>, cam: Query<&Transform, With<GameCamera>>, mut log: Local<SettledLog>) {
    let Ok(eye) = cam.single() else { return };
    if !dev.0 {
        return;
    }
    if let Some(line) = log.update(&readout(&rig, eye.translation), time.elapsed_secs()) {
        println!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_view_is_printed_once_settled_and_once_only() {
        let mut log = SettledLog::default();
        assert_eq!(log.update("a\nb", 0.0), None, "just changed");
        assert_eq!(log.update("a\nb", 0.2), None, "not settled yet");
        assert_eq!(log.update("a\nb", 0.5).as_deref(), Some("a  |  b"), "settled: one line");
        assert_eq!(log.update("a\nb", 2.0), None, "already printed");
        assert_eq!(log.update("c", 2.1), None, "moving again");
        assert_eq!(log.update("d", 2.3), None);
        assert_eq!(log.update("d", 2.8).as_deref(), Some("d"), "only where it stopped");
    }

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
