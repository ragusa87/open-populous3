//! Training (pop3-rev-analysis.md "Training buildings"): a follower inside a built training unit of its tribe sets
//! the building's cost (`train_cost`); the mana distribution fills its store (`crate::production`), and once the
//! store reaches the cost the trainee is replaced by a new person of the building's kind at the door. Ordered out
//! before, it leaves unchanged and the store is given back (`Production::refund`).

use crate::building::{Building, BuildingId, BuildingKind, Stage, DOOR_GAP};
use crate::map::GameMap;
use crate::slots;
use crate::time::Every;
use crate::unit::{Action, Inside, Order, UnitKind};

/// The cost is recomputed on this rhythm while training ("Cost").
pub const RECOST_EVERY: Every = Every::new(16);
/// The queue's spots are this far apart along the walls: a quarter cell ("The queue at the door" [exe]).
pub const QUEUE_STEP: i32 = 128;
/// One lap of the walls full, the queue goes on this much further out [ours: past one lap is not traced].
pub const QUEUE_ROW: i32 = 192;
/// Turns before the next is let in after one went in, and after one came out ("The queue at the door" [exe]).
pub const ADMIT_GAP: u8 = 16;
pub const LEFT_GAP: u8 = 12;

/// Where the queue's `rank`-th person stands (0: the head, a step beside the door, which stays free for whoever goes
/// in or out): along the building's walls from its door,
/// a lap of spots at the door's distance, then the next lap further out, turning the other way [ours], like a snake.
pub fn queue_spot(b: &Building, rank: usize) -> (u16, u16) {
    let f = b.kind.footprint();
    let mut rank = rank as i32 + 1;
    for ring in 0.. {
        let (hx, hz) = (f.half.0 + DOOR_GAP + ring * QUEUE_ROW, f.half.1 + DOOR_GAP + ring * QUEUE_ROW);
        let lap = 4 * (hx + hz);
        let spots = lap / QUEUE_STEP;
        if rank >= spots {
            rank -= spots;
            continue;
        }
        let d = if ring % 2 == 0 { rank * QUEUE_STEP } else { (lap - rank * QUEUE_STEP) % lap };
        let (lx, lz) = along_walls(hx, hz, d);
        return b.local_point((f.offset.0 + lx, f.offset.1 + lz));
    }
    unreachable!()
}

/// The point `d` along the rectangle `±hx × ±hz` (its own frame), from the middle of its -z side (the door's),
/// going towards +x first.
fn along_walls(hx: i32, hz: i32, d: i32) -> (i32, i32) {
    let legs = [(hx, (1, 0)), (2 * hz, (0, 1)), (2 * hx, (-1, 0)), (2 * hz, (0, -1)), (hx, (1, 0))];
    let (mut x, mut z, mut left) = (0, -hz, d);
    for (len, (dx, dz)) in legs {
        let step = left.min(len);
        (x, z) = (x + dx * step, z + dz * step);
        left -= step;
        if left == 0 {
            break;
        }
    }
    (x, z)
}

/// Percent of the cost by the number of that kind the tribe already has, up to each bound (`TRAIN_MANA_BAND_*`);
/// above the last, `TOP_BAND`.
pub const BANDS: [(usize, u32); 5] = [(3, 100), (7, 125), (11, 150), (15, 175), (20, 200)];
pub const TOP_BAND: u32 = 250;

/// The kind a building trains, None for the others.
pub fn trains(kind: BuildingKind) -> Option<UnitKind> {
    match kind {
        BuildingKind::Temple => Some(UnitKind::Preacher),
        BuildingKind::SpyTraining => Some(UnitKind::Spy),
        BuildingKind::WarriorTraining => Some(UnitKind::Warrior),
        BuildingKind::FirewarriorTraining => Some(UnitKind::Firewarrior),
        _ => None,
    }
}

/// Mana to train one `kind` (`HUMAN_TRAIN_MANA_*`; the computer players' 1000 once tribes have a controller).
pub fn train_mana(kind: UnitKind) -> u32 {
    match kind {
        UnitKind::Warrior | UnitKind::Preacher => 3500,
        UnitKind::Spy | UnitKind::Firewarrior => 4000,
        _ => 0,
    }
}

/// Mana to train one `kind` for a tribe that has `count` of them already.
pub fn train_cost(kind: UnitKind, count: usize) -> u16 {
    let band = BANDS.iter().find(|&&(bound, _)| count <= bound).map_or(TOP_BAND, |&(_, p)| p);
    (train_mana(kind) * band / 100).min(u16::MAX as u32) as u16
}

impl GameMap {
    /// Unit `i`, sent into the training unit `building`: straight in when nobody is queuing and there is room, else
    /// at the end of its queue.
    pub(crate) fn join_queue(&mut self, i: usize, building: BuildingId) {
        let Some(b) = self.building(building) else { return };
        let coming = self.units.iter().filter(|u| u.heading_into() == Some(building)).count();
        let (door, full) = (b.door(), !b.queue.is_empty() || b.admit_gate > 0 || self.people_inside(building) + coming >= b.kind.capacity() as usize);
        let id = self.units[i].id;
        if !full {
            self.units[i].go_enter(building, door);
            return;
        }
        self.units[i].queue_up(building);
        if let Some(b) = self.buildings.iter_mut().find(|b| b.id == building)
            && !b.queue.contains(&id)
        {
            b.queue.push(id);
        }
    }

    /// Each training unit's queue: those who left it dropped, everyone at its spot, the head let in when there is
    /// room and the gate is open.
    pub(crate) fn tend_queues(&mut self) {
        for k in 0..self.buildings.len() {
            let id = self.buildings[k].id;
            if trains(self.buildings[k].kind).is_none() {
                continue;
            }
            let units = &self.units;
            let queuing = |u: &crate::unit::UnitId| units.binary_search_by_key(u, |x| x.id).ok().is_some_and(|i| matches!(units[i].action, Action::Queuing { building, .. } if building == id) && units[i].is_alive());
            let b = &mut self.buildings[k];
            b.queue.retain(queuing);
            b.admit_gate = b.admit_gate.saturating_sub(1);
            let door = b.door();
            let spots: Vec<(crate::unit::UnitId, (u16, u16))> = b.queue.iter().enumerate().map(|(rank, &u)| (u, queue_spot(b, rank))).collect();
            for (rank, &(u, at)) in spots.iter().enumerate() {
                let ahead = if rank == 0 { door } else { spots[rank - 1].1 };
                if let Some(u) = self.unit_mut(u) {
                    u.queue_at(at);
                    if (u.x, u.z) == at {
                        u.face(ahead);
                    }
                }
            }
            let b = &self.buildings[k];
            let coming = self.units.iter().filter(|u| u.heading_into() == Some(id)).count();
            let room = b.stage() == Stage::Built && self.people_inside(id) + coming < b.kind.capacity() as usize && b.admit_gate == 0;
            if let (true, Some(&head)) = (room, b.queue.first()) {
                let door = b.door();
                let b = &mut self.buildings[k];
                b.queue.remove(0);
                b.admit_gate = ADMIT_GAP;
                if let Some(i) = self.unit_index(head) {
                    self.units[i].go_enter(id, door);
                }
            }
        }
    }

    /// Each built training unit: its cost set from its trainee, or cleared without one; with the store full, the
    /// trainee is replaced by a new person of its kind at the door, who walks to the nearest free spot.
    pub(crate) fn tend_training(&mut self) {
        let now = self.now;
        for k in 0..self.buildings.len() {
            let b = &self.buildings[k];
            let Some(output) = trains(b.kind).filter(|_| b.stage() == Stage::Built) else { continue };
            let (id, owner, door) = (b.id, b.owner, b.door());
            let trainee = self.units_inside(id).next().map(|u| u.id);
            let count = self.units.iter().filter(|u| u.owner == owner && u.kind == output && u.is_alive()).count();
            let p = self.buildings[k].production.get_or_insert_default();
            let Some(trainee) = trainee else {
                p.cost = 0;
                continue;
            };
            if !p.is_active() || RECOST_EVERY.fires(now, 0) {
                p.cost = train_cost(output, count);
            }
            if p.stored < p.cost {
                continue;
            }
            p.stored = 0;
            p.cost = 0;
            let i = self.unit_index(trainee).unwrap();
            let gone = self.units.remove(i);
            self.buildings[k].admit_gate = LEFT_GAP;
            let taken = self.taken_spots(&[]);
            let spot = slots::free_spots_near(&self.ground(), slots::spot_of(door), &taken, 1).first().copied();
            let made = self.spawn(owner, output, (gone.x, gone.z));
            made.statuses.ghost = gone.statuses.ghost;
            made.inside = Some(Inside { building: id, door });
            let (x, z) = spot.map_or(door, slots::spot_centre);
            made.start(Order::MoveTo { x, z });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::{Action, UnitId};

    #[test]
    fn the_cost_grows_with_the_band() {
        assert_eq!(train_cost(UnitKind::Warrior, 0), 3500);
        assert_eq!(train_cost(UnitKind::Warrior, 3), 3500);
        assert_eq!(train_cost(UnitKind::Warrior, 4), 4375);
        assert_eq!(train_cost(UnitKind::Firewarrior, 12), 7000);
        assert_eq!(train_cost(UnitKind::Spy, 20), 8000);
        assert_eq!(train_cost(UnitKind::Preacher, 21), 8750);
    }

    #[test]
    fn only_training_units_train() {
        assert_eq!(trains(BuildingKind::FirewarriorTraining), Some(UnitKind::Firewarrior));
        assert_eq!(trains(BuildingKind::Hut { size: 1 }), None);
    }

    fn training_hut(map: &GameMap) -> usize {
        map.buildings.iter().position(|b| b.owner == 0 && b.kind == BuildingKind::WarriorTraining && b.stage() == Stage::Built && b.inside == 0).unwrap()
    }

    fn send_brave(map: &mut GameMap, hut: usize) -> UnitId {
        let brave = map.units.iter().find(|u| u.owner == 0 && u.kind == UnitKind::Brave && u.inside.is_none() && u.campfire().is_none()).unwrap().id;
        for c in map.enter_orders(0, &[brave], hut) {
            map.apply(&c);
        }
        let id = map.buildings[hut].id;
        (0..800).find(|_| {
            map.tick();
            map.units_inside(id).count() == 1
        });
        assert_eq!(map.units_inside(id).next().map(|u| u.id), Some(brave), "inside");
        brave
    }

    #[test]
    fn a_brave_inside_comes_out_a_warrior_once_the_store_is_full() {
        let mut map = GameMap::sandbox_buildings();
        let hut = training_hut(&map);
        let warriors = map.units.iter().filter(|u| u.owner == 0 && u.kind == UnitKind::Warrior).count();
        let brave = send_brave(&mut map, hut);
        map.tick();
        let p = map.buildings[hut].production.unwrap();
        assert_eq!(p.cost, train_cost(UnitKind::Warrior, warriors));
        map.buildings[hut].production.as_mut().unwrap().stored = p.cost - 1;
        (0..32).find(|_| {
            map.tick();
            map.unit(brave).is_none()
        })
        .expect("trained after the next income");
        assert_eq!(map.buildings[hut].production.map(|p| (p.stored, p.cost)), Some((0, 0)));
        let made = map.units.last().unwrap();
        let building = map.buildings[hut].id;
        assert_eq!((made.owner, made.kind, made.inside.map(|i| i.building)), (0, UnitKind::Warrior, Some(building)), "made inside, where the brave was");
        assert!(made.health.current() == made.health.max(), "full life");
        let id = made.id;
        (0..200).find(|_| {
            map.tick();
            map.unit(id).is_some_and(|u| u.inside.is_none() && u.action == Action::Idle)
        })
        .expect("walks out by the door, then off it");
    }

    #[test]
    fn a_trainee_ordered_out_leaves_unchanged_and_the_store_goes_back() {
        let mut map = GameMap::sandbox_buildings();
        let hut = training_hut(&map);
        let brave = send_brave(&mut map, hut);
        map.tick();
        map.buildings[hut].production.as_mut().unwrap().stored = 250;
        let (x, z) = map.buildings[hut].door();
        map.apply(&crate::command::Command::OrderUnit { player: 0, unit: brave, order: Order::MoveTo { x: x.wrapping_add(2048), z } });
        (0..300).find(|_| {
            map.tick();
            map.buildings[hut].production.unwrap().stored == 0
        })
        .expect("given back");
        assert_eq!(map.unit(brave).unwrap().kind, UnitKind::Brave);
        assert_eq!(map.buildings[hut].production.unwrap().cost, 0);
    }

    #[test]
    fn the_queue_goes_round_the_walls_from_the_door_then_further_out() {
        let b = Building::new(BuildingKind::WarriorTraining, 0, 64 * 512, 64 * 512, 0);
        assert_ne!(queue_spot(&b, 0), b.door(), "the doorway stays free");
        assert!(crate::map::torus_dist2(queue_spot(&b, 0), b.door()) <= (QUEUE_STEP as i64).pow(2), "the head waits beside it");
        let f = b.kind.footprint();
        let lap = (4 * (f.half.0 + DOOR_GAP + f.half.1 + DOOR_GAP) / QUEUE_STEP) as usize;
        for rank in 0..lap - 1 {
            assert!(b.covers(queue_spot(&b, rank), DOOR_GAP) && !b.covers(queue_spot(&b, rank), DOOR_GAP - 1), "rank {rank} hugs the walls");
        }
        let next = queue_spot(&b, lap - 1);
        assert!(!b.covers(next, DOOR_GAP) && b.covers(next, DOOR_GAP + QUEUE_ROW), "the second lap a row further out");
        let one = |r| crate::map::torus_dist2(queue_spot(&b, r), queue_spot(&b, r + 1));
        assert!((0..lap - 2).all(|r| one(r) <= (QUEUE_STEP as i64).pow(2)), "a quarter cell apart");
    }

    #[test]
    fn braves_sent_together_queue_round_it_and_go_in_one_after_the_other() {
        let mut map = GameMap::sandbox_buildings();
        let hut = training_hut(&map);
        let id = map.buildings[hut].id;
        let braves: Vec<UnitId> = map.units.iter().filter(|u| u.owner == 0 && u.kind == UnitKind::Brave && u.inside.is_none() && u.campfire().is_none()).take(4).map(|u| u.id).collect();
        for c in map.enter_orders(0, &braves, hut) {
            map.apply(&c);
        }
        map.tick();
        assert_eq!(map.buildings[hut].queue.len(), 3, "one goes straight in, the others queue");
        let queued = map.buildings[hut].queue.clone();
        (0..400).find(|_| {
            map.tick();
            queued.iter().all(|&u| matches!(map.unit(u).unwrap().action, Action::Queuing { at, .. } if at == (map.unit(u).unwrap().x, map.unit(u).unwrap().z)))
        })
        .expect("at their spots");
        let b = &map.buildings[hut];
        assert!(queued.iter().enumerate().all(|(r, &u)| map.unit(u).map(|u| (u.x, u.z)) == Some(queue_spot(b, r))), "in arrival order round the walls");
        map.tick();
        let b = &map.buildings[hut];
        for (r, &u) in queued.iter().enumerate() {
            let ahead = if r == 0 { b.door() } else { queue_spot(b, r - 1) };
            let u = map.unit(u).unwrap();
            let (dx, dz) = (crate::unit::torus_delta(u.x, ahead.0), crate::unit::torus_delta(u.z, ahead.1));
            assert_eq!(u.facing, crate::unit::octant(dx, dz), "rank {r} faces the one ahead");
        }
        let leaving = queued[0];
        map.apply(&crate::command::Command::OrderUnit { player: 0, unit: leaving, order: Order::Stop });
        map.tick();
        assert_eq!(map.buildings[hut].queue, queued[1..], "one ordered off leaves, the others move up");
        (0..600).find(|_| {
            map.tick();
            map.units_inside(id).any(|u| u.action == Action::Idle)
        })
        .expect("the first one inside, training");
        let inside = map.units_inside(id).next().map(|u| u.id).unwrap();
        let (x, z) = map.buildings[hut].door();
        map.apply(&crate::command::Command::OrderUnit { player: 0, unit: inside, order: Order::MoveTo { x: x.wrapping_add(2048), z } });
        let walked_in = (0..300).find(|_| {
            map.tick();
            map.units_inside(id).any(|u| u.id == queued[1])
        });
        assert!(walked_in.is_some(), "the head goes in once there is room");
    }
}
