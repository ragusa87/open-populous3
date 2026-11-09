//! A script's flyby (docs/specs/ai-scripts.md "Flybys"): while `GameMap::active_flyby` is `Some` and
//! not yet interrupted, this drives the camera along its scripted keyframes instead of the player
//! (`camera::camera_input` is suppressed the same way `armageddon::input_locked` already suppresses
//! it) and a "Flyby" banner blinks bottom-right. Space (if `allow_interrupt`) sends
//! `Command::CancelFlyby`: the banner switches to "Cancelled flyby" and the camera eases, over a
//! grace period (`game_core::ai_script::FLYBY_CANCEL_GRACE_TICKS`, ~3s, measured live - pop3-rev-analysis.md
//! "Flyby cancel"), from wherever it was at
//! that moment to the flyby's own last position (`end_target` if the script set one, else each
//! channel's own last keyframe) — the same place it would have settled at had it been let play out,
//! just reached early. Once the grace period is up the player regains control, read straight off
//! `active_flyby` disappearing (`GameMap::tend_flyby`), no separate client-side timer.
//!
//! The zoom (-100..100) -> `pitch` mapping is a piecewise-linear fit through real (zoom, pitch) pairs
//! measured live off two independent flyby captures, matched to level 1's own `FLYBY_SET_EVENT_ZOOM`
//! keyframes (pop3-rev-analysis.md "The AI-script zoom channel, traced end to end"): the script's bounded
//! `-100..100` never reaches `PITCH_RANGE`'s own confirmed extremes (those come from physically holding the
//! manual zoom key to the whole-planet globe view, a different input entirely) - `zoom_to_pitch` used to be
//! fit against that wrong range. Zoom 80..100 is still a guess (no level script traced so far exercises past
//! 80); the per-keyframe easing is a guess too, same spirit as `camera.rs`'s own `DEFAULT_FOV` comment.
//!
//! The original's "zoom" is a coordinated multi-field blend, not just a pitch+bend orbit at a fixed lens:
//! `zoom_to_fov` drives `CameraRig.fov` the same way `zoom_to_pitch` drives pitch, fit to live-measured
//! vertical FOV values at the same keyframes (pop3-rev-analysis.md "Zoom changes effective FOV too" - a real
//! telephoto effect zoomed in, not a wider view as `camera::DEFAULT_FOV`'s own "matched by eye" guess assumed).

use crate::camera::CameraRig;
use crate::keymap::{Shortcut, Shortcuts};
use crate::sim_time::SimNow;
use crate::units::{GameSchedule, PLAYER};
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::ai_script::FLYBY_CANCEL_GRACE_TICKS;
use game_core::command::Command;
use pop3_ai_script::interp::ResolvedFlyby;
use std::f32::consts::TAU;

/// Blinks per second ("Flyby" while playing).
const BLINK_HZ: f32 = 2.0;

pub struct FlybyPlugin;

impl Plugin for FlybyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_banner).add_systems(Update, (banner, cancel_input).in_set(crate::menu::Gameplay));
    }
}

/// `true` while a flyby is playing and hasn't been interrupted — one of the four cases
/// `input_lock::input_locked` composes.
pub fn flyby_active(map: Res<CurrentMap>) -> bool {
    map.0.active_flyby.as_ref().is_some_and(|f| f.interrupted_at.is_none())
}

/// What's remembered across frames while a flyby plays: the camera as it was the moment the flyby
/// began (the "prev" value `channel_value` eases from, before a channel's first keyframe), and, once
/// cancelled, the camera as it was at that exact moment (what the cancel-grace easing eases *from*).
pub(crate) struct Playback {
    start: CameraRig,
    cancel_from: Option<CameraRig>,
}

/// Poses `rig` from the active flyby's keyframes at `now`; once cancelled, eases it instead from
/// wherever it was at that moment towards the flyby's own last position (`final_pose`) over
/// `FLYBY_CANCEL_GRACE_TICKS`. Forgets everything the moment the flyby disappears
/// (`GameMap::tend_flyby`): by then the camera has already eased to (finished on its own) or towards
/// (cancelled) that same last position, so there's nothing left to restore.
pub fn drive_camera(map: Res<CurrentMap>, now: Res<SimNow>, mut rig: ResMut<CameraRig>, mut state: Local<Option<Playback>>) {
    let Some(active) = &map.0.active_flyby else {
        *state = None;
        return;
    };
    let playback = state.get_or_insert_with(|| Playback { start: rig.clone(), cancel_from: None });
    let map_size = pop3_format::MAP_SIZE as f32;
    match active.interrupted_at {
        None => {
            let t = now.turns_since(active.start);
            let xs: Vec<(f32, i32, i32)> = active.flyby.positions.iter().map(|k| (half_cell(k.x), k.start, k.duration)).collect();
            let zs: Vec<(f32, i32, i32)> = active.flyby.positions.iter().map(|k| (half_cell(k.z), k.start, k.duration)).collect();
            let angles: Vec<(f32, i32, i32)> = active.flyby.angles.iter().map(|k| (render_yaw(k.angle), k.start, k.duration)).collect();
            let zooms: Vec<(f32, i32, i32)> = active.flyby.zooms.iter().map(|k| (zoom_to_pitch(k.zoom as f32), k.start, k.duration)).collect();
            let fovs: Vec<(f32, i32, i32)> = active.flyby.zooms.iter().map(|k| (zoom_to_fov(k.zoom as f32), k.start, k.duration)).collect();
            rig.focus.x = channel_value(playback.start.focus.x, &xs, t, Some(map_size)).rem_euclid(map_size);
            rig.focus.y = channel_value(playback.start.focus.y, &zs, t, Some(map_size)).rem_euclid(map_size);
            rig.yaw = channel_value(playback.start.yaw, &angles, t, Some(TAU));
            rig.pitch = channel_value(playback.start.pitch, &zooms, t, None);
            rig.fov = channel_value(playback.start.fov, &fovs, t, None);
        }
        Some(at) => {
            let from = playback.cancel_from.get_or_insert_with(|| rig.clone()).clone();
            let (tx, tz, tyaw, tpitch, tfov) = final_pose(&active.flyby, &playback.start);
            let p = now.turns_since(at) / FLYBY_CANCEL_GRACE_TICKS.get() as f32;
            rig.focus.x = ease_to(from.focus.x, tx, p, Some(map_size)).rem_euclid(map_size);
            rig.focus.y = ease_to(from.focus.y, tz, p, Some(map_size)).rem_euclid(map_size);
            rig.yaw = ease_to(from.yaw, tyaw, p, Some(TAU));
            rig.pitch = ease_to(from.pitch, tpitch, p, None);
            rig.fov = ease_to(from.fov, tfov, p, None);
        }
    }
}

/// One piecewise-linear fit, in degrees, over `knots` sorted by zoom; `z` clamped to the knots' own span.
fn piecewise_deg(knots: &[(f32, f32)], z: f32) -> f32 {
    let z = z.clamp(knots[0].0, knots[knots.len() - 1].0);
    let i = knots.iter().rposition(|&(k, _)| k <= z).unwrap_or(0).min(knots.len() - 2);
    let ((z0, d0), (z1, d1)) = (knots[i], knots[i + 1]);
    let p = (z - z0) / (z1 - z0);
    d0 + (d1 - d0) * p
}

/// `-100 (out) .. 100 (in)` (ai-scripts.md "Flybys": "z from -100 (out) to 100 (in)") onto pitch: a
/// piecewise-linear fit through real (zoom, pitch) pairs traced off two independent flyby captures
/// (pop3-rev-analysis.md "The AI-script zoom channel, traced end to end"). `PITCH_KNOTS`' first 4 points are
/// that trace; the last (100, `PITCH_RANGE.0`) extends the same slope the 10..80 trace already has out to
/// full zoom-in - unconfirmed past 80, but it lands within half a degree of a straight extrapolation, so
/// likely close. Distance is left alone, same as manual zoom (`camera::adjust_view`).
const PITCH_KNOTS: [(f32, f32); 5] = [(-100.0, 55.37), (0.0, 20.56), (10.0, 19.51), (80.0, 12.70), (100.0, 10.5)];

fn zoom_to_pitch(z: f32) -> f32 {
    piecewise_deg(&PITCH_KNOTS, z).to_radians()
}

/// `-100..100` onto vertical field of view: a piecewise-linear fit through live-measured FOV_y values at the
/// same keyframes `PITCH_KNOTS` was fit from (pop3-rev-analysis.md "Zoom changes effective FOV too": the
/// zoom channel also blends the per-axis screen-scale factor `Q`, `focal_px = Q·2^(S-16)`,
/// `FOV_y = 2·atan(half_height/focal_px)` - window-size-independent once converted to degrees). Zoom
/// 80..100 has no trace either; `FOV_KNOTS`' last point extends the 0..80 trace's own slope, the same
/// extrapolation spirit as `PITCH_KNOTS`.
const FOV_KNOTS: [(f32, f32); 4] = [(-100.0, 21.6), (0.0, 21.3), (80.0, 12.3), (100.0, 10.05)];

fn zoom_to_fov(z: f32) -> f32 {
    piecewise_deg(&FOV_KNOTS, z).to_radians()
}

/// A raw `POS`/`TOOLTIP`/`END_TARGET` x or z value is a cell index at half-cell resolution
/// (pop3-rev-analysis.md "Correction: raw byte is HALF-cell, not a literal cell index").
fn half_cell(raw: i32) -> f32 {
    raw as f32 * 0.5
}

/// A raw `FLYBY_SET_EVENT_ANGLE` value (2048ths of a turn) as a render yaw (`nature::angle_yaw`).
fn render_yaw(raw: i32) -> f32 {
    crate::nature::angle_yaw(raw.rem_euclid(2048) as u16).render()
}

/// Where the camera would end up if the flyby played all the way through: `end_target` if the script
/// set one, else each channel's own last keyframe, else (no data at all) wherever the camera already
/// was when the flyby began (`start`). `end_target` carries no zoom (ai-scripts.md "Flybys"), so
/// pitch and fov always fall back to the last zoom keyframe regardless.
fn final_pose(flyby: &ResolvedFlyby, start: &CameraRig) -> (f32, f32, f32, f32, f32) {
    let (x, z, yaw) = match &flyby.end_target {
        Some(end) => (half_cell(end.x), half_cell(end.z), render_yaw(end.angle)),
        None => (
            flyby.positions.last().map_or(start.focus.x, |k| half_cell(k.x)),
            flyby.positions.last().map_or(start.focus.y, |k| half_cell(k.z)),
            flyby.angles.last().map_or(start.yaw, |k| render_yaw(k.angle)),
        ),
    };
    let pitch = flyby.zooms.last().map_or(start.pitch, |k| zoom_to_pitch(k.zoom as f32));
    let fov = flyby.zooms.last().map_or(start.fov, |k| zoom_to_fov(k.zoom as f32));
    (x, z, yaw, pitch, fov)
}

/// One channel's value at turn `t`: finds the keyframe with the greatest `start <= t` and eases from
/// the one before it (or `prev`, for the first) over its own `duration` (smoothstep); holds at
/// `prev` before the first keyframe starts, and at the last keyframe's value once past it. `period`
/// wraps the eased delta the short way round (map cells for position, a turn for angle), like
/// `camera::flight_point`'s own wrap; `None` for a plain (unwrapped) value like zoom/distance.
fn channel_value(prev: f32, keyframes: &[(f32, i32, i32)], t: f32, period: Option<f32>) -> f32 {
    let Some(i) = keyframes.iter().rposition(|&(_, start, _)| start as f32 <= t) else { return prev };
    let (value, start, duration) = keyframes[i];
    let from = if i == 0 { prev } else { keyframes[i - 1].0 };
    let p = if duration <= 0 { 1.0 } else { (t - start as f32) / duration as f32 };
    ease_to(from, value, p, period)
}

/// Eases `from` towards `to` over local progress `p` (0..1, smoothstep, clamped), the short way round
/// a wrap of `period` if given.
fn ease_to(from: f32, to: f32, p: f32, period: Option<f32>) -> f32 {
    let p = p.clamp(0.0, 1.0);
    let eased = p * p * (3.0 - 2.0 * p);
    let delta = match period {
        Some(period) => {
            let d = to - from;
            (d + period / 2.0).rem_euclid(period) - period / 2.0
        }
        None => to - from,
    };
    from + delta * eased
}

#[derive(Component)]
struct FlybyBanner;

fn spawn_banner(mut commands: Commands) {
    commands.spawn((
        FlybyBanner,
        Text::new(""),
        TextFont { font_size: FontSize::Px(16.0), ..default() },
        TextLayout::justify(Justify::Right),
        // `camera_debug`'s own readout sits right below (bottom: px(8)), up to 4 lines tall in dev
        // mode (on by default) - clear of it, not stacked on top of it.
        Node { position_type: PositionType::Absolute, bottom: px(100), right: px(10), ..default() },
        Visibility::Hidden,
    ));
}

/// "Flyby" blinking while playing, steady "Cancelled flyby" while interrupted (its grace period,
/// `FLYBY_CANCEL_GRACE_TICKS`), hidden otherwise. Real time (`Res<Time>`), like every other blink in
/// the client (`progress_bar::blink_bars`): it should keep blinking even while the game itself pauses.
fn banner(map: Res<CurrentMap>, time: Res<Time>, mut q: Query<(&mut Text, &mut Visibility), With<FlybyBanner>>) {
    let Ok((mut text, mut vis)) = q.single_mut() else { return };
    match &map.0.active_flyby {
        None => {
            vis.set_if_neq(Visibility::Hidden);
        }
        Some(active) if active.interrupted_at.is_some() => {
            if text.0 != "Cancelled flyby" {
                text.0 = "Cancelled flyby".to_string();
            }
            vis.set_if_neq(Visibility::Inherited);
        }
        Some(_) => {
            if text.0 != "Flyby" {
                text.0 = "Flyby".to_string();
            }
            let on = (time.elapsed_secs() * BLINK_HZ).fract() < 0.5;
            vis.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
        }
    }
}

/// Space, while a flyby is playing and interruptible: issues `Command::CancelFlyby`
/// (`units::select_and_order`'s pattern for turning player input into a command).
fn cancel_input(keys: Shortcuts, map: Res<CurrentMap>, mut schedule: ResMut<GameSchedule>) {
    if keys.just_pressed(Shortcut::CancelFlyby) && let Some(active) = &map.0.active_flyby && active.flyby.allow_interrupt && active.interrupted_at.is_none() {
        schedule.issue(Command::CancelFlyby { player: PLAYER });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::PITCH_RANGE;

    /// pop3-rev-analysis.md "The AI-script zoom channel, traced end to end": real (zoom, pitch) pairs measured
    /// live off two independent flyby captures, matched to level 1's own `FLYBY_SET_EVENT_ZOOM` keyframes -
    /// pinned so a later cleanup can't simplify the piecewise curve back into a straight line across
    /// `PITCH_RANGE` (the wrong range entirely: that one comes from manual zoom's much wider physical reach)
    /// without a test catching it.
    #[test]
    fn zoom_is_calibrated_against_real_flyby_pitch_traces() {
        for &(z, expected_deg) in &[(-100.0, 55.37), (10.0, 19.51), (80.0, 12.70), (0.0, 20.56)] {
            let got_deg = zoom_to_pitch(z).to_degrees();
            assert!((got_deg - expected_deg).abs() < 6.0, "zoom {z}: got {got_deg}°, expected ~{expected_deg}°");
        }
    }

    #[test]
    fn full_zoom_in_still_reaches_the_confirmed_pitch_range_floor() {
        assert!((zoom_to_pitch(100.0) - PITCH_RANGE.0).abs() < 1e-6, "unconfirmed past zoom 80, but lands here by construction");
    }

    /// pop3-rev-analysis.md "Zoom changes effective FOV too": live-measured vertical FOV at the same
    /// keyframes `zoom_to_pitch` was calibrated from - zoomed in narrows the lens (a telephoto effect), it
    /// does not widen it the way `camera::DEFAULT_FOV`'s "matched by eye" guess alone would suggest.
    #[test]
    fn zoom_is_calibrated_against_real_flyby_fov_traces() {
        for &(z, expected_deg) in &[(-100.0, 21.6), (0.0, 21.3), (80.0, 12.3)] {
            let got_deg = zoom_to_fov(z).to_degrees();
            assert!((got_deg - expected_deg).abs() < 1.0, "zoom {z}: got {got_deg}°, expected ~{expected_deg}°");
        }
    }

    #[test]
    fn zooming_in_narrows_the_fov_not_widens_it() {
        assert!(zoom_to_fov(80.0) < zoom_to_fov(0.0), "a telephoto effect, confirmed live");
    }

    #[test]
    fn channel_holds_before_the_first_keyframe_and_after_the_last() {
        let keyframes = [(10.0, 4, 80), (20.0, 100, 10)];
        assert_eq!(channel_value(5.0, &keyframes, 0.0, None), 5.0, "nothing has started yet");
        assert_eq!(channel_value(5.0, &keyframes, 200.0, None), 20.0, "holds at the last value");
    }

    #[test]
    fn channel_eases_from_the_previous_keyframe_smoothly() {
        let keyframes = [(10.0, 0, 10), (20.0, 10, 10)];
        assert_eq!(channel_value(0.0, &keyframes, 0.0, None), 0.0, "p=0 starts at prev");
        assert_eq!(channel_value(0.0, &keyframes, 10.0, None), 10.0, "p=1 reaches the keyframe");
        assert_eq!(channel_value(0.0, &keyframes, 20.0, None), 20.0, "second keyframe reached in turn");
        let mid = channel_value(0.0, &keyframes, 5.0, None);
        assert!(mid > 0.0 && mid < 10.0, "eases in between, {mid}");
    }

    #[test]
    fn a_wrapped_channel_takes_the_short_way_round() {
        // From 10 to 350 on a 0..360 wrap: the short way is backward through 0, not forward through 180.
        let keyframes = [(350.0, 0, 10)];
        let eased = channel_value(10.0, &keyframes, 5.0, Some(360.0));
        assert!((0.0..10.0).contains(&eased.rem_euclid(360.0)) || eased.rem_euclid(360.0) > 350.0, "{eased}");
    }

    #[test]
    fn empty_keyframes_hold_at_prev() {
        assert_eq!(channel_value(42.0, &[], 1000.0, None), 42.0);
    }

    use pop3_ai_script::interp::{ResolvedEndTarget, ResolvedPos, ResolvedZoom};

    #[test]
    fn half_cell_halves_the_raw_byte() {
        assert_eq!(half_cell(20), 10.0);
        assert_eq!(half_cell(1), 0.5);
    }

    /// pop3-rev-analysis.md "D3D axis caveat": `yaw_true = -raw * 360/2048` (mod 360), confirmed against
    /// two independently captured full-flyby traces to within ~0.5-4° on 4 keyframes each (TODO.md
    /// "Camera flyby state") - a regression test so the mirror negation in `render_yaw` doesn't get
    /// "simplified away" later with nothing to catch it.
    #[test]
    fn render_yaw_is_calibrated_against_real_flyby_traces() {
        for &(raw, expected_deg) in &[(0, 0.0), (681, 240.29), (744, 229.22), (1438, 107.23)] {
            let got_deg = render_yaw(raw).to_degrees().rem_euclid(360.0);
            let diff = (got_deg - expected_deg).rem_euclid(360.0);
            let diff = diff.min(360.0 - diff);
            assert!(diff < 5.0, "raw {raw}: got {got_deg}°, expected ~{expected_deg}°");
        }
    }

    #[test]
    fn final_pose_prefers_end_target_over_the_last_keyframe() {
        let flyby = ResolvedFlyby {
            positions: vec![ResolvedPos { x: 1, z: 1, start: 0, duration: 1 }],
            end_target: Some(ResolvedEndTarget { x: 20, z: 22, angle: 0, unknown: 0 }),
            ..ResolvedFlyby::default()
        };
        let (x, z, _, _, _) = final_pose(&flyby, &CameraRig::default());
        assert_eq!((x, z), (10.0, 11.0), "half-cell resolution, not a literal cell index");
    }

    #[test]
    fn final_pose_falls_back_to_the_last_keyframe_without_an_end_target() {
        let flyby = ResolvedFlyby {
            positions: vec![ResolvedPos { x: 1, z: 1, start: 0, duration: 1 }, ResolvedPos { x: 9, z: 8, start: 10, duration: 1 }],
            zooms: vec![ResolvedZoom { zoom: 100, start: 0, duration: 1 }],
            ..ResolvedFlyby::default()
        };
        let (x, z, _, pitch, fov) = final_pose(&flyby, &CameraRig::default());
        assert_eq!((x, z), (4.5, 4.0), "the last position keyframe, not the first, halved");
        assert!((pitch - PITCH_RANGE.0).abs() < 1e-6, "the last zoom keyframe");
        assert!((fov - zoom_to_fov(100.0)).abs() < 1e-6, "the last zoom keyframe, for fov too");
    }

    #[test]
    fn final_pose_with_no_data_at_all_is_wherever_it_started() {
        let mut start = CameraRig::default();
        start.focus = Vec2::new(5.0, 6.0);
        start.yaw = 1.0;
        start.pitch = 0.9;
        assert_eq!(final_pose(&ResolvedFlyby::default(), &start), (5.0, 6.0, 1.0, 0.9, start.fov));
    }
}
