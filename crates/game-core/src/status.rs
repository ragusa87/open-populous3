//! Spell statuses on a person (pop3-rev-analysis.md "Unit record": flag bits, each with its timer). Shield and
//! bloodlust run out (`Timed`, pop3-rev-analysis.md "Status markers over people", Blinking).

use crate::time::{Every, Tick};

/// A timer's step: `*_COUNT_X8` durations count steps of 8 turns.
pub const STEP: Every = Every::new(8);
/// `SHIELD_COUNT_X8` (file 180, exe 180): about 120 s.
pub const SHIELD_STEPS: u8 = 180;
/// `BLOODLUST_COUNT_X8` (file 180, exe 90).
pub const BLOODLUST_STEPS: u8 = 180;
/// The marker blinks while this many steps or fewer are left: the last 120 turns, 10 s.
pub const BLINK_STEPS: u8 = 15;
/// `SHIELD_NUM_PEOPLE` (file 6): nearest own followers shielded per cast (docs/specs/spells.md).
pub const SHIELD_NUM_PEOPLE: u32 = 6;

/// A status that runs out: the steps left, none when off.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timed(u8);

impl Timed {
    pub fn steps(n: u8) -> Self {
        Timed(n)
    }

    pub fn is_on(self) -> bool {
        self.0 > 0
    }

    pub fn steps_left(self) -> u8 {
        self.0
    }

    /// Whether its marker shows at `now` on a person at `phase`: always until its last `BLINK_STEPS`, then 2 turns
    /// on, 2 off, as in the original ("Status markers over people").
    pub fn shown(self, now: Tick, phase: u32) -> bool {
        self.is_on() && (self.0 > BLINK_STEPS || now.to_wire().wrapping_add(phase) & 2 == 0)
    }

    fn step(&mut self) {
        self.0 = self.0.saturating_sub(1);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Statuses {
    pub invisible: bool,
    pub shield: Timed,
    pub bloodlust: Timed,
    pub hypnotized: bool,
    /// A Ghost Army copy: no real unit.
    pub ghost: bool,
}

impl Statuses {
    pub fn is_none(self) -> bool {
        self == Statuses::default()
    }

    /// One turn at `now` for a person at `phase`: on its `STEP`, each running timer loses a step.
    pub fn tick(&mut self, now: Tick, phase: u32) {
        if STEP.fires(now, phase) {
            self.shield.step();
            self.bloodlust.step();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_by_default() {
        assert!(Statuses::default().is_none());
        assert!(!Statuses { shield: Timed::steps(1), ..Statuses::default() }.is_none());
    }

    #[test]
    fn a_step_every_8_turns_at_the_persons_phase_until_it_runs_out() {
        let mut s = Statuses { shield: Timed::steps(SHIELD_STEPS), bloodlust: Timed::steps(2), ..Statuses::default() };
        let run = |s: &mut Statuses, turns: std::ops::Range<u32>| turns.for_each(|n| s.tick(Tick::from_wire(n), 3));
        run(&mut s, 1..5);
        assert_eq!(s.shield.steps_left(), SHIELD_STEPS, "its first step is on turn 5 (5 + 3 = 8)");
        run(&mut s, 5..6);
        assert_eq!((s.shield.steps_left(), s.bloodlust.steps_left()), (SHIELD_STEPS - 1, 1));
        run(&mut s, 6..14);
        assert!(!s.bloodlust.is_on(), "ran out");
        let last = 5 + 8 * (SHIELD_STEPS as u32 - 1);
        run(&mut s, 14..last);
        assert_eq!(s.shield.steps_left(), 1);
        run(&mut s, last..last + 1);
        assert!(s.is_none(), "about 120 s");
    }

    #[test]
    fn blinks_2_turns_on_2_off_in_its_last_steps_each_person_at_its_phase() {
        let shown = |t: Timed, phase: u32| (0..8).map(|n| t.shown(Tick::from_wire(n), phase)).collect::<Vec<_>>();
        assert!(shown(Timed::steps(BLINK_STEPS + 1), 0).iter().all(|&s| s), "steady until the last steps");
        assert_eq!(shown(Timed::steps(BLINK_STEPS), 0), [true, true, false, false, true, true, false, false]);
        assert_eq!(shown(Timed::steps(1), 1), [true, false, false, true, true, false, false, true], "another phase");
        assert!(shown(Timed::default(), 0).iter().all(|&s| !s), "off");
    }
}
