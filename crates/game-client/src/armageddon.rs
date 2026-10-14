//! Armageddon, client side (`docs/specs/outcome.md` "Armageddon"; `TODO.md` for what's left): v1 only needs to
//! know whether a match is active and where it's centred (`GameMap::armageddon`, "the Armageddon flag") to take
//! the camera over and lock the player out — there's no multi-turn fight to render yet. The atmosphere tint,
//! survivor HUD and cast-confirmed flash from the full design are deliberately not built here (`TODO.md`).

use crate::camera::CameraRig;
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

fn track_armageddon(map: Res<CurrentMap>, clock: Res<SimClock>, mut watch: ResMut<ArmageddonWatch>) {
    if watch.game != clock.game {
        *watch = ArmageddonWatch { game: clock.game, at: None };
    }
    watch.at = map.0.armageddon().map(|(x, z)| Vec2::new(x as f32, z as f32) / WORLD_UNITS_PER_CELL as f32);
}

/// Flies the camera to the match's target and holds there for as long as it's active. v1: a simple fly-to-and-hold,
/// not the full design's continuous orbit (`TODO.md`) — there's no multi-turn fight yet for an orbit to read as
/// anything but a jump-cut.
fn auto_camera_armageddon(watch: Res<ArmageddonWatch>, mut rig: ResMut<CameraRig>, mut at: Local<Option<Vec2>>) {
    if *at != watch.at {
        *at = watch.at;
        if let Some(to) = watch.at {
            rig.fly_to(to);
        }
    }
}
