//! A tribe's people counted by what they do and by kind (the Stats tab, docs/specs/ui-and-editor.md,
//! and the computer players), which unit a pick adds next, and the tribe's housing: its population
//! against the room its huts give (docs/specs/huts-and-training.md rule 4).

use crate::building::{BuildingKind, Stage};
use crate::map::GameMap;
use crate::unit::{Action, Unit, UnitKind};

/// What a unit is doing, as the Stats tab rows group it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum State {
    Idle,
    Housed,
    Working,
    InBoat,
    InBalloon,
}

impl State {
    pub const ALL: [State; 5] = [State::Idle, State::Housed, State::Working, State::InBoat, State::InBalloon];

    fn index(self) -> usize {
        State::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }
}

/// The counted kinds, in column order (not the shaman, not wildmen).
pub const KINDS: [UnitKind; 5] = [UnitKind::Brave, UnitKind::Warrior, UnitKind::Firewarrior, UnitKind::Preacher, UnitKind::Spy];

fn kind_index(kind: UnitKind) -> Option<usize> {
    KINDS.iter().position(|&k| k == kind)
}

/// Units per kind, in `KINDS` order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts(pub [u16; 5]);

impl Counts {
    pub fn get(&self, kind: UnitKind) -> u16 {
        kind_index(kind).map_or(0, |k| self.0[k])
    }

    pub fn total(&self) -> u16 {
        self.0.iter().sum()
    }

    fn add(&mut self, kind: UnitKind) {
        if let Some(k) = kind_index(kind) {
            self.0[k] += 1;
        }
    }
}

/// A tribe's units per `State` and kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Headcount {
    rows: [Counts; 5],
}

impl Headcount {
    pub fn row(&self, state: State) -> &Counts {
        &self.rows[state.index()]
    }

    pub fn get(&self, state: State, kind: UnitKind) -> u16 {
        self.row(state).get(kind)
    }
}

/// Room per hut size 1..=3 (`MAX_POP_VALUE__HUT_1..3`).
pub const SUPPLY: [u16; 3] = [3, 5, 7];
/// A tribe never has room for more, whatever its huts.
pub const MAX_POPULATION: u16 = 200;

/// A tribe's population and its built huts per size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Housing {
    pub huts: [u16; 3],
    pub population: u16,
}

impl Housing {
    /// The room the huts give: their supply, at most `MAX_POPULATION`.
    pub fn room(&self) -> u16 {
        self.huts.iter().zip(SUPPLY).map(|(&n, s)| n * s).sum::<u16>().min(MAX_POPULATION)
    }

    /// No room left: breeding stops.
    pub fn full(&self) -> bool {
        self.population >= self.room()
    }
}

/// What `unit` is doing, None for those not counted: the dead, the shaman, wildmen.
pub fn state_of(map: &GameMap, unit: &Unit) -> Option<State> {
    if !unit.is_alive() || kind_index(unit.kind).is_none() {
        return None;
    }
    let leaving = matches!(unit.action, Action::Walking { .. } | Action::Stranded { .. });
    if let Some(inside) = unit.inside.filter(|_| !leaving) {
        let hut = map.building_at_corner(inside.site).map(|b| &map.buildings[b]).is_some_and(|b| matches!(b.kind, BuildingKind::Hut { .. }) && b.stage() == Stage::Built);
        return Some(if hut { State::Housed } else { State::Working });
    }
    let working = match unit.action {
        Action::Chopping { .. } | Action::Holding { .. } | Action::Flattening { .. } | Action::Building { .. } | Action::Entering { .. } | Action::Hammering | Action::Worshipping { .. } | Action::AroundFire { .. } => true,
        Action::Idle | Action::Walking { .. } | Action::Stranded { .. } | Action::Casting { .. } | Action::Landing { .. } | Action::Drowning | Action::Dying { .. } | Action::Dead { .. } => false,
    };
    Some(if working || unit.busy() { State::Working } else { State::Idle })
}

/// The kinds among the living units `ids` (the selection; an AI's group), unknown ids ignored.
pub fn kinds_of(map: &GameMap, ids: &[u32]) -> Counts {
    let mut counts = Counts::default();
    for u in map.units.iter().filter(|u| u.is_alive() && ids.contains(&u.id)) {
        counts.add(u.kind);
    }
    counts
}

impl GameMap {
    pub fn headcount(&self, tribe: u8) -> Headcount {
        let mut count = Headcount::default();
        for u in self.units.iter().filter(|u| u.owner == tribe) {
            if let Some(state) = state_of(self, u) {
                count.rows[state.index()].add(u.kind);
            }
        }
        count
    }

    /// The tribe's units doing `state`, of `kind` (any counted kind when None), in id order.
    pub fn units_in(&self, tribe: u8, state: State, kind: Option<UnitKind>) -> impl Iterator<Item = &Unit> {
        let mut units: Vec<&Unit> = self.units.iter().filter(|u| u.owner == tribe && kind.is_none_or(|k| u.kind == k) && state_of(self, u) == Some(state)).collect();
        units.sort_by_key(|u| u.id);
        units.into_iter()
    }

    /// The unit a pick adds: the lowest id doing `state` not in `taken`, of `kind`, or when None of the
    /// first kind in `KINDS` order with one left.
    pub fn next_unit(&self, tribe: u8, state: State, kind: Option<UnitKind>, taken: &[u32]) -> Option<u32> {
        let kinds: &[UnitKind] = match kind {
            Some(ref k) => std::slice::from_ref(k),
            None => &KINDS,
        };
        kinds.iter().find_map(|&k| self.units_in(tribe, state, Some(k)).map(|u| u.id).find(|id| !taken.contains(id)))
    }

    /// The tribe's population (living units but the shaman and wildmen) and its built huts per size.
    pub fn housing(&self, tribe: u8) -> Housing {
        let population = self.units.iter().filter(|u| u.owner == tribe && u.is_alive() && kind_index(u.kind).is_some()).count() as u16;
        let mut housing = Housing { huts: [0; 3], population };
        for b in self.buildings.iter().filter(|b| b.owner == tribe && b.stage() == Stage::Built) {
            if let BuildingKind::Hut { size } = b.kind {
                housing.huts[size.clamp(1, 3) as usize - 1] += 1;
            }
        }
        housing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::Building;
    use crate::totem::{Totem, TotemKind};
    use crate::unit::Inside;

    fn empty() -> GameMap {
        let mut map = GameMap::sandbox_units();
        map.units.clear();
        map.buildings.clear();
        map.totems.clear();
        map
    }

    fn add(map: &mut GameMap, owner: u8, kind: UnitKind) -> usize {
        let id = map.units.iter().map(|u| u.id).max().unwrap_or(0) + 1;
        map.units.push(Unit::new(id, owner, kind, (64 * 512, 64 * 512)));
        map.units.len() - 1
    }

    fn hut(map: &mut GameMap, owner: u8, size: u8, at: u16) -> (u16, u16) {
        map.buildings.push(Building::new(BuildingKind::Hut { size }, owner, at, at, 0));
        (at, at)
    }

    #[test]
    fn an_empty_tribe_counts_nothing() {
        let map = empty();
        assert_eq!(map.headcount(0), Headcount::default());
        assert_eq!(map.housing(0), Housing::default());
    }

    #[test]
    fn the_shaman_wildmen_the_dead_and_other_tribes_are_not_counted() {
        let mut map = empty();
        add(&mut map, 0, UnitKind::Shaman);
        add(&mut map, 0, UnitKind::Wildman);
        let dead = add(&mut map, 0, UnitKind::Brave);
        map.units[dead].action = Action::Dying { left: crate::time::Countdown::new(crate::time::Ticks::new(3)) };
        add(&mut map, 1, UnitKind::Warrior);
        assert_eq!(map.headcount(0), Headcount::default());
        assert_eq!(map.housing(0).population, 0);
        assert_eq!(map.headcount(1).get(State::Idle, UnitKind::Warrior), 1);
    }

    #[test]
    fn each_brave_in_its_row() {
        let mut map = empty();
        let site = hut(&mut map, 0, 1, 10 * 512);
        let site_build = (20 * 512, 20 * 512);
        map.buildings.push(Building { used: 0, ..Building::new(BuildingKind::DrumTower, 0, site_build.0, site_build.1, 0) });
        add(&mut map, 0, UnitKind::Brave);
        let housed = add(&mut map, 0, UnitKind::Brave);
        map.units[housed].inside = Some(Inside { site, door: site });
        let builder = add(&mut map, 0, UnitKind::Brave);
        map.units[builder].inside = Some(Inside { site: site_build, door: site_build });
        map.units[builder].action = Action::Hammering;
        let chopping = add(&mut map, 0, UnitKind::Brave);
        map.units[chopping].action = Action::Chopping { tree: (0, 0), left: crate::time::Countdown::new(crate::time::Ticks::new(5)) };
        let carrying = add(&mut map, 0, UnitKind::Brave);
        map.units[carrying].carrying = 1;
        map.units[carrying].action = Action::Walking { to: (0, 0) };
        let praying = add(&mut map, 0, UnitKind::Brave);
        map.units[praying].action = Action::Worshipping { site: (0, 0) };
        let walking = add(&mut map, 0, UnitKind::Brave);
        map.units[walking].action = Action::Walking { to: (0, 0) };
        let leaving = add(&mut map, 0, UnitKind::Brave);
        map.units[leaving].inside = Some(Inside { site, door: site });
        map.units[leaving].action = Action::Walking { to: (0, 0) };
        let count = map.headcount(0);
        assert_eq!(count.get(State::Idle, UnitKind::Brave), 3, "standing, walking on a plain move, leaving the hut");
        assert_eq!(count.get(State::Housed, UnitKind::Brave), 1);
        assert_eq!(count.get(State::Working, UnitKind::Brave), 4, "builder inside a site, chopping, carrying, praying");
        assert_eq!(count.row(State::InBoat).total() + count.row(State::InBalloon).total(), 0);
        let all: u16 = State::ALL.iter().map(|&s| count.row(s).total()).sum();
        assert_eq!(all, map.housing(0).population);
    }

    #[test]
    fn praying_at_a_totem_is_working() {
        let mut map = empty();
        map.totems = vec![Totem::new(TotemKind::StoneHead, (0, 0))];
        let u = add(&mut map, 0, UnitKind::Preacher);
        map.units[u].action = Action::Worshipping { site: (0, 0) };
        assert_eq!(map.headcount(0).get(State::Working, UnitKind::Preacher), 1);
    }

    #[test]
    fn picks_go_in_id_order_and_skip_the_taken() {
        let mut map = empty();
        let warrior = add(&mut map, 0, UnitKind::Warrior);
        let a = add(&mut map, 0, UnitKind::Brave);
        let b = add(&mut map, 0, UnitKind::Brave);
        let (w, a, b) = (map.units[warrior].id, map.units[a].id, map.units[b].id);
        assert_eq!(map.next_unit(0, State::Idle, Some(UnitKind::Brave), &[]), Some(a));
        assert_eq!(map.next_unit(0, State::Idle, Some(UnitKind::Brave), &[a]), Some(b));
        assert_eq!(map.next_unit(0, State::Idle, Some(UnitKind::Brave), &[a, b]), None);
        assert_eq!(map.next_unit(0, State::Idle, None, &[]), Some(a), "braves first, though the warrior's id is lower");
        assert_eq!(map.next_unit(0, State::Idle, None, &[a, b]), Some(w));
        assert_eq!(map.next_unit(0, State::Housed, None, &[]), None);
        assert_eq!(map.units_in(0, State::Idle, None).map(|u| u.id).collect::<Vec<_>>(), vec![w, a, b]);
    }

    #[test]
    fn kinds_of_ignores_unknown_and_dead_ids() {
        let mut map = empty();
        let a = add(&mut map, 0, UnitKind::Spy);
        let dead = add(&mut map, 0, UnitKind::Spy);
        map.units[dead].action = Action::Dead { left: crate::time::Countdown::new(crate::time::Ticks::new(3)) };
        let ids = [map.units[a].id, map.units[dead].id, 999];
        assert_eq!(kinds_of(&map, &ids).get(UnitKind::Spy), 1);
        assert_eq!(kinds_of(&map, &ids).total(), 1);
    }

    #[test]
    fn housing_counts_built_huts_by_size_up_to_two_hundred() {
        let mut map = empty();
        assert!(map.housing(0).full(), "no hut: no room");
        hut(&mut map, 0, 1, 512);
        hut(&mut map, 0, 1, 2 * 512);
        hut(&mut map, 0, 3, 3 * 512);
        hut(&mut map, 1, 2, 4 * 512);
        map.buildings.push(Building { used: 0, ..Building::new(BuildingKind::Hut { size: 2 }, 0, 5 * 512, 5 * 512, 0) });
        for _ in 0..12 {
            add(&mut map, 0, UnitKind::Brave);
        }
        let housing = map.housing(0);
        assert_eq!(housing.huts, [2, 0, 1], "built huts of the tribe only");
        assert_eq!(housing.room(), 13);
        assert_eq!(housing.population, 12);
        assert!(!housing.full());
        add(&mut map, 0, UnitKind::Warrior);
        assert!(map.housing(0).full(), "at the room");
        assert_eq!(Housing { huts: [0, 0, 29], population: 0 }.room(), MAX_POPULATION);
    }
}
