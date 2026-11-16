//! Bookkeeping `GameMap::damage` carries alongside a unit's `Health`: who hit it last, and — once a hit proves
//! fatal — what kind of damage did it (pop3-rev-analysis.md "Health and healing": "last attacker's tribe +0xB0",
//! "else: last attacker (+0xB0) = none"; docs/specs/coordinators.md "World events"). Centralised here so every
//! emitter (melee, a bolt, a spell effect...) funnelling through `WorldEvent::Damage` gets it for free instead of
//! tracking it on its own.

use crate::effect::DamageKind;
use crate::tribes::Owner;
use crate::unit::UnitId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hurt {
    attacker: Option<Owner>,
    cause: Option<DamageKind>,
    /// The Angel of Death that grabbed it: whatever it later dies of counts as that angel's kill.
    angel: Option<UnitId>,
}

impl Hurt {
    /// The tribe that last hit it, if any and not healed away since.
    pub fn attacker(self) -> Option<Owner> {
        self.attacker
    }

    /// What killed it, once it has.
    pub fn cause(self) -> Option<DamageKind> {
        self.cause
    }

    /// A hit lands, from `from`'s tribe if known.
    pub(crate) fn hit(&mut self, from: Option<Owner>) {
        self.attacker = from;
    }

    /// Healed back to full: the original forgets the last attacker once there is nothing left to heal.
    pub(crate) fn healed(&mut self) {
        self.attacker = None;
    }

    /// The hit that just killed it.
    pub(crate) fn died_from(&mut self, kind: DamageKind) {
        self.cause = Some(kind);
    }

    /// Grabbed by the Angel of Death `angel`.
    pub(crate) fn seized_by(&mut self, angel: UnitId) {
        self.angel = Some(angel);
    }

    /// The angel that grabbed it, forgotten once asked: each death is credited once.
    pub(crate) fn take_seizer(&mut self) -> Option<UnitId> {
        self.angel.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_the_attacker_until_healed_full_or_struck_again() {
        let mut hurt = Hurt::default();
        assert_eq!(hurt.attacker(), None);
        hurt.hit(Some(Owner::Tribe(crate::tribes::TribeId::Red)));
        assert_eq!(hurt.attacker(), Some(Owner::Tribe(crate::tribes::TribeId::Red)));
        hurt.hit(Some(Owner::Tribe(crate::tribes::TribeId::Blue)));
        assert_eq!(hurt.attacker(), Some(Owner::Tribe(crate::tribes::TribeId::Blue)), "the latest attacker, not the first");
        hurt.healed();
        assert_eq!(hurt.attacker(), None);
    }

    #[test]
    fn the_angel_that_seized_it_is_handed_over_once() {
        let mut hurt = Hurt::default();
        let angel = crate::id::Id::from_raw(9);
        hurt.seized_by(angel);
        assert_eq!(hurt.take_seizer(), Some(angel));
        assert_eq!(hurt.take_seizer(), None, "credited once");
    }

    #[test]
    fn remembers_what_killed_it() {
        let mut hurt = Hurt::default();
        assert_eq!(hurt.cause(), None);
        hurt.died_from(DamageKind::Lightning);
        assert_eq!(hurt.cause(), Some(DamageKind::Lightning));
    }
}
