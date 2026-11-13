//! A spy's disguise (docs/specs/units.md "Spies", pop3-rev-analysis.md "Spy disguise and infiltration"): the
//! tribe it passes as, and a settling countdown during which it still counts as its real tribe everywhere.

use crate::map::GameMap;
use crate::time::{Countdown, Every, Tick, Ticks};
use crate::tribes::{Owner, TribeId};
use crate::unit::{Target, Unit, UnitId, UnitKind};

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

    /// The kind `viewer` sees it as: a settled disguise passes as a brave to everyone but its owner, who still sees
    /// its spy.
    pub fn shown_kind(&self, viewer: TribeId) -> UnitKind {
        if self.disguised() && self.owner != Owner::Tribe(viewer) { UnitKind::Brave } else { self.kind }
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

/// Spies meeting check their cell on this rhythm ("What un-disguises a spy" [exe]).
pub const MEETING_EVERY: Every = Every::new(4);

/// Spies (id, real owner, cell) unmasked by meeting: every one sharing its cell with a spy of another real tribe,
/// allies or not.
pub fn unmasked_by_meeting(spies: &[(UnitId, Owner, (i32, i32))]) -> Vec<UnitId> {
    spies.iter().filter(|&&(_, owner, cell)| spies.iter().any(|&(_, o, c)| c == cell && o != owner)).map(|&(id, _, _)| id).collect()
}

impl GameMap {
    /// Every 4 turns, spies standing on a cell with another tribe's spy lose their disguise.
    pub(crate) fn tend_spy_meetings(&mut self) {
        if !MEETING_EVERY.fires(self.now, 0) {
            return;
        }
        let spies: Vec<_> = self.units.iter().filter(|u| u.kind == UnitKind::Spy && u.is_alive() && u.in_the_open()).map(|u| (u.id, u.owner, u.cell())).collect();
        for id in unmasked_by_meeting(&spies) {
            if let Some(u) = self.unit_mut(id) {
                u.reveal();
            }
        }
    }

    /// A unit an attack is aimed at loses its disguise the turn the attack starts, long before contact (an attack
    /// spotted on its own too). Other orders aimed at it do not reveal it.
    pub(crate) fn reveal_targeted(&mut self, target: Target) {
        if let Target::Unit(id) = target && let Some(u) = self.unit_mut(id) {
            u.reveal();
        }
    }

    /// `Command::RevealSpy`: `unit` loses its disguise if it passes as one of `player`'s own people (a select click on
    /// it: one still settling is an ordinary enemy, one passing as a third tribe not a lookalike).
    pub(crate) fn reveal_spy(&mut self, player: TribeId, unit: UnitId) {
        if let Some(u) = self.unit_mut(unit).filter(|u| u.counts_as(player)) {
            u.reveal();
        }
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
    fn a_settled_disguise_looks_like_a_brave_to_all_but_its_owner() {
        let mut u = spy();
        assert_eq!(u.shown_kind(TribeId::Red), UnitKind::Spy, "undisguised");
        u.disguise_as(TribeId::Red, DISGUISE_DELAY);
        assert_eq!(u.shown_kind(TribeId::Red), UnitKind::Spy, "still settling");
        settle(&mut u);
        assert_eq!(u.shown_kind(TribeId::Blue), UnitKind::Spy);
        assert_eq!(u.shown_kind(TribeId::Red), UnitKind::Brave);
        assert_eq!(u.shown_kind(TribeId::Green), UnitKind::Brave);
        u.reveal();
        assert_eq!(u.shown_kind(TribeId::Red), UnitKind::Spy);
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
        use crate::effect::DamageKind;
        use crate::unit::{Action, Order, Target, UnitId, UnitKind};

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
    
        fn settled_spy(map: &mut GameMap) -> UnitId {
            spy_disguised(map, TribeId::Red, true)
        }

        #[test]
        fn an_attack_aimed_at_a_spy_reveals_it_at_once_other_orders_do_not() {
            let mut map = bare();
            let spy = settled_spy(&mut map);
            let warrior = add(&mut map, TribeId::Red, UnitKind::Warrior, (33024 + 4000, 34048));
            map.step(&[Command::OrderUnit { player: TribeId::Red, unit: warrior, order: Order::Attack { target: Target::Unit(spy) } }]);
            assert!(!map.unit(spy).unwrap().disguised(), "the turn it starts, far from contact");
            let mut map = bare();
            let spy = settled_spy(&mut map);
            let own = add(&mut map, TribeId::Blue, UnitKind::Warrior, (33024 + 4000, 34048));
            map.step(&[Command::OrderUnit { player: TribeId::Blue, unit: own, order: Order::Attack { target: Target::Unit(spy) } }]);
            assert!(map.unit(spy).unwrap().disguised(), "its own side cannot attack it");
        }

        #[test]
        fn a_blow_reveals_a_spell_does_not() {
            let mut map = bare();
            let spy = settled_spy(&mut map);
            map.damage(spy, 1, DamageKind::Blast, Some(TribeId::Green.into()));
            assert!(map.unit(spy).unwrap().disguised(), "a blast");
            map.damage(spy, 1, DamageKind::Melee, Some(TribeId::Green.into()));
            assert!(!map.unit(spy).unwrap().disguised());
        }

        #[test]
        fn spies_of_two_tribes_on_one_cell_unmask_each_other() {
            let (a, b, c) = (UnitId::from_raw(1), UnitId::from_raw(2), UnitId::from_raw(3));
            let (blue, red) = (TribeId::Blue.into(), TribeId::Red.into());
            assert_eq!(super::super::unmasked_by_meeting(&[(a, blue, (1, 1)), (b, red, (1, 1)), (c, red, (2, 1))]), [a, b]);
            assert!(super::super::unmasked_by_meeting(&[(a, blue, (1, 1)), (b, blue, (1, 1))]).is_empty(), "same tribe");
            let mut map = bare();
            let spy = settled_spy(&mut map);
            add(&mut map, TribeId::Green, UnitKind::Spy, (33024 + 10, 34048));
            map.run(crate::time::Ticks::new(4));
            assert!(!map.unit(spy).unwrap().disguised());
        }

        #[test]
        fn a_click_reveals_a_spy_passing_as_the_clickers_own_people_only() {
            let mut map = bare();
            let spy = settled_spy(&mut map);
            map.step(&[Command::RevealSpy { player: TribeId::Green, unit: spy }]);
            assert!(map.unit(spy).unwrap().disguised(), "passing as red, not green");
            map.step(&[Command::RevealSpy { player: TribeId::Blue, unit: spy }]);
            assert!(map.unit(spy).unwrap().disguised(), "not by its own side");
            map.step(&[Command::RevealSpy { player: TribeId::Red, unit: spy }]);
            assert!(!map.unit(spy).unwrap().disguised());
            map.unit_mut(spy).unwrap().disguise_as(TribeId::Red, super::super::DISGUISE_DELAY);
            map.step(&[Command::RevealSpy { player: TribeId::Red, unit: spy }]);
            assert!(map.unit(spy).unwrap().statuses.disguise.is_some(), "still settling: an ordinary enemy");
        }

        #[test]
        fn giving_up_and_sulking_drops_the_tribes_own_disguises() {
            let mut map = bare();
            let spy = settled_spy(&mut map);
            map.ai_set_give_up_and_sulk(TribeId::Blue, true);
            assert!(!map.unit(spy).unwrap().disguised());
        }

        #[test]
        fn hypnotised_it_comes_back_undisguised() {
            let mut map = bare();
            let spy = settled_spy(&mut map);
            map.hypnotize(spy, TribeId::Green);
            assert_eq!(map.unit(spy).unwrap().statuses.disguise, None);
        }

        #[test]
        fn walking_and_time_never_reveal() {
            let mut map = bare();
            let spy = settled_spy(&mut map);
            map.step(&[Command::OrderUnit { player: TribeId::Blue, unit: spy, order: Order::MoveTo { x: 35000, z: 34048 } }]);
            map.run(crate::time::Ticks::new(300));
            assert!(map.unit(spy).unwrap().disguised());
        }
    }
}
