//! Fighting numbers (docs/specs/combat.md): a blow's strength and how long each move lasts.

use crate::unit::UnitKind;

/// A blow never hits softer than this, however weak the attacker ("Fight" [exe]).
pub const MIN_STRENGTH: u16 = 32;
/// A bloodlusted attacker's blows hit this many times harder (constant.dat value).
pub const BLOODLUST_STRENGTH: u16 = 3;

/// The three blows of a melee.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blow {
    /// Both hurt; a follower struck is knocked back.
    Knock,
    /// Both hurt.
    Trade,
    /// Only the one struck is hurt.
    Clean,
}

/// What a fighter is doing, for how long it lasts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Strike(Blow),
    /// Struck; `knocked`: cut short by a knock back.
    Reel { knocked: bool },
    /// The challenger's shove before a melee.
    Shove,
}

/// How hard `kind` hits at `life` of `max`: its full strength scaled by its life, never under `MIN_STRENGTH`.
pub fn strength(kind: UnitKind, life: u16, max: u16, bloodlusted: bool) -> u16 {
    let scaled = kind.stats().fight_damage as u32 * life as u32 / max.max(1) as u32;
    let base = (scaled as u16).max(MIN_STRENGTH);
    if bloodlusted { base.saturating_mul(BLOODLUST_STRENGTH) } else { base }
}

/// Turns `kind` spends on `mv` ("Combat" [exe, measured]): the shaman is quicker, the firewarrior twice as slow.
pub fn move_turns(kind: UnitKind, mv: Move) -> u32 {
    let shaman = kind == UnitKind::Shaman;
    let turns = match mv {
        Move::Strike(Blow::Knock) | Move::Strike(Blow::Trade) | Move::Shove if shaman => 4,
        Move::Strike(Blow::Clean) if shaman => 5,
        Move::Strike(Blow::Knock) | Move::Shove => 6,
        Move::Strike(Blow::Trade) | Move::Strike(Blow::Clean) => 7,
        Move::Reel { knocked: true } => 4,
        Move::Reel { knocked: false } => 7,
    };
    turns * kind.stats().pace
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strength_falls_with_life_but_not_under_the_floor() {
        assert_eq!(strength(UnitKind::Brave, 1000, 1000, false), 60);
        assert_eq!(strength(UnitKind::Brave, 500, 1000, false), MIN_STRENGTH);
        assert_eq!(strength(UnitKind::Warrior, 1800, 1800, false), 360);
        assert_eq!(strength(UnitKind::Warrior, 900, 1800, false), 180);
        assert_eq!(strength(UnitKind::Warrior, 900, 1800, true), 540);
    }

    #[test]
    fn move_lengths() {
        let all = [Move::Strike(Blow::Knock), Move::Strike(Blow::Trade), Move::Strike(Blow::Clean), Move::Reel { knocked: false }, Move::Reel { knocked: true }, Move::Shove];
        assert_eq!(all.map(|m| move_turns(UnitKind::Brave, m)), [6, 7, 7, 7, 4, 6]);
        assert_eq!(all.map(|m| move_turns(UnitKind::Shaman, m)), [4, 4, 5, 7, 4, 4]);
        assert_eq!(move_turns(UnitKind::Firewarrior, Move::Strike(Blow::Trade)), 14);
    }
}
