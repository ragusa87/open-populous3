//! The simple blast (pop3-rev-analysis.md "Components"), an effect (`effect::EffectKind::SimpleBlast`): 3 turns at
//! radius 2, 4 then 5 cells around its centre. Each turn every person in the radius takes `DAMAGE_PERSON` (the
//! caster's tribe and the shielded are spared by `GameMap::damage`); each person within the turn's radius and
//! `PUSH_RANGE`, of any tribe but the caster's own shaman, is pushed away and up, cumulatively across the turns it
//! stays in range, scaled by the ground's own slope ("Blast push and flight"): allies (the caster's own tribe,
//! never the caster's shaman) only on the last turn, at most `BLAST_FRIEND_AFFECT`. Each turn every vehicle in the
//! radius takes `DAMAGE_PERSON` too (`GameMap::damage_vehicle` spares the caster's tribe; pop3-rev-analysis.md
//! "Life, damage and destruction"). Every building whose centre ever enters the radius across the blast's 3
//! turns takes `DAMAGE_BLDG` wear once (confirmed: unlike people/vehicles, not repeated every turn it stays in
//! range), feeding the same accumulator hacking and firewarrior bolts do (combat.md "Attacking a building").

use crate::building::{Building, BuildingId};
use crate::effect::{DamageKind, Effect, EffectKind};
use crate::event::{PushKind, WorldEvent};
use crate::motion::Velocity;
use crate::terrain::Heightmap;
use crate::time::{Countdown, Ticks};
use crate::id::Registry;
use crate::unit::{isqrt, torus_delta, Unit, UnitKind};
use crate::vehicle::Vehicle;
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
/// `BLAST_DAMAGE_BLDG` (exe 40, pop3-rev-analysis.md "Components"/"Attacking a building"): once, to each building
/// the blast ever reaches, feeding its wear accumulator.
pub const DAMAGE_BLDG: u16 = 40;
/// `BLAST_FRIEND_AFFECT` (exe 5): at most this many of the caster's own followers are pushed, on the last turn
/// only. Which ones, when more are in range, is not traced: a guess, kept to iteration order.
pub const BLAST_FRIEND_AFFECT: u8 = 5;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SimpleBlast {
    /// Turns done.
    turn: u8,
    /// Buildings already charged `DAMAGE_BLDG`: it is once per building, not every turn it stays in range.
    worn: Vec<BuildingId>,
}

/// A blast cast by tribe `owner` at `at` (world units).
pub fn effect(owner: u8, at: (u16, u16)) -> Effect {
    Effect::new(owner, at, Countdown::new(Ticks::new(RADII.len() as u32)), EffectKind::SimpleBlast(SimpleBlast::default()))
}

impl SimpleBlast {
    /// Its current (or, between ticks, next) turn's radius, cells (`RADII`): for the debug overlay, `None` once
    /// it's run its course.
    pub fn radius_cells(&self) -> Option<f32> {
        RADII.get(self.turn as usize).map(|&r| r as f32 / WORLD_UNITS_PER_CELL as f32)
    }

    /// One turn of the blast at `at` cast by `owner`: the pushes and damage it deals now.
    pub fn turn(&mut self, at: (u16, u16), owner: u8, units: &[Unit], vehicles: &Registry<Vehicle>, buildings: &[Building], terrain: &Heightmap) -> Vec<WorldEvent> {
        let Some(&radius) = RADII.get(self.turn as usize) else { return Vec::new() };
        let last_turn = self.turn as usize + 1 == RADII.len();
        self.turn += 1;
        let reach = radius.min(PUSH_RANGE);
        let mut events = Vec::new();
        let mut allies_pushed = 0u8;
        for u in units.iter().filter(|u| u.is_alive() && u.in_the_open()) {
            let is_casters_shaman = u.owner == owner && u.kind == UnitKind::Shaman;
            let is_ally = u.owner == owner && !is_casters_shaman;
            let pushable = !is_casters_shaman && (!is_ally || (last_turn && allies_pushed < BLAST_FRIEND_AFFECT));
            if pushable {
                if let Some(vel) = push_scaled(terrain, at, (u.x, u.z), reach) {
                    allies_pushed += is_ally as u8;
                    events.push(WorldEvent::Push { unit: u.id, vel, kind: PushKind::Spell });
                }
            }
            if push(at, (u.x, u.z), radius).is_some() {
                events.push(WorldEvent::Damage { unit: u.id, amount: DAMAGE_PERSON, kind: DamageKind::Blast, from: Some(owner) });
            }
        }
        for (vehicle, _) in vehicles.iter().filter(|(_, v)| push(at, (v.x, v.z), radius).is_some()) {
            events.push(WorldEvent::DamageVehicle { vehicle, amount: DAMAGE_PERSON, from: owner });
        }
        let newly_reached: Vec<BuildingId> = buildings.iter().filter(|b| offset(at, b.centre()).2 <= radius && !self.worn.contains(&b.id)).map(|b| b.id).collect();
        for building in newly_reached {
            events.push(WorldEvent::Wear { building, amount: DAMAGE_BLDG });
            self.worn.push(building);
        }
        events
    }
}

/// Horizontal distance from `at` to `pos` (torus-wrapped), and the raw deltas it came from.
fn offset(at: (u16, u16), pos: (u16, u16)) -> (i32, i32, u32) {
    let (dx, dz) = (torus_delta(at.0, pos.0), torus_delta(at.1, pos.1));
    (dx, dz, isqrt(dx.unsigned_abs().pow(2) + dz.unsigned_abs().pow(2)))
}

/// The push on someone at `pos` from a blast at `at`, None beyond `reach`: away from the centre (straight up on it),
/// `(PUSH_RANGE - d) / PUSH_RANGE` of the full push at distance `d`.
pub fn push(at: (u16, u16), pos: (u16, u16), reach: u32) -> Option<Velocity> {
    let (dx, dz, d) = offset(at, pos);
    if d > reach {
        return None;
    }
    let fade = PUSH_RANGE.saturating_sub(d) as i32;
    let out = PUSH_OUT * fade / PUSH_RANGE as i32;
    let along = |delta: i32| if d == 0 { 0 } else { (out * delta / d as i32) as i16 };
    Some(Velocity::new(along(dx), (PUSH_UP * fade / PUSH_RANGE as i32) as i16, along(dz)))
}

/// Like `push`, but the horizontal magnitude is scaled by the ground's own slope along the push (`slope_factor`):
/// pop3-rev-analysis.md "Blast push and flight".
fn push_scaled(terrain: &Heightmap, at: (u16, u16), pos: (u16, u16), reach: u32) -> Option<Velocity> {
    let (dx, dz, d) = offset(at, pos);
    if d > reach {
        return None;
    }
    let fade = PUSH_RANGE.saturating_sub(d) as i32;
    let out = slope_factor(terrain, pos, dx, dz, d, PUSH_OUT * fade / PUSH_RANGE as i32);
    let along = |delta: i32| if d == 0 { 0 } else { (out * delta / d as i32) as i16 };
    Some(Velocity::new(along(dx), (PUSH_UP * fade / PUSH_RANGE as i32) as i16, along(dz)))
}

/// `out`, scaled by how the ground rises or falls 32 world units ahead along the push direction `(dx, dz)` at
/// distance `d`: about ×1.06 on flat ground, up to ×2 where it falls away, floored at 3 against a rising slope
/// (pop3-rev-analysis.md "Blast push and flight": `len ± len × clamp(|h|, 1, 16) / 16`, at least 3).
fn slope_factor(terrain: &Heightmap, pos: (u16, u16), dx: i32, dz: i32, d: u32, out: i32) -> i32 {
    if d == 0 || out == 0 {
        return out;
    }
    let (ox, oz) = (32 * dx / d as i32, 32 * dz / d as i32);
    let ahead = (pos.0.wrapping_add(ox as i16 as u16), pos.1.wrapping_add(oz as i16 as u16));
    let here = terrain.height_at(pos.0 as u32, pos.1 as u32, WORLD_UNITS_PER_CELL);
    let there = terrain.height_at(ahead.0 as u32, ahead.1 as u32, WORLD_UNITS_PER_CELL);
    let h = here - there;
    let magnitude = h.unsigned_abs().clamp(1, 16) as i32;
    (if h >= 0 { out + out * magnitude / 16 } else { out - out * magnitude / 16 }).max(3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::Id;
    use pop3_format::MAP_SIZE;

    fn flat() -> Heightmap {
        Heightmap::new(MAP_SIZE)
    }

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

    /// A blast landing exactly on a unit: it is in every turn's radius from turn one, so it is damaged and pushed
    /// (cumulatively) on all 3 turns.
    #[test]
    fn a_blast_on_the_unit_hits_and_pushes_every_turn() {
        let units = [Unit::new(Id::from_raw(1), 1, UnitKind::Brave, (5000, 5000))];
        let vehicles = Registry::default();
        let buildings: Vec<Building> = Vec::new();
        let terrain = flat();
        let mut blast = SimpleBlast::default();
        for turn in 0..RADII.len() {
            let events = blast.turn((5000, 5000), 0, &units, &vehicles, &buildings, &terrain);
            let pushes = events.iter().filter(|e| matches!(e, WorldEvent::Push { .. })).count();
            let damage = events.iter().filter(|e| matches!(e, WorldEvent::Damage { .. })).count();
            assert_eq!((pushes, damage), (1, 1), "turn {turn}: hit dead centre");
        }
        assert!(blast.turn((5000, 5000), 0, &units, &vehicles, &buildings, &terrain).is_empty(), "over after 3 turns");
    }

    /// A blast landing next to a unit (not on it): the unit only enters the blast once the turn's own radius
    /// (the "landing zone") grows enough to reach it, never before.
    #[test]
    fn a_blast_next_to_the_unit_waits_for_its_radius_to_reach_it() {
        let at = (5000, 5000);
        let two_cells = RADII[0];
        let four_cells = RADII[1];
        // Outside the 2-cell first turn, inside the 4-cell second turn.
        let unit = Unit::new(Id::from_raw(1), 1, UnitKind::Brave, (5000 + two_cells as u16 + 1, 5000));
        let units = [unit];
        let vehicles = Registry::default();
        let buildings: Vec<Building> = Vec::new();
        let terrain = flat();
        let mut blast = SimpleBlast::default();
        let hit = |events: &[WorldEvent]| events.iter().any(|e| matches!(e, WorldEvent::Damage { .. }));
        assert!(!hit(&blast.turn(at, 0, &units, &vehicles, &buildings, &terrain)), "just outside the 2-cell radius: not yet");
        assert!(hit(&blast.turn(at, 0, &units, &vehicles, &buildings, &terrain)), "the 4-cell radius now reaches it");
        assert!(four_cells > two_cells);
    }

    #[test]
    fn three_turns_growing_damage_and_cumulative_push_except_allies_until_the_last_turn() {
        let unit = |id, x| Unit::new(Id::from_raw(id), 1, UnitKind::Brave, (x, 5000));
        let units = [
            unit(1, 5000),
            unit(2, 5000 + 1200),
            unit(3, 5000 + 1500),
            Unit::new(Id::from_raw(4), 0, UnitKind::Brave, (5000, 5000)),
            Unit::new(Id::from_raw(5), 0, UnitKind::Shaman, (5000, 5000)),
            Unit::new(Id::from_raw(6), 1, UnitKind::Shaman, (5000, 5000)),
        ];
        let mut blast = SimpleBlast::default();
        let vehicles = Registry::default();
        let buildings: Vec<Building> = Vec::new();
        let terrain = flat();
        let mut turn = || blast.turn((5000, 5000), 0, &units, &vehicles, &buildings, &terrain);
        let pushed = |events: &[WorldEvent]| events.iter().filter_map(|e| if let WorldEvent::Push { unit, .. } = e { Some(unit.raw()) } else { None }).collect::<Vec<_>>();
        let hit = |events: &[WorldEvent]| events.iter().filter_map(|e| if let WorldEvent::Damage { unit, .. } = e { Some(unit.raw()) } else { None }).collect::<Vec<_>>();
        let first = turn();
        assert_eq!(pushed(&first), [1, 6], "2 cells: the caster's own brave (4) is an ally, not pushed before the last turn");
        assert_eq!(hit(&first), [1, 4, 5, 6], "everyone in the radius: GameMap::damage spares the caster's tribe");
        let second = turn();
        assert_eq!(pushed(&second), [1, 2, 6], "4 cells, pushed again (cumulative): 1 and 6 still in range, 2 now within 2.5 cells too");
        assert_eq!(hit(&second), [1, 2, 3, 4, 5, 6]);
        let third = turn();
        assert_eq!(pushed(&third), [1, 2, 4, 6], "last turn: the ally (4) is finally pushed too, 3 stays beyond 2.5 cells throughout");
        assert_eq!(hit(&third), [1, 2, 3, 4, 5, 6]);
        assert!(turn().is_empty(), "over");
    }

    #[test]
    fn at_most_blast_friend_affect_allies_are_pushed_on_the_last_turn() {
        let units: Vec<Unit> = (0..BLAST_FRIEND_AFFECT as u32 + 2).map(|i| Unit::new(Id::from_raw(i as u32 + 1), 0, UnitKind::Brave, (5000, 5000))).collect();
        let vehicles = Registry::default();
        let buildings: Vec<Building> = Vec::new();
        let terrain = flat();
        let mut blast = SimpleBlast::default();
        blast.turn((5000, 5000), 0, &units, &vehicles, &buildings, &terrain);
        blast.turn((5000, 5000), 0, &units, &vehicles, &buildings, &terrain);
        let last = blast.turn((5000, 5000), 0, &units, &vehicles, &buildings, &terrain);
        let pushed = last.iter().filter(|e| matches!(e, WorldEvent::Push { .. })).count();
        assert_eq!(pushed, BLAST_FRIEND_AFFECT as usize, "capped even though every ally is in range");
    }

    #[test]
    fn the_horizontal_push_is_scaled_by_the_slope_along_it() {
        let mut downhill = flat();
        let mut uphill = flat();
        // Grid corners, world units exactly: `at` and `pos` sit on grid lines so `height_at` reads pure corner
        // heights, no bilinear blending to reason about. `pos` is on the corner 2 cells east of `at`; "ahead" (32
        // units further east) blends that corner with the next one east, so tilting the next corner tilts the
        // ground just ahead of the unit without moving the ground under it.
        let (here_cell, ahead_cell) = (12, 13);
        downhill.set(here_cell, 10, 100);
        downhill.set(ahead_cell, 10, 0);
        uphill.set(here_cell, 10, 0);
        uphill.set(ahead_cell, 10, 1024);
        let at = (10 * WORLD_UNITS_PER_CELL as u16, 10 * WORLD_UNITS_PER_CELL as u16);
        let pos = (here_cell as u16 * WORLD_UNITS_PER_CELL as u16, 10 * WORLD_UNITS_PER_CELL as u16);
        let flat_vel = push_scaled(&flat(), at, pos, PUSH_RANGE).unwrap();
        let down_vel = push_scaled(&downhill, at, pos, PUSH_RANGE).unwrap();
        let up_vel = push_scaled(&uphill, at, pos, PUSH_RANGE).unwrap();
        assert!(down_vel.x > flat_vel.x, "pushed harder where the ground falls away ahead");
        assert!(up_vel.x < flat_vel.x, "pushed less climbing a rise");
        assert_eq!(flat_vel.y, down_vel.y, "the slope rule never touches the vertical push");
        assert_eq!(flat_vel.y, up_vel.y);
    }

    #[test]
    fn an_enemy_at_the_centre_loses_three_turns_of_damage_the_casters_and_the_shielded_none() {
        let mut map = crate::map::GameMap::sandbox_walk();
        let s = &map.units[0];
        let at = (s.x.wrapping_add(2048), s.z);
        let enemy = map.spawn(1, UnitKind::Brave, at).id;
        let mine = map.spawn(0, UnitKind::Brave, (at.0, at.1.wrapping_add(200))).id;
        let shielded = map.spawn(1, UnitKind::Brave, (at.0, at.1.wrapping_sub(200))).id;
        map.unit_mut(shielded).unwrap().statuses.shield = crate::status::Timed::steps(crate::status::SHIELD_STEPS);
        map.add_effect(effect(0, at));
        map.run(Ticks::new(RADII.len() as u32));
        assert!(map.effects.is_empty());
        let health = |id| map.unit(id).unwrap().health.current();
        assert_eq!(health(enemy), 1000 - 3 * DAMAGE_PERSON);
        assert_eq!(health(mine), 1000, "the caster's brave is not hurt");
        assert!(map.unit(mine).unwrap().motion.airborne(), "but pushed (on the last turn, as an ally)");
        assert_eq!(health(shielded), 1000);
        assert!(!map.unit(shielded).unwrap().motion.airborne(), "nor pushed");
    }

    #[test]
    fn a_building_at_the_centre_wears_down_by_damage_bldg_once_not_every_turn() {
        let mut map = crate::map::GameMap::sandbox_walk();
        let s = &map.units[0];
        let at = (s.x.wrapping_add(2048), s.z);
        let building = map.add_building(crate::building::Building::new(crate::building::BuildingKind::Hut { size: 1 }, 1, at.0, at.1, 0));
        map.add_effect(effect(0, at));
        map.run(Ticks::new(RADII.len() as u32));
        assert_eq!(map.building(building).unwrap().wear, DAMAGE_BLDG, "once for the whole blast, unlike people/vehicles which take it every turn in range");
    }
}
