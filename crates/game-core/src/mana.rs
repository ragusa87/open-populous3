//! A player's mana (pop3-rev-analysis.md "Mana rules"): what is gained or spent during a turn waits in
//! `incoming` (`add`, the original's `add_mana`); once per turn `tick` adds the income, charges the spells
//! first and banks the rest. Only `tick` changes the bank and the spell charges.

use crate::spell_book::SpellBook;
use crate::time::{Every, Tick};

/// An amount of mana.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Mana(pub u32);

impl Mana {
    pub const ZERO: Mana = Mana(0);

    pub fn saturating_sub(self, other: Mana) -> Mana {
        Mana(self.0.saturating_sub(other.0))
    }
}

impl std::ops::Add for Mana {
    type Output = Mana;
    fn add(self, other: Mana) -> Mana {
        Mana(self.0.saturating_add(other.0))
    }
}

impl std::ops::AddAssign for Mana {
    fn add_assign(&mut self, other: Mana) {
        *self = *self + other;
    }
}

/// `MAX_MANA` (constant.dat).
pub const MAX_MANA: Mana = Mana(1_000_000);
/// `START_MANA` (constant.dat).
pub const START_MANA: Mana = Mana(30_000);
/// Income rhythm: `MANA_UPDATE_FREQ` 15, every 16 turns.
pub const MANA_EVERY: Every = Every::new(16);
/// Income per update, a guess until income from people (huts-and-training.md rule 7): about what five
/// recharging spells got before.
pub const INCOME: Mana = Mana(560);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManaPool {
    pub bank: Mana,
    /// Gained or spent this turn, not distributed yet.
    pub incoming: Mana,
    /// The last income.
    pub received: Mana,
    /// Charged into spells since the last income.
    pub used: Mana,
}

impl Default for ManaPool {
    fn default() -> Self {
        ManaPool { bank: START_MANA, incoming: Mana::ZERO, received: Mana::ZERO, used: Mana::ZERO }
    }
}

impl ManaPool {
    /// Mana gained during the turn (`add_mana` 0x41A4F0): settled by the next `tick`.
    pub fn add(&mut self, amount: Mana) {
        self.incoming += amount;
    }

    /// The turn's update (0x41A550): the income on update turns, then `distribute`.
    pub fn tick(&mut self, now: Tick, book: &mut SpellBook) {
        if MANA_EVERY.fires(now, 0) {
            self.received = INCOME;
            self.used = Mana::ZERO;
            self.incoming += INCOME;
        }
        self.distribute(book);
    }

    /// `incoming` charges the spells first (`SpellBook::charge`), the rest goes to the bank (0x41A590).
    pub fn distribute(&mut self, book: &mut SpellBook) {
        let left = book.charge(self.incoming);
        self.used += self.incoming.saturating_sub(left);
        self.bank = (self.bank + left).min(MAX_MANA);
        self.incoming = Mana::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spell_book::{Availability, SpellKind};

    fn book(known: &[SpellKind]) -> SpellBook {
        let mut b = SpellBook::new();
        for &k in known {
            b.set(k, Availability::Known);
        }
        b
    }

    #[test]
    fn spells_charge_first_and_the_rest_is_banked() {
        let mut b = book(&[SpellKind::Blast]);
        let mut pool = ManaPool { bank: Mana::ZERO, ..ManaPool::default() };
        pool.add(Mana(1000));
        pool.distribute(&mut b);
        assert_eq!(b.slot(SpellKind::Blast).unwrap().charges, 4, "4 charges of 40");
        assert_eq!((pool.used, pool.bank, pool.incoming), (Mana(160), Mana(840), Mana::ZERO));
    }

    #[test]
    fn the_bank_is_capped() {
        let mut pool = ManaPool { bank: Mana(999_990), ..ManaPool::default() };
        pool.add(Mana(100));
        pool.distribute(&mut SpellBook::new());
        assert_eq!(pool.bank, MAX_MANA);
    }

    #[test]
    fn added_mana_waits_for_the_tick() {
        let mut b = book(&[SpellKind::Blast]);
        let mut pool = ManaPool::default();
        pool.add(Mana(40));
        assert_eq!(b.slot(SpellKind::Blast).unwrap().charges, 0);
        pool.tick(Tick::ZERO.next(), &mut b);
        assert_eq!(b.slot(SpellKind::Blast).unwrap().charges, 1);
    }

    #[test]
    fn income_comes_every_16_turns() {
        let mut b = SpellBook::new();
        let mut pool = ManaPool { bank: Mana::ZERO, ..ManaPool::default() };
        let mut now = Tick::ZERO;
        for _ in 0..32 {
            now = now.next();
            pool.tick(now, &mut b);
        }
        assert_eq!((pool.bank, pool.received), (Mana(2 * INCOME.0), INCOME));
    }

    #[test]
    fn used_counts_since_the_last_income() {
        let mut b = book(&[SpellKind::Swarm]);
        let mut pool = ManaPool { bank: Mana::ZERO, ..ManaPool::default() };
        pool.tick(Tick::ZERO, &mut b);
        assert_eq!((pool.used, pool.bank), (Mana(480), Mana(80)), "4 swarm charges of 120");
        pool.tick(Tick::ZERO.next(), &mut b);
        assert_eq!(pool.used, Mana(480), "kept between incomes");
    }
}
