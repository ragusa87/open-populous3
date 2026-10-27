//! A script's flyby (docs/specs/ai-scripts.md "Flybys"): while `GameMap::active_flyby` is `Some` and
//! not yet interrupted, this drives the camera along its scripted keyframes instead of the player
//! (`camera::camera_input` is suppressed the same way `armageddon::input_locked` already suppresses
//! it) and a "Flyby" banner blinks bottom-right. Space (if `allow_interrupt`) sends
//! `Command::CancelFlyby`: the banner switches to "Cancelled flyby" and the camera eases, over a
//! short grace period (`game_core::ai_script::FLYBY_CANCEL_GRACE_TICKS`), from wherever it was at
//! that moment to the flyby's own last position (`end_target` if the script set one, else each
//! channel's own last keyframe) — the same place it would have settled at had it been let play out,
//! just reached early. Once the grace period is up the player regains control, read straight off
//! `active_flyby` disappearing (`GameMap::tend_flyby`), no separate client-side timer.
//!
//! The zoom (-100..100) -> `distance` mapping and the per-keyframe easing are both unspecified in the
//! spec, so both are our own guesses, same spirit as `camera.rs`'s own `DEFAULT_FOV` comment.

use crate::camera::{CameraRig, DISTANCE_RANGE};
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

/// `true` while a flyby is playing and hasn't been interrupted — the run condition `camera_input`
/// gates itself on, next to `armageddon::input_locked`.
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
            let xs: Vec<(f32, i32, i32)> = active.flyby.positions.iter().map(|k| (k.x as f32, k.start, k.duration)).collect();
            let zs: Vec<(f32, i32, i32)> = active.flyby.positions.iter().map(|k| (k.z as f32, k.start, k.duration)).collect();
            let angles: Vec<(f32, i32, i32)> = active.flyby.angles.iter().map(|k| (render_yaw(k.angle), k.start, k.duration)).collect();
            let zooms: Vec<(f32, i32, i32)> = active.flyby.zooms.iter().map(|k| (zoom_to_distance(k.zoom as f32), k.start, k.duration)).collect();
            rig.focus.x = channel_value(playback.start.focus.x, &xs, t, Some(map_size)).rem_euclid(map_size);
            rig.focus.y = channel_value(playback.start.focus.y, &zs, t, Some(map_size)).rem_euclid(map_size);
            rig.yaw = channel_value(playback.start.yaw, &angles, t, Some(TAU));
            rig.distance = channel_value(playback.start.distance, &zooms, t, None);
        }
        Some(at) => {
            let from = playback.cancel_from.get_or_insert_with(|| rig.clone()).clone();
            let (tx, tz, tyaw, tdist) = final_pose(&active.flyby, &playback.start);
            let p = now.turns_since(at) / FLYBY_CANCEL_GRACE_TICKS.get() as f32;
            rig.focus.x = ease_to(from.focus.x, tx, p, Some(map_size)).rem_euclid(map_size);
            rig.focus.y = ease_to(from.focus.y, tz, p, Some(map_size)).rem_euclid(map_size);
            rig.yaw = ease_to(from.yaw, tyaw, p, Some(TAU));
            rig.distance = ease_to(from.distance, tdist, p, None);
        }
    }
}

/// `-100 (out) .. 100 (in)` onto `DISTANCE_RANGE`, reversed: our own guess, the spec gives no scale
/// (ai-scripts.md "Flybys": "z from -100 (out) to 100 (in)").
fn zoom_to_distance(z: f32) -> f32 {
    let t = (z.clamp(-100.0, 100.0) + 100.0) / 200.0;
    DISTANCE_RANGE.1 + (DISTANCE_RANGE.0 - DISTANCE_RANGE.1) * t
}

/// A raw `FLYBY_SET_EVENT_ANGLE` value (2048ths of a turn) as a render yaw (`nature::angle_yaw`).
fn render_yaw(raw: i32) -> f32 {
    crate::nature::angle_yaw(raw.rem_euclid(2048) as u16).render()
}

/// Where the camera would end up if the flyby played all the way through: `end_target` if the script
/// set one, else each channel's own last keyframe, else (no data at all) wherever the camera already
/// was when the flyby began (`start`). `end_target` carries no zoom (ai-scripts.md "Flybys"), so
/// distance always falls back to the last zoom keyframe regardless.
fn final_pose(flyby: &ResolvedFlyby, start: &CameraRig) -> (f32, f32, f32, f32) {
    let (x, z, yaw) = match &flyby.end_target {
        Some(end) => (end.x as f32, end.z as f32, render_yaw(end.angle)),
        None => (
            flyby.positions.last().map_or(start.focus.x, |k| k.x as f32),
            flyby.positions.last().map_or(start.focus.y, |k| k.z as f32),
            flyby.angles.last().map_or(start.yaw, |k| render_yaw(k.angle)),
        ),
    };
    let distance = flyby.zooms.last().map_or(start.distance, |k| zoom_to_distance(k.zoom as f32));
    (x, z, yaw, distance)
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

    #[test]
    fn zoom_maps_in_to_near_and_out_to_far() {
        assert_eq!(zoom_to_distance(100.0), DISTANCE_RANGE.0);
        assert_eq!(zoom_to_distance(-100.0), DISTANCE_RANGE.1);
        assert_eq!(zoom_to_distance(0.0), (DISTANCE_RANGE.0 + DISTANCE_RANGE.1) / 2.0);
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
    fn final_pose_prefers_end_target_over_the_last_keyframe() {
        let flyby = ResolvedFlyby {
            positions: vec![ResolvedPos { x: 1, z: 1, start: 0, duration: 1 }],
            end_target: Some(ResolvedEndTarget { x: 20, z: 21, angle: 0, unknown: 0 }),
            ..ResolvedFlyby::default()
        };
        let (x, z, _, _) = final_pose(&flyby, &CameraRig::default());
        assert_eq!((x, z), (20.0, 21.0));
    }

    #[test]
    fn final_pose_falls_back_to_the_last_keyframe_without_an_end_target() {
        let flyby = ResolvedFlyby {
            positions: vec![ResolvedPos { x: 1, z: 1, start: 0, duration: 1 }, ResolvedPos { x: 9, z: 8, start: 10, duration: 1 }],
            zooms: vec![ResolvedZoom { zoom: 100, start: 0, duration: 1 }],
            ..ResolvedFlyby::default()
        };
        let (x, z, _, dist) = final_pose(&flyby, &CameraRig::default());
        assert_eq!((x, z), (9.0, 8.0), "the last position keyframe, not the first");
        assert_eq!(dist, DISTANCE_RANGE.0, "the last zoom keyframe");
    }

    #[test]
    fn final_pose_with_no_data_at_all_is_wherever_it_started() {
        let mut start = CameraRig::default();
        start.focus = Vec2::new(5.0, 6.0);
        start.yaw = 1.0;
        start.distance = 50.0;
        assert_eq!(final_pose(&ResolvedFlyby::default(), &start), (5.0, 6.0, 1.0, 50.0));
    }
}
