//! Followers kneeling to their shaman (pop3-rev-analysis.md "Worshipping the shaman", person state 19 sub-state 9
//! [exe]): a sub-state of standing idle, so any order ends it. The shaman recruits from the 3 × 3 cells around her
//! own, whatever she does: one standing follower of her tribe every 8 turns at her own phase, everyone at once while
//! she stands at her reincarnation site. A worshipper turns to face her every 4 turns, and every 32 turns stays only
//! while a shaman of its tribe is within 4.6 cells on both axes.

use crate::map::GameMap;
use crate::time::{Countdown, Every, Tick, Ticks};
use crate::tribes::Owner;
use crate::unit::{Action, Unit, UnitKind, octant, torus_delta};

/// The shaman recruits on this rhythm, at her own phase [exe].
pub const RECRUIT: Every = Every::new(8);
/// Recruits a turn away from her site, and at it [exe].
pub const RECRUIT_CAP: usize = 1;
pub const RECRUIT_CAP_AT_SITE: usize = 200;
/// A worshipper's stay, checked again at its end [exe].
pub const STAY: Ticks = Ticks::new(32);
/// It stays while a shaman of its tribe is this close on both axes, world units [exe].
pub const STAY_REACH: i32 = 2360;
/// It turns to face her on this rhythm, at its own phase [exe].
pub const FACE: Every = Every::new(4);

/// A follower kneeling to its shaman: since when (for its prayer's arm cycles) and its stay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kneel {
    pub since: Tick,
    pub stay: Countdown,
}

/// Whether `u` may be recruited by a shaman of `owner`: a follower of her tribe (a ghost too) standing idle on the
/// ground, not already kneeling.
pub fn recruitable(u: &Unit, owner: Owner) -> bool {
    UnitKind::FOLLOWERS.contains(&u.kind)
        && u.owner == owner
        && u.is_alive()
        && u.action == Action::Idle
        && u.in_the_open()
        && u.aboard.is_none()
        && !u.motion.airborne()
        && u.kneeling().is_none()
}

/// Whether the cell `c` is one of the 3 × 3 cells around `centre`, across the map's edge of `size` cells.
pub fn around(centre: (i32, i32), c: (i32, i32), size: i32) -> bool {
    let d = |a: i32, b: i32| (a - b).rem_euclid(size).min((b - a).rem_euclid(size));
    d(c.0, centre.0) <= 1 && d(c.1, centre.1) <= 1
}

/// Whether `at` is within `STAY_REACH` of `shaman` on both axes.
pub fn in_reach(at: (u16, u16), shaman: (u16, u16)) -> bool {
    torus_delta(at.0, shaman.0).abs() <= STAY_REACH && torus_delta(at.1, shaman.1).abs() <= STAY_REACH
}

/// A shaman recruiting this turn: her tribe, where she stands, and how many she may take.
struct Recruiter {
    owner: Owner,
    at: (u16, u16),
    cell: (i32, i32),
    cap: usize,
}

impl GameMap {
    /// Every shaman's recruiting on her turn, then every worshipper's turn: facing her, and its stay checked.
    pub(crate) fn tend_kneeling(&mut self) {
        let now = self.now;
        let size = self.terrain.size() as i32;
        let shamans: Vec<Recruiter> = self
            .units
            .iter()
            .filter(|u| u.kind == UnitKind::Shaman && u.is_alive() && !u.statuses.ghost && RECRUIT.fires(now, u.id.raw()))
            .filter_map(|u| {
                let tribe = u.owner.tribe()?;
                let at_site = u.action == Action::Idle && self.sites.iter().any(|s| s.owner == tribe && s.enabled && u.cell() == (crate::unit::cell_of(s.x), crate::unit::cell_of(s.z)));
                Some(Recruiter { owner: Owner::Tribe(tribe), at: (u.x, u.z), cell: u.cell(), cap: if at_site { RECRUIT_CAP_AT_SITE } else { RECRUIT_CAP } })
            })
            .collect();
        for r in shamans {
            for u in self.units.iter_mut().filter(|u| recruitable(u, r.owner) && around(r.cell, u.cell(), size)).take(r.cap) {
                u.kneel(Kneel { since: now, stay: Countdown::new(STAY) });
                u.face(r.at);
            }
        }
        for i in 0..self.units.len() {
            let Some(mut k) = self.units[i].kneeling() else { continue };
            let shaman = self.units[i].owner.tribe().and_then(|t| self.shaman_of(t)).filter(|s| s.is_alive()).map(|s| (s.x, s.z));
            let u = &mut self.units[i];
            if let Some(at) = shaman
                && FACE.fires(now, u.id.raw())
            {
                let (dx, dz) = (torus_delta(u.x, at.0), torus_delta(u.z, at.1));
                if (dx, dz) != (0, 0) {
                    u.facing = octant(dx, dz);
                }
            }
            if k.stay.tick() {
                if shaman.is_some_and(|at| in_reach((u.x, u.z), at)) {
                    k.stay = Countdown::new(STAY);
                } else {
                    u.stand_up();
                    continue;
                }
            }
            u.kneel(k);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slots::cell_centre;
    use crate::tribes::TribeId;

    fn scene() -> (GameMap, Vec<crate::unit::UnitId>) {
        let mut map = GameMap::sandbox_walk();
        map.units.retain(|u| u.kind == UnitKind::Shaman && u.owner == TribeId::Blue);
        let her = map.units[0].cell();
        map.units[0].action = Action::Walking { to: (0, 0) };
        let braves = [(1, 0), (-1, 1), (0, -1), (2, 0)].map(|(dx, dz)| map.spawn(TribeId::Blue, UnitKind::Brave, cell_centre((her.0 + dx, her.1 + dz))).id);
        (map, braves.to_vec())
    }

    fn kneeling(map: &GameMap, ids: &[crate::unit::UnitId]) -> usize {
        ids.iter().filter(|&&id| map.unit(id).unwrap().kneeling().is_some()).count()
    }

    #[test]
    fn the_3x3_cells_around_across_the_edge() {
        assert!(around((0, 5), (127, 6), 128));
        assert!(!around((0, 5), (2, 5), 128));
    }

    #[test]
    fn one_recruit_every_8_turns_from_her_3x3_cells_whatever_she_does() {
        let (mut map, braves) = scene();
        let her = map.units[0].id;
        let first = (1..=8).find(|_| { map.tick_kneeling_only(); kneeling(&map, &braves) > 0 }).unwrap();
        assert_eq!(kneeling(&map, &braves), 1);
        assert!(RECRUIT.fires(map.now, her.raw()), "on her phase: turn {first}");
        for _ in 0..8 {
            map.tick_kneeling_only();
        }
        assert_eq!(kneeling(&map, &braves), 2);
        for _ in 0..80 {
            map.tick_kneeling_only();
        }
        assert_eq!(kneeling(&map, &braves), 3, "the one two cells off never");
        let k = map.unit(braves[0]).unwrap();
        assert_eq!(k.facing, octant(torus_delta(k.x, map.units[0].x), torus_delta(k.z, map.units[0].z)), "facing her");
    }

    #[test]
    fn it_stays_while_she_is_near_and_an_order_ends_it() {
        let (mut map, braves) = scene();
        (0..40).for_each(|_| map.tick_kneeling_only());
        let b = braves.iter().copied().find(|&id| map.unit(id).unwrap().kneeling().is_some()).unwrap();
        let (x, z) = (map.units[0].x, map.units[0].z);
        (map.units[0].x, map.units[0].z) = (x.wrapping_add(4 * 512), z);
        (0..STAY.get()).for_each(|_| map.tick_kneeling_only());
        assert!(map.unit(b).unwrap().kneeling().is_some(), "4 cells: still in reach");
        (map.units[0].x, map.units[0].z) = (x.wrapping_add(10 * 512), z);
        (0..STAY.get()).for_each(|_| map.tick_kneeling_only());
        assert!(map.unit(b).unwrap().kneeling().is_none(), "gone too far: back to standing");
        let mut u = Unit::new(crate::unit::UnitId::from_raw(99), TribeId::Blue, UnitKind::Brave, (0, 0));
        u.kneel(Kneel { since: Tick::ZERO, stay: Countdown::new(STAY) });
        u.order(crate::unit::Order::MoveTo { x: 2000, z: 0 });
        assert!(u.kneeling().is_none(), "an order ends it");
    }

    #[test]
    fn at_her_site_everyone_at_once() {
        let (mut map, braves) = scene();
        let site = map.sites.iter().find(|s| s.owner == TribeId::Blue).map(|s| (s.x, s.z)).unwrap();
        let her = crate::unit::cell_of(site.0);
        let her = (her, crate::unit::cell_of(site.1));
        (map.units[0].x, map.units[0].z, map.units[0].action) = (site.0, site.1, Action::Idle);
        for (k, &id) in braves.iter().take(3).enumerate() {
            let u = map.unit_mut(id).unwrap();
            (u.x, u.z) = cell_centre((her.0 + k as i32 - 1, her.1 + 1));
        }
        (0..8).for_each(|_| map.tick_kneeling_only());
        assert_eq!(kneeling(&map, &braves[..3]), 3);
    }

    impl GameMap {
        fn tick_kneeling_only(&mut self) {
            self.now = self.now.next();
            self.tend_kneeling();
        }
    }
}
