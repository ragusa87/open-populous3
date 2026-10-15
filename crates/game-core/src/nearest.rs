//! The nearest-N selector shared by Magical Shield, Bloodlust, Invisibility and Hypnotism
//! (pop3-rev-analysis.md 0x515E30, "nearest-N selector"): the `n` closest units to a point matching `pred`,
//! nearest first. Ties (equal distance) break on the lowest unit id: not confirmed against the original
//! (docs/plans/spells.md "Magical Shield"), but keeps the pick replay-safe across lockstep peers.

use crate::unit::{torus_delta, Unit, UnitId};

pub fn nearest_n(units: &[Unit], at: (u16, u16), n: usize, pred: impl Fn(&Unit) -> bool) -> Vec<UnitId> {
    let mut found: Vec<(u32, UnitId)> = units.iter().filter(|u| pred(u)).map(|u| (distance_sq(at, (u.x, u.z)), u.id)).collect();
    found.sort_by_key(|&(d, id)| (d, id));
    found.truncate(n);
    found.into_iter().map(|(_, id)| id).collect()
}

fn distance_sq(a: (u16, u16), b: (u16, u16)) -> u32 {
    let (dx, dz) = (torus_delta(a.0, b.0), torus_delta(a.1, b.1));
    (dx * dx + dz * dz) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::Id;
    use crate::unit::UnitKind;

    fn person(id: u32, at: (u16, u16)) -> Unit {
        Unit::new(Id::from_raw(id), 0, UnitKind::Brave, at)
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
}
