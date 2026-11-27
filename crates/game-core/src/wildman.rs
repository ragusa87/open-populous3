//! Wildmen's idle loop (pop3-rev-analysis.md "Wildmen", fully traced): with nothing else to do, an idle
//! wildman looks for a tree or plant to eat beside, within `TREE_SEARCH_RADIUS_CELLS`; failing that, a shore
//! cell to drink at, within `SHORE_SEARCH_RADIUS_CELLS`. Confirmed `[exe]`: the full cycle is state 1 -> 8 ->
//! (5 -> 15 eating, or 6 -> 16 drinking) -> 8 -> ..., with no hunger or thirst stat anywhere - both searches only
//! ever pick where an idle animation plays, never consuming anything. A meal lasts `meal_ticks` and the next
//! tree search waits `tree_rest` from arriving ("Eating at a tree"). No plant scenery (types 7/8, trees.md) is
//! modelled yet, so only trees are searched here; the original's own 14-byte coastal-shape table is not
//! implemented either, so "shore" substitutes `is_shore_cell`'s own neighbour-based approximation (a walkable
//! cell with a sea neighbour) - **both `[ours]`**. Also `[ours]`: `SEEK_EVERY` (how often an idle wildman
//! looks at all), `GRAZE_TICKS` (how long it drinks: state 16's timer is not pinned to a turn count), the walk
//! to the shore never given up on time (state 6's timer is not pinned either), and the tree rest stopping the
//! shore search too.
//! Drowning at the shore needs no code here: it already falls out of the generic per-turn drowning check once a
//! wildman stands on a wet spot (`Unit::tick`'s own drowning branch, kind-independent).

use crate::map::{torus_cells, torus_dist2, GameMap};
use crate::path::{Ground, Mobility};
use crate::time::{Every, Tick, Ticks};
use crate::unit::{cell_of, is_sea, Action, UnitKind};

/// A tree or plant within this many cells is eaten beside: the same radius byte as the shore search
/// ("Eating at a tree" [exe]).
pub const TREE_SEARCH_RADIUS_CELLS: i32 = 12;
/// A shore cell within this many cells is drunk at if no tree/plant was found [exe, confirmed].
pub const SHORE_SEARCH_RADIUS_CELLS: i32 = 12;
/// How often an idle wildman looks for somewhere to go: a guess, no rhythm given for state 8 itself.
pub const SEEK_EVERY: Every = Every::new(16);
/// How long it drinks at the shore before wandering off again: a guess, state 16's timer is not pinned.
pub const GRAZE_TICKS: Ticks = Ticks::secs(8);
/// A meal beside a tree: `MEAL_MIN` + rand(`MEAL_SPAN`) turns (state 15's `+0x70`, "Eating at a tree" [exe]).
pub const MEAL_MIN: u32 = 50;
pub const MEAL_SPAN: u32 = 50;
/// No tree search for 4 x (`REST_MIN` + rand(`REST_SPAN`)) turns from arriving at a tree (`+0x7B`, from the
/// per-type byte 64 halved, ticked down every 4 turns: "Eating at a tree" [exe]).
pub const REST_MIN: u32 = 32;
pub const REST_SPAN: u32 = 32;
pub const REST_STEP: u32 = 4;

/// A walk to a tree is given up after this long (state 5's `+0x70`, "Eating at a tree" [exe]).
pub const TREE_WALK_TICKS: Ticks = Ticks::new(128);
/// Within this many world units of its tree on both axes, a wildman has arrived ("Eating at a tree" [exe]).
pub const TREE_REACH: i32 = 120;

/// Where a wildman walks to eat (a tree) or drink (a shore spot).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GrazeWalk {
    pub drink: bool,
    pub at: (u16, u16),
    /// When it stops walking there (none: never, the shore's).
    pub give_up: Option<Tick>,
}

/// What a wildman on its way to graze does this turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalkTurn {
    KeepGoing,
    Arrived,
    GiveUp,
}

/// Its walk this turn: given up once its tree or shore spot is gone or its time is out; at a tree, arrived
/// within `TREE_REACH` (the shore's arrival is the walk's own end).
pub fn walk_turn(walk: GrazeWalk, from: (u16, u16), now: Tick, still_there: bool) -> WalkTurn {
    let near = |a: u16, b: u16| crate::unit::torus_delta(a, b).abs() <= TREE_REACH;
    if !still_there || walk.give_up.is_some_and(|t| now.since(t) >= 0) {
        WalkTurn::GiveUp
    } else if !walk.drink && near(from.0, walk.at.0) && near(from.1, walk.at.1) {
        WalkTurn::Arrived
    } else {
        WalkTurn::KeepGoing
    }
}

fn draw(id: crate::unit::UnitId, now: Tick, salt: u32) -> u32 {
    crate::map::Lcg(id.raw().wrapping_mul(2_654_435_761).wrapping_add(now.to_wire()) ^ salt).next_u32()
}

/// How long a wildman arriving at its tree at `now` eats there.
pub fn meal_ticks(id: crate::unit::UnitId, now: Tick) -> Ticks {
    Ticks::new(MEAL_MIN + draw(id, now, 0) % MEAL_SPAN)
}

/// How long after arriving at its tree at `now` a wildman looks for a tree again.
pub fn tree_rest(id: crate::unit::UnitId, now: Tick) -> Ticks {
    Ticks::new(REST_STEP * (REST_MIN + draw(id, now, 0x5EED) % REST_SPAN))
}

impl GameMap {
    /// Idle wildmen, on their own rhythm, go eat beside the nearest tree in reach, or else drink at the
    /// nearest shore cell in reach.
    pub(crate) fn tend_wildmen(&mut self) {
        let now = self.now;
        for i in 0..self.units.len() {
            let u = &self.units[i];
            if u.kind != UnitKind::Wildman {
                continue;
            }
            if let Some(walk) = u.graze_walk() {
                let there = if walk.drink { self.is_shore_cell((cell_of(walk.at.0), cell_of(walk.at.1))) } else { self.tree_at(walk.at) };
                match walk_turn(walk, (u.x, u.z), now, there) {
                    WalkTurn::GiveUp => self.units[i].stop_grazing(),
                    WalkTurn::Arrived => self.units[i].start_grazing(now),
                    WalkTurn::KeepGoing => {}
                }
            } else if let Action::Grazing { drink: false, at, .. } = u.action {
                if !self.tree_at(at) {
                    self.units[i].stop_grazing();
                }
            }
        }
        let seeking: Vec<usize> = (0..self.units.len())
            .filter(|&i| {
                let u = &self.units[i];
                u.kind == UnitKind::Wildman && u.action == Action::Idle && SEEK_EVERY.fires(now, u.id.raw()) && now.since(u.tree_rest_until) >= 0
            })
            .collect();
        for i in seeking {
            let at = (self.units[i].x, self.units[i].z);
            if let Some(tree) = self.nearest_tree(at) {
                self.units[i].go_graze(GrazeWalk { drink: false, at: tree, give_up: Some(now + TREE_WALK_TICKS) });
            } else if let Some(shore) = self.nearest_shore_cell(at) {
                self.units[i].go_graze(GrazeWalk { drink: true, at: shore, give_up: None });
            }
        }
    }

    /// Whether a standing tree is still at `at`.
    fn tree_at(&self, at: (u16, u16)) -> bool {
        self.trees.iter().any(|t| t.is_visible() && (t.x, t.z) == at)
    }

    /// The nearest visible tree within `TREE_SEARCH_RADIUS_CELLS` of `at`, if any.
    fn nearest_tree(&self, at: (u16, u16)) -> Option<(u16, u16)> {
        self.trees
            .iter()
            .filter(|t| t.is_visible() && torus_cells(at, (t.x, t.z)) <= TREE_SEARCH_RADIUS_CELLS)
            .min_by_key(|t| torus_dist2(at, (t.x, t.z)))
            .map(|t| (t.x, t.z))
    }

    /// Whether `cell` is dry, walkable, and has a sea cell among its 4 neighbours (`[ours]`: a stand-in for the
    /// original's own coastal-shape table, not implemented).
    fn is_shore_cell(&self, cell: (i32, i32)) -> bool {
        let size = self.terrain.size() as i32;
        let wrap = |c: (i32, i32)| (c.0.rem_euclid(size), c.1.rem_euclid(size));
        !is_sea(&self.terrain, cell)
            && self.ground().passable(Mobility::Walk, cell)
            && [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dz)| is_sea(&self.terrain, wrap((cell.0 + dx, cell.1 + dz))))
    }

    /// The nearest shore cell's centre within `SHORE_SEARCH_RADIUS_CELLS` of `at`, if any.
    fn nearest_shore_cell(&self, at: (u16, u16)) -> Option<(u16, u16)> {
        let size = self.terrain.size() as i32;
        let (cx, cz) = (crate::unit::cell_of(at.0), crate::unit::cell_of(at.1));
        let r = SHORE_SEARCH_RADIUS_CELLS;
        (-r..=r)
            .flat_map(|dz| (-r..=r).map(move |dx| (dx, dz)))
            .map(|(dx, dz)| ((cx + dx).rem_euclid(size), (cz + dz).rem_euclid(size)))
            .filter(|&cell| self.is_shore_cell(cell))
            .map(crate::slots::cell_centre)
            .min_by_key(|&p| torus_dist2(at, p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tribes::Owner;
    use crate::terrain::Heightmap;
    use crate::time::Ticks as T;

    fn map_with(land: u16) -> GameMap {
        let mut terrain = Heightmap::new(64);
        for z in 0..64 {
            for x in 0..64 {
                terrain.set(x, z, land);
            }
        }
        GameMap::bare("wildman-test", terrain, Vec::new())
    }

    const AT: (u16, u16) = (32 * 512 + 256, 32 * 512 + 256);

    #[test]
    fn an_idle_wildman_walks_to_the_nearest_tree_in_reach_then_eats() {
        let mut map = map_with(64);
        let w = map.spawn(Owner::Wild, UnitKind::Wildman, AT).id;
        map.trees = vec![crate::tree::Tree::new((35, 32), 0, 3)];
        let tree_pos = (map.trees[0].x, map.trees[0].z);
        map.run(T::new(SEEK_EVERY.period()));
        let arrived = (0..300).find(|_| {
            map.run(T::new(1));
            matches!(map.unit(w).unwrap().action, Action::Grazing { drink: false, .. })
        });
        assert!(arrived.is_some(), "eating beside the tree");
        let (dx, dz) = (crate::unit::torus_delta(map.unit(w).unwrap().x, tree_pos.0), crate::unit::torus_delta(map.unit(w).unwrap().z, tree_pos.1));
        assert!((dx.pow(2) + dz.pow(2)) < (3 * 512i32).pow(2), "stood close to the tree it is eating beside");
    }

    #[test]
    fn with_no_tree_in_reach_it_drinks_at_the_shore_instead() {
        let mut map = map_with(64);
        for z in 25..40 {
            for x in 40..64 {
                map.terrain.set(x, z, 0);
            }
        }
        let w = map.spawn(Owner::Wild, UnitKind::Wildman, AT).id;
        map.run(T::new(SEEK_EVERY.period()));
        let arrived = (0..400).find(|_| {
            map.run(T::new(1));
            matches!(map.unit(w).unwrap().action, Action::Grazing { drink: true, .. })
        });
        assert!(arrived.is_some(), "drinking at the shore");
        let u = map.unit(w).unwrap();
        assert!(map.is_shore_cell((cell_of(u.x), cell_of(u.z))), "standing on the shore it drinks at");
    }

    #[test]
    fn it_wanders_off_again_after_grazing_a_while() {
        let mut map = map_with(64);
        let w = eating_wildman(&mut map);
        assert!(matches!(map.unit(w).unwrap().action, Action::Grazing { .. }), "test setup: it is grazing");
        map.run(T::new(MEAL_MIN + MEAL_SPAN));
        assert_eq!(map.unit(w).unwrap().action, Action::Idle, "left on its own after its meal");
        map.run(T::new(SEEK_EVERY.period() * 2));
        assert_eq!(map.unit(w).unwrap().action, Action::Idle, "resting before the next tree");
        map.run(T::new(REST_STEP * (REST_MIN + REST_SPAN)));
        let back = (0..300).any(|_| {
            map.run(T::new(1));
            matches!(map.unit(w).unwrap().action, Action::Grazing { drink: false, .. })
        });
        assert!(back, "eats again once rested");
    }

    #[test]
    fn a_meal_and_the_rest_after_it_stay_in_the_traced_ranges() {
        let id = |n| crate::unit::UnitId::from_raw(n);
        for n in 0..200 {
            let now = Tick::from_wire(n * 37);
            let meal = meal_ticks(id(n), now).get();
            let rest = tree_rest(id(n), now).get();
            assert!((MEAL_MIN..MEAL_MIN + MEAL_SPAN).contains(&meal));
            assert!((REST_STEP * REST_MIN..REST_STEP * (REST_MIN + REST_SPAN)).contains(&rest) && rest % REST_STEP == 0);
            assert!(meal < rest, "the meal ends inside the rest");
        }
    }

    #[test]
    fn a_walk_to_a_tree_arrives_near_it_and_gives_up_when_it_is_gone_or_too_long() {
        let tree = GrazeWalk { drink: false, at: (1000, 1000), give_up: Some(Tick::from_wire(128)) };
        let now = Tick::from_wire(10);
        assert_eq!(walk_turn(tree, (900, 1100), now, true), WalkTurn::Arrived, "within 120 on both axes");
        assert_eq!(walk_turn(tree, (879, 1000), now, true), WalkTurn::KeepGoing);
        assert_eq!(walk_turn(tree, (900, 1000), now, false), WalkTurn::GiveUp, "the tree is gone");
        assert_eq!(walk_turn(tree, (0, 0), Tick::from_wire(128), true), WalkTurn::GiveUp, "out of time");
        let shore = GrazeWalk { drink: true, at: (1000, 1000), give_up: None };
        assert_eq!(walk_turn(shore, (1000, 1000), Tick::from_wire(9999), true), WalkTurn::KeepGoing, "the walk's own end");
        assert_eq!(walk_turn(shore, (0, 0), now, false), WalkTurn::GiveUp, "no longer shore");
    }

    fn eating_wildman(map: &mut GameMap) -> crate::unit::UnitId {
        let w = map.spawn(Owner::Wild, UnitKind::Wildman, AT).id;
        map.trees = vec![crate::tree::Tree::new((35, 32), 0, 3)];
        map.run(T::new(SEEK_EVERY.period()));
        (0..300).find(|_| {
            map.run(T::new(1));
            matches!(map.unit(w).unwrap().action, Action::Grazing { .. })
        });
        w
    }

    #[test]
    fn it_faces_its_tree_close_by_and_stops_eating_once_it_is_gone() {
        let mut map = map_with(64);
        let w = eating_wildman(&mut map);
        let u = map.unit(w).unwrap();
        let Action::Grazing { drink: false, at, .. } = u.action else { panic!("eating: {:?}", u.action) };
        assert_eq!(at, (map.trees[0].x, map.trees[0].z));
        let (dx, dz) = (crate::unit::torus_delta(u.x, at.0), crate::unit::torus_delta(u.z, at.1));
        assert!(dx.abs() <= TREE_REACH && dz.abs() <= TREE_REACH);
        assert_eq!(u.facing, crate::unit::octant(dx, dz));
        map.trees.clear();
        map.run(T::new(1));
        assert_eq!(map.unit(w).unwrap().action, Action::Idle);
    }

    #[test]
    fn a_wildman_walking_to_a_tree_that_goes_stops() {
        let mut map = map_with(64);
        let w = map.spawn(Owner::Wild, UnitKind::Wildman, AT).id;
        map.trees = vec![crate::tree::Tree::new((40, 32), 0, 3)];
        map.run(T::new(SEEK_EVERY.period()));
        assert!(map.unit(w).unwrap().graze_walk().is_some(), "test setup: on its way");
        map.trees.clear();
        map.run(T::new(1));
        assert_eq!(map.unit(w).unwrap().action, Action::Idle);
        assert!(map.unit(w).unwrap().graze_walk().is_none());
    }

    #[test]
    fn is_shore_cell_wants_dry_walkable_ground_next_to_the_sea() {
        let mut map = map_with(64);
        for z in 29..=32 {
            for x in 30..=33 {
                map.terrain.set(x, z, 0);
            }
        }
        assert!(!map.is_shore_cell((31, 30)), "wet itself: not shore, that's open sea");
        assert!(map.is_shore_cell((30, 30)), "dry, right next to that sea cell");
        assert!(!map.is_shore_cell((10, 10)), "far from any sea: just inland");
    }
}
