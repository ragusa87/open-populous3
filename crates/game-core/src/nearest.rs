//! The nearest-N selector shared by Magical Shield, Bloodlust, Invisibility and Hypnotism
//! (pop3-rev-analysis.md 0x515E30, "nearest-N selector"): the `n` closest units to a point matching `pred`,
//! nearest first. Ties (equal distance) break on the lowest unit id: not confirmed against the original
//! (docs/plans/spells.md "Magical Shield"), but keeps the pick replay-safe across lockstep peers.
//!
//! The selector only ever looks at its own 3 × 3 cells around the target (pop3-rev-analysis.md "Components",
//! "Nearest-N selector": "The 3 × 3 cells around the target"), not the whole map: each of its four callers
//! composes `in_block` into its own `pred` (never built into `nearest_n`/`nearest_n_fresh_first` themselves, since
//! `convert::nearest_wildman` uses the plain `nearest_n` with its own, differently-sized range instead).

use crate::unit::{cell_of, torus_delta, Unit, UnitId};

pub fn nearest_n(units: &[Unit], at: (u16, u16), n: usize, pred: impl Fn(&Unit) -> bool) -> Vec<UnitId> {
    nearest_n_fresh_first(units, at, n, pred, |_| true)
}

/// Half the side of the 3 × 3 cells the nearest-N selector reaches around its target.
pub const BLOCK_HALF_CELLS: f32 = 1.5;

/// Whether `pos` lies in the 3 × 3 cells around `at` (`size` cells per side, wrapped): the nearest-N selector's own
/// area, composed into `pred` by `GameMap::cast_nearest_status` (Shield, Bloodlust, Invisibility) and
/// `hypnotism::Hypnotism::turn`.
pub fn in_block(at: (u16, u16), pos: (u16, u16), size: i32) -> bool {
    let near = |a: i32, b: i32| {
        let d = (b - a).rem_euclid(size);
        d.min(size - d) <= 1
    };
    near(cell_of(at.0), cell_of(pos.0)) && near(cell_of(at.1), cell_of(pos.1))
}

/// Like `nearest_n`, but a candidate `fresh` of the spell's own effect (not already shielded, bloodlusted,
/// invisible or hypnotised) sorts before one that already has it, regardless of distance: a recast reaches new
/// targets first, and only re-touches (refreshing its timer to full) one that already has it once there is
/// nobody fresh left within the nearest `n` (confirmed, docs/specs/spells.md "Magical Shield").
pub fn nearest_n_fresh_first(units: &[Unit], at: (u16, u16), n: usize, pred: impl Fn(&Unit) -> bool, fresh: impl Fn(&Unit) -> bool) -> Vec<UnitId> {
    let mut found: Vec<(bool, u32, UnitId)> = units.iter().filter(|u| pred(u)).map(|u| (!fresh(u), distance_sq(at, (u.x, u.z)), u.id)).collect();
    found.sort_by_key(|&(stale, d, id)| (stale, d, id));
    found.truncate(n);
    found.into_iter().map(|(_, _, id)| id).collect()
}

fn distance_sq(a: (u16, u16), b: (u16, u16)) -> u32 {
    let (dx, dz) = (torus_delta(a.0, b.0), torus_delta(a.1, b.1));
    (dx * dx + dz * dz) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tribes::TribeId;
    use crate::id::Id;
    use crate::unit::UnitKind;

    fn person(id: u32, at: (u16, u16)) -> Unit {
        Unit::new(Id::from_raw(id), TribeId::Blue, UnitKind::Brave, at)
    }

    #[test]
    fn picks_the_n_nearest_in_order() {
        let units = [person(1, (0, 1000)), person(2, (0, 10)), person(3, (0, 500))];
        assert_eq!(nearest_n(&units, (0, 0), 2, |_| true), [Id::from_raw(2), Id::from_raw(3)]);
    }

    #[test]
    fn filters_by_the_predicate() {
        let units = [person(1, (0, 10)), person(2, (0, 20))];
        assert_eq!(nearest_n(&units, (0, 0), 5, |u| u.id != Id::from_raw(1)), [Id::from_raw(2)]);
    }

    #[test]
    fn ties_break_on_the_lowest_id() {
        let units = [person(5, (0, 10)), person(2, (0, 10))];
        assert_eq!(nearest_n(&units, (0, 0), 5, |_| true), [Id::from_raw(2), Id::from_raw(5)]);
    }

    #[test]
    fn wraps_across_the_torus_edge() {
        let units = [person(1, (65530, 0))];
        assert_eq!(nearest_n(&units, (10, 0), 1, |_| true), [Id::from_raw(1)], "close the other way round");
    }

    #[test]
    fn fresh_first_prefers_untouched_candidates_over_closer_already_touched_ones() {
        let units = [person(1, (0, 10)), person(2, (0, 20)), person(3, (0, 30))];
        let fresh_except_1 = |u: &Unit| u.id != Id::from_raw(1);
        // #1 is closest but already has it; #2 and #3 are fresh and farther: still picked first.
        assert_eq!(nearest_n_fresh_first(&units, (0, 0), 2, |_| true, fresh_except_1), [Id::from_raw(2), Id::from_raw(3)]);
        // Not enough fresh ones to fill 2 slots: the closest already-touched one fills the rest.
        let fresh_only_3 = |u: &Unit| u.id == Id::from_raw(3);
        assert_eq!(nearest_n_fresh_first(&units, (0, 0), 2, |_| true, fresh_only_3), [Id::from_raw(3), Id::from_raw(1)]);
    }

    #[test]
    fn in_block_covers_only_its_own_cell_and_the_ring_around_it() {
        assert!(in_block((0, 0), (0, 0), 128), "its own cell");
        assert!(in_block((0, 0), (600, 600), 128), "one cell over in both axes");
        assert!(!in_block((0, 0), (1200, 0), 128), "two cells over: outside");
    }

    #[test]
    fn in_block_wraps_across_the_torus_edge() {
        // Cell 127's neighbour the other way round is cell 0, not "far away" in plain subtraction.
        assert!(in_block((127 * 512, 0), (0, 0), 128));
    }
}
