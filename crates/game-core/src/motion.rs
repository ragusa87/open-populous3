//! Free motion of a unit off its walk (flung by a spell, falling), like the original's person velocity
//! (pop3-rev-analysis.md "Unit record": x, vertical, z).

/// Per tick: `x`, `z` in world units (512 per cell), `y` in terrain height units, up positive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Velocity {
    pub x: i16,
    pub y: i16,
    pub z: i16,
}

impl Velocity {
    pub const ZERO: Velocity = Velocity { x: 0, y: 0, z: 0 };

    pub fn new(x: i16, y: i16, z: i16) -> Self {
        Velocity { x, y, z }
    }

    pub fn is_zero(self) -> bool {
        self == Velocity::ZERO
    }
}

/// A unit's free motion: its velocity and its height above the ground.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Motion {
    pub velocity: Velocity,
    /// Above the ground under it, in terrain height units (0 = on the ground).
    pub lift: u16,
}

impl Motion {
    pub const STILL: Motion = Motion { velocity: Velocity::ZERO, lift: 0 };

    pub fn new(velocity: Velocity, lift: u16) -> Self {
        Motion { velocity, lift }
    }

    pub fn airborne(self) -> bool {
        self.lift > 0
    }

    /// On the ground and not moving.
    pub fn is_still(self) -> bool {
        !self.airborne() && self.velocity.is_zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_by_default() {
        assert!(Velocity::default().is_zero());
        assert!(!Velocity::new(0, -1, 0).is_zero());
    }

    #[test]
    fn still_only_on_the_ground_and_not_moving() {
        assert!(Motion::default().is_still());
        assert!(!Motion::new(Velocity::ZERO, 3).is_still() && Motion::new(Velocity::ZERO, 3).airborne());
        assert!(!Motion::new(Velocity::new(2, 0, 0), 0).is_still());
    }
}
