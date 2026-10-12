//! Simulation time as the client draws it: tick lengths in seconds, how far a countdown is `alpha`
//! into the current tick, and the drawn moment (`SimNow`). Floats stay on this side; `game_core::time`
//! has none.
//!
//! Two clocks, one rule:
//! - Everything in the game world (units, smoke, flames, nods, shakes, later spell effects) follows
//!   simulation time, `SimNow`: it stops while paused, runs faster with the game speed, jumps with the
//!   ticks when several run in one frame, and is the same on every client and on a replay.
//! - UI, camera and mouse feedback (blinking, tooltips, the cursor, camera flights, a hovered tree's
//!   reaction) and debug logs stay on Bevy's real clock (`Res<Time>`), so they work while paused.
//!
//! A world pose is computed from the drawn moment and when it started, never accumulated per frame
//! (`x += v * dt` drifts on pause and cannot be rebuilt after a jump): pose = f(start tick, now). Ages
//! are counted in whole ticks first, loops taken modulo on whole ticks (`SimNow::secs_in`), so long
//! games keep their precision. Randomness in a visual is seeded from a stable id (`seed`).

use bevy::prelude::Resource;
use game_core::time::{Countdown, Tick, Ticks, TICKS_PER_SECOND};

pub const TICK_SECS: f32 = 1.0 / TICKS_PER_SECOND as f32;

pub fn ticks_f32(t: Ticks) -> f32 {
    t.get() as f32
}

/// 0 when it starts, 1 when it ends.
pub fn progress(c: Countdown, alpha: f32) -> f32 {
    ((ticks_f32(c.done()) + alpha) / ticks_f32(c.total()).max(1.0)).clamp(0.0, 1.0)
}

/// Whole seconds left, rounded up.
pub fn secs_left(c: Countdown) -> u32 {
    c.left().get().div_ceil(TICKS_PER_SECOND)
}

/// The moment the world is drawn at: `tick` and `frac` (0..1) of the next. Set each frame by
/// `units::run_ticks` from the map's tick counter alone (`drawn`), so any client with the same tick
/// draws the same moment.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct SimNow {
    pub tick: Tick,
    pub frac: f32,
}

impl SimNow {
    /// The drawn moment when the map is at `now`, the last step ran `step` ticks and `alpha` of the next
    /// step's time has passed: one step behind, like the units gliding from where they were before it
    /// (`SimClock::cell_pos`).
    pub fn drawn(now: Tick, step: u32, alpha: f32) -> SimNow {
        let ahead = alpha.clamp(0.0, 1.0) * step as f32;
        let whole = (ahead as u32).min(step);
        let frac = if whole == step { 0.0 } else { ahead - whole as f32 };
        SimNow { tick: Tick::from_wire(now.to_wire().wrapping_sub(step)) + Ticks::new(whole), frac }
    }

    /// Seconds since `start`, negative before it.
    pub fn secs_since(self, start: Tick) -> f32 {
        (self.tick.since(start) as f32 + self.frac) * TICK_SECS
    }

    /// Seconds into a loop of `period` started at `start` (0 before it started).
    pub fn secs_in(self, start: Tick, period: Ticks) -> f32 {
        let since = self.tick.since(start);
        if since < 0 || period == Ticks::ZERO {
            return 0.0;
        }
        ((since as u32 % period.get()) as f32 + self.frac) * TICK_SECS
    }
}

/// A phase 0..1 drawn from a stable id: the same object always looks the same.
pub fn seed(id: u32) -> f32 {
    (id.wrapping_mul(0x9E37_79B9) >> 8) as f32 / (1u32 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drawn_moment_is_one_step_behind_the_tick() {
        let at = |n: u32, step: u32, alpha: f32| SimNow::drawn(Tick::from_wire(n), step, alpha);
        assert_eq!(at(10, 1, 0.0), SimNow { tick: Tick::from_wire(9), frac: 0.0 });
        assert_eq!(at(10, 1, 0.5), SimNow { tick: Tick::from_wire(9), frac: 0.5 });
        assert_eq!(at(10, 1, 1.0), SimNow { tick: Tick::from_wire(10), frac: 0.0 });
        assert_eq!(at(10, 4, 0.5), SimNow { tick: Tick::from_wire(8), frac: 0.0 });
        assert_eq!(at(10, 1, 0.3), at(10, 1, 0.3), "a function of the tick and alpha only");
        let (before, after) = (at(10, 4, 1.0), at(14, 4, 0.0));
        assert_eq!(before, after, "continuous across a step");
    }

    #[test]
    fn ages_and_loops_count_whole_ticks_first() {
        let now = SimNow { tick: Tick::from_wire(30), frac: 0.5 };
        assert!((now.secs_since(Tick::from_wire(18)) - 12.5 * TICK_SECS).abs() < 1e-6);
        assert!(now.secs_since(Tick::from_wire(40)) < 0.0, "not started yet");
        let period = Ticks::new(24);
        let long = SimNow { tick: Tick::from_wire(30 + 24 * 1_000_000), frac: 0.5 };
        assert_eq!(now.secs_in(Tick::ZERO, period), long.secs_in(Tick::ZERO, period), "no precision lost in long games");
        assert!((now.secs_in(Tick::ZERO, period) - 6.5 * TICK_SECS).abs() < 1e-6);
        assert_eq!(now.secs_in(Tick::from_wire(40), period), 0.0);
    }

    #[test]
    fn seeds_are_phases_fixed_by_id() {
        assert_eq!(seed(7), seed(7));
        assert_ne!(seed(7), seed(8));
        assert!((0..1000).map(seed).all(|s| (0.0..1.0).contains(&s)));
    }

    #[test]
    fn progress_runs_between_ticks() {
        let total = Ticks::new(4);
        assert_eq!(progress(Countdown::new(total), 0.0), 0.0);
        assert_eq!(progress(Countdown::with_left(total, Ticks::new(2)), 0.5), 0.625);
        assert_eq!(progress(Countdown::with_left(total, Ticks::ZERO), 0.9), 1.0);
    }

    #[test]
    fn seconds_left_round_up() {
        assert_eq!(secs_left(Countdown::with_left(Ticks::secs(3), Ticks::new(13))), 2);
        assert_eq!(secs_left(Countdown::with_left(Ticks::secs(3), Ticks::new(12))), 1);
    }
}
