//! Armageddon, client side (`docs/specs/outcome.md` "Armageddon"; `TODO.md` for what's left): while a match is active
//! (`GameMap::armageddon`, "the Armageddon flag") the player is locked out and the camera flies to the arena, then
//! turns around it for the whole match (`orbit_yaw`). The atmosphere tint, survivor HUD and cast-confirmed flash are
//! not built yet (`TODO.md`).

use crate::camera::CameraRig;
use crate::menu::AppState;
use crate::units::SimClock;
use crate::world::CurrentMap;
use bevy::prelude::*;
use pop3_format::WORLD_UNITS_PER_CELL;

/// Whether a match is active right now, and where (cells) — the one place the client reads
/// `GameMap::armageddon()`.
#[derive(Resource, Default)]
pub struct ArmageddonWatch {
    game: u32,
    at: Option<Vec2>,
}

pub struct ArmageddonPlugin;

impl Plugin for ArmageddonPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ArmageddonWatch>()
            .add_systems(Update, track_armageddon.in_set(crate::menu::Gameplay))
            .add_systems(Update, auto_camera_armageddon.in_set(crate::menu::Gameplay).run_if(armageddon_active));
    }
}

/// `true` while a match is active — the run condition `camera_input`, `select_and_order` and the spell-casting
/// systems gate themselves on, so the player can't look around, select a unit, give an order, or cast anything
/// (backed by the real guarantee, `GameMap::apply`'s command lock — this is UI-level so it doesn't look
/// responsive when it isn't).
pub fn armageddon_active(watch: Res<ArmageddonWatch>) -> bool {
    watch.at.is_some()
}

/// `true` while the player can't meaningfully act at all because of Armageddon specifically: a match
/// resolving, or the game already over (`AppState::Ended`) — the match keeps running past the win/lose
/// dialog (`game_end.rs`, the "victory lap" epilogue, `docs/specs/outcome.md`). One of the four cases
/// `input_lock::input_locked` composes for every manual-input system (camera, selection, orders,
/// spell-casting) instead of leaving `Gameplay` to stop them.
pub fn input_locked(watch: Res<ArmageddonWatch>, state: Res<State<AppState>>) -> bool {
    armageddon_active(watch) || *state.get() == AppState::Ended
}

fn track_armageddon(map: Res<CurrentMap>, clock: Res<SimClock>, mut watch: ResMut<ArmageddonWatch>) {
    if watch.game != clock.game {
        *watch = ArmageddonWatch { game: clock.game, at: None };
    }
    watch.at = map.0.armageddon().map(|(x, z)| Vec2::new(x as f32, z as f32) / WORLD_UNITS_PER_CELL as f32);
}

/// The camera's turn around the arena, radians a second (`[ours]`): a full turn in about 40 s.
pub const ORBIT_SPEED: f32 = std::f32::consts::TAU / 40.0;
/// How far out and how steeply it looks at the arena while turning (`[ours]`): the whole arena in view.
pub const ORBIT_DISTANCE: f32 = 34.0;
pub const ORBIT_PITCH: f32 = 0.6;

/// The camera's yaw `dt` seconds of orbiting after `yaw`, kept in `[0, TAU)`.
pub fn orbit_yaw(yaw: f32, dt: f32) -> f32 {
    (yaw + ORBIT_SPEED * dt).rem_euclid(std::f32::consts::TAU)
}

/// Flies the camera to the match's arena, framing it whole, then turns around it for as long as the match lasts.
fn auto_camera_armageddon(time: Res<Time>, watch: Res<ArmageddonWatch>, mut rig: ResMut<CameraRig>, mut at: Local<Option<Vec2>>) {
    if *at != watch.at {
        *at = watch.at;
        if let Some(to) = watch.at {
            rig.fly_to(to);
            rig.set_ground_view(ORBIT_PITCH, ORBIT_DISTANCE);
        }
    }
    rig.yaw = orbit_yaw(rig.yaw, time.delta_secs());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_orbit_turns_steadily_and_wraps() {
        assert!((orbit_yaw(0.0, 10.0) - ORBIT_SPEED * 10.0).abs() < 1e-5);
        assert!(orbit_yaw(std::f32::consts::TAU - 0.01, 1.0) < 1.0, "wrapped");
    }
}
