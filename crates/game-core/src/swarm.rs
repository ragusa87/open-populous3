//! Swarm, insect plague (pop3-rev-analysis.md "Per spell" 5, "Insect plague ('Swarm')"), an effect
//! (`effect::EffectKind::Swarm`): no terrain change, no drawable body of its own (same convention as
//! `swamp` - only its separately-spawned bee-dots are drawn, client-side). A cloud wanders loosely around
//! its cast point ("home") for `DURATION` turns: beyond `LEASH_CELLS` of it, it steers back; otherwise it
//! picks a fresh random heading every so many turns. On its own rhythms it stings whoever it currently
//! covers: every `STING_EVERY` turns, enemies in its 3 x 3 cells (reusing `swamp::cells`) take
//! `PERSON_DAMAGE` and panic (no friendly fire, shamans and Angels of Death immune - a broader exemption than
//! Lightning/Lava's own "only the caster's own shaman", confirmed by row 5's plain "shamans... immune" and,
//! for the angel, TODO.md "Units": "shamans/angels excluded by a type table"), and an enemy
//! vehicle there gets its passengers put out (`WorldEvent::PutOut`, TODO.md "Units" - "Swarm spell, on
//! people aboard a vehicle"); every `RAID_EVERY` turns, while it still has one left (`MAX_RAIDS`), an enemy
//! building there with people inside is raided (`WorldEvent::Raid`, its occupants also stung).

use crate::angle::{self, Angle};
use crate::effect::{DamageKind, Effect, EffectKind};
use crate::event::WorldEvent;
use crate::map::{GameMap, Lcg};
use crate::time::{Countdown, Ticks};
use crate::unit::{cell_of, torus_delta, UnitKind};
use pop3_format::WORLD_UNITS_PER_CELL;
use crate::tribes::Owner;

/// 200 turns, 16.7 s [exe].
pub const DURATION: Ticks = Ticks::new(200);
/// Steers back towards home once further than this [exe].
pub const LEASH_CELLS: i32 = 6;
/// World units covered a turn while wandering (already our own units, cross-confirmed) [exe].
pub const SPEED: i32 = 80;
/// A fresh random heading at most this many turns after the last, while within the leash [exe].
pub const REAIM_TURNS: u32 = 32;
/// Person damage/panic and vehicle eviction, every this many turns, in its 3 x 3 cells [exe].
pub const STING_EVERY: Ticks = Ticks::new(8);
/// A sting's own damage [exe].
pub const PERSON_DAMAGE: u16 = 100;
/// A building raid attempt, every this many turns [exe].
pub const RAID_EVERY: Ticks = Ticks::new(64);
/// At most this many successful building raids over its life [exe].
pub const MAX_RAIDS: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Swarm {
    /// Its own current position, wandering; `Effect.x/z` (its "home") never moves.
    x: u16,
    z: u16,
    heading: Angle,
    reaim: Countdown,
    sting: Countdown,
    raid: Countdown,
    raids_left: u8,
}

/// Cast by tribe `owner` at `at` (world units): its home, and its own starting position.
pub fn effect(owner: impl Into<Owner>, at: (u16, u16)) -> Effect {
    let swarm = Swarm {
        x: at.0,
        z: at.1,
        heading: Angle::ZERO,
        reaim: Countdown::new(Ticks::new(REAIM_TURNS)),
        sting: Countdown::new(STING_EVERY),
        raid: Countdown::new(RAID_EVERY),
        raids_left: MAX_RAIDS,
    };
    Effect::new(owner, at, Countdown::new(DURATION), EffectKind::Swarm(swarm))
}

impl Swarm {
    /// Its own current (wandering) position, world units.
    pub fn pos(&self) -> (u16, u16) {
        (self.x, self.z)
    }

    /// One turn at `home` (its cast point), cast by `owner`: wanders one step, then stings and raids on
    /// their own rhythms.
    pub fn turn(&mut self, home: (u16, u16), owner: Owner, world: &GameMap, rng: &mut Lcg) -> Vec<WorldEvent> {
        self.wander(home, rng);
        let mut events = Vec::new();
        if self.sting.tick() {
            self.sting = Countdown::new(STING_EVERY);
            events.extend(self.sting_at(owner, world));
        }
        if self.raids_left > 0 && self.raid.tick() {
            self.raid = Countdown::new(RAID_EVERY);
            events.extend(self.raid_at(owner, world));
        }
        events
    }

    /// One wandering step: steers towards `home` once more than `LEASH_CELLS` away (plus a little jitter),
    /// else keeps its heading until `reaim` draws a fresh one; then moves `SPEED` along it.
    fn wander(&mut self, home: (u16, u16), rng: &mut Lcg) {
        let (dx, dz) = (torus_delta(home.0, self.x), torus_delta(home.1, self.z));
        let leash = (LEASH_CELLS * WORLD_UNITS_PER_CELL as i32).pow(2);
        if dx * dx + dz * dz > leash {
            let jitter = (rng.next_u32() % 127) as i32 - 63;
            self.heading = angle::atan2(-dz, -dx).turned(jitter);
        } else if self.reaim.tick() {
            self.heading = Angle::new(rng.next_u32() % angle::TURN);
            self.reaim = Countdown::new(Ticks::new(1 + rng.next_u32() % REAIM_TURNS));
        }
        let (mx, mz) = angle::rotate((SPEED, 0), self.heading.steps());
        self.x = self.x.wrapping_add(mx as u16);
        self.z = self.z.wrapping_add(mz as u16);
    }

    /// Enemies in its current 3 x 3 cells sting (damage + panic unless shielded), spying no shaman nor the
    /// caster's own tribe; an enemy vehicle there with passengers gets them put out.
    fn sting_at(&self, owner: Owner, world: &GameMap) -> Vec<WorldEvent> {
        let cells = crate::swamp::cells((self.x, self.z), world.terrain.size() as i32);
        let mut events = Vec::new();
        for u in world.units.iter().filter(|u| u.is_alive() && u.in_the_open() && u.owner != owner && !matches!(u.kind, UnitKind::Shaman | UnitKind::AngelOfDeath) && cells.contains(&u.cell())) {
            events.push(WorldEvent::Damage { unit: u.id, amount: PERSON_DAMAGE, kind: DamageKind::Swarm, from: Some(owner) });
            if !u.statuses.shield.is_on() {
                events.push(WorldEvent::Panic { unit: u.id });
            }
        }
        for (id, _) in world.vehicles.iter().filter(|(_, v)| v.owner != owner && !v.passengers.is_empty() && cells.contains(&v.cell())) {
            events.push(WorldEvent::PutOut { vehicle: id });
        }
        events
    }

    /// An enemy building with people inside, in its current 3 x 3 cells, if any: raided (its occupants
    /// thrown out, each also stung), one fewer raid left.
    fn raid_at(&mut self, owner: Owner, world: &GameMap) -> Vec<WorldEvent> {
        if self.raids_left == 0 {
            return Vec::new();
        }
        let cells = crate::swamp::cells((self.x, self.z), world.terrain.size() as i32);
        let at_centre = |c: (u16, u16)| (cell_of(c.0), cell_of(c.1));
        let Some(b) = world.buildings.iter().find(|b| b.owner != owner && b.inside > 0 && cells.contains(&at_centre(b.centre()))) else {
            return Vec::new();
        };
        self.raids_left -= 1;
        let id = b.id;
        let mut events = vec![WorldEvent::Raid { building: id }];
        events.extend(
            world
                .units
                .iter()
                .filter(|u| u.is_alive() && u.inside.is_some_and(|i| i.building == id))
                .map(|u| WorldEvent::Damage { unit: u.id, amount: PERSON_DAMAGE, kind: DamageKind::Swarm, from: Some(owner) }),
        );
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tribes::TribeId;
    use crate::building::{Building, BuildingKind};
    use crate::command::Command;
    use crate::spell::Spell;
    use crate::spell_book::{Availability, SpellKind};
    use crate::terrain::Heightmap;
    use crate::unit::{Inside, Unit};
    use crate::vehicle::{Vehicle, VehicleKind};

    const HOME: (u16, u16) = (64 * WORLD_UNITS_PER_CELL as u16 + 256, 64 * WORLD_UNITS_PER_CELL as u16 + 256);

    fn bare() -> GameMap {
        GameMap::bare("swarm-test", Heightmap::new(128), Vec::new())
    }

    fn bare_swarm() -> Swarm {
        let Effect { kind: EffectKind::Swarm(swarm), .. } = effect(TribeId::Blue, HOME) else { unreachable!() };
        swarm
    }

    #[test]
    fn stays_within_its_leash_and_the_same_seed_wanders_the_same_shape() {
        let run = |seed: u32| {
            let mut swarm = bare_swarm();
            let mut rng = Lcg(seed);
            let mut path = Vec::new();
            for _ in 0..DURATION.get() {
                swarm.wander(HOME, &mut rng);
                let (dx, dz) = (torus_delta(HOME.0, swarm.x), torus_delta(HOME.1, swarm.z));
                path.push((dx, dz));
            }
            path
        };
        let path = run(7);
        assert_eq!(path, run(7), "deterministic");
        let margin = SPEED; // one more step, for the turn it was found beyond the leash
        let leash = LEASH_CELLS * WORLD_UNITS_PER_CELL as i32 + margin;
        assert!(path.iter().all(|&(dx, dz)| dx * dx + dz * dz <= leash * leash), "never wanders far past its leash");
        assert!(path.iter().any(|&(dx, dz)| dx != 0 || dz != 0), "does wander, not stuck at home");
    }

    #[test]
    fn stings_enemies_in_reach_spares_the_casters_tribe_any_shaman_and_any_angel_of_death() {
        let mut map = bare();
        let enemy = map.spawn(TribeId::Red, UnitKind::Brave, HOME).id;
        let own = map.spawn(TribeId::Blue, UnitKind::Brave, HOME).id;
        let shaman = map.spawn(TribeId::Red, UnitKind::Shaman, HOME).id;
        let angel = map.spawn(TribeId::Red, UnitKind::AngelOfDeath, HOME).id;
        let far = map.spawn(TribeId::Red, UnitKind::Brave, (HOME.0.wrapping_add(10 * WORLD_UNITS_PER_CELL as u16), HOME.1)).id;
        let shielded = map.spawn(TribeId::Red, UnitKind::Brave, HOME).id;
        map.unit_mut(shielded).unwrap().statuses.shield = crate::status::Timed::steps(crate::status::SHIELD_STEPS);
        let events = bare_swarm().sting_at(Owner::Tribe(TribeId::Blue), &map);
        let damaged: Vec<_> = events.iter().filter_map(|e| if let WorldEvent::Damage { unit, kind, .. } = e { Some((*unit, *kind)) } else { None }).collect();
        let panicked: Vec<_> = events.iter().filter_map(|e| if let WorldEvent::Panic { unit } = e { Some(*unit) } else { None }).collect();
        assert_eq!(damaged, [(enemy, DamageKind::Swarm), (shielded, DamageKind::Swarm)], "only the enemy braves, not its own tribe, a shaman, an angel, or one out of reach");
        assert_eq!(panicked, [enemy], "the shielded one panics too, only `GameMap::damage` (not this) gates its actual damage");
        assert!(!damaged.iter().any(|&(u, _)| u == own || u == shaman || u == angel || u == far));
    }

    #[test]
    fn evicts_an_enemy_vehicles_passengers_in_reach() {
        let mut map = bare();
        let enemy = map.add_vehicle(Vehicle::new(VehicleKind::Boat, TribeId::Red, HOME, crate::angle::Angle::ZERO));
        map.vehicles.get_mut(enemy).unwrap().passengers.push(crate::id::Id::from_raw(1));
        let own = map.add_vehicle(Vehicle::new(VehicleKind::Boat, TribeId::Blue, HOME, crate::angle::Angle::ZERO));
        map.vehicles.get_mut(own).unwrap().passengers.push(crate::id::Id::from_raw(2));
        let empty = map.add_vehicle(Vehicle::new(VehicleKind::Boat, TribeId::Red, HOME, crate::angle::Angle::ZERO));
        let events = bare_swarm().sting_at(Owner::Tribe(TribeId::Blue), &map);
        assert_eq!(events, vec![WorldEvent::PutOut { vehicle: enemy }]);
        let _ = empty;
    }

    #[test]
    fn raids_an_enemy_building_with_people_inside_up_to_max_raids_times() {
        let mut map = bare();
        let hut = map.add_building(Building::new(BuildingKind::Hut { size: 1 }, TribeId::Red, HOME.0, HOME.1, 0));
        let inside = map.spawn(TribeId::Red, UnitKind::Brave, HOME).id;
        map.unit_mut(inside).unwrap().inside = Some(Inside { building: hut, door: HOME });
        let i = map.building_index(hut).unwrap();
        map.buildings[i].inside = 1;
        let mut swarm = bare_swarm();
        for n in 0..MAX_RAIDS {
            let events = swarm.raid_at(Owner::Tribe(TribeId::Blue), &map);
            assert_eq!(events, vec![WorldEvent::Raid { building: hut }, WorldEvent::Damage { unit: inside, amount: PERSON_DAMAGE, kind: DamageKind::Swarm, from: Some(Owner::Tribe(TribeId::Blue)) }], "raid {n}");
        }
        assert_eq!(swarm.raids_left, 0);
        assert_eq!(swarm.raid_at(Owner::Tribe(TribeId::Blue), &map), Vec::new(), "none left");
    }

    #[test]
    fn no_target_in_reach_raids_nothing() {
        let mut map = bare();
        map.add_building(Building::new(BuildingKind::Hut { size: 1 }, TribeId::Red, HOME.0, HOME.1, 0)); // empty: nobody inside
        let mut swarm = bare_swarm();
        assert_eq!(swarm.raid_at(Owner::Tribe(TribeId::Blue), &map), Vec::new());
        assert_eq!(swarm.raids_left, MAX_RAIDS, "untouched: nothing to raid");
    }

    #[test]
    fn turning_moves_it_and_can_sting() {
        let mut map = bare();
        let enemy = Unit::new(crate::id::Id::from_raw(900), TribeId::Red, UnitKind::Brave, HOME);
        map.units.push(enemy);
        let mut swarm = bare_swarm();
        let mut rng = Lcg(3);
        let mut any_events = false;
        for _ in 0..STING_EVERY.get() {
            any_events |= !swarm.turn(HOME, Owner::Tribe(TribeId::Blue), &map, &mut rng).is_empty();
        }
        assert!(swarm.pos() != HOME, "it moved");
        assert!(any_events, "stung on its own rhythm");
    }

    #[test]
    fn casting_it_through_the_spell_book_adds_the_controller_effect() {
        let mut map = GameMap::sandbox_walk();
        map.spell_books[0].set(SpellKind::Swarm, Availability::Unlimited);
        let at = (map.units[0].x, map.units[0].z);
        map.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::Swarm { at } });
        map.run(crate::unit::CAST_RELEASE);
        assert!(map.effects.entries().any(|(_, e)| matches!(e.kind, EffectKind::Swarm(_))));
    }
}
