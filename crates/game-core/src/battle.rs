//! Battle scenarios: small fights between two tribes on flat land, run to the end, checking who is left (the
//! headcount of followers, `GameMap::headcount`). Tests only.

use crate::map::GameMap;
use crate::terrain::Heightmap;
use crate::time::Ticks;
use crate::unit::{Order, Target, UnitId, UnitKind};
use crate::vehicle::{Vehicle, VehicleKind};

/// Turns between two looks at who is left.
const BATTLE_STEP: Ticks = Ticks::new(12);
const SIZE: usize = 128;
const LAND: u16 = 64;

/// Flat land everywhere, nobody on it.
fn field(seed: u32) -> GameMap {
    let mut terrain = Heightmap::new(SIZE);
    (0..SIZE as i32).for_each(|z| (0..SIZE as i32).for_each(|x| terrain.set(x, z, LAND)));
    GameMap::bare("Battle", terrain, Vec::new()).seeded(seed)
}

/// The middle of cell `(x, z)`.
fn at(x: i32, z: i32) -> (u16, u16) {
    (x as u16 * 512 + 256, z as u16 * 512 + 256)
}

/// `kinds` of `tribe` on foot, in a row from `(x, z)`, a cell apart.
fn on_foot(map: &mut GameMap, tribe: u8, kinds: &[UnitKind], (x, z): (i32, i32)) {
    for (k, &kind) in kinds.iter().enumerate() {
        map.spawn(tribe, kind, at(x, z + k as i32));
    }
}

/// A `kind` of `tribe` aboard a vehicle of `vehicle` kind at `(x, z)`.
fn aboard(map: &mut GameMap, tribe: u8, kind: UnitKind, vehicle: VehicleKind, (x, z): (i32, i32)) {
    let id = map.add_vehicle(Vehicle::new(vehicle, tribe, at(x, z), crate::angle::Angle::new(0)));
    let u = map.spawn(tribe, kind, at(x, z));
    u.aboard = Some(id);
    let unit = u.id;
    map.vehicles.get_mut(id).unwrap().seat(unit, tribe);
}

/// Everyone goes for the nearest enemy it may attack or preach to.
fn charge(map: &mut GameMap) {
    let orders: Vec<(UnitId, u8, UnitId)> = map
        .units
        .iter()
        .filter_map(|u| {
            let near = |t: &&crate::unit::Unit| crate::map::torus_dist2((u.x, u.z), (t.x, t.z));
            let target = map.units.iter().filter(|t| crate::attack::may_attack(u, t) || crate::sermon::may_preach(u, t)).min_by_key(near)?;
            Some((u.id, u.owner, target.id))
        })
        .collect();
    for (unit, player, target) in orders {
        map.apply(&crate::command::Command::OrderUnit { player, unit, order: Order::Attack { target: Target::Unit(target) } });
    }
}

/// Runs until one of `tribes` has no follower left, a look every `BATTLE_STEP`: the other one, or None after
/// `max_turns`.
fn run_battle(map: &mut GameMap, tribes: (u8, u8), max_turns: u32) -> Option<u8> {
    for _ in 0..max_turns / BATTLE_STEP.get() {
        map.run(BATTLE_STEP);
        match (map.headcount(tribes.0).total(), map.headcount(tribes.1).total()) {
            (0, 0) => return None,
            (0, _) => return Some(tribes.1),
            (_, 0) => return Some(tribes.0),
            _ => {}
        }
    }
    None
}

/// The winner of the scene `setup` makes, over several seeds: the same every time, or the seeds' results.
fn winner(setup: impl Fn(&mut GameMap), max_turns: u32) -> Result<u8, Vec<Option<u8>>> {
    let results: Vec<Option<u8>> = (0..8)
        .map(|seed| {
            let mut map = field(seed);
            setup(&mut map);
            charge(&mut map);
            run_battle(&mut map, (0, 1), max_turns)
        })
        .collect();
    match results[0] {
        Some(w) if results.iter().all(|&r| r == Some(w)) => Ok(w),
        _ => Err(results),
    }
}

#[test]
fn a_warrior_beats_two_braves() {
    let won = winner(
        |map| {
            on_foot(map, 0, &[UnitKind::Warrior], (60, 64));
            on_foot(map, 1, &[UnitKind::Brave, UnitKind::Brave], (64, 64));
        },
        3000,
    );
    assert_eq!(won, Ok(0));
}

#[test]
fn a_preacher_beats_five_warriors() {
    let won = winner(
        |map| {
            on_foot(map, 0, &[UnitKind::Preacher], (60, 64));
            on_foot(map, 1, &[UnitKind::Warrior; 5], (66, 62));
        },
        12_000,
    );
    assert_eq!(won, Ok(0));
}

#[test]
#[ignore = "firewarriors don't throw from vehicles yet"]
fn a_firewarrior_in_a_balloon_beats_five_warriors_on_foot() {
    let won = winner(
        |map| {
            aboard(map, 0, UnitKind::Firewarrior, VehicleKind::Balloon, (60, 64));
            on_foot(map, 1, &[UnitKind::Warrior; 5], (66, 62));
        },
        6000,
    );
    assert_eq!(won, Ok(0));
}

#[test]
#[ignore = "firewarriors don't throw from vehicles yet"]
fn a_firewarrior_in_a_balloon_beats_a_preacher_on_foot() {
    let won = winner(
        |map| {
            aboard(map, 0, UnitKind::Firewarrior, VehicleKind::Balloon, (60, 64));
            on_foot(map, 1, &[UnitKind::Preacher], (64, 64));
        },
        6000,
    );
    assert_eq!(won, Ok(0));
}
