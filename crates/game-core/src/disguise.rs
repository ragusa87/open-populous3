//! A spy's disguise (docs/specs/units.md "Spies", pop3-rev-analysis.md "Spy disguise and infiltration"): the
//! tribe it passes as, and a settling countdown during which it still counts as its real tribe everywhere.

use crate::time::{Countdown, Tick, Ticks};
use crate::tribes::{Owner, TribeId};
use crate::unit::{Unit, UnitKind};

/// `SPY_DISGUISE_DELAY` (exe default 63 turns, constants.md "Misc"): a timing, so the real number is our default.
pub const DISGUISE_DELAY: Ticks = Ticks::new(63);
/// The countdown is 6 bits wide in the original: a longer delay from a file is clamped.
const MAX_DELAY: Ticks = Ticks::new(63);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Disguise {
    pub apparent: TribeId,
    settling: Countdown,
}

impl Disguise {
    /// Disguising as `apparent`: a fresh countdown of `delay`, whatever disguise came before.
    pub fn new(apparent: TribeId, delay: Ticks) -> Disguise {
        Disguise { apparent, settling: Countdown::new(delay.min(MAX_DELAY)) }
    }

    pub fn settled(self) -> bool {
        self.settling.left() == Ticks::ZERO
    }

    pub fn tick(&mut self) {
        self.settling.tick();
    }
}

impl Unit {
    /// Whether it currently passes as one of `tribe`'s own people: only a spy with a settled disguise as `tribe`.
    /// While settling it still counts as its real tribe, which is never a disguise.
    pub fn counts_as(&self, tribe: TribeId) -> bool {
        self.kind == UnitKind::Spy && self.statuses.disguise.is_some_and(|d| d.settled() && d.apparent == tribe && Owner::Tribe(tribe) != self.owner)
    }

    /// Whether it passes as one of `viewer`'s own people: every automatic look for an enemy (spotting, a preacher's
    /// sermon, an angel's hunt, the computer's defence) skips it. An order aimed at it still finds it (and reveals
    /// it).
    pub fn fools(&self, viewer: impl Into<Owner>) -> bool {
        viewer.into().tribe().is_some_and(|t| self.counts_as(t))
    }

    /// Whether it is a spy whose disguise has settled (`counts_as` some other tribe).
    pub fn disguised(&self) -> bool {
        self.statuses.disguise.is_some_and(|d| d.settled() && Owner::Tribe(d.apparent) != self.owner)
    }

    /// The colour `viewer` sees it in at `now`: its owner sees it blink between its real and apparent tribe every
    /// 2 turns while settling, then the disguise; everyone else its real tribe until it settles, then the disguise.
    pub fn shown_owner(&self, viewer: TribeId, now: Tick) -> Owner {
        let Some(d) = self.statuses.disguise else { return self.owner };
        if d.settled() {
            Owner::Tribe(d.apparent)
        } else if self.owner == Owner::Tribe(viewer) && now.to_wire() & 2 != 0 {
            Owner::Tribe(d.apparent)
        } else {
            self.owner
        }
    }

    /// Takes on `tribe`'s look: re-arms the full settling countdown, even switching from another disguise.
    /// Ignored for anyone but a spy; disguising as its own tribe just drops any disguise.
    pub fn disguise_as(&mut self, tribe: TribeId, delay: Ticks) {
        if self.kind != UnitKind::Spy {
            return;
        }
        self.statuses.disguise = (self.owner != Owner::Tribe(tribe)).then(|| Disguise::new(tribe, delay));
    }

    /// Back to its own look at once. Whether it had a disguise to lose.
    pub fn reveal(&mut self) -> bool {
        self.statuses.disguise.take().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::UnitId;

    fn spy() -> Unit {
        Unit::new(UnitId::from_raw(1), TribeId::Blue, UnitKind::Spy, (0, 0))
    }

    fn settle(u: &mut Unit) {
        for _ in 0..DISGUISE_DELAY.get() {
            u.statuses.disguise.as_mut().unwrap().tick();
        }
    }

    #[test]
    fn counts_as_its_real_tribe_while_settling_then_as_the_disguise() {
        let mut u = spy();
        u.disguise_as(TribeId::Red, DISGUISE_DELAY);
        assert!(!u.counts_as(TribeId::Red) && !u.disguised());
        for _ in 0..DISGUISE_DELAY.get() - 1 {
            u.statuses.disguise.as_mut().unwrap().tick();
        }
        assert!(!u.counts_as(TribeId::Red), "one turn left");
        u.statuses.disguise.as_mut().unwrap().tick();
        assert!(u.counts_as(TribeId::Red) && u.disguised());
        assert!(!u.counts_as(TribeId::Green) && !u.counts_as(TribeId::Blue));
    }

    #[test]
    fn switching_disguise_starts_over_undisguised() {
        let mut u = spy();
        u.disguise_as(TribeId::Red, DISGUISE_DELAY);
        settle(&mut u);
        u.disguise_as(TribeId::Green, DISGUISE_DELAY);
        assert!(!u.counts_as(TribeId::Red) && !u.counts_as(TribeId::Green), "its real tribe again");
        assert_eq!(u.shown_owner(TribeId::Red, Tick::ZERO), Owner::Tribe(TribeId::Blue));
        settle(&mut u);
        assert!(u.counts_as(TribeId::Green) && !u.counts_as(TribeId::Red));
    }

    #[test]
    fn only_a_spy_disguises_and_never_as_its_own_tribe() {
        let mut brave = Unit::new(UnitId::from_raw(2), TribeId::Blue, UnitKind::Brave, (0, 0));
        brave.disguise_as(TribeId::Red, DISGUISE_DELAY);
        assert_eq!(brave.statuses.disguise, None);
        let mut u = spy();
        u.disguise_as(TribeId::Red, DISGUISE_DELAY);
        u.disguise_as(TribeId::Blue, DISGUISE_DELAY);
        assert_eq!(u.statuses.disguise, None, "its own tribe: undisguised");
    }

    #[test]
    fn the_owner_sees_it_blink_while_settling_others_a_hard_cut() {
        let mut u = spy();
        u.disguise_as(TribeId::Red, DISGUISE_DELAY);
        let seen = |u: &Unit, viewer| (0..4).map(|n| u.shown_owner(viewer, Tick::from_wire(n))).collect::<Vec<_>>();
        let (blue, red) = (Owner::Tribe(TribeId::Blue), Owner::Tribe(TribeId::Red));
        assert_eq!(seen(&u, TribeId::Blue), [blue, blue, red, red]);
        assert_eq!(seen(&u, TribeId::Green), [blue; 4]);
        settle(&mut u);
        assert_eq!(seen(&u, TribeId::Blue), [red; 4]);
        assert_eq!(seen(&u, TribeId::Green), [red; 4]);
    }

    #[test]
    fn a_longer_delay_is_clamped_to_6_bits() {
        assert_eq!(Disguise::new(TribeId::Red, Ticks::new(500)).settling.total(), Ticks::new(63));
    }

    #[test]
    fn reveal_drops_it() {
        let mut u = spy();
        assert!(!u.reveal());
        u.disguise_as(TribeId::Red, DISGUISE_DELAY);
        settle(&mut u);
        assert!(u.reveal() && !u.disguised());
    }

    #[test]
    fn changing_tribe_loses_it() {
        let mut u = spy();
        u.disguise_as(TribeId::Red, DISGUISE_DELAY);
        u.change_owner(TribeId::Green);
        assert_eq!(u.statuses.disguise, None);
    }

    mod on_the_map {
        use crate::command::Command;
        use crate::map::GameMap;
        use crate::tribes::TribeId;
        use crate::unit::{Action, Order, UnitId, UnitKind};

        pub(crate) fn bare() -> GameMap {
            let mut map = GameMap::sandbox_units();
            map.units.clear();
            map.buildings.clear();
            map.totems.clear();
            map
        }

        pub(crate) fn add(map: &mut GameMap, owner: TribeId, kind: UnitKind, at: (u16, u16)) -> UnitId {
            map.spawn(owner, kind, at).id
        }

        fn disguise(map: &mut GameMap, unit: UnitId, tribe: TribeId) {
            map.step(&[Command::OrderUnit { player: TribeId::Blue, unit, order: Order::Disguise { tribe } }]);
        }

        #[test]
        fn the_order_disguises_a_spy_without_stopping_it_and_settles_on_the_map() {
            let mut map = bare();
            let spy = add(&mut map, TribeId::Blue, UnitKind::Spy, (33024, 34048));
            map.step(&[Command::OrderUnit { player: TribeId::Blue, unit: spy, order: Order::MoveTo { x: 37000, z: 34048 } }]);
            disguise(&mut map, spy, TribeId::Red);
            assert!(matches!(map.unit(spy).unwrap().action, Action::Walking { .. }), "still on its way");
            map.run(crate::time::Ticks::new(61));
            assert!(!map.unit(spy).unwrap().counts_as(TribeId::Red));
            map.run(crate::time::Ticks::new(1));
            assert!(map.unit(spy).unwrap().counts_as(TribeId::Red));
        }

        #[test]
        fn the_order_is_ignored_by_anyone_but_a_spy() {
            let mut map = bare();
            let brave = add(&mut map, TribeId::Blue, UnitKind::Brave, (33024, 34048));
            disguise(&mut map, brave, TribeId::Red);
            assert_eq!(map.unit(brave).unwrap().statuses.disguise, None);
        }

        #[test]
        fn a_queued_disguise_waits_its_turn() {
            let mut map = bare();
            let spy = add(&mut map, TribeId::Blue, UnitKind::Spy, (33024, 34048));
            map.step(&[Command::OrderUnit { player: TribeId::Blue, unit: spy, order: Order::MoveTo { x: 33600, z: 34048 } }]);
            map.step(&[Command::QueueOrder { player: TribeId::Blue, unit: spy, order: Order::Disguise { tribe: TribeId::Red } }]);
            assert_eq!(map.unit(spy).unwrap().statuses.disguise, None, "still walking");
            for _ in 0..400 {
                map.step(&[]);
                if map.unit(spy).unwrap().statuses.disguise.is_some() {
                    return;
                }
            }
            panic!("never disguised once there");
        }
    
        fn spy_disguised(map: &mut GameMap, as_tribe: TribeId, settled: bool) -> UnitId {
            let spy = add(map, TribeId::Blue, UnitKind::Spy, (33024, 34048));
            let delay = if settled { crate::time::Ticks::ZERO } else { super::DISGUISE_DELAY };
            map.unit_mut(spy).unwrap().disguise_as(as_tribe, delay);
            spy
        }

        fn left_alone_by(kind: UnitKind, as_tribe: TribeId, settled: bool) -> bool {
            let mut map = bare();
            let spy = spy_disguised(&mut map, as_tribe, settled);
            add(&mut map, TribeId::Red, kind, (33024 + 300, 34048));
            map.run(crate::time::Ticks::new(40));
            let u = map.unit(spy).unwrap();
            u.health.is_full() && matches!(u.action, Action::Idle)
        }

        #[test]
        fn a_settled_spy_is_left_alone_by_the_tribe_it_passes_as() {
            assert!(left_alone_by(UnitKind::Warrior, TribeId::Red, true));
            assert!(!left_alone_by(UnitKind::Warrior, TribeId::Red, false), "still settling: an enemy");
            assert!(!left_alone_by(UnitKind::Warrior, TribeId::Green, true), "passing as another tribe");
        }

        #[test]
        fn a_preacher_converts_a_spy_unless_it_has_settled_as_the_preachers_tribe() {
            assert!(left_alone_by(UnitKind::Preacher, TribeId::Red, true));
            assert!(!left_alone_by(UnitKind::Preacher, TribeId::Red, false));
            assert!(!left_alone_by(UnitKind::Preacher, TribeId::Green, true));
        }
    }
}
