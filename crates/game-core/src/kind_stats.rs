//! What each kind of person is made of, in one table (docs/specs/combat.md): life, healing, and later its fighting
//! numbers. Values from pop3-rev-analysis.md, each field saying where.

use crate::unit::UnitKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KindStats {
    /// Full life (constant.dat values; wildmen have none, a brave's stands in).
    pub life: u16,
    /// Life regained every `HEAL_EVERY` turns, None for those who never heal ("Health and healing" [exe, measured]).
    pub heal: Option<u16>,
}

const SHAMAN: KindStats = KindStats { life: 2000, heal: Some(32) };
const BRAVE: KindStats = KindStats { life: 1000, heal: Some(4) };
const WARRIOR: KindStats = KindStats { life: 1800, heal: Some(6) };
const PREACHER: KindStats = KindStats { life: 1100, heal: Some(4) };
const SPY: KindStats = KindStats { life: 600, heal: Some(4) };
const FIREWARRIOR: KindStats = KindStats { life: 700, heal: Some(6) };
const WILDMAN: KindStats = KindStats { life: 1000, heal: None };

impl UnitKind {
    pub const fn stats(self) -> &'static KindStats {
        match self {
            UnitKind::Shaman => &SHAMAN,
            UnitKind::Brave => &BRAVE,
            UnitKind::Warrior => &WARRIOR,
            UnitKind::Preacher => &PREACHER,
            UnitKind::Spy => &SPY,
            UnitKind::Firewarrior => &FIREWARRIOR,
            UnitKind::Wildman => &WILDMAN,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_as_traced() {
        let rows: Vec<_> = UnitKind::ALL.iter().map(|k| (k.name(), k.stats().life, k.stats().heal)).collect();
        assert_eq!(
            rows,
            [
                ("Shaman", 2000, Some(32)),
                ("Brave", 1000, Some(4)),
                ("Warrior", 1800, Some(6)),
                ("Preacher", 1100, Some(4)),
                ("Spy", 600, Some(4)),
                ("Firewarrior", 700, Some(6)),
                ("Wildman", 1000, None),
            ]
        );
    }
}
