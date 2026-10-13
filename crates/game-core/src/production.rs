//! What a training unit (warrior, firewarrior, spy building), and later a boat or balloon hut, makes from
//! stored mana (pop3-rev-analysis.md "Training buildings"): the mana it has taken so far towards the cost
//! of what it makes now. Filled by the mana distribution (TODO "Mana"); the tooltip's bar reads it.

use crate::map::GameMap;

/// Most mana a share can bring and still leave the building short of mana (the panel's blinking bar,
/// pop3-rev-analysis.md "The panel's training bar": hut `+0x14` bit 0x1000).
pub const SHORT_SHARE: u16 = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Production {
    /// Mana taken so far (`+0x98`).
    pub stored: u16,
    /// Mana it needs to make what it makes now (`+0x96`); 0 while it makes nothing.
    pub cost: u16,
    /// Its last share was `SHORT_SHARE` or less.
    pub short: bool,
}

impl Production {
    /// Whether something is being made.
    pub fn is_active(&self) -> bool {
        self.cost > 0
    }
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
    use crate::building::{Building, BuildingKind};

    #[test]
    fn only_a_building_making_something_has_a_production() {
        let mut map = GameMap::sandbox_buildings();
        let id = map.add_building(Building::new(BuildingKind::WarriorTraining, 0, 0, 0, 0));
        let i = map.building_index(id).unwrap();
        assert_eq!(map.production(i), None);
        map.buildings[i].production = Some(Production { stored: 0, cost: 0, short: false });
        assert_eq!(map.production(i), None, "nothing to make");
        let making = Production { stored: 100, cost: 3500, short: true };
        map.buildings[i].production = Some(making);
        assert_eq!(map.production(i), Some(making));
    }
}
