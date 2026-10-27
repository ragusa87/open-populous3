//! A player's mana (pop3-rev-analysis.md "Mana rules"): what is gained or spent during a turn waits in
//! `incoming` (`add`); once per turn `tick` adds the income, feeds the training
//! units (`crate::production`), charges the spells and banks the rest. Only `tick` changes the bank, the
//! spell charges and the training units' stores.
//!
//! `advisor_eta`/`advisor_fraction` are the HUD mana bar's own basis (pop3-rev-analysis.md "The mana bar
//! (HUD)"): the bar is not `bank / MAX_MANA` at all, it is `received` (last turn's income) against a pacing
//! estimate for whatever spell is currently charging, always full with nothing charging.

use crate::production::{self, Production};
use crate::spell_book::{SpellBook, SpellSlot};
use crate::time::{Every, Tick, Ticks};

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
    /// The HUD mana bar's own target (pop3-rev-analysis.md "The mana bar (HUD)", `advisor_fraction`):
    /// `Mana::ZERO` when no spell is charging. Recomputed every income turn from the charging candidate.
    pub advisor_eta: Mana,
}

impl Default for ManaPool {
    fn default() -> Self {
        ManaPool { bank: START_MANA, incoming: Mana::ZERO, received: Mana::ZERO, used: Mana::ZERO, advisor_eta: Mana::ZERO }
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
            self.advisor_eta = advisor_eta(book);
        }
        self.distribute(book, stores);
    }

    /// The HUD mana bar's own fill fraction (`0.0..=1.0`, "The mana bar (HUD)"): always full with nothing
    /// charging (`advisor_eta` zero); otherwise how well `received` kept pace with `advisor_eta`'s estimate,
    /// scaled the same ~1.78x the exe does (`456 / 256`).
    pub fn advisor_fraction(&self) -> f32 {
        if self.advisor_eta == Mana::ZERO {
            return 1.0;
        }
        let scale = ((self.advisor_eta.0 as u64 * 456 / 256) as u32).max(1);
        let value = self.received.0.min(scale);
        value as f32 / scale as f32
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

/// `[ours]`: the exe scales its own advisor estimate by an undecoded global byte (`0x89D161`,
/// pop3-rev-analysis.md "The mana bar (HUD)"). Lacking it, this reads `SpellKind::advisor_seconds` as "the
/// candidate should be full in about this long" and spreads its own cost over that many income periods.
fn advisor_eta(book: &SpellBook) -> Mana {
    let Some(kind) = book.charging_candidate() else { return Mana::ZERO };
    let periods = (Ticks::secs(kind.advisor_seconds()).get() / MANA_EVERY.period()).max(1);
    Mana(kind.cost() / periods)
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

    #[test]
    fn advisor_fraction_is_full_with_nothing_charging() {
        assert_eq!(ManaPool::default().advisor_fraction(), 1.0);
    }

    #[test]
    fn advisor_fraction_reflects_how_income_kept_pace() {
        let short = ManaPool { advisor_eta: Mana(100), received: Mana(50), ..ManaPool::default() };
        assert!((short.advisor_fraction() - 50.0 / 178.0).abs() < 0.001, "scale = 100 * 456 / 256 = 178");
        let caught_up = ManaPool { advisor_eta: Mana(100), received: Mana(1000), ..ManaPool::default() };
        assert_eq!(caught_up.advisor_fraction(), 1.0, "never past a full bar");
    }

    #[test]
    fn advisor_eta_is_zero_with_no_charging_spell() {
        let mut pool = ManaPool::default();
        pool.tick(Tick::ZERO, &mut SpellBook::new(), &mut []);
        assert_eq!(pool.advisor_eta, Mana::ZERO);
    }

    #[test]
    fn advisor_eta_is_spread_from_the_charging_candidate() {
        let mut b = book(&[SpellKind::Blast]);
        let mut pool = ManaPool::default();
        pool.tick(Tick::ZERO, &mut b, &mut []);
        assert_eq!(pool.advisor_eta, Mana(1), "blast: cost 40 over 30s / 16 turns = 22 periods");
    }
}
