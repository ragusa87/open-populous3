//! Simulation time. One tick is one turn of the original game (`pop3-rev-analysis.md` "Turns and timing"),
//! 12 per second. `Tick` is a moment (the map's counter), `Ticks` a length; durations are built from the
//! original's turns (`Ticks::new`) or from our own guesses in seconds (`Ticks::secs`, `Ticks::millis`),
//! never from frame time. The counter wraps after about 11 years: moments are compared by subtracting them.

use std::ops::{Add, Mul, Sub};

pub const TICKS_PER_SECOND: u32 = 12;

/// A moment: the simulation's tick counter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Tick(u32);

/// A length of time, in ticks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ticks(u32);

impl Tick {
    pub const ZERO: Tick = Tick(0);

    pub fn next(self) -> Tick {
        Tick(self.0.wrapping_add(1))
    }

    /// For the wire codec and replays only.
    pub fn from_wire(n: u32) -> Tick {
        Tick(n)
    }

    pub fn to_wire(self) -> u32 {
        self.0
    }
}

impl Ticks {
    pub const ZERO: Ticks = Ticks(0);

    /// A value from the original, in its turns.
    pub const fn new(n: u32) -> Ticks {
        Ticks(n)
    }

    /// Our own guess, in whole seconds.
    pub const fn secs(s: u32) -> Ticks {
        Ticks(s * TICKS_PER_SECOND)
    }

    /// Our own guess, in milliseconds, to the nearest tick.
    pub const fn millis(ms: u32) -> Ticks {
        Ticks((ms * TICKS_PER_SECOND + 500) / 1000)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

impl Add<Ticks> for Tick {
    type Output = Tick;
    fn add(self, d: Ticks) -> Tick {
        Tick(self.0.wrapping_add(d.0))
    }
}

impl Sub for Tick {
    type Output = Ticks;
    fn sub(self, earlier: Tick) -> Ticks {
        Ticks(self.0.wrapping_sub(earlier.0))
    }
}

impl Add for Ticks {
    type Output = Ticks;
    fn add(self, o: Ticks) -> Ticks {
        Ticks(self.0 + o.0)
    }
}

impl Mul<u32> for Ticks {
    type Output = Ticks;
    fn mul(self, k: u32) -> Ticks {
        Ticks(self.0 * k)
    }
}

/// Counts down a length; keeps it for progress (animations, bars).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Countdown {
    left: Ticks,
    total: Ticks,
}

impl Countdown {
    pub fn new(total: Ticks) -> Countdown {
        Countdown { left: total, total }
    }

    /// Part way through: `left` of `total` to go.
    pub fn with_left(total: Ticks, left: Ticks) -> Countdown {
        Countdown { left: left.min(total), total }
    }

    /// One tick; true on the tick it ends (the `total`-th call), and on every call after.
    pub fn tick(&mut self) -> bool {
        self.left.0 = self.left.0.saturating_sub(1);
        self.left.0 == 0
    }

    pub fn left(self) -> Ticks {
        self.left
    }

    pub fn done(self) -> Ticks {
        Ticks(self.total.0 - self.left.0)
    }

    pub fn total(self) -> Ticks {
        self.total
    }
}

/// A power-of-two rhythm, like the original's `turn & 15 == 0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Every(u32);

impl Every {
    pub const fn new(period: u32) -> Every {
        assert!(period.is_power_of_two(), "a rhythm is a power of two");
        Every(period - 1)
    }

    /// Whether it fires at `tick` for something offset by `phase` (each unit its own, to spread the work).
    pub fn fires(self, tick: Tick, phase: u32) -> bool {
        tick.0.wrapping_add(phase) & self.0 == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_from_turns_seconds_and_millis() {
        assert_eq!(Ticks::new(20).get(), 20);
        assert_eq!(Ticks::secs(5).get(), 60);
        assert_eq!(Ticks::millis(800).get(), 10);
        assert_eq!(Ticks::millis(250).get(), 3);
        assert_eq!(Ticks::secs(1) * 2 + Ticks::new(1), Ticks::new(25));
    }

    #[test]
    fn a_countdown_ends_on_its_last_tick() {
        let mut c = Countdown::new(Ticks::new(3));
        assert!(!c.tick());
        assert_eq!((c.done(), c.left(), c.total()), (Ticks::new(1), Ticks::new(2), Ticks::new(3)));
        assert!(!c.tick());
        assert!(c.tick());
        assert!(c.tick(), "stays ended");
        assert_eq!(c.done(), Ticks::new(3));
        assert_eq!(Countdown::with_left(Ticks::new(3), Ticks::new(1)).done(), Ticks::new(2));
    }

    #[test]
    fn a_rhythm_fires_once_per_period_at_its_phase() {
        let every = Every::new(4);
        let fired: Vec<u32> = (0..12).filter(|&t| every.fires(Tick(t), 0)).collect();
        assert_eq!(fired, [0, 4, 8]);
        let fired: Vec<u32> = (0..12).filter(|&t| every.fires(Tick(t), 1)).collect();
        assert_eq!(fired, [3, 7, 11]);
    }

    #[test]
    fn moments_survive_the_wrap() {
        let before = Tick(u32::MAX - 1);
        let after = before + Ticks::new(5);
        assert_eq!(after, Tick(3));
        assert_eq!(after - before, Ticks::new(5));
        assert_eq!(Tick(u32::MAX).next(), Tick::ZERO);
        let every = Every::new(16);
        let fired = (0..64u32).map(|i| Tick(u32::MAX - 31).0.wrapping_add(i)).filter(|&t| every.fires(Tick(t), 0)).count();
        assert_eq!(fired, 4, "the rhythm runs on across the wrap");
    }
}
