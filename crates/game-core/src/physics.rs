//! Free motion of flung or falling units, one tick at a time (pop3-rev-analysis.md "Person physics": gravity 32
//! per turn, velocity caps and air drag applied every step). Only units off the ground move here; walking is
//! `unit`'s.

use crate::motion::{Motion, Velocity};

/// Vertical speed lost per tick, in terrain height units.
pub const GRAVITY: i16 = 32;
/// Each horizontal axis never exceeds this, in world units per tick ("Person physics": 192).
pub const HORIZONTAL_SPEED_CAP: i16 = 192;
/// Vertical speed never exceeds this either way, in terrain height units per tick ("Person physics": 512).
pub const VERTICAL_SPEED_CAP: i16 = 512;
/// Lost per tick off each horizontal axis, towards 0, while airborne ("Person physics": 2, falling and blown
/// alike); gravity alone shapes the vertical axis, so it is only capped, never dragged.
pub const AIR_DRAG: i16 = 2;

/// Air drag towards 0 on `s`, never overshooting past it.
fn drag(s: i16) -> i16 {
    if s > 0 {
        s - AIR_DRAG.min(s)
    } else {
        s + AIR_DRAG.min(-s)
    }
}

/// One tick's velocity caps, then horizontal air drag ("Person physics", both applied every step while airborne).
fn decay(v: Velocity) -> Velocity {
    Velocity {
        x: drag(v.x.clamp(-HORIZONTAL_SPEED_CAP, HORIZONTAL_SPEED_CAP)),
        y: v.y.clamp(-VERTICAL_SPEED_CAP, VERTICAL_SPEED_CAP),
        z: drag(v.z.clamp(-HORIZONTAL_SPEED_CAP, HORIZONTAL_SPEED_CAP)),
    }
}

/// A unit reaching the ground: its vertical speed then (negative falling), for the landing rules to come.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Touchdown {
    pub speed: i16,
}

/// One tick from `pos` (world units, wrapping): moves by the velocity, then gravity pulls; at or below
/// `ground` (its height under a position) the unit lands, still.
pub fn step(pos: (u16, u16), motion: Motion, ground: impl Fn((u16, u16)) -> i32) -> ((u16, u16), Motion, Option<Touchdown>) {
    let Some(height) = motion.height else { return (pos, motion, None) };
    let v = decay(motion.velocity);
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
        assert_eq!(heights, [198, 264, 298, 300, 270, 208, 114], "air drag doesn't touch the vertical axis");
        assert_eq!(touchdown, Some(Touchdown { speed: -126 }));
        assert_eq!((pos, m), ((1048, 65442), Motion::STILL), "moved every tick, wrapping, horizontal drag slowing it down each step");
    }

    #[test]
    fn velocity_caps_clamp_every_axis_every_step() {
        let m = Motion::flying(Velocity::new(300, 600, -300), 1000);
        let (_, m, _) = step((0, 0), m, flat);
        assert_eq!((m.velocity.x, m.velocity.z), (HORIZONTAL_SPEED_CAP - AIR_DRAG, -(HORIZONTAL_SPEED_CAP - AIR_DRAG)), "capped, then dragged");
        assert_eq!(m.velocity.y, VERTICAL_SPEED_CAP - GRAVITY, "capped, then gravity for the next step");
        let m = Motion::flying(Velocity::new(-300, -600, 300), 1000);
        let (_, m, _) = step((0, 0), m, flat);
        assert_eq!((m.velocity.x, m.velocity.z), (-(HORIZONTAL_SPEED_CAP - AIR_DRAG), HORIZONTAL_SPEED_CAP - AIR_DRAG));
        assert_eq!(m.velocity.y, -VERTICAL_SPEED_CAP - GRAVITY, "a downward cap keeps falling under gravity, no cap on that direction's growth here");
    }

    #[test]
    fn air_drag_settles_at_zero_without_overshooting() {
        let m = Motion::flying(Velocity::new(3, 0, -3), 1000);
        let (_, m, _) = step((0, 0), m, flat);
        assert_eq!((m.velocity.x, m.velocity.z), (1, -1));
        let (_, m, _) = step((0, 0), m, flat);
        assert_eq!((m.velocity.x, m.velocity.z), (0, 0), "down to zero, not past it");
        let (_, m, _) = step((0, 0), m, flat);
        assert_eq!((m.velocity.x, m.velocity.z), (0, 0), "stays there");
    }

    #[test]
    fn lands_on_higher_ground_in_its_way() {
        let wall = |(x, _): (u16, u16)| if x >= 200 { 500 } else { 0 };
        let (pos, m, down) = step((100, 0), Motion::flying(Velocity::new(150, 0, 0), 300), wall);
        assert_eq!((pos, m, down), ((248, 0), Motion::STILL, Some(Touchdown { speed: 0 })), "148 after drag, still past the wall's x");
    }
}
