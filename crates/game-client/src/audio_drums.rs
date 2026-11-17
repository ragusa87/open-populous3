//! The drum loops chained under the music (sound.md "Music and drums"): each time the music starts a random
//! drum bank is chosen, then one loop after the other, each picked from how much the local player is fighting.
//! Pure; `audio.rs` plays them.

/// How much the local player is fighting, set by the game every turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Intensity {
    #[default]
    Calm,
    /// A local unit is attacking.
    #[allow(dead_code, reason = "set by the fight wiring to come (TODO.md Sound)")]
    Attacking,
    /// A local unit is in a challenge or a melee.
    #[allow(dead_code, reason = "set by the fight wiring to come (TODO.md Sound)")]
    Melee,
}

/// What the next loop depends on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrumState {
    last: Option<Intensity>,
    /// Loops played in a row at `Melee`.
    melee_loops: u32,
}

impl DrumState {
    /// The bank entry of the next loop, for a bank of `entries`: 0 calm (1 once first, just after a fight),
    /// 1 attacking, 2 in a melee and every 4th melee loop 3 when the bank has one. Which loop counts as the 4th is
    /// our reading.
    pub fn next(&mut self, now: Intensity, entries: usize) -> usize {
        let entry = match now {
            Intensity::Calm if matches!(self.last, Some(Intensity::Attacking | Intensity::Melee)) => 1,
            Intensity::Calm => 0,
            Intensity::Attacking => 1,
            Intensity::Melee => {
                self.melee_loops += 1;
                if self.melee_loops.is_multiple_of(4) && entries > 3 { 3 } else { 2 }
            }
        };
        if now != Intensity::Melee {
            self.melee_loops = 0;
        }
        self.last = Some(now);
        entry.min(entries.saturating_sub(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Intensity::*;

    fn run(steps: &[Intensity], entries: usize) -> Vec<usize> {
        let mut s = DrumState::default();
        steps.iter().map(|&i| s.next(i, entries)).collect()
    }

    #[test]
    fn calm_plays_the_calm_loop_after_one_bridge_loop_past_a_fight() {
        assert_eq!(run(&[Calm, Calm], 4), vec![0, 0]);
        assert_eq!(run(&[Calm, Attacking, Calm, Calm], 4), vec![0, 1, 1, 0]);
        assert_eq!(run(&[Melee, Calm, Calm], 4), vec![2, 1, 0]);
    }

    #[test]
    fn a_melee_plays_its_loop_and_every_fourth_the_extra_one() {
        assert_eq!(run(&[Melee; 8], 4), vec![2, 2, 2, 3, 2, 2, 2, 3]);
        assert_eq!(run(&[Melee; 4], 3), vec![2, 2, 2, 2], "a bank without a fourth loop");
        assert_eq!(run(&[Melee, Melee, Attacking, Melee, Melee, Melee, Melee], 4), vec![2, 2, 1, 2, 2, 2, 3], "the count restarts");
    }

    #[test]
    fn a_short_bank_never_gives_an_entry_it_lacks() {
        assert_eq!(run(&[Melee], 2), vec![1]);
        assert_eq!(run(&[Calm], 0), vec![0]);
    }
}
