//! Followers kneeling to their shaman (`game_core::kneel`, docs/specs/units.md "Worshipping the shaman"): drawn in
//! the prayer pose, its arms going up and down once, then held still 4-11 turns, again.

use game_core::map::Lcg;
use game_core::time::Tick;
use game_core::unit::Unit;

/// Turns each prayer frame shows (the Pray pose's 4 frames a second, 12 turns a second).
const TURNS_PER_FRAME: u32 = 3;
/// The still hold between two arm cycles: 4-11 turns ("Worshipping the shaman"; the original draws it from a
/// second, cosmetic generator).
const HOLD_MIN: u32 = 4;
const HOLD_SPAN: u32 = 8;

/// The facing to draw `unit` praying in, while it kneels to its shaman.
pub fn worship_facing(unit: &Unit) -> Option<u8> {
    unit.kneeling().map(|_| unit.facing)
}

/// The prayer frame (of `frames`) `turns` after a unit of `id` knelt: each arm cycle, then a hold on the first
/// frame whose length is seeded by `id` and the cycle.
pub fn prayer_frame(frames: usize, turns: u32, id: u32) -> usize {
    if frames == 0 {
        return 0;
    }
    let cycle = frames as u32 * TURNS_PER_FRAME;
    let (mut left, mut n) = (turns, 0u32);
    loop {
        if left < cycle {
            return (left / TURNS_PER_FRAME) as usize;
        }
        left -= cycle;
        let hold = HOLD_MIN + Lcg(id.wrapping_mul(2_654_435_761) ^ n.wrapping_mul(0x9E37_79B9)).next_u32() % HOLD_SPAN;
        if left < hold {
            return 0;
        }
        left -= hold;
        n += 1;
    }
}

/// Turns since `unit` knelt at `now`, if it kneels.
pub fn kneeling_for(unit: &Unit, now: Tick) -> Option<u32> {
    unit.kneeling().map(|k| now.since(k.since).max(0) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_arm_cycle_then_a_still_hold_of_4_to_11_turns() {
        let frames = 4;
        let cycle: Vec<usize> = (0..12).map(|t| prayer_frame(frames, t, 7)).collect();
        assert_eq!(cycle, [0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3]);
        let held = (12..40).take_while(|&t| prayer_frame(frames, t, 7) == 0).count() as u32 - TURNS_PER_FRAME;
        assert!((HOLD_MIN..HOLD_MIN + HOLD_SPAN).contains(&held), "held {held} turns, then the next cycle's first frame");
        assert_eq!(prayer_frame(frames, 12 + held + 3, 7), 1, "the next cycle");
        assert_eq!(prayer_frame(0, 5, 7), 0);
    }
}
