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

    /// A new person's full life, rolled from its kind's `life` (pop3-rev-analysis.md "Starting and maximum life"
    /// [exe, measured]): `life × 3/4 + roll % (life / 4)` for a kind that `heals`, the full `life` for one that
    /// does not (a wildman, an angel).
    pub fn at_birth(life: u16, heals: bool, roll: u32) -> Self {
        if !heals {
            return Health::full(life);
        }
        let quarter = (life / 4) as u32;
        let max = life as u32 * 3 / 4 + if quarter == 0 { 0 } else { roll % quarter };
        Health::full(max as u16)
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

    /// Gains `points` with no clamp (pop3-rev-analysis.md "Health and healing": the original adds the heal while
    /// below max with no clamp, so one heal tick can leave `current` up to one heal's worth over `max`; the next
    /// tick then takes the already-full branch and heals no further).
    pub fn heal_past_max(&mut self, points: u16) {
        self.current = self.current.saturating_add(points);
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
    fn a_new_person_s_life_is_rolled_in_the_top_quarter_unless_it_never_heals() {
        assert_eq!(Health::at_birth(1000, true, 0), Health::full(750));
        assert_eq!(Health::at_birth(1000, true, 249), Health::full(999));
        assert_eq!(Health::at_birth(1000, true, 250), Health::full(750), "wraps");
        assert_eq!(Health::at_birth(2000, true, 7), Health::full(1507));
        assert_eq!(Health::at_birth(32, false, 5), Health::full(32));
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

    #[test]
    fn heal_past_max_can_overshoot_by_one_heal() {
        let mut h = Health::new(95, 100);
        h.heal_past_max(10);
        assert_eq!((h.current(), h.is_full()), (105, true), "over max by up to one heal's worth");
    }
}
