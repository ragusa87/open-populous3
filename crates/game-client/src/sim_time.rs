//! Simulation time as the client draws it: tick lengths in seconds, and how far a countdown is `alpha`
//! into the current tick. Floats stay on this side; `game_core::time` has none.

use game_core::time::{Countdown, Ticks, TICKS_PER_SECOND};

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

#[cfg(test)]
mod tests {
    use super::*;

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
