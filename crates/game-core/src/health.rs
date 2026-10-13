//! Hit points of a unit (later vehicles, buildings): an integer count up to a maximum.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Health {
    current: u16,
    max: u16,
}

impl Health {
    /// Full health.
    pub fn full(max: u16) -> Self {
        Health { current: max, max }
    }

    /// `current` capped at `max`.
    pub fn new(current: u16, max: u16) -> Self {
        Health { current: current.min(max), max }
    }

    pub fn current(self) -> u16 {
        self.current
    }

    pub fn max(self) -> u16 {
        self.max
    }

    pub fn is_full(self) -> bool {
        self.current >= self.max
    }

    pub fn is_zero(self) -> bool {
        self.current == 0
    }

    /// A quarter of the maximum or less: the original's low-health flag ("Status markers over people"), shown over the person.
    pub fn is_low(self) -> bool {
        self.max / 4 >= self.current
    }

    /// Loses `points` (down to 0); true when that leaves none.
    pub fn damage(&mut self, points: u16) -> bool {
        self.current = self.current.saturating_sub(points);
        self.is_zero()
    }

    /// Gains `points`, up to the maximum.
    pub fn heal(&mut self, points: u16) {
        self.current = self.current.saturating_add(points).min(self.max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_at_a_quarter_or_less() {
        assert!(!Health::new(251, 1000).is_low());
        assert!(Health::new(250, 1000).is_low(), "a quarter exactly");
        assert!(Health::new(0, 1000).is_low());
        assert!(!Health::full(1000).is_low());
    }

    #[test]
    fn damage_stops_at_zero() {
        let mut h = Health::full(100);
        assert!(!h.damage(30));
        assert_eq!(h.current(), 70);
        assert!(h.damage(500));
        assert_eq!((h.current(), h.is_zero()), (0, true));
    }

    #[test]
    fn heal_stops_at_max() {
        let mut h = Health::new(95, 100);
        h.heal(3);
        assert!(!h.is_full());
        h.heal(10);
        assert_eq!((h.current(), h.is_full()), (100, true));
        assert_eq!(Health::new(300, 100).current(), 100);
    }
}
