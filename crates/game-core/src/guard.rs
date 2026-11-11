//! Guard the shaman (`G`, pop3-rev-analysis.md "Guard the shaman"): a selected follower's own continuous escort
//! order (`Order::Guard`, `Action::Guarding`, `GameMap::go_guard`/`tend_guards`). She is not protected or
//! untargetable while guarded - live-tested and disproven in the trace: a guarded shaman joins the same fight
//! ring as her guards instead of being shielded from it, the ordinary auto-attack/melee-join rule already covers
//! the observed "guards take the hit" effect without a dedicated protection mechanic.
//!
//! Circling (`orbit_point`, `[ours]`, not itself traced - only `GUARD_RANGE` below comes from the exe): while
//! she stands still, several guards drawing on top of each other at her exact position looked wrong (reported
//! live), so each is given its own fixed point on a ring around her instead, continuously turning; one that
//! falls behind (beyond `GUARD_RANGE`) walks straight back to her current position first, same as before, then
//! resumes circling once it is close again.

use crate::angle;
use crate::time::Tick;

/// How far a guard may stray from the shaman's current position, per axis, before it walks back towards her
/// [exe, confirmed].
pub const GUARD_RANGE: i32 = 824;

/// Distance (world units) a guard keeps from her while circling: closer than `GUARD_RANGE` so it settles into
/// orbit well before the catch-up threshold would fire again.
pub const ORBIT_RADIUS: i32 = 400;
/// Guards spread evenly around the ring, up to this many before a wider ring starts further out (so a slot
/// already walking towards its own spot is never shifted by a later guard joining or an earlier one leaving).
pub const RING_SLOTS: u16 = 8;
/// World units added to the radius per extra ring past the first.
pub const RING_GAP: i32 = 250;
/// Ticks for one full turn around the ring while she stands still: a guess, no rhythm traced.
pub const ORBIT_TURN_TICKS: u32 = 240;

/// Where guard `slot` of a shaman at `centre` stands this tick: a fixed point on its own ring, turning
/// steadily over `ORBIT_TURN_TICKS` (so it keeps walking even once it arrives, instead of only turning when
/// nudged) - same ring order regardless of how many other guards currently share it, so one dropping out or
/// joining never reshuffles anyone else's spot.
pub fn orbit_point(centre: (u16, u16), slot: u16, now: Tick) -> (u16, u16) {
    let ring = (slot / RING_SLOTS) as i32;
    let base = (slot % RING_SLOTS) as u32 * (angle::TURN / RING_SLOTS as u32);
    let spin = (now.to_wire() % ORBIT_TURN_TICKS) * angle::TURN / ORBIT_TURN_TICKS;
    let radius = ORBIT_RADIUS + ring * RING_GAP;
    let (dx, dz) = angle::rotate((radius, 0), base.wrapping_add(spin));
    (centre.0.wrapping_add(dx as u16), centre.1.wrapping_add(dz as u16))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_slots_on_the_same_ring_are_never_at_the_same_point() {
        let centre = (10_000, 10_000);
        for t in [0u32, 1, 50, 120, 239] {
            let now = Tick::from_wire(t);
            let points: Vec<_> = (0..RING_SLOTS).map(|s| orbit_point(centre, s, now)).collect();
            for i in 0..points.len() {
                for j in i + 1..points.len() {
                    assert_ne!(points[i], points[j], "slots {i} and {j} at tick {t}: {points:?}");
                }
            }
        }
    }

    #[test]
    fn a_slot_keeps_turning_as_ticks_pass_even_though_its_centre_never_moves() {
        let centre = (10_000, 10_000);
        let a = orbit_point(centre, 0, Tick::from_wire(0));
        let b = orbit_point(centre, 0, Tick::from_wire(ORBIT_TURN_TICKS / 4));
        assert_ne!(a, b, "a quarter turn later it stands somewhere else on the ring");
    }

    #[test]
    fn a_full_turn_returns_to_the_same_point() {
        let centre = (10_000, 10_000);
        assert_eq!(orbit_point(centre, 3, Tick::from_wire(17)), orbit_point(centre, 3, Tick::from_wire(17 + ORBIT_TURN_TICKS)));
    }

    #[test]
    fn a_second_ring_sits_further_out_than_the_first() {
        let centre = (10_000, 10_000);
        let now = Tick::from_wire(0);
        let inner = orbit_point(centre, 0, now);
        let outer = orbit_point(centre, RING_SLOTS, now);
        let dist2 = |p: (u16, u16)| {
            let (dx, dz) = (p.0 as i32 - centre.0 as i32, p.1 as i32 - centre.1 as i32);
            dx * dx + dz * dz
        };
        assert!(dist2(outer) > dist2(inner));
    }
}
