//! The simple blast (pop3-rev-analysis.md "Components"), an effect (`effect::EffectKind::SimpleBlast`): 3 turns at
//! radius 2, 4 then 5 cells around its centre. Each turn every person in the radius takes `DAMAGE_PERSON` (the
//! caster's tribe and the shielded are spared by `GameMap::damage`); each person within the turn's radius and
//! `PUSH_RANGE`, of any tribe but the caster's own shaman, is pushed away and up, once.

use crate::effect::{DamageKind, Effect, EffectEvent, EffectKind};
use crate::motion::Velocity;
use crate::time::{Countdown, Ticks};
use crate::unit::{isqrt, torus_delta, Unit, UnitKind};
use pop3_format::WORLD_UNITS_PER_CELL;

/// Radius of each turn, in world units.
pub const RADII: [u32; 3] = [2 * WORLD_UNITS_PER_CELL, 4 * WORLD_UNITS_PER_CELL, 5 * WORLD_UNITS_PER_CELL];
/// Only people this close to the centre are pushed (2.5 cells).
pub const PUSH_RANGE: u32 = 1280;
/// The push at the centre: outwards in world units per tick, up in height units per tick; it fades to 0 at
/// `PUSH_RANGE`.
pub const PUSH_OUT: i32 = 140;
pub const PUSH_UP: i32 = 98;
/// `BLAST_DAMAGE_PERSON` (exe 50, not in the file): each turn, to each person in the radius.
pub const DAMAGE_PERSON: u16 = 50;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SimpleBlast {
    /// Turns done.
    turn: u8,
    /// Units pushed so far (ids): each only once.
    pushed: Vec<u32>,
}

/// A blast cast by tribe `owner` at `at` (world units).
pub fn effect(owner: u8, at: (u16, u16)) -> Effect {
    Effect::new(owner, at, Countdown::new(Ticks::new(RADII.len() as u32)), EffectKind::SimpleBlast(SimpleBlast::default()))
}

impl SimpleBlast {
    /// One turn of the blast at `at` cast by `owner`: the pushes and damage it deals now.
    pub fn turn(&mut self, at: (u16, u16), owner: u8, units: &[Unit]) -> Vec<EffectEvent> {
        let Some(&radius) = RADII.get(self.turn as usize) else { return Vec::new() };
        self.turn += 1;
        let reach = radius.min(PUSH_RANGE);
        let mut events = Vec::new();
        for u in units.iter().filter(|u| u.is_alive() && u.inside.is_none()) {
            let pushable = !(u.owner == owner && u.kind == UnitKind::Shaman) && !self.pushed.contains(&u.id);
            if let Some(vel) = push(at, (u.x, u.z), reach).filter(|_| pushable) {
                self.pushed.push(u.id);
                events.push(EffectEvent::Push { unit: u.id, vel });
            }
            if push(at, (u.x, u.z), radius).is_some() {
                events.push(EffectEvent::Damage { unit: u.id, amount: DAMAGE_PERSON, kind: DamageKind::Blast });
            }
        }
        events
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
        let units = [
            unit(1, 5000),
            unit(2, 5000 + 1200),
            unit(3, 5000 + 1500),
            Unit::new(4, 0, UnitKind::Brave, (5000, 5000)),
            Unit::new(5, 0, UnitKind::Shaman, (5000, 5000)),
            Unit::new(6, 1, UnitKind::Shaman, (5000, 5000)),
        ];
        let mut blast = SimpleBlast::default();
        let mut turn = || blast.turn((5000, 5000), 0, &units);
        let pushed = |events: &[EffectEvent]| events.iter().filter_map(|e| if let EffectEvent::Push { unit, .. } = e { Some(*unit) } else { None }).collect::<Vec<_>>();
        let hit = |events: &[EffectEvent]| events.iter().filter_map(|e| if let EffectEvent::Damage { unit, .. } = e { Some(*unit) } else { None }).collect::<Vec<_>>();
        let first = turn();
        assert_eq!(pushed(&first), [1, 4, 6], "2 cells: the caster's own brave too, not the caster's shaman");
        assert_eq!(hit(&first), [1, 4, 5, 6], "everyone in the radius: GameMap::damage spares the caster's tribe");
        let second = turn();
        assert_eq!(pushed(&second), [2], "4 cells, pushed only within 2.5");
        assert_eq!(hit(&second), [1, 2, 3, 4, 5, 6]);
        assert_eq!(pushed(&turn()), Vec::<u32>::new());
        assert!(turn().is_empty(), "over");
    }

    #[test]
    fn an_enemy_at_the_centre_loses_three_turns_of_damage_the_casters_and_the_shielded_none() {
        let mut map = crate::map::GameMap::sandbox_walk();
        let s = &map.units[0];
        let at = (s.x.wrapping_add(2048), s.z);
        let enemy = map.spawn(1, UnitKind::Brave, at).id;
        let mine = map.spawn(0, UnitKind::Brave, (at.0, at.1.wrapping_add(200))).id;
        let shielded = map.spawn(1, UnitKind::Brave, (at.0, at.1.wrapping_sub(200))).id;
        map.unit_mut(shielded).unwrap().statuses.shielded = true;
        map.add_effect(effect(0, at));
        map.run(Ticks::new(RADII.len() as u32));
        assert!(map.effects.is_empty());
        let health = |id| map.unit(id).unwrap().health.current();
        assert_eq!(health(enemy), 1000 - 3 * DAMAGE_PERSON);
        assert_eq!(health(mine), 1000, "the caster's brave is not hurt");
        assert!(map.unit(mine).unwrap().motion.airborne(), "but pushed");
        assert_eq!(health(shielded), 1000);
        assert!(!map.unit(shielded).unwrap().motion.airborne(), "nor pushed");
    }
}
