//! Prayer gauges (docs/specs/worship.md, traced in pop3-rev-analysis.md "The gauge"): counted in
//! steps, `STEP` per unit of the level's `PrayTime`. They update on `RHYTHM`, filling on a soft curve
//! of how many pray out of how many the place needs, and drain linearly when nobody prays.

use crate::time::Every;

/// Steps per `PrayTime` unit: a full group fills one unit an update.
pub const STEP: u32 = 256;
/// Steps lost each update nobody prays: linear, as fast as a full group fills it.
pub const DRAIN: u32 = STEP;
/// The gauge only moves every 4 turns (`class_counter & 3 == 0` [exe]), the same turns for every
/// gauge at once (phase 0).
pub const RHYTHM: Every = Every::new(4);

/// Steps gained an update with `praying` counted out of `needed`: `STEP / (needed − n + 1)²`, `n`
/// capped at `needed` [exe].
pub fn gain(praying: u16, needed: u16) -> u32 {
    if praying == 0 {
        return 0;
    }
    let needed = needed.max(1) as u32;
    let n = (praying as u32).min(needed);
    let short = needed - n + 1;
    STEP / (short * short)
}

/// The gauge after one update: filling with `praying` out of `needed` (capped at `full`), or draining
/// when nobody prays. Call only on `RHYTHM`'s ticks.
pub fn step(steps: u32, praying: u16, needed: u16, full: u32) -> u32 {
    if praying == 0 {
        steps.saturating_sub(DRAIN)
    } else {
        (steps + gain(praying, needed)).min(full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_soft_curve_of_who_prays() {
        assert_eq!(gain(8, 8), STEP);
        assert_eq!(gain(12, 8), STEP, "more than needed: no faster");
        assert_eq!(gain(7, 8), STEP / 4, "25 %");
        assert_eq!(gain(6, 8), 28, "about 11 %");
        assert_eq!(gain(5, 8), 16, "about 6 %");
        assert_eq!(gain(4, 8), 10, "about 3 %");
        assert_eq!(gain(1, 8), 4, "about 1.6 %");
        assert_eq!(gain(1, 16), 1, "a lone prayer still moves");
        assert_eq!(gain(0, 8), 0);
        assert_eq!(gain(1, 0), STEP, "no count: one is enough");
    }

    #[test]
    fn fills_up_to_full_and_drains_linearly() {
        assert_eq!(step(0, 8, 8, 1000), STEP);
        assert_eq!(step(900, 8, 8, 1000), 1000, "capped");
        assert_eq!(step(1000, 0, 8, 1000), 1000 - DRAIN);
        assert_eq!(step(100, 0, 8, 1000), 0);
        assert_eq!(step(500, 0, 1, 1000), step(500, 0, 16, 1000), "the same drain whatever the place needs");
    }

    #[test]
    fn the_rhythm_fires_every_4_turns() {
        use crate::time::Tick;
        let fired = (0..12u32).filter(|&t| RHYTHM.fires(Tick::from_wire(t), 0)).collect::<Vec<_>>();
        assert_eq!(fired, [0, 4, 8]);
    }
}
