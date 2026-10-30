//! Spotting (docs/specs/combat.md "Spotting"): people who see an enemy attack it on their own. On its rhythm each
//! fighter that is not busy looks over a square of cells around it, row by row, and goes for the first enemy it finds.

use crate::building::BuildingId;
use crate::coordinator::Link;
use crate::map::GameMap;
use crate::unit::{Action, Target, Unit, UnitId, UnitKind};

/// How far `u` looks now, cells; None when it is busy ("Auto-attack" [exe]). Walking on an order it does not look
/// (0); working, only its own cell (`sight_at_work`); idle or hacking at a building, its full `sight`.
pub fn sight_now(u: &Unit) -> Option<u8> {
    if !u.is_alive() || (!u.in_the_open() && !u.kind.stats().throws) || u.thrown() {
        return None;
    }
    let stats = u.kind.stats();
    match u.action {
        Action::Idle | Action::Holding { .. } | Action::Hacking { .. } | Action::Preaching => Some(stats.sight),
        Action::Walking { .. } | Action::Stranded { .. } | Action::Attacking { .. } | Action::Guarding { .. } => Some(0),
        Action::Queuing { .. } | Action::Chopping { .. } | Action::PickingUpWood { .. } | Action::Flattening { .. } | Action::Building { .. } | Action::Hammering | Action::AroundFire { .. } | Action::Worshipping { .. } | Action::Grazing { .. } => Some(stats.sight_at_work),
        Action::Challenge { .. } | Action::Melee { .. } | Action::Listening { .. } | Action::Panicking { .. } | Action::OnFire { .. } | Action::Tumbling | Action::Settling { .. } | Action::Casting { .. } | Action::Landing | Action::Drowning | Action::Entering { .. } | Action::Boarding { .. } | Action::Alighting { .. } | Action::Throwing { .. } | Action::Dying { .. } | Action::Dead { .. } | Action::Spirit { .. } => None,
        #[cfg(test)]
        Action::Gathered { .. } => None,
    }
}

/// Kinds that look for a melee to start: the preacher only for a preacher or a shaman (`attack::may_attack`), the
/// firewarrior throws fire instead, the shaman only looks during some orders, spies and wildmen never look.
pub fn looks_for_melee(kind: UnitKind) -> bool {
    matches!(kind, UnitKind::Brave | UnitKind::Warrior | UnitKind::Preacher)
}

impl GameMap {
    /// How far `u` looks this turn, cells across, if it looks at all: a fighter by `sight_now`, a reloaded
    /// thrower as far as its fire reaches ("Auto-attack" [exe]).
    pub fn looking(&self, u: &Unit) -> Option<u8> {
        let sight = sight_now(u)?;
        if u.kind.stats().throws {
            (u.reload == 0 && sight > 0).then(|| crate::firebolt::reach_cells(self.altitude(u), self.in_tower(u)))
        } else {
            looks_for_melee(u.kind).then_some(sight)
        }
    }
}

/// The cells `sight` around `centre` covers, row by row from a corner: a square of `sight` rounded down to even, plus
/// one ("Auto-attack" [exe]).
pub fn square(centre: (i32, i32), sight: u8, size: i32) -> impl Iterator<Item = (i32, i32)> {
    let r = (sight / 2) as i32;
    (-r..=r).flat_map(move |dz| (-r..=r).map(move |dx| ((centre.0 + dx).rem_euclid(size), (centre.1 + dz).rem_euclid(size))))
}

/// The living people outside, by cell: built once for a turn of looking.
pub struct UnitGrid(Vec<((i32, i32), UnitId)>);

impl UnitGrid {
    pub fn new(units: &[Unit]) -> Self {
        let mut cells: Vec<((i32, i32), UnitId)> = units.iter().filter(|u| u.is_alive() && u.in_the_open()).map(|u| (u.cell(), u.id)).collect();
        cells.sort_by_key(|&(cell, id)| (cell.1, cell.0, id));
        UnitGrid(cells)
    }

    /// The people in `cell`, in id order.
    pub fn at(&self, cell: (i32, i32)) -> impl Iterator<Item = UnitId> + '_ {
        let key = (cell.1, cell.0);
        let start = self.0.partition_point(|&(c, _)| (c.1, c.0) < key);
        self.0[start..].iter().take_while(move |&&(c, _)| c == cell).map(|&(_, id)| id)
    }
}

impl GameMap {
    /// On the looking rhythm, fighters that are not busy attack the first enemy they see.
    pub(crate) fn spot_enemies(&mut self) {
        let now = self.now;
        let looks = |u: &Unit| u.kind.stats().look_every.fires(now, 0) && self.looking(u).is_some_and(|s| s > 0);
        let lookers: Vec<usize> = (0..self.units.len()).filter(|&i| looks(&self.units[i])).collect();
        if lookers.is_empty() {
            return;
        }
        let grid = UnitGrid::new(&self.units);
        for i in lookers {
            if let Some(target) = self.first_seen(i, &grid) {
                self.go_attack(i, target);
            }
        }
    }

    /// The first enemy unit `i` sees, cell by cell: a person to fight, else (when idle) one already fighting or a
    /// building. An invisible person is skipped entirely (traced, pop3-rev-analysis.md "Per spell" 6): this is
    /// auto-targeting only, a player who already has their id can still order an attack on them directly
    /// (`attack::may_attack`, untraced either way). A wildman (0xFF tribeless) is excluded here too, confirmed
    /// (pop3-rev-analysis.md "Wildmen"): `attack::may_attack` itself no longer excludes one, since an explicit
    /// player order is allowed through, so the scan must filter it out on its own.
    fn first_seen(&self, i: usize, grid: &UnitGrid) -> Option<Target> {
        let u = &self.units[i];
        let sight = self.looking(u)?;
        let throws = u.kind.stats().throws;
        let idle = matches!(u.action, Action::Idle | Action::Holding { .. }) && !throws;
        let size = self.terrain.size() as i32;
        square(u.cell(), sight, size).find_map(|cell| {
            let people = grid
                .at(cell)
                .filter_map(|id| self.unit(id))
                .filter(|t| crate::attack::may_attack(u, t) && t.kind != UnitKind::Wildman && !t.locked() && !t.statuses.invisible.is_on() && !self.tribes.allied(u.owner, t.owner));
            let mut fighting = None;
            for t in people {
                match self.linked(t) {
                    _ if throws => return Some(Target::Unit(t.id)),
                    Some(Link::Melee(_)) if idle && fighting.is_none() => fighting = Some(Target::Unit(t.id)),
                    Some(Link::Melee(_)) => {}
                    _ => return Some(Target::Unit(t.id)),
                }
            }
            fighting.or_else(|| if idle { self.building_in(u, cell).map(Target::Building) } else { None })
        })
    }

    /// An enemy building `u` may wear down standing on `cell`.
    fn building_in(&self, u: &Unit, cell: (i32, i32)) -> Option<BuildingId> {
        let centre = crate::slots::cell_centre(cell);
        self.buildings
            .iter()
            .find(|b| b.covers(centre, 0) && crate::attack::may_attack_building(u, b))
            .map(|b| b.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::Ticks;

    /// A brave of tribe 0 standing idle, and one of tribe 1 `gap` cells east of it.
    fn watch(gap: i32) -> (GameMap, UnitId, UnitId) {
        let mut map = GameMap::sandbox_units();
        let s = map.shaman_of(0).unwrap();
        let at = (s.x.wrapping_add(1024), s.z.wrapping_add(3 * 512));
        let a = map.spawn(0, UnitKind::Brave, at).id;
        let t = map.spawn(1, UnitKind::Brave, (at.0.wrapping_add((gap * 512) as u16), at.1)).id;
        (map, a, t)
    }

    fn attacking(map: &GameMap, u: UnitId) -> Option<Target> {
        match map.unit(u)?.action {
            Action::Attacking { target, .. } => Some(target),
            _ => None,
        }
    }

    #[test]
    fn a_brave_sees_two_cells_away_not_three() {
        let (mut map, a, _) = watch(2);
        map.run(Ticks::new(4));
        assert!(map.unit(a).is_some_and(|u| u.action != Action::Idle), "it went for the enemy");
        let (mut map, a, _) = watch(3);
        map.run(Ticks::new(4));
        assert_eq!(attacking(&map, a), None);
    }

    #[test]
    fn the_automatic_scan_never_picks_a_wildman_even_in_reach() {
        let (mut map, a, t) = watch(1);
        map.unit_mut(t).unwrap().kind = UnitKind::Wildman;
        map.run(Ticks::new(4));
        assert_eq!(attacking(&map, a), None, "the scan skips it, unlike an explicit order (attack.rs)");
    }

    #[test]
    fn a_preacher_fights_a_preacher_it_sees_never_a_brave() {
        let (mut map, a, t) = watch(1);
        map.unit_mut(a).unwrap().kind = UnitKind::Preacher;
        map.run(Ticks::new(4));
        assert_ne!(attacking(&map, a), Some(Target::Unit(t)), "a brave is no fight for a preacher");
        let (mut map, a, t) = watch(1);
        map.unit_mut(a).unwrap().kind = UnitKind::Preacher;
        map.unit_mut(t).unwrap().kind = UnitKind::Preacher;
        map.run(Ticks::new(4));
        assert!(map.linked(map.unit(t).unwrap()).is_some() || attacking(&map, a) == Some(Target::Unit(t)));
    }

    #[test]
    fn row_by_row_from_a_corner() {
        let cells: Vec<_> = square((10, 10), 3, 128).collect();
        assert_eq!(cells.len(), 9, "3 rounds down to 2: a 3 × 3 square");
        assert_eq!((cells[0], cells[1], cells[8]), ((9, 9), (10, 9), (11, 11)));
        assert_eq!(square((0, 0), 1, 128).collect::<Vec<_>>(), [(0, 0)], "its own cell only");
        assert!(square((0, 0), 5, 128).any(|c| c == (126, 126)), "across the map's edge");
    }

    #[test]
    fn who_looks_how_far() {
        let (map, a, _) = watch(9);
        let mut u = map.unit(a).unwrap().clone();
        assert_eq!(sight_now(&u), Some(5));
        u.action = Action::Walking { to: (0, 0) };
        assert_eq!(sight_now(&u), Some(0), "walking on an order");
        u.action = Action::Hammering;
        assert_eq!(sight_now(&u), Some(1), "working: its own cell");
        u.action = Action::Panicking { away: (1, 0), left: crate::time::Countdown::new(Ticks::new(3)) };
        assert_eq!(sight_now(&u), None, "busy");
        assert!(!looks_for_melee(UnitKind::Spy) && !looks_for_melee(UnitKind::Wildman) && looks_for_melee(UnitKind::Warrior) && looks_for_melee(UnitKind::Preacher));
    }

    #[test]
    fn a_reloaded_firewarrior_looks_as_far_as_it_throws() {
        let (mut map, a, _) = watch(4);
        map.unit_mut(a).unwrap().kind = UnitKind::Firewarrior;
        let u = map.unit(a).unwrap().clone();
        assert_eq!(map.looking(&u), Some(crate::firebolt::reach_cells(map.altitude(&u), false)));
        map.unit_mut(a).unwrap().reload = 3;
        assert_eq!(map.looking(map.unit(a).unwrap()), None, "reloading");
        map.unit_mut(a).unwrap().reload = 0;
        map.run(Ticks::new(4));
        let thrown_at = match map.unit(a).unwrap().action {
            Action::Throwing { target: Target::Unit(x), .. } | Action::Attacking { target: Target::Unit(x), .. } => map.unit(x),
            _ => None,
        };
        assert!(thrown_at.is_some_and(|x| x.owner != 0), "throws at the first enemy it sees");

    }

    #[test]
    fn a_listener_sitting_down_to_a_sermon_is_not_picked_as_a_target() {
        let mut map = GameMap::sandbox_units().seeded(11);
        let s = map.shaman_of(0).unwrap();
        let at = (s.x.wrapping_add(1024), s.z.wrapping_sub(15 * 512));
        map.spawn(0, UnitKind::Preacher, at);
        let l = map.spawn(1, UnitKind::Warrior, (at.0.wrapping_add(512), at.1)).id;
        let listening = |m: &GameMap| matches!(m.unit(l).map(|u| u.action), Some(Action::Listening { .. }));
        (1..10).find(|_| {
            map.tick();
            listening(&map)
        })
        .expect("sat down");
        let at_l = map.unit(l).map(|u| (u.x, u.z)).unwrap();
        let a = map.spawn(0, UnitKind::Brave, (at_l.0.wrapping_add(512), at_l.1)).id;
        map.run(Ticks::new(10));
        assert!(listening(&map), "still listening");
        assert_eq!(attacking(&map, a), None, "a listener is left alone");
    }

    #[test]
    fn the_grid_finds_people_by_cell() {
        let (map, a, t) = watch(2);
        let grid = UnitGrid::new(&map.units);
        let (ca, ct) = (map.unit(a).unwrap().cell(), map.unit(t).unwrap().cell());
        assert!(grid.at(ca).any(|id| id == a));
        assert_eq!(grid.at(ct).collect::<Vec<_>>(), [t]);
    }
}
