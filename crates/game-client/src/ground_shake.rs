//! Ground Shake: a camera shake for Volcano's own pre-eruption rumble and Earthquake's own tremor
//! (`game_core::effect::GroundShake`, shared between the two the same way `lava` is shared by their aftermaths)
//! — a kick and a roll on the camera itself, not a trembling terrain mesh: screen shake is the standard way games
//! sell "the ground is shaking" without the terrain itself looking like it's glitching underfoot. Peak kick
//! (`CAMERA_KICK`/`CAMERA_ROLL`) and the jitter shape are our own invention, picked by eye against a throwaway
//! prototype: the trace gives no magnitude for either's own "rumbling"/"shake".
//!
//! `camera_shake` runs right after `camera::apply_rig` (`camera::CameraPlugin`'s own chain): `apply_rig` is the
//! only system that writes the camera's `Transform`, and it reads back its own previous frame's `Transform` to
//! lerp 25% toward a freshly computed pose each frame — a shake folded into that system, or into `CameraRig`
//! itself (lerped and read by other systems besides), would partly feed back into next frame's own smoothing
//! instead of being a clean, one-way offset on top of the pose `apply_rig` already settled on.

use crate::camera::{CameraRig, GameCamera};
use crate::sim_time::seed;
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::effect::GroundShake;
use game_core::time::TICKS_PER_SECOND;
use pop3_format::WORLD_UNITS_PER_CELL;
use std::f32::consts::TAU;

/// World units to the cell they fall in, unwrapped (`game_core::unit::cell_of`, private to that crate).
fn cell_of(v: u16) -> i32 {
    (v as u32 / WORLD_UNITS_PER_CELL) as i32
}

/// How far the camera kicks (render units) and rolls (radians) at full combined strength, right at an
/// epicentre: picked by eye against a POC, not an original-game number — small enough to read as a tremor, not a
/// disorienting lurch.
const CAMERA_KICK: f32 = 0.08;
const CAMERA_ROLL: f32 = 0.015;
/// How fast the camera trembles (Hz): same reasoning.
const FREQ: f32 = 2.2;

fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}
fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = clamp01((x - a) / (b - a));
    t * t * (3.0 - 2.0 * t)
}

/// How much `at` (cell coordinates, continuous: the camera's own focus, not a mesh corner) is still trembling:
/// `shake.intensity` at `centre`, fading to 0 by `shake.radius_cells` — you feel the shake if you're looking near
/// the quake, not if you're looking clear across the map.
fn strength_at(shake: GroundShake, centre: (i32, i32), at: Vec2) -> f32 {
    let d = ((at.x - centre.0 as f32).powi(2) + (at.y - centre.1 as f32).powi(2)).sqrt();
    shake.intensity * (1.0 - smoothstep(shake.radius_cells * 0.45, shake.radius_cells, d))
}

/// One shake's own trembling, in `[-1, 1]`: two summed sines out of phase per `key` (`sim_time::seed`, the same
/// stable per-id phase every other view uses) so two different keys (an effect's own two axes, or two
/// simultaneous shakes) don't move in lockstep.
fn jitter(key: (i32, i32), elapsed_secs: f32) -> f32 {
    let k = (key.0 as u32).wrapping_mul(0x27D4_EB2F) ^ (key.1 as u32).wrapping_mul(0x9E37_79B9);
    let phase = seed(k) * TAU;
    0.6 * (elapsed_secs * FREQ * TAU + phase).sin() + 0.4 * (elapsed_secs * FREQ * TAU * 2.13 + phase * 1.7 + 1.3).sin()
}

/// The camera's own combined kick (render units, in its own right/up plane) and roll (radians, about its own
/// forward axis) at `at` (the camera's live focus): summed over every `active` shake (its own id, `GroundShake`,
/// centre cell and elapsed seconds) — two simultaneous quakes shake harder together, not just two independent,
/// overlapping trembles. Each shake's own two axes (and so two simultaneous shakes themselves) move out of phase
/// (`jitter`'s own per-key phase, keyed by the effect's id and an axis index: no new jitter shape needed).
pub(crate) fn shake_offset(active: &[(u32, GroundShake, (i32, i32), f32)], at: Vec2) -> (Vec2, f32) {
    let mut kick = Vec2::ZERO;
    let mut roll = 0.0;
    for &(id, shake, centre, elapsed) in active {
        let strength = strength_at(shake, centre, at);
        if strength <= 0.0 {
            continue;
        }
        let (jx, jy) = (jitter((id as i32, 0), elapsed), jitter((id as i32, 1), elapsed));
        kick += Vec2::new(jx, jy) * strength;
        roll += jx * strength;
    }
    (kick * CAMERA_KICK, roll * CAMERA_ROLL)
}

/// Every currently active `GroundShake` effect: its id, shake, centre cell and seconds elapsed since it started.
fn active_shakes(map: &game_core::map::GameMap) -> Vec<(u32, GroundShake, (i32, i32), f32)> {
    map.effects
        .entries()
        .filter_map(|(id, e)| {
            let shake = e.ground_shake()?;
            let elapsed = e.left.done().get() as f32 / TICKS_PER_SECOND as f32;
            Some((id.raw(), shake, (cell_of(e.x), cell_of(e.z)), elapsed))
        })
        .collect()
}

/// Shakes the camera itself: summed over every currently active `GroundShake` effect, on top of whatever
/// `camera::apply_rig` just settled the `Transform` on (see the module doc for why it must run strictly after).
pub(crate) fn camera_shake(map: Res<CurrentMap>, rig: Res<CameraRig>, mut cam: Query<&mut Transform, With<GameCamera>>) {
    let active = active_shakes(&map.0);
    if active.is_empty() {
        return;
    }
    let (kick, roll) = shake_offset(&active, rig.focus);
    for mut t in &mut cam {
        let (right, up) = (t.rotation * Vec3::X, t.rotation * Vec3::Y);
        t.translation += right * kick.x + up * kick.y;
        t.rotation *= Quat::from_rotation_z(roll);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHAKE: GroundShake = GroundShake { radius_cells: 8.0, intensity: 1.0 };

    #[test]
    fn strength_is_full_at_the_centre_and_gone_at_the_radius() {
        assert_eq!(strength_at(SHAKE, (0, 0), Vec2::ZERO), 1.0);
        assert_eq!(strength_at(SHAKE, (0, 0), Vec2::new(8.0, 0.0)), 0.0);
        let mid = strength_at(SHAKE, (0, 0), Vec2::new(4.0, 0.0));
        assert!(mid > 0.0 && mid < 1.0, "fades in between: {mid}");
    }

    #[test]
    fn strength_scales_with_intensity() {
        let half = GroundShake { radius_cells: 8.0, intensity: 0.5 };
        assert_eq!(strength_at(half, (0, 0), Vec2::ZERO), 0.5);
    }

    #[test]
    fn jitter_stays_in_range_and_differs_between_keys() {
        for t in [0.0, 0.3, 1.7, 6.4] {
            let j = jitter((5, 0), t);
            assert!((-1.0..=1.0).contains(&j), "out of range: {j}");
        }
        assert_ne!(jitter((5, 0), 1.0), jitter((5, 1), 1.0), "an effect's two axes don't move in lockstep");
        assert_ne!(jitter((5, 0), 1.0), jitter((6, 0), 1.0), "two different shakes don't move in lockstep");
    }

    #[test]
    fn no_active_shakes_means_no_offset() {
        assert_eq!(shake_offset(&[], Vec2::ZERO), (Vec2::ZERO, 0.0));
    }

    #[test]
    fn a_shake_far_outside_its_radius_gives_no_offset() {
        let far = [(1, SHAKE, (0, 0), 1.0)];
        assert_eq!(shake_offset(&far, Vec2::new(100.0, 0.0)), (Vec2::ZERO, 0.0));
    }

    #[test]
    fn two_shakes_at_the_same_spot_move_the_camera_more_than_one_alone() {
        let one = [(1, SHAKE, (0, 0), 1.0)];
        let two = [(1, SHAKE, (0, 0), 1.0), (2, SHAKE, (0, 0), 1.0)];
        let (kick_one, _) = shake_offset(&one, Vec2::ZERO);
        let (kick_two, _) = shake_offset(&two, Vec2::ZERO);
        assert!(kick_two.length() > kick_one.length(), "two quakes together shake harder: {kick_one} vs {kick_two}");
    }

    #[test]
    fn shake_offset_is_deterministic() {
        let active = [(1, SHAKE, (0, 0), 0.7)];
        assert_eq!(shake_offset(&active, Vec2::new(1.0, 2.0)), shake_offset(&active, Vec2::new(1.0, 2.0)));
    }
}
