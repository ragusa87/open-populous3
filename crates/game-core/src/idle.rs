//! A follower standing idle (pop3-rev-analysis.md "Bored poses", person state 19): every 32 turns, on its own
//! phase, a roll for a short shuffle around its spot or a bored pose (drawn by the client, `units::art::bored`).

use crate::map::Lcg;
use crate::time::{Countdown, Every, Tick, Ticks};
use crate::unit::UnitId;
use pop3_format::WORLD_UNITS_PER_CELL;

/// The standing loop's check: every 32 turns on the unit's own phase ("Bored poses").
pub const CHECK: Every = Every::new(32);
/// A shuffle wins when `rnd & 31 > 18`: 13/32 ("Bored poses").
const SHUFFLE_ABOVE: u32 = 18;
/// A bored pose wins when `rnd & 63 > 60`: 3/64, over a shuffle rolled the same turn ("Bored poses").
const BORED_ABOVE: u32 = 60;
/// A shuffle lasts `4..=31` turns ("Bored poses", "Tumbling and ground recovery").
const SHUFFLE_MIN: u32 = 4;
const SHUFFLE_SPAN: u32 = 28;
/// How far from its spot a shuffle steps: a guess, the trace only says "a step". Kept inside its standing spot
/// (`slots::PER_CELL`), which holds one person each.
pub const SHUFFLE_STEP: i32 = WORLD_UNITS_PER_CELL as i32 / 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fidget {
    /// Face `facing`, step that way off its spot, stand there until `hold` ends.
    Shuffle { facing: u8, hold: Ticks },
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
        Some(Fidget::Shuffle { facing: (rng.next_u32() & 7) as u8, hold: Ticks::new(SHUFFLE_MIN + rng.next_u32() % SHUFFLE_SPAN) })
    } else {
        None
    }
}

/// The tick of the check `now` falls after, for a unit of `id`.
pub fn last_check(id: UnitId, now: Tick) -> Tick {
    let elapsed = now.to_wire().wrapping_add(id.raw()) % CHECK.period();
    Tick::from_wire(now.to_wire().wrapping_sub(elapsed))
}

/// A shuffle under way: the point it steps to (`None` once there or blocked), the way it then faces, and how
/// long it lasts in all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shuffle {
    pub to: Option<(u16, u16)>,
    pub facing: u8,
    pub left: Countdown,
}

/// One step of `SHUFFLE_STEP` from `spot` towards heading `facing` (0 = +z, 2 = +x, 4 = -z, 6 = -x).
pub fn step_from((x, z): (u16, u16), facing: u8) -> (u16, u16) {
    const DIAGONAL: i32 = 181;
    let (dx, dz): (i32, i32) = [(0, 256), (DIAGONAL, DIAGONAL), (256, 0), (DIAGONAL, -DIAGONAL), (0, -256), (-DIAGONAL, -DIAGONAL), (-256, 0), (-DIAGONAL, DIAGONAL)][facing as usize & 7];
    (x.wrapping_add((dx * SHUFFLE_STEP / 256) as u16), z.wrapping_add((dz * SHUFFLE_STEP / 256) as u16))
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
    fn a_step_stays_inside_its_standing_spot_whichever_way() {
        assert_eq!(step_from((1000, 1000), 0), (1000, 1064));
        assert_eq!(step_from((1000, 1000), 6), (936, 1000));
        assert_eq!(step_from((0, 0), 4), (0, 65472), "wraps");
        let centre = crate::slots::spot_centre((10, 10));
        assert!((0..8).all(|f| crate::slots::spot_of(step_from(centre, f)) == (10, 10)));
    }
}
