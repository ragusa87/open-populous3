//! A follower standing idle (pop3-rev-analysis.md "Bored poses", person state 19): every 32 turns, on its own
//! phase, a roll for a shuffle (a turn on the spot, held a while) or a bored pose (drawn by the client,
//! `units::art::bored`).

use crate::map::Lcg;
use crate::time::{Every, Tick, Ticks};
use crate::unit::UnitId;

/// The standing loop's check: every 32 turns on the unit's own phase ("Bored poses").
pub const CHECK: Every = Every::new(32);
/// A shuffle wins when `rnd & 31 > 18`: 13/32 ("Bored poses").
const SHUFFLE_ABOVE: u32 = 18;
/// A bored pose wins when `rnd & 63 > 60`: 3/64, over a shuffle rolled the same turn ("Bored poses").
const BORED_ABOVE: u32 = 60;
/// A shuffle lasts `4..=31` turns ("Bored poses", "Tumbling and ground recovery").
const SHUFFLE_MIN: u32 = 4;
const SHUFFLE_SPAN: u32 = 28;
/// A shuffle turns 135° (768 of 2048), 3 eighths of a turn: + when its hold is odd, - when even ("Bored poses").
const SHUFFLE_TURN: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fidget {
    /// Turn on the spot (`turned`) and stand still until `hold` ends.
    Shuffle { hold: Ticks },
    Bored,
}

/// What a unit standing idle starts at `now`: nothing but on its check turns, then mostly nothing. Seeded from
/// its id and the tick (as `unit::settle_ticks`), so the client draws the same roll without any shared state.
pub fn roll(id: UnitId, now: Tick) -> Option<Fidget> {
    if !CHECK.fires(now, id.raw()) {
        return None;
    }
    let mut rng = Lcg(id.raw().wrapping_mul(2_654_435_761) ^ now.to_wire().wrapping_mul(0x9E37_79B9));
    rng.next_u32();
    let shuffle = rng.next_u32() & 31 > SHUFFLE_ABOVE;
    let bored = rng.next_u32() & 63 > BORED_ABOVE;
    if bored {
        Some(Fidget::Bored)
    } else if shuffle {
        Some(Fidget::Shuffle { hold: Ticks::new(SHUFFLE_MIN + rng.next_u32() % SHUFFLE_SPAN) })
    } else {
        None
    }
}

/// The tick of the check `now` falls after, for a unit of `id`.
pub fn last_check(id: UnitId, now: Tick) -> Tick {
    let elapsed = now.to_wire().wrapping_add(id.raw()) % CHECK.period();
    Tick::from_wire(now.to_wire().wrapping_sub(elapsed))
}

/// The facing (eighths of a turn) after a shuffle held `hold` from `facing`: ±135°, its sign from the hold's
/// parity, so the one roll draws both ("Bored poses" [exe]). Kept afterwards: no turn back.
pub fn turned(facing: u8, hold: Ticks) -> u8 {
    let by = if hold.get() % 2 == 1 { SHUFFLE_TURN } else { 8 - SHUFFLE_TURN };
    (facing + by) % 8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rolls(id: u32) -> Vec<(Tick, Fidget)> {
        (0..32 * 4096).map(Tick::from_wire).filter_map(|t| roll(UnitId::from_raw(id), t).map(|f| (t, f))).collect()
    }

    #[test]
    fn only_on_the_unit_s_own_check_turns() {
        let id = UnitId::from_raw(5);
        assert!(rolls(5).iter().all(|&(t, _)| CHECK.fires(t, 5) && last_check(id, t) == t));
        assert_eq!(last_check(id, Tick::from_wire(27 + 10)), Tick::from_wire(27));
    }

    #[test]
    fn at_the_traced_odds() {
        let all = rolls(9);
        let rate = |f: fn(&Fidget) -> bool| all.iter().filter(|(_, x)| f(x)).count() as f32 / 4096.0;
        let bored = rate(|f| *f == Fidget::Bored);
        let shuffle = rate(|f| matches!(f, Fidget::Shuffle { .. }));
        assert!((0.03..0.065).contains(&bored), "3/64: {bored}");
        assert!((0.34..0.44).contains(&shuffle), "13/32 of the rest: {shuffle}");
        assert!(all.iter().all(|(_, f)| !matches!(f, Fidget::Shuffle { hold, .. } if !(4..=31).contains(&hold.get()))));
        assert_eq!(roll(UnitId::from_raw(9), all[0].0), Some(all[0].1), "the same roll every time");
    }

    #[test]
    fn a_shuffle_turns_135_degrees_its_way_from_the_hold_s_parity() {
        assert_eq!(turned(0, Ticks::new(5)), 3);
        assert_eq!(turned(0, Ticks::new(4)), 5);
        assert_eq!(turned(6, Ticks::new(31)), 1, "wraps");
    }
}
