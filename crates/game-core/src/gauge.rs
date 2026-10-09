//! Prayer gauges (docs/specs/worship.md): counted in steps, `STEP` per unit of the level's
//! `PrayTime`. They fill on a soft curve of how many pray out of how many the place needs, and drain
//! linearly when nobody prays.

/// Steps per `PrayTime` unit: a full group fills one unit a tick.
pub const STEP: u32 = 256;
/// Steps lost each tick nobody prays: linear, as fast as a full group fills it.
pub const DRAIN: u32 = STEP;

/// Steps gained a tick with `praying` counted out of `needed`: `STEP * (n / needed)²`, `n` capped at
/// `needed`.
pub fn gain(praying: u16, needed: u16) -> u32 {
    let needed = needed.max(1) as u32;
    let n = (praying as u32).min(needed);
    STEP * n * n / (needed * needed)
}

/// The gauge after one tick: filling with `praying` out of `needed` (capped at `full`), or draining
/// when nobody prays.
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
        assert_eq!(gain(4, 8), STEP / 4);
        assert_eq!(gain(6, 8), 144, "about 56 %");
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
}
