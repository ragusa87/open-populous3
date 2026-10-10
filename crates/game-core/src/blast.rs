//! The simple blast (pop3-rev-analysis.md "Components"): 3 turns at radius 2, 4 then 5 cells around its centre.
//! Each person of another tribe within the turn's radius and `PUSH_RANGE` is pushed away and up, once (damage,
//! allies and shields to come).

use crate::motion::Velocity;
use crate::unit::{isqrt, torus_delta, Unit};
use pop3_format::WORLD_UNITS_PER_CELL;

/// Radius of each turn, in world units.
pub const RADII: [u32; 3] = [2 * WORLD_UNITS_PER_CELL, 4 * WORLD_UNITS_PER_CELL, 5 * WORLD_UNITS_PER_CELL];
/// Only people this close to the centre are pushed (2.5 cells).
pub const PUSH_RANGE: u32 = 1280;
/// The push at the centre: outwards in world units per tick, up in height units per tick; it fades to 0 at
/// `PUSH_RANGE`.
pub const PUSH_OUT: i32 = 140;
pub const PUSH_UP: i32 = 98;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blast {
    /// Centre, world units.
    pub at: (u16, u16),
    /// The caster's tribe: its people are not pushed.
    pub owner: u8,
    /// Turns done.
    turn: u8,
    /// Units pushed so far (ids): each only once.
    pushed: Vec<u32>,
}

impl Blast {
    pub fn new(at: (u16, u16), owner: u8) -> Self {
        Blast { at, owner, turn: 0, pushed: Vec::new() }
    }

    pub fn is_over(&self) -> bool {
        self.turn as usize >= RADII.len()
    }

    /// One turn: the units it pushes now (index in `units`, push), each only once over the blast.
    pub fn turn(&mut self, units: &[Unit]) -> Vec<(usize, Velocity)> {
        let Some(&radius) = RADII.get(self.turn as usize) else { return Vec::new() };
        self.turn += 1;
        let reach = radius.min(PUSH_RANGE);
        let hits: Vec<(usize, Velocity)> = units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.owner != self.owner && u.is_alive() && u.inside.is_none() && !self.pushed.contains(&u.id))
            .filter_map(|(i, u)| Some((i, push(self.at, (u.x, u.z), reach)?)))
            .collect();
        self.pushed.extend(hits.iter().map(|&(i, _)| units[i].id));
        hits
    }
}

/// The push on someone at `pos` from a blast at `at`, None beyond `reach`: away from the centre (straight up on it),
/// `(PUSH_RANGE - d) / PUSH_RANGE` of the full push at distance `d`.
pub fn push(at: (u16, u16), pos: (u16, u16), reach: u32) -> Option<Velocity> {
    let (dx, dz) = (torus_delta(at.0, pos.0), torus_delta(at.1, pos.1));
    let d = isqrt(dx.unsigned_abs().pow(2) + dz.unsigned_abs().pow(2));
    if d > reach {
        return None;
    }
    let fade = PUSH_RANGE.saturating_sub(d) as i32;
    let out = PUSH_OUT * fade / PUSH_RANGE as i32;
    let along = |delta: i32| if d == 0 { 0 } else { (out * delta / d as i32) as i16 };
    Some(Velocity::new(along(dx), (PUSH_UP * fade / PUSH_RANGE as i32) as i16, along(dz)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::UnitKind;

    #[test]
    fn pushes_away_and_up_fading_with_distance() {
        let at = (1000, 1000);
        assert_eq!(push(at, at, PUSH_RANGE), Some(Velocity::new(0, 98, 0)), "straight up on the centre");
        assert_eq!(push(at, (1640, 1000), PUSH_RANGE), Some(Velocity::new(70, 49, 0)), "half way: half the push");
        assert_eq!(push(at, (1000, 360), PUSH_RANGE), Some(Velocity::new(0, 49, -70)));
        assert_eq!(push(at, (1000 + 1280, 1000), PUSH_RANGE), Some(Velocity::ZERO), "the edge");
        assert_eq!(push(at, (1000 + 1281, 1000), PUSH_RANGE), None);
        assert_eq!(push((10, 0), (65500, 0), PUSH_RANGE).map(|v| v.x < 0), Some(true), "across the map's edge");
    }

    #[test]
    fn three_turns_growing_and_each_unit_once() {
        let unit = |id, x| Unit::new(id, 1, UnitKind::Brave, (x, 5000));
        let units = [unit(1, 5000), unit(2, 5000 + 1200), unit(3, 5000 + 1500), Unit::new(4, 0, UnitKind::Brave, (5000, 5000))];
        let mut blast = Blast::new((5000, 5000), 0);
        let ids = |hits: Vec<(usize, Velocity)>| hits.iter().map(|&(i, _)| units[i].id).collect::<Vec<_>>();
        assert_eq!(ids(blast.turn(&units)), [1], "2 cells: the caster's own brave is spared");
        assert_eq!(ids(blast.turn(&units)), [2], "4 cells, pushed only within 2.5");
        assert!(blast.turn(&units).is_empty() && blast.is_over());
        assert!(blast.turn(&units).is_empty());
    }
}
