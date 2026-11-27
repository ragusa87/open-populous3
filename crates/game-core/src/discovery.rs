//! A human tribe's discovery on its way (pop3-rev-analysis.md "Discovery cinematic: the reward flies to the panel"):
//! a building or spell from a vault or a totem lands only once its cinematic has carried it to the panel. The
//! cinematic starts `HIDE` turns after the grant and lasts `flight` turns, so the reward lands `HIDE + flight` turns
//! after it; the client draws it from `Discovery::started`. Computer tribes, and other rewards, get theirs at once.

use crate::building::Reward;
use crate::map::GameMap;
use crate::terrain::DirtyRect;
use crate::time::{Tick, Ticks};
use crate::tribes::TribeId;

/// Turns between the grant and the cinematic's start [exe: the discovery's hide delay].
pub const HIDE: Ticks = Ticks::new(6);
/// A building's cinematic: 19 + 23 + 31 drawn frames, then a 50-frame flight to the panel, at a guessed 30 frames a
/// second [ours: the exe counts drawn frames; its last stage runs until the last face arrives, up to 101 frames].
pub const BUILDING_FLIGHT: Ticks = Ticks::new(50);
/// A spell's: 1 + 13 + 4 frames, then an 18-frame flight to the panel [ours, as `BUILDING_FLIGHT`; the exe's last
/// stage speeds up until it reaches the button].
pub const SPELL_FLIGHT: Ticks = Ticks::new(15);

/// How long `reward`'s cinematic lasts, None when it has none.
pub fn flight(reward: Reward) -> Option<Ticks> {
    match reward {
        Reward::Building(_) => Some(BUILDING_FLIGHT),
        Reward::Spell(_) | Reward::OneShot(_) => Some(SPELL_FLIGHT),
        _ => None,
    }
}

/// A reward granted to a human tribe, not landed yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Discovery {
    pub owner: TribeId,
    pub reward: Reward,
    /// Where it was granted (world units): the vault's middle, the totem.
    pub from: (u16, u16),
    /// The grant's turn.
    pub granted: Tick,
}

impl Discovery {
    /// The turn its cinematic starts.
    pub fn started(&self) -> Tick {
        self.granted + HIDE
    }

    /// The turn it lands.
    pub fn lands(&self) -> Tick {
        self.started() + flight(self.reward).unwrap_or(Ticks::new(0))
    }
}

impl GameMap {
    /// `reward` granted to `owner` at `from`: a human tribe's building or spell waits for its cinematic, anything
    /// else lands now. The ground it reshaped at once, if any.
    pub(crate) fn discover(&mut self, owner: TribeId, reward: Reward, from: (u16, u16)) -> Option<DirtyRect> {
        if flight(reward).is_none() || self.tribes.is_computer(owner) {
            return self.give(owner, reward);
        }
        self.discoveries.push(Discovery { owner, reward, from, granted: self.now });
        None
    }

    /// Each tick: the discoveries whose cinematic has arrived land, in the order they were granted.
    pub(crate) fn tend_discoveries(&mut self) {
        let now = self.now;
        let (due, left): (Vec<Discovery>, Vec<Discovery>) = self.discoveries.drain(..).partition(|d| now.since(d.lands()) >= 0);
        self.discoveries = left;
        for d in due {
            let _ = self.give(d.owner, d.reward);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::BuildingKind;
    use crate::spell_book::SpellKind;

    fn map(computer: bool) -> GameMap {
        let mut map = GameMap::sandbox_walk();
        map.tribes = crate::tribes::Tribes::from_owners([TribeId::Blue, TribeId::Red]).with_ai_scripts(if computer { [0, 1, 0, 0] } else { [0; 4] });
        map
    }

    fn knows(map: &GameMap, kind: SpellKind) -> bool {
        map.spell_books[1].slots.iter().any(|s| s.kind == kind && s.availability == crate::spell_book::Availability::Known)
    }

    #[test]
    fn a_human_tribes_spell_lands_once_its_cinematic_arrives() {
        let mut map = map(false);
        map.spell_books[1].set(SpellKind::Swarm, crate::spell_book::Availability::Discoverable);
        let _ = map.discover(TribeId::Red, Reward::Spell(SpellKind::Swarm), (100, 100));
        let lands = map.discoveries[0].lands();
        assert_eq!(lands.since(map.now), (HIDE + SPELL_FLIGHT).get() as i32);
        while map.now.since(lands) < 0 {
            assert!(!knows(&map, SpellKind::Swarm), "not before it arrives");
            map.now = map.now.next();
            map.tend_discoveries();
        }
        assert!(knows(&map, SpellKind::Swarm) && map.discoveries.is_empty());
        assert_eq!(map.granted, vec![(TribeId::Red, Reward::Spell(SpellKind::Swarm))]);
    }

    #[test]
    fn a_computer_tribe_and_mana_get_theirs_at_once() {
        let mut map = map(true);
        map.spell_books[1].set(SpellKind::Swarm, crate::spell_book::Availability::Discoverable);
        let _ = map.discover(TribeId::Red, Reward::Spell(SpellKind::Swarm), (0, 0));
        assert!(knows(&map, SpellKind::Swarm) && map.discoveries.is_empty());
        let mut map = self::map(false);
        let _ = map.discover(TribeId::Red, Reward::Mana(10), (0, 0));
        assert!(map.discoveries.is_empty());
    }

    #[test]
    fn a_building_flies_longer_than_a_spell() {
        assert_eq!(flight(Reward::Building(BuildingKind::Temple)), Some(BUILDING_FLIGHT));
        assert_eq!(flight(Reward::OneShot(SpellKind::Swarm)), Some(SPELL_FLIGHT));
        assert!(BUILDING_FLIGHT > SPELL_FLIGHT);
        assert!(HIDE + BUILDING_FLIGHT < Ticks::new(82), "under the original's cap");
    }
}
