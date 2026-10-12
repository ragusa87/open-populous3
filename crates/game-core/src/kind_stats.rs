//! What each kind of person is made of, in one table (docs/specs/combat.md): life, healing, and later its fighting
//! numbers. Values from pop3-rev-analysis.md, each field saying where.

use crate::time::Every;
use crate::unit::UnitKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KindStats {
    /// Full life (constant.dat values; wildmen have none, a brave's stands in).
    pub life: u16,
    /// Life regained every `HEAL_EVERY` turns, None for those who never heal ("Health and healing" [exe, measured]).
    pub heal: Option<u16>,
    /// A blow's strength at full life (constant.dat values, "Combat").
    pub fight_damage: u16,
    /// Who gives way when a melee is full: a lower rank is replaced ("Fight" [exe]).
    pub rank: u8,
    /// Turns per animation frame in a fight: the firewarrior's moves last twice as long ("Combat" [exe, measured]).
    pub pace: u32,
    /// Takes part in fights at all: never a wildman.
    pub fights: bool,
    /// Joins a melee already going on: never a firewarrior ("Fight" [exe]); it still fights when challenged.
    pub joins_melees: bool,
    /// Attacks by throwing fire from afar (`crate::firebolt`): the firewarrior.
    pub throws: bool,
    /// Wear it adds to a building per turn of hacking ("Attacking a building" [exe]).
    pub sabotage: u16,
    /// How far it looks for enemies, cells, with no order, and while working ("Auto-attack" [exe]).
    pub sight: u8,
    pub sight_at_work: u8,
    /// Its rhythm of looking ("Auto-attack" [exe]).
    pub look_every: Every,
}

const SHAMAN: KindStats = KindStats { life: 2000, heal: Some(32), fight_damage: 60, rank: 2, pace: 1, fights: true, joins_melees: true, throws: false, sabotage: 10, sight: 3, sight_at_work: 1, look_every: Every::new(4) };
const BRAVE: KindStats = KindStats { life: 1000, heal: Some(4), fight_damage: 60, rank: 1, pace: 1, fights: true, joins_melees: true, throws: false, sabotage: 7, sight: 5, sight_at_work: 1, look_every: Every::new(4) };
const WARRIOR: KindStats = KindStats { life: 1800, heal: Some(6), fight_damage: 360, rank: 2, pace: 1, fights: true, joins_melees: true, throws: false, sabotage: 12, sight: 5, sight_at_work: 1, look_every: Every::new(4) };
const PREACHER: KindStats = KindStats { life: 1100, heal: Some(4), fight_damage: 60, rank: 1, pace: 1, fights: true, joins_melees: true, throws: false, sabotage: 7, sight: 3, sight_at_work: 1, look_every: Every::new(2) };
const SPY: KindStats = KindStats { life: 600, heal: Some(4), fight_damage: 60, rank: 1, pace: 1, fights: true, joins_melees: true, throws: false, sabotage: 7, sight: 0, sight_at_work: 0, look_every: Every::new(4) };
const FIREWARRIOR: KindStats = KindStats { life: 700, heal: Some(6), fight_damage: 60, rank: 3, pace: 2, fights: true, joins_melees: false, throws: true, sabotage: 16, sight: 11, sight_at_work: 5, look_every: Every::new(4) };
const WILDMAN: KindStats = KindStats { life: 1000, heal: None, fight_damage: 0, rank: 0, pace: 1, fights: false, joins_melees: false, throws: false, sabotage: 0, sight: 0, sight_at_work: 0, look_every: Every::new(4) };

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
    fn fighting_numbers() {
        let rows: Vec<_> = UnitKind::ALL.iter().map(|k| (k.stats().fight_damage, k.stats().rank, k.stats().pace, k.stats().fights, k.stats().joins_melees)).collect();
        assert_eq!(rows, [(60, 2, 1, true, true), (60, 1, 1, true, true), (360, 2, 1, true, true), (60, 1, 1, true, true), (60, 1, 1, true, true), (60, 3, 2, true, false), (0, 0, 1, false, false)]);
    }

    #[test]
    fn wear_dealt_to_buildings() {
        assert_eq!(UnitKind::ALL.map(|k| k.stats().sabotage), [10, 7, 12, 7, 7, 16, 0]);
    }

    #[test]
    fn how_far_each_looks() {
        assert_eq!(UnitKind::ALL.map(|k| (k.stats().sight, k.stats().sight_at_work, k.stats().look_every.period())), [(3, 1, 4), (5, 1, 4), (5, 1, 4), (3, 1, 2), (0, 0, 4), (11, 5, 4), (0, 0, 4)]);
    }

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
