//! What a training unit (warrior, firewarrior, spy building), and later a boat or balloon hut, makes from
//! stored mana (pop3-rev-analysis.md "Training buildings"): the mana it has taken so far towards the cost
//! of what it makes now. Filled by the mana distribution (TODO "Mana"); the tooltip's bar reads it.

use crate::map::GameMap;
use crate::mana::Mana;

/// A building's share of an income is at most its cost / `MAX_SHARE_DIV` ("Distribution").
pub const MAX_SHARE_DIV: u16 = 32;
/// Mana a building making nothing gives back to the tribe each turn, until its store is empty.
pub const REFUND: u16 = 100;

/// Most mana a share can bring and still leave the building short of mana (the panel's blinking bar,
/// pop3-rev-analysis.md "The panel's training bar").
pub const SHORT_SHARE: u16 = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Production {
    /// Mana taken so far.
    pub stored: u16,
    /// Mana it needs to make what it makes now; 0 while it makes nothing.
    pub cost: u16,
    /// Its last share was `SHORT_SHARE` or less.
    pub short: bool,
}

impl Production {
    /// Whether something is being made.
    pub fn is_active(&self) -> bool {
        self.cost > 0
    }

    /// Making something and not full: it would take mana (as `SpellSlot::takes_mana`).
    pub fn takes_mana(&self) -> bool {
        self.is_active() && self.stored < self.cost
    }

    /// Takes what it can of `share` (at most `cost / MAX_SHARE_DIV` and what it still needs); returns
    /// the rest.
    pub fn give(&mut self, share: Mana) -> Mana {
        let taken = share.0.min((self.cost / MAX_SHARE_DIV) as u32).min((self.cost - self.stored.min(self.cost)) as u32);
        self.stored += taken as u16;
        Mana(share.0 - taken)
    }

    /// While making nothing, gives back up to `REFUND` of its store.
    pub fn refund(&mut self) -> Mana {
        if self.is_active() {
            return Mana::ZERO;
        }
        let back = self.stored.min(REFUND);
        self.stored -= back;
        Mana(back as u32)
    }
}

/// The first share of the distribution: `mana` shared evenly between the `stores` that take it, each
/// taking what it can (`Production::give`) and short when that is `SHORT_SHARE` or less. Turns without mana
/// to share leave them as they are. Returns what they did not take.
pub fn charge(stores: &mut [&mut Production], mana: Mana) -> Mana {
    if mana == Mana::ZERO {
        return mana;
    }
    share(stores, mana, true)
}

/// The second share, once every spell is full: like `charge`, without changing whether they are short.
pub fn top_up(stores: &mut [&mut Production], mana: Mana) -> Mana {
    share(stores, mana, false)
}

fn share(stores: &mut [&mut Production], mana: Mana, mark_short: bool) -> Mana {
    let takers = stores.iter().filter(|p| p.takes_mana()).count() as u32;
    if takers == 0 {
        return mana;
    }
    let share = Mana(mana.0 / takers);
    let mut left = mana.saturating_sub(Mana(share.0 * takers));
    for p in stores.iter_mut().filter(|p| p.takes_mana()) {
        let rest = p.give(share);
        if mark_short {
            p.short = share.saturating_sub(rest).0 <= SHORT_SHARE as u32;
        }
        left += rest;
    }
    left
}

impl GameMap {
    /// Building `i`'s production, None while it makes nothing.
    pub fn production(&self, i: usize) -> Option<Production> {
        self.buildings.get(i)?.production.filter(Production::is_active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tribes::TribeId;
    use crate::building::{Building, BuildingKind};

    fn warrior(stored: u16) -> Production {
        Production { stored, cost: 3500, short: false }
    }

    #[test]
    fn a_share_is_at_most_a_32nd_of_the_cost_and_what_is_missing() {
        let mut p = warrior(0);
        assert_eq!(p.give(Mana(280)), Mana(171), "3500 / 32 = 109 taken");
        assert_eq!(p.stored, 109);
        let mut p = warrior(3450);
        assert_eq!(p.give(Mana(280)), Mana(230), "only the 50 missing");
    }

    #[test]
    fn stores_share_evenly_and_pass_on_what_they_cannot_take() {
        let (mut a, mut b, mut idle) = (warrior(0), warrior(3490), Production::default());
        let left = charge(&mut [&mut a, &mut b, &mut idle], Mana(201));
        assert_eq!((a.stored, b.stored, idle.stored), (100, 3500, 0), "100 each, the full one took 10");
        assert_eq!(left, Mana(91), "90 not taken and the odd one");
        assert_eq!(charge(&mut [&mut idle], Mana(50)), Mana(50), "nobody takes");
    }

    #[test]
    fn short_from_a_small_first_share_only() {
        let mut p = warrior(0);
        charge(&mut [&mut p], Mana(20));
        assert!(p.short, "20 is short");
        charge(&mut [&mut p], Mana::ZERO);
        assert!(p.short, "a turn without income changes nothing");
        charge(&mut [&mut p], Mana(280));
        assert!(!p.short, "109 is enough");
        charge(&mut [&mut p], Mana::ZERO);
        assert!(!p.short, "a turn without income changes nothing");
        top_up(&mut [&mut p], Mana(10));
        assert!(!p.short, "the second share does not say");
    }

    #[test]
    fn a_store_making_nothing_gives_back_100_a_turn() {
        let mut p = Production { stored: 250, cost: 0, short: false };
        assert_eq!((p.refund(), p.refund(), p.refund(), p.refund()), (Mana(100), Mana(100), Mana(50), Mana::ZERO));
        assert_eq!(warrior(250).refund(), Mana::ZERO, "making something: kept");
    }

    #[test]
    fn only_a_building_making_something_has_a_production() {
        let mut map = GameMap::sandbox_buildings();
        let id = map.add_building(Building::new(BuildingKind::WarriorTraining, TribeId::Blue, 0, 0, 0));
        let i = map.building_index(id).unwrap();
        assert_eq!(map.production(i), None);
        map.buildings[i].production = Some(Production { stored: 0, cost: 0, short: false });
        assert_eq!(map.production(i), None, "nothing to make");
        let making = Production { stored: 100, cost: 3500, short: true };
        map.buildings[i].production = Some(making);
        assert_eq!(map.production(i), Some(making));
    }
}
