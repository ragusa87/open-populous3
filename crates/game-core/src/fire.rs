//! Burning buildings (pop3-rev-analysis.md "Shared person and building states", Burning building), set on fire by
//! `WorldEvent::SetFire` (lightning). A building burns for `BURN_TICKS`: on its `THROW_OUT_AT`th turn the people
//! inside are thrown out by the door and panic, on its `HIT_AT`th it loses one piece of wood, with damage smoke
//! (`smoke::damage`). With wood left it stands
//! as under construction, which braves repair with wood once the fire is out; with none left it is destroyed.

use crate::building::{Building, BuildingId, Stage};
use crate::event::WorldEvent;
use crate::map::GameMap;
use crate::terrain::DirtyRect;
use crate::time::{Countdown, Tick, Ticks};
use crate::unit::{Order, UnitId};

/// 127 turns (10.6 s), the original's burning building (pop3-rev-analysis.md "Burning building").
pub const BURN_TICKS: Ticks = Ticks::new(127);
/// The turn of its fire its people are thrown out (timer 119).
pub const THROW_OUT_AT: Ticks = Ticks::new(8);
/// The turn of its fire it takes its structural hit of 100 wood, one piece (timer 79).
pub const HIT_AT: Ticks = Ticks::new(48);

/// What a turn of its fire does to a building.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireTurn {
    /// Not burning, or nothing this turn.
    Nothing,
    ThrowOut,
    /// One piece of wood lost; `destroyed` when it was its last.
    Hit { destroyed: bool },
}

impl Building {
    /// Sets it on fire for `BURN_TICKS`, lit by tribe `by`; false when it does not catch: fireproof
    /// (`BuildingKind::fireproof`), only a plan (ours, a guess) or already burning (its fire is not lit
    /// again, and `ignited_by` keeps whichever tribe lit it first).
    pub fn ignite(&mut self, by: u8) -> bool {
        if self.kind.fireproof() || self.stage() == Stage::Blueprint || self.burning.is_some() {
            return false;
        }
        self.burning = Some(Countdown::new(BURN_TICKS));
        self.ignited_by = Some(by);
        true
    }

    /// The tick its fire was lit, the map being at `now`; None when it does not burn.
    pub fn lit_at(&self, now: Tick) -> Option<Tick> {
        self.burning.map(|c| Tick::from_wire(now.to_wire().wrapping_sub(c.done().get())))
    }

    /// One turn of its fire, out when it has burnt `BURN_TICKS`.
    pub fn burn(&mut self) -> FireTurn {
        let Some(fire) = self.burning.as_mut() else { return FireTurn::Nothing };
        let out = fire.tick();
        let done = fire.done();
        if out {
            self.burning = None;
        }
        match done {
            THROW_OUT_AT => FireTurn::ThrowOut,
            HIT_AT => FireTurn::Hit { destroyed: self.lose_a_piece() },
            _ => FireTurn::Nothing,
        }
    }

    /// Loses one piece of the wood its blueprint asks for (`BuildingKind::wood_cost`); whether none is left. A kind
    /// with no blueprint (`wood_cost` 0: guard post, gate...) has none to lose.
    pub(crate) fn lose_a_piece(&mut self) -> bool {
        if self.kind.wood_cost() == 0 {
            return false;
        }
        self.used = self.used.min(self.kind.wood_cost()).saturating_sub(1);
        self.used == 0
    }
}

impl GameMap {
    /// Sets the building `building` on fire, lit by tribe `by` (`Building::ignite`).
    pub(crate) fn set_fire(&mut self, building: BuildingId, by: u8) {
        if let Some(i) = self.building_index(building) {
            self.buildings[i].ignite(by);
        }
    }

    /// Throws everyone inside `building` out by its door; their panics, to apply next.
    pub(crate) fn throw_out(&mut self, building: BuildingId) -> Vec<WorldEvent> {
        let Some(i) = self.building_index(building) else { return Vec::new() };
        let out: Vec<UnitId> = self.units.iter().filter(|u| u.is_alive() && u.inside.is_some_and(|i| i.building == building)).map(|u| u.id).collect();
        let door = self.buildings[i].door();
        self.empty_building(building, door);
        out.into_iter().map(|unit| WorldEvent::Panic { unit }).collect()
    }

    /// Removes the building at `i`: its braves stop working on it.
    pub(crate) fn remove_building(&mut self, i: usize) {
        let id = self.buildings.remove(i).id;
        for u in self.units.iter_mut().filter(|u| u.work == Some(id)) {
            u.work = None;
            u.start(Order::Stop);
        }
    }

    /// Each burning building's fire burns on: its people thrown out and panicking, its wood lost with damage smoke,
    /// a building with none left removed (anyone still inside put out first).
    pub(crate) fn tend_fires(&mut self) -> Vec<DirtyRect> {
        let turns: Vec<(BuildingId, FireTurn)> = self.buildings.iter_mut().map(|b| (b.id, b.burn())).filter(|t| t.1 != FireTurn::Nothing).collect();
        let (mut events, mut dirty) = (Vec::new(), Vec::new());
        for (id, turn) in turns {
            match turn {
                FireTurn::ThrowOut => events.extend(self.throw_out(id)),
                FireTurn::Hit { destroyed } => {
                    let Some(i) = self.building_index(id) else { continue };
                    let b = &self.buildings[i];
                    events.push(WorldEvent::SpawnEffect(crate::smoke::damage(b.owner, b.centre())));
                    if destroyed {
                        if let Some(tribe) = b.ignited_by {
                            events.push(WorldEvent::BuildingBurnedDown { tribe });
                        }
                        self.throw_out(id);
                        dirty.extend(self.burn_down(id));
                    }
                }
                FireTurn::Nothing => {}
            }
        }
        dirty.extend(self.apply_events(events));
        dirty
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::BuildingKind;
    use crate::unit::Action;

    /// The turns (1-based) of a fire on which something happens, and what.
    fn timeline(b: &mut Building) -> Vec<(u32, FireTurn)> {
        (1..=BURN_TICKS.get() + 5).map(|n| (n, b.burn())).filter(|t| t.1 != FireTurn::Nothing).collect()
    }

    #[test]
    fn people_out_on_turn_8_a_piece_lost_on_turn_48_out_after_127() {
        let mut hut = Building::new(BuildingKind::Hut { size: 1 }, 0, 0, 0, 0);
        assert!(hut.ignite(0));
        assert!(!hut.ignite(0), "already burning");
        assert_eq!(hut.stage(), Stage::Built, "nothing lost yet");
        (1..BURN_TICKS.get()).for_each(|n| {
            hut.burn();
            assert_eq!(hut.lit_at(Tick::from_wire(200)), Some(Tick::from_wire(200 - n)));
        });
        hut.burn();
        assert_eq!((hut.burning, hut.lit_at(Tick::from_wire(200))), (None, None));
        let mut hut = Building::new(BuildingKind::Hut { size: 1 }, 0, 0, 0, 0);
        hut.ignite(0);
        assert_eq!(timeline(&mut hut), [(8, FireTurn::ThrowOut), (48, FireTurn::Hit { destroyed: false })]);
        assert_eq!(hut.stage(), Stage::UnderConstruction { used: 2, of: 3 }, "repaired like a construction");
        hut.used = 1;
        hut.ignite(0);
        assert_eq!(timeline(&mut hut)[1], (48, FireTurn::Hit { destroyed: true }), "no wood left");
    }

    #[test]
    fn the_library_the_prison_and_plans_never_burn() {
        assert!(!Building::new(BuildingKind::Vault, 0, 0, 0, 0).ignite(0));
        assert!(!Building::new(BuildingKind::Prison, 0, 0, 0, 0).ignite(0));
        assert!(!Building::site(BuildingKind::Temple, 0, 0, 0, 0).ignite(0));
        let mut post = Building::new(BuildingKind::GuardPost, 0, 0, 0, 0);
        assert!(post.ignite(0));
        assert_eq!(timeline(&mut post)[1], (48, FireTurn::Hit { destroyed: false }), "its wood unknown: stands");
        let mut frame = Building { used: 0, ..Building::new(BuildingKind::Temple, 0, 0, 0, 0) };
        assert!(frame.ignite(0));
        assert_eq!(timeline(&mut frame)[1], (48, FireTurn::Hit { destroyed: true }), "a bare frame burns down");
    }

    fn sandbox_hut(map: &GameMap) -> usize {
        map.buildings.iter().position(|b| b.owner == 0 && b.kind == BuildingKind::Hut { size: 1 } && b.stage() == Stage::Built && b.inside == 0).unwrap()
    }

    fn rest_inside(map: &mut GameMap, hut: usize, n: usize) -> Vec<UnitId> {
        let units: Vec<UnitId> = map.units.iter().filter(|u| u.owner == 0 && u.kind == crate::unit::UnitKind::Brave && u.inside.is_none() && u.campfire().is_none()).take(n).map(|u| u.id).collect();
        for c in map.enter_orders(0, &units, hut) {
            map.apply(&c);
        }
        let id = map.buildings[hut].id;
        (0..800).find(|_| {
            map.tick();
            map.people_inside(id) == n
        });
        assert_eq!(map.people_inside(id), n);
        units
    }

    fn panicking(map: &GameMap, people: &[UnitId]) -> usize {
        map.units.iter().filter(|u| people.contains(&u.id) && matches!(u.action, Action::Panicking { .. })).count()
    }

    #[test]
    fn its_people_come_out_in_panic_on_turn_8_and_braves_repair_it_once_the_fire_is_out() {
        let mut map = GameMap::sandbox_buildings();
        let hut = sandbox_hut(&map);
        let id = map.buildings[hut].id;
        let people = rest_inside(&mut map, hut, 2);
        map.apply_events(vec![WorldEvent::SetFire { building: id, by: 0 }]);
        for _ in 1..THROW_OUT_AT.get() {
            map.tick();
        }
        assert_eq!((map.people_inside(id), panicking(&map, &people)), (2, 0), "still inside on turn 7");
        map.tick();
        assert_eq!(panicking(&map, &people), people.len(), "thrown out, panicking");
        assert!((0..40).any(|_| {
            map.tick();
            map.people_inside(id) == 0
        }), "out by the door");
        while map.buildings[hut].burning.is_some() {
            map.tick();
        }
        assert_eq!(map.buildings[hut].stage(), Stage::UnderConstruction { used: 2, of: 3 });
        assert_eq!(panicking(&map, &people), 0, "calm again");
        for c in map.build_orders(0, &people, hut) {
            map.apply(&c);
        }
        let repaired = (0..4000).find(|_| {
            map.tick();
            map.buildings[hut].stage() == Stage::Built
        });
        assert!(repaired.is_some(), "braves bring the lost piece back");
    }

    #[test]
    fn lightning_spares_the_people_inside_who_come_out_panicking() {
        let mut map = GameMap::sandbox_buildings();
        let hut = sandbox_hut(&map);
        let id = map.buildings[hut].id;
        let people = rest_inside(&mut map, hut, 2);
        map.add_effect(crate::lightning::effect(1, map.buildings[hut].centre()));
        for _ in 0..THROW_OUT_AT.get() {
            map.tick();
        }
        assert!(map.buildings[hut].burning.is_some());
        assert!(people.iter().all(|&p| map.unit(p).is_some_and(|u| u.is_alive())), "not struck");
        assert_eq!(panicking(&map, &people), people.len());
        assert_eq!(map.building(id).map(|b| b.used), Some(3));
    }

    #[test]
    fn a_building_burnt_to_nothing_is_removed_on_turn_48() {
        let mut map = GameMap::sandbox_buildings();
        let hut = sandbox_hut(&map);
        let id = map.buildings[hut].id;
        map.buildings[hut].used = 1;
        map.apply_events(vec![WorldEvent::SetFire { building: id, by: 0 }]);
        for _ in 1..HIT_AT.get() {
            map.tick();
        }
        assert!(map.building(id).is_some());
        let smokes = |map: &GameMap| map.effects.iter().filter(|(_, e)| e.kind == crate::effect::EffectKind::Smoke).map(|(_, e)| (e.x, e.z)).collect::<Vec<_>>();
        assert!(smokes(&map).is_empty());
        let at = map.buildings[hut].centre();
        map.tick();
        assert!(map.building(id).is_none());
        assert_eq!(smokes(&map), [at], "damage smoke where it stood");
        map.run(crate::smoke::DAMAGE_TICKS + Ticks::new(1));
        assert!(smokes(&map).is_empty(), "gone up");
    }

    #[test]
    fn a_building_burnt_down_tallies_the_igniting_tribe_s_fires_put_out() {
        use pop3_format::header::TRIBES;
        let mut map = GameMap::sandbox_buildings();
        let hut = sandbox_hut(&map);
        let id = map.buildings[hut].id;
        map.buildings[hut].used = 1;
        map.apply_events(vec![WorldEvent::SetFire { building: id, by: 2 }]);
        for _ in 0..HIT_AT.get() {
            map.tick();
        }
        assert!(map.building(id).is_none(), "burnt down");
        let mut expected = [0; TRIBES];
        expected[2] = 1;
        assert_eq!(map.fires_put_out, expected, "tallied to the tribe that lit it, not the owner");
    }
}
