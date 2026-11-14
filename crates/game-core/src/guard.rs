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

/// Distance (world units) a guard keeps from her while circling: the ~200-unit working radius measured live
/// ("Guard the shaman" [exe, measured]).
pub const ORBIT_RADIUS: i32 = 200;
/// Guards spread evenly around the ring, up to this many before a wider ring starts further out (so a slot
/// already walking towards its own spot is never shifted by a later guard joining or an earlier one leaving).
pub const RING_SLOTS: u16 = 8;
/// World units added to the radius per extra ring past the first: a guess.
pub const RING_GAP: i32 = 150;
/// World units a point of any ring moves along it per tick while she stands still: ~35 a turn, one way round, a lap
/// of the first ring in ~36 turns ("Guard the shaman" [exe, measured]; the same pace on further rings is ours).
pub const ORBIT_PACE: u32 = 35;

/// Where guard `slot` of a shaman at `centre` stands this tick: a fixed point on its own ring, turning
/// steadily at `ORBIT_PACE` (so it keeps moving even once it arrives, instead of only turning when nudged) - same ring order regardless of how many other guards currently share it, so one dropping out or
/// joining never reshuffles anyone else's spot.
pub fn orbit_point(centre: (u16, u16), slot: u16, now: Tick) -> (u16, u16) {
    let ring = (slot / RING_SLOTS) as i32;
    let base = (slot % RING_SLOTS) as u32 * (angle::TURN / RING_SLOTS as u32);
    let radius = ORBIT_RADIUS + ring * RING_GAP;
    let circumference = radius as u64 * 710 / 113;
    let spin = (now.to_wire() as u64 * ORBIT_PACE as u64 * angle::TURN as u64 / circumference % angle::TURN as u64) as u32;
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
    fn a_lap_of_the_first_ring_takes_the_measured_36_turns() {
        let lap = ORBIT_RADIUS as u32 * 710 / 113 / ORBIT_PACE;
        assert!((35..=37).contains(&lap), "{lap}");
    }

    #[test]
    fn every_ring_turns_at_the_same_pace_along_the_ground() {
        let centre = (10_000, 10_000);
        for slot in [0, RING_SLOTS, 2 * RING_SLOTS] {
            for t in [0u32, 7, 100] {
                let (a, b) = (orbit_point(centre, slot, Tick::from_wire(t)), orbit_point(centre, slot, Tick::from_wire(t + 1)));
                let (dx, dz) = (b.0 as i32 - a.0 as i32, b.1 as i32 - a.1 as i32);
                let step = crate::unit::isqrt((dx * dx + dz * dz) as u32) as i32;
                assert!((step - ORBIT_PACE as i32).abs() <= 3, "slot {slot}, tick {t}: {step} a tick");
            }
        }
    }

    #[test]
    fn the_ring_turns_slower_than_any_guard_walks() {
        let slowest = [crate::unit::UnitKind::Brave, crate::unit::UnitKind::Warrior, crate::unit::UnitKind::Firewarrior, crate::unit::UnitKind::Preacher, crate::unit::UnitKind::Spy].map(|k| k.speed()).into_iter().min().unwrap();
        assert!((ORBIT_PACE as i32) < slowest);
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
