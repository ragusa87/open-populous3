//! Spell statuses on a person (pop3-rev-analysis.md "Unit record": flag bits, each with its timer).

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Statuses {
    pub invisible: bool,
    pub shielded: bool,
    pub bloodlust: bool,
    pub hypnotized: bool,
    /// A Ghost Army copy: no real unit.
    pub ghost: bool,
}

impl Statuses {
    pub fn is_none(self) -> bool {
        self == Statuses::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_by_default() {
        assert!(Statuses::default().is_none());
        assert!(!Statuses { shielded: true, ..Statuses::default() }.is_none());
    }
}
