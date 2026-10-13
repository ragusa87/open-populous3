//! A player's mana (pop3-rev-analysis.md "Mana rules"): what is gained or spent during a turn waits in
//! `incoming` (`add`); once per turn `tick` adds the income, feeds the training
//! units (`crate::production`), charges the spells and banks the rest. Only `tick` changes the bank, the
//! spell charges and the training units' stores.

use crate::production::{self, Production};
use crate::spell_book::{SpellBook, SpellSlot};
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
    /// Mana gained during the turn ("Spending and other sources"): settled by the next `tick`.
    pub fn add(&mut self, amount: Mana) {
        self.incoming += amount;
    }

    /// The turn's update ("Every turn, per tribe"): the income on update turns, then `distribute`.
    pub fn tick(&mut self, now: Tick, book: &mut SpellBook, stores: &mut [&mut Production]) {
        if MANA_EVERY.fires(now, 0) {
            self.received = INCOME;
            self.used = Mana::ZERO;
            self.incoming += INCOME;
        }
        self.distribute(book, stores);
    }

    /// The distribution ("Distribution"): the training units' `stores` making nothing give some back; those
    /// making something take their share of half of `incoming` (`production::charge`); the rest charges
    /// the spells (`SpellBook::charge`); with every spell full, the stores take another share of what is
    /// left, and the rest goes to the bank.
    pub fn distribute(&mut self, book: &mut SpellBook, stores: &mut [&mut Production]) {
        for store in stores.iter_mut() {
            self.incoming += store.refund();
        }
        let half = Mana(self.incoming.0 / 2);
        let rest = self.incoming.saturating_sub(half) + production::charge(stores, half);
        let mut left = book.charge(rest);
        self.used += rest.saturating_sub(left);
        if !book.slots.iter().any(SpellSlot::takes_mana) {
            left = production::top_up(stores, left);
        }
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
        pool.distribute(&mut b, &mut []);
        assert_eq!(b.slot(SpellKind::Blast).unwrap().charges, 4, "4 charges of 40");
        assert_eq!((pool.used, pool.bank, pool.incoming), (Mana(160), Mana(840), Mana::ZERO));
    }

    #[test]
    fn training_units_take_their_share_before_the_spells() {
        let mut b = book(&[SpellKind::Swarm]);
        let mut pool = ManaPool { bank: Mana::ZERO, ..ManaPool::default() };
        let mut warrior = Production { stored: 0, cost: 3500, short: false };
        pool.add(Mana(560));
        pool.distribute(&mut b, &mut [&mut warrior]);
        assert_eq!(warrior.stored, 109, "a 32nd of its cost, out of half of 560");
        assert_eq!((pool.used, pool.bank), (Mana(451), Mana::ZERO), "the rest charges the spell, still not full");
    }

    #[test]
    fn with_every_spell_full_the_store_takes_a_second_share() {
        let mut pool = ManaPool { bank: Mana::ZERO, ..ManaPool::default() };
        let mut warrior = Production { stored: 0, cost: 3500, short: false };
        pool.add(Mana(560));
        pool.distribute(&mut SpellBook::new(), &mut [&mut warrior]);
        assert_eq!((warrior.stored, pool.bank), (218, Mana(342)));
    }

    #[test]
    fn a_store_making_nothing_goes_back_to_the_bank() {
        let mut pool = ManaPool { bank: Mana::ZERO, ..ManaPool::default() };
        let mut done = Production { stored: 150, cost: 0, short: false };
        pool.distribute(&mut SpellBook::new(), &mut [&mut done]);
        assert_eq!((done.stored, pool.bank), (50, Mana(100)));
    }

    #[test]
    fn the_bank_is_capped() {
        let mut pool = ManaPool { bank: Mana(999_990), ..ManaPool::default() };
        pool.add(Mana(100));
        pool.distribute(&mut SpellBook::new(), &mut []);
        assert_eq!(pool.bank, MAX_MANA);
    }

    #[test]
    fn added_mana_waits_for_the_tick() {
        let mut b = book(&[SpellKind::Blast]);
        let mut pool = ManaPool::default();
        pool.add(Mana(40));
        assert_eq!(b.slot(SpellKind::Blast).unwrap().charges, 0);
        pool.tick(Tick::ZERO.next(), &mut b, &mut []);
        assert_eq!(b.slot(SpellKind::Blast).unwrap().charges, 1);
    }

    #[test]
    fn income_comes_every_16_turns() {
        let mut b = SpellBook::new();
        let mut pool = ManaPool { bank: Mana::ZERO, ..ManaPool::default() };
        let mut now = Tick::ZERO;
        for _ in 0..32 {
            now = now.next();
            pool.tick(now, &mut b, &mut []);
        }
        assert_eq!((pool.bank, pool.received), (Mana(2 * INCOME.0), INCOME));
    }

    #[test]
    fn used_counts_since_the_last_income() {
        let mut b = book(&[SpellKind::Swarm]);
        let mut pool = ManaPool { bank: Mana::ZERO, ..ManaPool::default() };
        pool.tick(Tick::ZERO, &mut b, &mut []);
        assert_eq!((pool.used, pool.bank), (Mana(480), Mana(80)), "4 swarm charges of 120");
        pool.tick(Tick::ZERO.next(), &mut b, &mut []);
        assert_eq!(pool.used, Mana(480), "kept between incomes");
    }
}
