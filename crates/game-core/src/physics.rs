//! Free motion of flung or falling units, one tick at a time (pop3-rev-analysis.md "Shared person and building
//! states": gravity 32 per turn). Only units off the ground move here; walking is `unit`'s.

use crate::motion::{Motion, Velocity};

/// Vertical speed lost per tick, in terrain height units.
pub const GRAVITY: i16 = 32;

/// A unit reaching the ground: its vertical speed then (negative falling), for the landing rules to come.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Touchdown {
    pub speed: i16,
}

/// One tick from `pos` (world units, wrapping): moves by the velocity, then gravity pulls; at or below
/// `ground` (its height under a position) the unit lands, still.
pub fn step(pos: (u16, u16), motion: Motion, ground: impl Fn((u16, u16)) -> i32) -> ((u16, u16), Motion, Option<Touchdown>) {
    let Some(height) = motion.height else { return (pos, motion, None) };
    let v = motion.velocity;
    let pos = (pos.0.wrapping_add_signed(v.x), pos.1.wrapping_add_signed(v.z));
    let height = height + v.y as i32;
    if height <= ground(pos) {
        return (pos, Motion::STILL, Some(Touchdown { speed: v.y }));
    }
    (pos, Motion::flying(Velocity { y: v.y.saturating_sub(GRAVITY), ..v }, height), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(_: (u16, u16)) -> i32 {
        100
    }

    #[test]
    fn on_the_ground_nothing_moves() {
        let m = Motion { velocity: Velocity::new(5, 0, 0), height: None };
        assert_eq!(step((10, 10), m, flat), ((10, 10), m, None));
    }

    #[test]
    fn rises_slows_falls_and_lands() {
        let (mut pos, mut m) = ((0, 65530), Motion::flying(Velocity::new(140, 98, -20), 100));
        let mut heights = Vec::new();
        let touchdown = loop {
            let (p, next, down) = step(pos, m, flat);
            (pos, m) = (p, next);
            if down.is_some() {
                break down;
            }
            heights.push(m.height.unwrap());
        };
        assert_eq!(heights, [198, 264, 298, 300, 270, 208, 114]);
        assert_eq!(touchdown, Some(Touchdown { speed: -126 }));
        assert_eq!((pos, m), ((8 * 140, 65530 - 8 * 20), Motion::STILL), "moved every tick, wrapping");
    }

    #[test]
    fn lands_on_higher_ground_in_its_way() {
        let wall = |(x, _): (u16, u16)| if x >= 200 { 500 } else { 0 };
        let (pos, m, down) = step((100, 0), Motion::flying(Velocity::new(150, 0, 0), 300), wall);
        assert_eq!((pos, m, down), ((250, 0), Motion::STILL, Some(Touchdown { speed: 0 })));
    }
}
