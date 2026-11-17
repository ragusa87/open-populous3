//! Where a sound on the map is heard from: its loudness and left/right balance for the camera, on the
//! wrapping map. Our own rules (the original's are not traced, sound.md "Playback in the client"): the
//! listener is the camera focus on the ground, full volume near it, fading out to silence further away, the
//! reach widening as the camera rises. Pure, in cells; `audio.rs` applies it.

use crate::camera::{torus_delta, GROUND_VIEW};
use bevy::prelude::*;

/// Distances in cells, at the default ground view: full volume up to `FULL`, silent from `SILENT`. A guess.
pub const FULL: f32 = 6.0;
pub const SILENT: f32 = 28.0;
/// How far the reach scales with the camera's height over the default view's.
pub const REACH_SCALE: (f32, f32) = (0.5, 3.0);
/// Below this distance (cells) the balance eases back to the middle, so a sound at the focus does not jump
/// from side to side.
pub const PAN_NEAR: f32 = 2.0;

/// Where the sounds are heard from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    /// The camera focus, cells.
    pub focus: Vec2,
    /// The screen's right on the ground, cells (any length).
    pub right: Vec2,
    /// The eye's height over the focus, cells (`CameraRig::altitude`).
    pub altitude: f32,
}

impl Default for Listener {
    fn default() -> Self {
        Listener { focus: Vec2::ZERO, right: Vec2::X, altitude: ground_altitude() }
    }
}

fn ground_altitude() -> f32 {
    GROUND_VIEW.0.sin() * GROUND_VIEW.1
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Falloff {
    pub full: f32,
    pub silent: f32,
}

impl Falloff {
    /// 1 inside `full`, 0 from `silent`, a smoothstep between.
    pub fn gain(&self, distance: f32) -> f32 {
        let t = ((distance - self.full) / (self.silent - self.full).max(f32::EPSILON)).clamp(0.0, 1.0);
        1.0 - t * t * (3.0 - 2.0 * t)
    }
}

/// The falloff for the camera's current height: zoomed out, sounds carry further.
pub fn reach(listener: &Listener) -> Falloff {
    let k = (listener.altitude / ground_altitude()).clamp(REACH_SCALE.0, REACH_SCALE.1);
    Falloff { full: FULL * k, silent: SILENT * k }
}

/// Balance, -1 (left) to 1 (right), of a sound at `delta` from the listener.
pub fn pan(delta: Vec2, right: Vec2) -> f32 {
    let side = delta.dot(right.normalize_or(Vec2::X));
    (side / delta.length().max(PAN_NEAR)).clamp(-1.0, 1.0)
}

/// How a sound is heard.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub gain: f32,
    pub pan: f32,
    /// Cells, the short way round the map.
    pub distance: f32,
}

/// How a sound at `at` (cells) is heard, or how far out of reach it is.
pub fn place(listener: &Listener, at: Vec2) -> Result<Placement, OutOfReach> {
    let delta = torus_delta(listener.focus, at);
    let distance = delta.length();
    let falloff = reach(listener);
    if distance >= falloff.silent {
        return Err(OutOfReach { distance, silent: falloff.silent });
    }
    Ok(Placement { gain: falloff.gain(distance), pan: pan(delta, listener.right), distance })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutOfReach {
    pub distance: f32,
    pub silent: f32,
}

/// Gap between the ears of the `SpatialListener` that `audio.rs` puts at the origin.
pub const EAR_GAP: f32 = 0.2;
/// How far from that listener a sound is put to give it its balance.
pub const EMITTER_RADIUS: f32 = 0.5;

/// Where to put a sound around the origin listener for its balance alone. Bevy's spatial audio (rodio
/// `Spatial`) scales each ear by `min(1, 1/d²)`: every point within `EMITTER_RADIUS` keeps both ears under
/// 1 unit away, so that factor stays 1 and our own `Placement::gain` is the only fading with distance; rodio's
/// balance keeps 0.75 per side in the middle, 1 and 0.5 fully to one side. rodio 0.22 gives the louder side to
/// the ear further from the sound, so a sound heard on the right sits on the left (to check by ear).
pub fn virtual_emitter(pan: f32) -> Vec3 {
    let pan = pan.clamp(-1.0, 1.0);
    Vec3::new(-pan * EMITTER_RADIUS, 0.0, -(1.0 - pan * pan).sqrt() * EMITTER_RADIUS)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP: f32 = pop3_format::MAP_SIZE as f32;

    fn at_ground(focus: Vec2) -> Listener {
        Listener { focus, ..Listener::default() }
    }

    #[test]
    fn distance_wraps_around_the_map() {
        let l = at_ground(Vec2::new(MAP - 1.0, 10.0));
        let p = place(&l, Vec2::new(1.0, 10.0)).unwrap();
        assert!((p.distance - 2.0).abs() < 1e-4, "{}", p.distance);
        assert!(p.pan > 0.0, "across the seam it is to the right, not 126 cells left");
    }

    #[test]
    fn loud_near_silent_far_fading_between() {
        let l = at_ground(Vec2::splat(50.0));
        assert_eq!(place(&l, Vec2::new(50.0 + FULL, 50.0)).unwrap().gain, 1.0);
        let gone = place(&l, Vec2::new(50.0, 50.0 + SILENT)).unwrap_err();
        assert_eq!(gone, OutOfReach { distance: SILENT, silent: SILENT });
        let gains: Vec<f32> = (0..20).map(|i| place(&l, Vec2::new(50.0 + FULL + i as f32, 50.0)).unwrap().gain).collect();
        assert!(gains.windows(2).all(|w| w[1] < w[0]), "{gains:?}");
    }

    #[test]
    fn a_higher_camera_hears_further() {
        let low = Listener { altitude: 0.1, ..Listener::default() };
        let high = Listener { altitude: 1000.0, ..Listener::default() };
        assert_eq!(reach(&Listener::default()), Falloff { full: FULL, silent: SILENT });
        assert_eq!(reach(&low).silent, SILENT * REACH_SCALE.0);
        assert_eq!(reach(&high).silent, SILENT * REACH_SCALE.1);
        assert!(place(&high, Vec2::new(SILENT + 5.0, 0.0)).is_ok());
    }

    #[test]
    fn balance_follows_the_screen_right() {
        assert_eq!(pan(Vec2::new(10.0, 0.0), Vec2::X), 1.0);
        assert_eq!(pan(Vec2::new(-10.0, 0.0), Vec2::X), -1.0);
        assert_eq!(pan(Vec2::new(10.0, 0.0), -Vec2::X), -1.0, "camera turned round");
        assert_eq!(pan(Vec2::new(0.0, 10.0), Vec2::X), 0.0, "straight ahead");
        assert_eq!(pan(Vec2::ZERO, Vec2::X), 0.0);
        assert_eq!(pan(Vec2::new(1.0, 0.0), Vec2::X), 0.5, "eased near the focus");
    }

    /// rodio 0.22 `Spatial::set_positions`: per-channel volume for an emitter and two ears.
    fn rodio_volumes(emitter: Vec3) -> (f32, f32) {
        let (left, right) = (Vec3::X * EAR_GAP / -2.0, Vec3::X * EAR_GAP / 2.0);
        let (ld, rd) = (left.distance(emitter), right.distance(emitter));
        let max_diff = left.distance(right);
        let l = ((((ld - rd) / max_diff + 1.0) / 4.0 + 0.5).min(1.0)) * (1.0 / (ld * ld)).min(1.0);
        let r = ((((rd - ld) / max_diff + 1.0) / 4.0 + 0.5).min(1.0)) * (1.0 / (rd * rd)).min(1.0);
        (l, r)
    }

    #[test]
    fn the_virtual_emitter_only_balances() {
        let ears = [Vec3::X * EAR_GAP / -2.0, Vec3::X * EAR_GAP / 2.0];
        let mut last_right = 0.0;
        for i in -10..=10 {
            let e = virtual_emitter(i as f32 / 10.0);
            assert!(ears.iter().all(|ear| ear.distance(e) <= 1.0), "rodio's distance factor stays 1");
            let (_, r) = rodio_volumes(e);
            assert!(r >= last_right, "the right side gets louder as the balance moves right");
            last_right = r;
        }
        let (l, r) = rodio_volumes(virtual_emitter(1.0));
        assert!(r > l, "heard on the right: {l} {r}");
        let (l, r) = rodio_volumes(virtual_emitter(-1.0));
        assert!(l > r);
        let (l, r) = rodio_volumes(virtual_emitter(0.0));
        assert!((l - r).abs() < 1e-5);
    }
}
