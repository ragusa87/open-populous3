//! Where a sound on the map is heard from (sound.md "Mixing"): a listener 16 cells from the camera's look-at
//! point along its heading, 2D distances on the wrapping map, nothing past 48 cells, the volume falling as
//! `1 - d²/48²`, and an equal-power balance from the sound's bearing, front and back folded together. Pure, in
//! cells; `audio.rs` applies it.

use crate::camera::torus_delta;
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

/// Cells from the look-at point to the listener, along the camera's heading (sound.md "Mixing").
pub const LISTENER_AHEAD: f32 = 16.0;
/// Past this, a sound is not started, and a playing one is stopped (sound.md "Mixing").
pub const CUTOFF: f32 = 48.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    /// The camera's look-at point, cells.
    pub focus: Vec2,
    /// The camera's heading on the ground (any length).
    pub forward: Vec2,
    /// The screen's right on the ground (any length).
    pub right: Vec2,
}

impl Default for Listener {
    fn default() -> Self {
        Listener { focus: Vec2::ZERO, forward: Vec2::Y, right: Vec2::X }
    }
}

impl Listener {
    pub fn ear(&self) -> Vec2 {
        self.focus + self.forward.normalize_or(Vec2::Y) * LISTENER_AHEAD
    }
}

/// How a sound is heard.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub gain: f32,
    /// -1 (left) to 1 (right): the bearing over 90°.
    pub pan: f32,
    /// Cells from the listener, the short way round the map.
    pub distance: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutOfReach {
    pub distance: f32,
}

/// `1 - d²/CUTOFF²`, 0 from the cut-off on.
pub fn falloff(distance: f32) -> f32 {
    (1.0 - (distance / CUTOFF).powi(2)).max(0.0)
}

/// The bearing of `delta` from the heading, folded into -90°..90° (behind counts as in front), over 90°.
pub fn pan(delta: Vec2, forward: Vec2, right: Vec2) -> f32 {
    let side = delta.dot(right.normalize_or(Vec2::X));
    let ahead = delta.dot(forward.normalize_or(Vec2::Y)).abs();
    if side == 0.0 && ahead == 0.0 {
        return 0.0;
    }
    side.atan2(ahead) / FRAC_PI_2
}

/// Left and right gains for a balance: equal power, both `cos 45°` in the middle.
pub fn equal_power(pan: f32) -> (f32, f32) {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * FRAC_PI_4;
    (angle.cos().max(0.0), angle.sin().max(0.0))
}

/// How a sound at `at` (cells) is heard, or how far out of reach it is.
pub fn place(listener: &Listener, at: Vec2) -> Result<Placement, OutOfReach> {
    let delta = torus_delta(listener.ear(), at);
    let distance = delta.length();
    if distance >= CUTOFF {
        return Err(OutOfReach { distance });
    }
    Ok(Placement { gain: falloff(distance), pan: pan(delta, listener.forward, listener.right), distance })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP: f32 = pop3_format::MAP_SIZE as f32;

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn the_listener_stands_ahead_of_the_look_at_point() {
        let l = Listener { focus: Vec2::new(10.0, 10.0), forward: Vec2::new(0.0, 2.0), right: Vec2::X };
        assert_eq!(l.ear(), Vec2::new(10.0, 10.0 + LISTENER_AHEAD));
        assert_eq!(place(&l, l.ear()).unwrap().gain, 1.0);
    }

    #[test]
    fn distance_wraps_around_the_map() {
        let l = Listener { focus: Vec2::new(MAP - 1.0, 10.0 - LISTENER_AHEAD), ..Listener::default() };
        let p = place(&l, Vec2::new(1.0, 10.0)).unwrap();
        assert!(near(p.distance, 2.0), "{}", p.distance);
        assert!(near(p.pan, 1.0), "across the seam it is to the right: {}", p.pan);
    }

    #[test]
    fn falls_off_as_the_square_and_stops_at_the_cutoff() {
        assert_eq!(falloff(0.0), 1.0);
        assert!(near(falloff(CUTOFF / 2.0), 0.75));
        assert_eq!(falloff(CUTOFF), 0.0);
        let l = Listener::default();
        assert_eq!(place(&l, l.ear() + Vec2::new(CUTOFF, 0.0)), Err(OutOfReach { distance: CUTOFF }));
        assert!(place(&l, l.ear() + Vec2::new(CUTOFF - 0.5, 0.0)).is_ok());
    }

    #[test]
    fn balance_is_the_bearing_with_front_and_back_folded() {
        let (f, r) = (Vec2::Y, Vec2::X);
        assert_eq!(pan(Vec2::new(5.0, 0.0), f, r), 1.0);
        assert_eq!(pan(Vec2::new(-5.0, 0.0), f, r), -1.0);
        assert_eq!(pan(Vec2::new(0.0, 5.0), f, r), 0.0);
        assert!(near(pan(Vec2::new(5.0, 5.0), f, r), 0.5), "45° right");
        assert_eq!(pan(Vec2::new(5.0, -5.0), f, r), pan(Vec2::new(5.0, 5.0), f, r), "behind folds onto in front");
        assert_eq!(pan(Vec2::new(5.0, 0.0), -f, -r), -1.0, "camera turned round");
        assert_eq!(pan(Vec2::ZERO, f, r), 0.0);
    }

    #[test]
    fn equal_power_keeps_the_loudness_across_the_balance() {
        for i in -10..=10 {
            let (l, r) = equal_power(i as f32 / 10.0);
            assert!(near(l * l + r * r, 1.0));
        }
        assert!(near(equal_power(-1.0).0, 1.0) && near(equal_power(-1.0).1, 0.0));
        assert!(near(equal_power(1.0).1, 1.0) && near(equal_power(1.0).0, 0.0));
        let (l, r) = equal_power(0.0);
        assert!(near(l, r));
    }
}
