//! Idle tribesmen near their shaman pray facing her. Drawn only: in the simulation they stay idle
//! and take orders as usual (docs/specs/units.md, "Worshipping the shaman").

use game_core::unit::{octant, torus_delta, Action, Unit, UnitKind};
use pop3_format::catalog::TRIBES;
use pop3_format::WORLD_UNITS_PER_CELL;

/// How close (world units, on each axis) the shaman must be.
pub const WORSHIP_RANGE: i32 = 3 * WORLD_UNITS_PER_CELL as i32;

/// Each tribe's shaman, by owner.
pub fn shamans(units: &[Unit]) -> [Option<&Unit>; TRIBES as usize] {
    let mut out = [None; TRIBES as usize];
    for u in units.iter().filter(|u| u.kind == UnitKind::Shaman) {
        if let Some(slot) = out.get_mut(u.owner as usize) {
            *slot = Some(u);
        }
    }
    out
}

/// The facing (eighths of a turn) towards `shaman` when `unit` worships her: an idle brave,
/// warrior, firewarrior or spy of her tribe, with her idle or walking within range.
pub fn worship_facing(unit: &Unit, shaman: Option<&Unit>) -> Option<u8> {
    let shaman = shaman?;
    let worshipper = matches!(unit.kind, UnitKind::Brave | UnitKind::Warrior | UnitKind::Firewarrior | UnitKind::Spy);
    let present = matches!(shaman.action, Action::Idle | Action::Walking { .. });
    if !worshipper || unit.action != Action::Idle || !present || unit.owner != shaman.owner {
        return None;
    }
    let (dx, dz) = (torus_delta(unit.x, shaman.x), torus_delta(unit.z, shaman.z));
    (dx.abs() <= WORSHIP_RANGE && dz.abs() <= WORSHIP_RANGE).then(|| octant(dx, dz))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CELL: u16 = WORLD_UNITS_PER_CELL as u16;

    fn unit(owner: u8, kind: UnitKind, cell: (u16, u16)) -> Unit {
        Unit::new(1, owner, kind, (cell.0 * CELL + CELL / 2, cell.1 * CELL + CELL / 2))
    }

    #[test]
    fn idle_tribesmen_turn_to_their_shaman() {
        let her = unit(0, UnitKind::Shaman, (10, 10));
        assert_eq!(worship_facing(&unit(0, UnitKind::Brave, (8, 10)), Some(&her)), Some(2), "she is towards +x");
        assert_eq!(worship_facing(&unit(0, UnitKind::Warrior, (10, 13)), Some(&her)), Some(4), "towards -z, 3 cells");
        assert_eq!(worship_facing(&unit(0, UnitKind::Spy, (12, 12)), Some(&her)), Some(5));
        assert_eq!(worship_facing(&unit(0, UnitKind::Firewarrior, (10, 9)), Some(&her)), Some(0));
    }

    #[test]
    fn range_wraps_around_the_torus() {
        let last = (65536 / WORLD_UNITS_PER_CELL - 1) as u16;
        let her = unit(0, UnitKind::Shaman, (0, 5));
        assert_eq!(worship_facing(&unit(0, UnitKind::Brave, (last, 5)), Some(&her)), Some(2));
        assert_eq!(worship_facing(&unit(0, UnitKind::Brave, (14, 5)), Some(&her)), None, "out of range");
    }

    #[test]
    fn only_idle_tribesmen_of_her_tribe_with_her_idle_or_walking() {
        let mut her = unit(0, UnitKind::Shaman, (10, 10));
        let brave = unit(0, UnitKind::Brave, (9, 10));
        assert_eq!(worship_facing(&unit(1, UnitKind::Brave, (9, 10)), Some(&her)), None, "other tribe");
        for kind in [UnitKind::Preacher, UnitKind::Wildman, UnitKind::Shaman] {
            assert_eq!(worship_facing(&unit(0, kind, (9, 10)), Some(&her)), None, "{kind:?}");
        }
        let mut busy = brave.clone();
        busy.action = Action::Walking { to: (0, 0) };
        assert_eq!(worship_facing(&busy, Some(&her)), None, "not idle");
        her.action = Action::Walking { to: (0, 0) };
        assert!(worship_facing(&brave, Some(&her)).is_some(), "she walks past");
        for action in [Action::Praying, Action::Casting { left: 3 }, Action::Dead { left: 3 }] {
            her.action = action;
            assert_eq!(worship_facing(&brave, Some(&her)), None, "{action:?}");
        }
        assert_eq!(worship_facing(&brave, None), None, "no shaman");
    }

    #[test]
    fn shamans_by_tribe() {
        let units = [unit(2, UnitKind::Shaman, (1, 1)), unit(0, UnitKind::Brave, (2, 2)), unit(0, UnitKind::Shaman, (3, 3))];
        let s = shamans(&units);
        assert_eq!((s[0].map(|u| u.x), s[1].is_none(), s[2].map(|u| u.x)), (Some(units[2].x), true, Some(units[0].x)));
    }
}
