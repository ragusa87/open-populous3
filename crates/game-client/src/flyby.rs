//! A script's flyby (docs/specs/ai-scripts.md "Flybys"): while `GameMap::active_flyby` is `Some` and
//! not yet interrupted, this drives the camera along its scripted keyframes instead of the player
//! (`camera::camera_input` is suppressed the same way `armageddon::input_locked` already suppresses
//! it) and a "Flyby" banner blinks bottom-right. Space (if `allow_interrupt`) sends
//! `Command::CancelFlyby`: the banner switches to "Cancelled flyby" for a short grace period
//! (`game_core::ai_script::FLYBY_CANCEL_GRACE_TICKS`), then the camera returns to exactly where it
//! was before the flyby and the player regains control — both read straight off `active_flyby`
//! disappearing, no separate client-side timer.
//!
//! The zoom (-100..100) -> `distance` mapping and the per-keyframe easing are both unspecified in the
//! spec, so both are our own guesses, same spirit as `camera.rs`'s own `DEFAULT_FOV` comment.

use crate::camera::{CameraRig, DISTANCE_RANGE};
use crate::keymap::{Shortcut, Shortcuts};
use crate::sim_time::SimNow;
use crate::units::{GameSchedule, PLAYER};
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::command::Command;
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

/// Poses `rig` from the active flyby's keyframes at `now`, or restores it to its own pre-flyby state
/// the moment the flyby disappears (finished or its cancel grace period over,
/// `GameMap::tend_flyby`). Held (untouched) while interrupted but not yet cleared, so the camera
/// stays exactly where it was at the moment of cancellation.
pub fn drive_camera(map: Res<CurrentMap>, now: Res<SimNow>, mut rig: ResMut<CameraRig>, mut pre: Local<Option<CameraRig>>) {
    let Some(active) = &map.0.active_flyby else {
        if let Some(snapshot) = pre.take() {
            *rig = snapshot;
        }
        return;
    };
    let snapshot = pre.get_or_insert_with(|| rig.clone()).clone();
    if active.interrupted_at.is_some() {
        return;
    }
    let t = now.turns_since(active.start);
    let map_size = pop3_format::MAP_SIZE as f32;
    let xs: Vec<(f32, i32, i32)> = active.flyby.positions.iter().map(|k| (k.x as f32, k.start, k.duration)).collect();
    let zs: Vec<(f32, i32, i32)> = active.flyby.positions.iter().map(|k| (k.z as f32, k.start, k.duration)).collect();
    let angles: Vec<(f32, i32, i32)> = active.flyby.angles.iter().map(|k| (crate::nature::angle_yaw(k.angle.rem_euclid(2048) as u16).render(), k.start, k.duration)).collect();
    let zooms: Vec<(f32, i32, i32)> = active.flyby.zooms.iter().map(|k| (zoom_to_distance(k.zoom as f32), k.start, k.duration)).collect();
    rig.focus.x = channel_value(snapshot.focus.x, &xs, t, Some(map_size)).rem_euclid(map_size);
    rig.focus.y = channel_value(snapshot.focus.y, &zs, t, Some(map_size)).rem_euclid(map_size);
    rig.yaw = channel_value(snapshot.yaw, &angles, t, Some(TAU));
    rig.distance = channel_value(snapshot.distance, &zooms, t, None);
}

/// `-100 (out) .. 100 (in)` onto `DISTANCE_RANGE`, reversed: our own guess, the spec gives no scale
/// (ai-scripts.md "Flybys": "z from -100 (out) to 100 (in)").
fn zoom_to_distance(z: f32) -> f32 {
    let t = (z.clamp(-100.0, 100.0) + 100.0) / 200.0;
    DISTANCE_RANGE.1 + (DISTANCE_RANGE.0 - DISTANCE_RANGE.1) * t
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
}
