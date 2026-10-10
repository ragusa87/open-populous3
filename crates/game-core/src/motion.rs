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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_by_default() {
        assert!(Velocity::default().is_zero());
        assert!(!Velocity::new(0, -1, 0).is_zero());
    }
}
