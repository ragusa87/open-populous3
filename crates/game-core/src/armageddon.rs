//! Armageddon (outcome.md "Armageddon": the user's design over the trace's one line, pop3-rev-analysis.md "Per spell"
//! 18). On cast: every shaman dying, dead or gone who still has a reincarnation site comes back (`revive_shamans`),
//! then every site goes for good; every building becomes one brave of its owner at its door, then every building,
//! totem and vehicle goes, their people set down where they were (`wipe_for_armageddon`); the arena is carved into
//! the ground (`arena_heights`: a square, flat floor, a walkable slope on each side, a cliff rim). Every tribe with
//! people on the map takes part. Then the match (`ArmageddonMatch`) teleports them in one per turn
//! (`Phase::Arriving`, the shamans first, to the arena's top corners; then each tribe's people in turn onto its own
//! slope; anyone standing in the sea at once, before drowning), holds a moment (`Phase::Gathering`), and lets them fight (`Phase::Fighting`): all run into the middle,
//! then go for the nearest free enemy; nobody goes for a shaman. While a tribe's own fighters live its shaman stays
//! shielded; the turn they are all dead her shield drops and a bolt (the Lightning spell's own effect, drawn) strikes
//! her. Anyone past 9 cells, thrown onto the rim or out, is struck too. The last side with fighters wins; sides
//! falling together, or none after `STALEMATE`, is a draw. The numbers are `[ours]`.

use crate::coordinator::{Coordinator, Step};
use crate::event::WorldEvent;
use crate::id::Id;
use crate::map::{GameMap, Lcg};
use crate::outcome::Reason;
use crate::time::{Countdown, Tick, Ticks};
use crate::tribes::{Owner, TribeId};
use crate::unit::{Action, Unit, UnitId, UnitKind};
use pop3_format::WORLD_UNITS_PER_CELL;

/// The arena, square, in cells from its centre (`arena_heights`): the flat floor up to `FLOOR_HALF`, then a slope
/// `SLOPE_CELLS` deep rising `SLOPE_STEP` a cell, then a rim `RIM_CELLS` wide `RIM_HEIGHT` over the slope's top, a
/// cliff nobody walks up (`path::MAX_CLIMB`). `[ours]`, but the half-width with its slopes is the trace's 9 cells:
/// past it, on the rim or beyond (only ever thrown there), one dies (`beyond_the_arena`).
pub const FLOOR_HALF: i32 = 5;
pub const SLOPE_CELLS: i32 = 4;
pub const SLOPE_STEP: u16 = 80;
pub const RIM_CELLS: i32 = 2;
pub const RIM_HEIGHT: u16 = 400;
/// The half-width up to the rim, and with it.
pub const ARENA_HALF_CELLS: i32 = FLOOR_HALF + SLOPE_CELLS;
pub const OUTER_HALF_CELLS: i32 = ARENA_HALF_CELLS + RIM_CELLS;
/// The floor's lowest height, well above the sea (`[ours]`).
pub const FLOOR_MIN: u16 = 64;
/// Fixed by phase (`[ours]`): the hold once everyone has arrived, before anyone fights, and the stalemate, a draw.
pub const GATHERING: Ticks = Ticks::secs(3);
pub const STALEMATE: Ticks = Ticks::secs(180);
/// A fighting shaman's shield, topped up to this many steps every turn: "permanent" (`[ours]`).
pub const SHIELD_STEPS: u8 = 2;
/// Rows of fighters on a slope, half a cell apart from its foot; the shaman stands behind the last.
const ROWS: usize = 6;
/// Where the fighters run first when the fight begins: this fraction of the way to the centre (`[ours]`).
const RALLY_KEEP: i32 = 4;
/// The most standing side by side in a row before rows fill up (`[ours]`).
const ROW: usize = 8;
const CELL: i32 = WORLD_UNITS_PER_CELL as i32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// One person teleported in per turn: `next` in `arrivals`.
    Arriving { next: usize },
    Gathering(Countdown),
    Fighting(Countdown),
}

/// Armageddon's match: its centre (the client camera's target too), its phase, who arrives where and in what order,
/// and which tribes are still in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArmageddonMatch {
    pub at: (u16, u16),
    pub phase: Phase,
    arrivals: Vec<(UnitId, (u16, u16))>,
    in_match: [bool; crate::totem::TRIBES],
}

impl ArmageddonMatch {
    fn new(at: (u16, u16), tribes: impl Iterator<Item = TribeId>, arrivals: Vec<(UnitId, (u16, u16))>) -> Self {
        let mut in_match = [false; crate::totem::TRIBES];
        tribes.for_each(|t| in_match[t.as_usize()] = true);
        ArmageddonMatch { at, phase: Phase::Arriving { next: 0 }, arrivals, in_match }
    }

    pub fn fighting(&self) -> bool {
        matches!(self.phase, Phase::Fighting(_))
    }
}

impl Coordinator for ArmageddonMatch {
    /// Global: no unit links to it, it reads the arena straight off `world`.
    fn link_of(_u: &Unit) -> Option<Id<Self>> {
        None
    }

    fn tick(&mut self, _id: Id<Self>, _members: &[UnitId], _now: Tick, world: &GameMap, _rng: &mut Lcg) -> Step {
        let timed_out = match &mut self.phase {
            Phase::Arriving { next } => {
                let Some(&(unit, to)) = self.arrivals.get(*next) else {
                    self.phase = Phase::Gathering(Countdown::new(GATHERING));
                    return Step::default();
                };
                *next += 1;
                let here = world.unit(unit).is_some_and(Unit::is_alive);
                return Step { finished: false, events: here.then_some(WorldEvent::Arrive { unit, to }).into_iter().collect() };
            }
            Phase::Gathering(left) => {
                if !left.tick() {
                    return Step::default();
                }
                self.phase = Phase::Fighting(Countdown::new(STALEMATE));
                return Step { finished: false, events: rally(world, self.at) };
            }
            Phase::Fighting(left) => left.tick(),
        };
        let still: Vec<TribeId> = TribeId::ALL.into_iter().filter(|t| self.in_match[t.as_usize()]).collect();
        let (standing, fallen): (Vec<TribeId>, Vec<TribeId>) = still.into_iter().partition(|&t| fighters(world, t) > 0);
        if standing.is_empty() {
            return Step { finished: true, events: vec![WorldEvent::Declare { winner: None, reason: Reason::Armageddon }] };
        }
        let mut events: Vec<WorldEvent> = fallen.iter().flat_map(|&t| struck_down(world, t)).collect();
        events.extend(beyond_the_arena(world, self.at).map(|u| WorldEvent::SpawnEffect(crate::lightning::effect(Owner::Wild, (u.x, u.z)))));
        fallen.iter().for_each(|t| self.in_match[t.as_usize()] = false);
        events.extend(standing.iter().filter_map(|&t| shaman(world, t)).map(|u| WorldEvent::Shield { unit: u.id, steps: SHIELD_STEPS }));
        let one_side = standing.iter().all(|&t| world.tribes.same_team(t, standing[0]));
        if one_side || timed_out {
            events.push(WorldEvent::Declare { winner: one_side.then_some(standing[0]), reason: Reason::Armageddon });
            return Step { finished: true, events };
        }
        events.extend(charges(world));
        Step { finished: false, events }
    }
}

/// A tribe's living fighters in the match: its followers, never its shaman or a Ghost Army copy.
fn fighters(world: &GameMap, tribe: TribeId) -> usize {
    world.units.iter().filter(|u| u.owner == Owner::Tribe(tribe) && UnitKind::FOLLOWERS.contains(&u.kind) && !u.statuses.ghost && u.is_alive()).count()
}

/// A tribe's real (not a ghost) living shaman.
fn shaman(world: &GameMap, tribe: TribeId) -> Option<&Unit> {
    world.units.iter().find(|u| u.owner == Owner::Tribe(tribe) && u.kind == UnitKind::Shaman && !u.statuses.ghost && u.is_alive())
}

/// A tribe whose fighters are all dead: its shaman's shield drops and a bolt of lightning, the spell's own effect
/// cast by nobody (so it spares no shaman), strikes where she stands (no site to come back to).
fn struck_down(world: &GameMap, tribe: TribeId) -> Vec<WorldEvent> {
    let Some(her) = shaman(world, tribe) else { return Vec::new() };
    vec![WorldEvent::Shield { unit: her.id, steps: 0 }, WorldEvent::SpawnEffect(crate::lightning::effect(Owner::Wild, (her.x, her.z)))]
}

/// The fight begins: every fighter runs towards the middle, keeping its place in the ranks
/// (`RALLY_KEEP`-th of the way out from the centre it was), into the enemies running in from the other slopes.
fn rally(world: &GameMap, centre: (u16, u16)) -> Vec<WorldEvent> {
    world
        .units
        .iter()
        .filter(|u| UnitKind::FOLLOWERS.contains(&u.kind) && u.is_alive())
        .map(|u| {
            let (dx, dz) = offset(centre, (u.x, u.z));
            let to = ((centre.0 as i32 + dx / RALLY_KEEP) as u16, (centre.1 as i32 + dz / RALLY_KEEP) as u16);
            WorldEvent::Rally { unit: u.id, to }
        })
        .collect()
}

/// `to` off `from` the short way round the torus (world units).
fn offset(from: (u16, u16), to: (u16, u16)) -> (i32, i32) {
    (to.0.wrapping_sub(from.0) as i16 as i32, to.1.wrapping_sub(from.1) as i16 as i32)
}

/// The living people on the ground past `ARENA_HALF_CELLS` from the centre, the trace's "anyone past 9 cells away
/// dies": thrown onto the rim or out of the arena. A bolt strikes each (drawn, like the shamans').
fn beyond_the_arena(world: &GameMap, centre: (u16, u16)) -> impl Iterator<Item = &Unit> {
    world.units.iter().filter(move |u| {
        let (dx, dz) = offset(centre, (u.x, u.z));
        u.is_alive() && !u.motion.airborne() && dx.abs().max(dz.abs()) > ARENA_HALF_CELLS * CELL
    })
}

/// Every idle fighter, and every one going for an enemy already fighting someone else while a free one is left, sent
/// at the nearest free enemy fighter it may attack (else the nearest): sight alone would leave them apart, and a
/// crowd waiting on one full fight would stand there running on the spot.
fn charges(world: &GameMap) -> Vec<WorldEvent> {
    let fighter = |u: &Unit| UnitKind::FOLLOWERS.contains(&u.kind) && u.is_alive();
    let busy = |t: &Unit| world.linked(t).is_some();
    world
        .units
        .iter()
        .filter(|u| fighter(u))
        .filter_map(|u| {
            let foe = |t: &Unit| fighter(t) && !world.tribes.same_team(u.owner, t.owner) && crate::attack::may_attack(u, t);
            let free = crate::nearest::nearest_n(&world.units, (u.x, u.z), 1, |t| foe(t) && !busy(t)).first().copied();
            let at = match u.action {
                Action::Idle => free.or_else(|| crate::nearest::nearest_n(&world.units, (u.x, u.z), 1, foe).first().copied())?,
                Action::Attacking { target: crate::unit::Target::Unit(t), .. } if !u.kind.stats().throws && world.unit(t).is_some_and(busy) => free?,
                _ => return None,
            };
            Some(WorldEvent::StrikeBack { unit: u.id, at })
        })
        .collect()
}

/// The height point nearest `at` (world units): the arena is carved around it and its people placed from it.
pub fn arena_centre(at: (u16, u16)) -> (u16, u16) {
    let snap = |v: u16| (((v as u32 + CELL as u32 / 2) / CELL as u32 * CELL as u32) % 65_536) as u16;
    (snap(at.0), snap(at.1))
}

/// The slope (0-3: south, east, north, west) of side `side` of `sides`: opposite ones for two.
fn slope_of(side: usize, sides: usize) -> usize {
    if sides == 2 { side * 2 } else { side % 4 }
}

/// Out of the arena's centre along slope `slope` by `depth`, then `along` across it (world units).
fn on_slope(slope: usize, depth: i32, along: i32) -> (i32, i32) {
    let (nx, nz) = [(0, 1), (1, 0), (0, -1), (-1, 0)][slope % 4];
    (nx * depth - nz * along, nz * depth + nx * along)
}

/// Where the `k`-th of `count` fighters of side `side` (of `sides`) stands, off the arena's centre: on its slope,
/// rows half a cell apart from the slope's foot up, each row spread evenly across the side (off its corners).
pub fn arena_offset(side: usize, sides: usize, k: usize, count: usize) -> (i32, i32) {
    let cols = count.div_ceil(ROWS).max(count.min(ROW)).max(1);
    let (row, col) = ((k / cols) as i32, (k % cols) as i32);
    let width = (2 * FLOOR_HALF - 1) * CELL;
    let along = -width / 2 + width * (2 * col + 1) / (2 * cols as i32);
    on_slope(slope_of(side, sides), FLOOR_HALF * CELL + CELL / 2 + row * CELL / 2, along)
}

/// Where side `side`'s shaman stands: at a top corner of the arena, the one at the end of her tribe's slope.
pub fn shaman_offset(side: usize, sides: usize) -> (i32, i32) {
    let top = ARENA_HALF_CELLS * CELL - CELL / 2;
    on_slope(slope_of(side, sides), top, top)
}

/// The order they arrive in: the shamans first, then one of each tribe in turn. `people`: each tribe's (shaman,
/// followers), in side order.
pub fn arrival_order<T: Copy>(people: &[(Option<T>, Vec<T>)]) -> Vec<T> {
    let mut order: Vec<T> = people.iter().filter_map(|(s, _)| *s).collect();
    let most = people.iter().map(|(_, f)| f.len()).max().unwrap_or(0);
    for k in 0..most {
        order.extend(people.iter().filter_map(|(_, f)| f.get(k).copied()));
    }
    order
}

/// The arena carved around `centre` (a height point) on a `size`-point torus, out to `OUTER_HALF_CELLS`: the floor
/// at `floor`, the slopes rising from it, the rim above their top (squares: the distance is the larger axis).
pub fn arena_heights(centre: (i32, i32), floor: u16, size: i32) -> Vec<((i32, i32), u16)> {
    let top = floor + SLOPE_STEP * SLOPE_CELLS as u16;
    let r = OUTER_HALF_CELLS;
    (-r..=r)
        .flat_map(|dz| (-r..=r).map(move |dx| (dx, dz)))
        .map(|(dx, dz)| {
            let d = dx.abs().max(dz.abs());
            let h = match d {
                _ if d <= FLOOR_HALF => floor,
                _ if d <= ARENA_HALF_CELLS => floor + SLOPE_STEP * (d - FLOOR_HALF) as u16,
                _ => top + RIM_HEIGHT,
            };
            (((centre.0 + dx).rem_euclid(size), (centre.1 + dz).rem_euclid(size)), h)
        })
        .collect()
}

impl GameMap {
    /// The "Armageddon flag": `Some(at)`, the cast target and the client camera's destination, while a match is
    /// active. The one place anything should read `self.coordinators.armageddons`.
    pub fn armageddon(&self) -> Option<(u16, u16)> {
        self.armageddon_match().map(|m| m.at)
    }

    pub fn armageddon_match(&self) -> Option<&ArmageddonMatch> {
        self.coordinators.armageddons.iter().next().map(|(_, m)| m)
    }

    /// Armageddon takes effect at `at` (module doc): the shamans back, the sites gone, the wipe, the arena carved,
    /// and the match, which then teleports everyone in. The ground it changed.
    pub(crate) fn start_armageddon(&mut self, at: (u16, u16)) -> crate::terrain::DirtyRect {
        let at = arena_centre(at);
        if let Some(fog) = self.fog.as_mut() {
            fog.reveal_all(((at.0 as u32 / WORLD_UNITS_PER_CELL) as i32, (at.1 as u32 / WORLD_UNITS_PER_CELL) as i32), crate::fog::ARMAGEDDON_SIGHT);
        }
        self.revive_shamans();
        let tribes: Vec<TribeId> = self.sites.iter().map(|s| s.owner).collect();
        tribes.into_iter().for_each(|t| self.remove_reincarnation(t));
        self.wipe_for_armageddon(at);
        let dirty = self.carve_arena(at);
        let order = self.armageddon_sides();
        let size = (self.terrain.size() * WORLD_UNITS_PER_CELL as usize) as i32;
        let place = |(dx, dz): (i32, i32)| (((at.0 as i32 + dx).rem_euclid(size)) as u16, ((at.1 as i32 + dz).rem_euclid(size)) as u16);
        let mut spots = std::collections::BTreeMap::new();
        let mut people = Vec::new();
        for (side, &tribe) in order.iter().enumerate() {
            let own = |u: &&Unit| u.owner == Owner::Tribe(tribe) && !u.statuses.ghost && u.is_alive();
            let shaman = self.units.iter().filter(own).find(|u| u.kind == UnitKind::Shaman).map(|u| u.id);
            let followers: Vec<&Unit> = self.units.iter().filter(own).filter(|u| UnitKind::FOLLOWERS.contains(&u.kind)).collect();
            if let Some(s) = shaman {
                spots.insert(s, place(shaman_offset(side, order.len())));
            }
            for (k, u) in followers.iter().enumerate() {
                spots.insert(u.id, place(arena_offset(side, order.len(), k, followers.len())));
            }
            people.push((shaman, followers.iter().map(|u| u.id).collect::<Vec<_>>()));
        }
        let in_sea: Vec<UnitId> = self.units.iter().filter(|u| spots.contains_key(&u.id) && crate::unit::is_sea(&self.terrain, u.cell())).map(|u| u.id).collect();
        for &u in &in_sea {
            self.apply_event(WorldEvent::Arrive { unit: u, to: spots[&u] });
        }
        let arrivals = arrival_order(&people).into_iter().filter(|u| !in_sea.contains(u)).map(|u| (u, spots[&u])).collect();
        let id = self.new_id_of();
        self.coordinators.armageddons.insert(id, ArmageddonMatch::new(at, order.into_iter(), arrivals));
        dirty
    }

    /// The tribes in the match: every tribe with a living person on the map, in or out of the level's roster;
    /// allies next to each other (`Tribes::teams`), then the others by id.
    fn armageddon_sides(&self) -> Vec<TribeId> {
        let has_people = |t: TribeId| self.units.iter().any(|u| u.owner == Owner::Tribe(t) && !u.statuses.ghost && u.is_alive());
        let mut order: Vec<TribeId> = self.tribes.teams().into_iter().flatten().filter(|&t| has_people(t)).collect();
        let rest: Vec<TribeId> = TribeId::ALL.into_iter().filter(|t| !order.contains(t) && has_people(*t)).collect();
        order.extend(rest);
        order
    }

    /// The arena carved into the ground at `at`, a height point (`arena_heights`), its floor at the centre's height
    /// but at least `FLOOR_MIN`. The ground it changed.
    fn carve_arena(&mut self, at: (u16, u16)) -> crate::terrain::DirtyRect {
        let size = self.terrain.size() as i32;
        let centre = ((at.0 as i32 / CELL), (at.1 as i32 / CELL));
        let floor = self.terrain.get(centre.0, centre.1).max(FLOOR_MIN);
        let heights = arena_heights(centre, floor, size);
        for &((x, z), h) in &heights {
            self.terrain.set(x, z, h);
        }
        crate::terrain::DirtyRect::bounding(heights.into_iter().map(|(p, _)| p))
    }

    /// Every tribe with a reincarnation site gets its shaman back, alive and whole, at the site: whether she is
    /// dying, dead waiting to come back, or gone from the map.
    fn revive_shamans(&mut self) {
        let sites: Vec<_> = self.sites.iter().filter(|s| s.enabled).cloned().collect();
        for site in sites {
            let life = self.birth_life(UnitKind::Shaman);
            match self.units.iter_mut().find(|u| u.owner == Owner::Tribe(site.owner) && u.kind == UnitKind::Shaman && !u.statuses.ghost) {
                Some(u) if u.is_alive() => {}
                Some(u) => u.revive_at(&site, life),
                None => {
                    self.spawn(site.owner, UnitKind::Shaman, site.spawn_point());
                }
            }
        }
    }

    /// The map cleared for the match: every building turned into one brave of its owner at its door (the trace's
    /// "turns every building into one person"), then every building, totem and vehicle gone, their people set down
    /// where they were; wildmen, angels and Ghost Army copies gone; every fight and formation over, everyone standing
    /// idle; the trees on the arena and its rim around `at` gone.
    fn wipe_for_armageddon(&mut self, at: (u16, u16)) {
        let doors: Vec<(TribeId, (u16, u16))> = self.buildings.iter().filter_map(|b| Some((b.owner.tribe()?, b.door()))).collect();
        for (tribe, door) in doors {
            self.spawn(tribe, UnitKind::Brave, door);
        }
        self.units.retain(|u| (u.kind == UnitKind::Shaman || UnitKind::FOLLOWERS.contains(&u.kind)) && !u.statuses.ghost);
        for u in &mut self.units {
            let here = u.inside.map_or((u.x, u.z), |i| i.door);
            if u.is_alive() {
                u.gather_at(here);
            }
        }
        self.buildings.clear();
        self.totems.clear();
        self.vehicles = Default::default();
        self.coordinators.challenges = Default::default();
        self.coordinators.melees = Default::default();
        self.formations.clear();
        let size = self.terrain.size() as i32;
        let centre = (crate::unit::cell_of(at.0), crate::unit::cell_of(at.1));
        let near = |(x, z): (i32, i32)| {
            let wrap = |d: i32| d.rem_euclid(size).min((-d).rem_euclid(size));
            let (dx, dz) = (wrap(x - centre.0), wrap(z - centre.1));
            dx.max(dz) <= OUTER_HALF_CELLS
        };
        self.trees.retain(|t| !near(t.cell()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::effect::DamageKind;
    use crate::spell::Spell;
    use crate::tribes::Tribes;

    const AT: (u16, u16) = (30 * 512 + 256, 30 * 512 + 256);

    fn two_tribes() -> GameMap {
        GameMap::generate(1)
    }

    fn followers(map: &GameMap, tribe: TribeId) -> Vec<&Unit> {
        map.units.iter().filter(|u| u.owner == Owner::Tribe(tribe) && UnitKind::FOLLOWERS.contains(&u.kind)).collect()
    }

    fn her(map: &GameMap, tribe: TribeId) -> &Unit {
        map.units.iter().find(|u| u.owner == Owner::Tribe(tribe) && u.kind == UnitKind::Shaman).unwrap()
    }

    fn cells_off(map: &GameMap, u: &Unit) -> i32 {
        let size = map.terrain.size() as i32;
        let wrap = |d: i32| d.rem_euclid(size).min((-d).rem_euclid(size));
        let (dx, dz) = (wrap(crate::unit::cell_of(u.x) - crate::unit::cell_of(AT.0)), wrap(crate::unit::cell_of(u.z) - crate::unit::cell_of(AT.1)));
        dx.max(dz)
    }

    fn past_gathering(map: &mut GameMap) {
        for _ in 0..2000 {
            if map.armageddon_match().is_none_or(ArmageddonMatch::fighting) {
                return;
            }
            map.tick();
        }
        panic!("test setup: never got to the fight");
    }

    #[test]
    fn casting_wipes_the_map_and_gathers_the_same_headcount_into_the_arena() {
        let mut map = two_tribes();
        for _ in 0..3 {
            map.spawn(TribeId::Blue, UnitKind::Warrior, (1000, 1000));
        }
        map.spawn(TribeId::Red, UnitKind::Brave, (60_000, 60_000));
        map.spawn(TribeId::Red, UnitKind::Firewarrior, (60_000, 60_000));
        let before: Vec<UnitId> = map.units.iter().map(|u| u.id).collect();
        map.start_armageddon(AT);
        assert!(map.buildings.is_empty() && map.totems.is_empty() && map.vehicles.is_empty());
        assert!(map.trees.iter().all(|t| cells_off(&map, &Unit::new(Id::from_raw(0), TribeId::Blue, UnitKind::Brave, (t.x, t.z))) > ARENA_HALF_CELLS - 1));
        assert!(before.iter().all(|id| map.unit(*id).is_some()), "the same people, not new ones");
        let kinds = |m: &GameMap, t| { let mut k: Vec<_> = followers(m, t).iter().map(|u| u.kind).collect(); k.sort_by_key(|k| *k as u8); k };
        assert_eq!(kinds(&map, TribeId::Red), vec![UnitKind::Brave, UnitKind::Firewarrior]);
        past_gathering(&mut map);
        assert_eq!(kinds(&map, TribeId::Blue).len(), 3);
        assert!(map.units.iter().all(|u| cells_off(&map, u) <= ARENA_HALF_CELLS), "everyone in the arena, the shamans too");
    }

    #[test]
    fn every_shaman_is_brought_back_then_every_site_goes() {
        let mut map = two_tribes();
        let red = her(&map, TribeId::Red).id;
        map.damage(red, u16::MAX, DamageKind::Lightning, None);
        map.units.retain(|u| !(u.owner == Owner::Tribe(TribeId::Blue) && u.kind == UnitKind::Shaman));
        map.start_armageddon(AT);
        assert!([TribeId::Blue, TribeId::Red].iter().all(|&t| her(&map, t).is_alive()));
        map.run(Ticks::new(3));
        assert!([TribeId::Blue, TribeId::Red].iter().all(|&t| cells_off(&map, her(&map, t)) <= ARENA_HALF_CELLS), "the first to arrive");
        assert_eq!(her(&map, TribeId::Red).id, red, "the same shaman, not a new one");
        let (life, h) = (map.balance.units.life(UnitKind::Shaman), her(&map, TribeId::Red).health);
        assert!(h.is_full() && (life * 3 / 4..life).contains(&h.max()), "whole again, her life rolled anew: {h:?}");
        assert!(map.sites.iter().all(|s| !s.enabled), "no site left, their stones gone with them");
    }

    #[test]
    fn a_struck_shaman_stays_dead() {
        let mut map = two_tribes();
        map.spawn(TribeId::Blue, UnitKind::Brave, (0, 0));
        map.start_armageddon(AT);
        past_gathering(&mut map);
        map.run(crate::unit::RESPAWN_TICKS + Ticks::secs(10));
        assert!(map.units.iter().all(|u| u.owner != Owner::Tribe(TribeId::Red) || !u.is_alive()), "never reincarnates");
    }

    #[test]
    fn a_second_cast_while_one_is_active_is_refused() {
        let mut map = two_tribes();
        map.start_armageddon(AT);
        assert!(!map.can_cast(TribeId::Blue, &Spell::Armageddon { at: (2, 2) }));
    }

    #[test]
    fn every_command_is_rejected_while_a_match_is_active() {
        let mut map = two_tribes();
        map.start_armageddon(AT);
        let before = map.units.clone();
        map.apply(&Command::Order { player: TribeId::Blue, order: crate::unit::Order::Stop });
        map.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::Blast { at: AT } });
        assert_eq!(map.units, before);
    }

    #[test]
    fn nobody_fights_while_they_gather() {
        let mut map = GameMap::sandbox_spells();
        map.spawn(TribeId::Blue, UnitKind::Warrior, (0, 0));
        map.spawn(TribeId::Red, UnitKind::Warrior, (0, 0));
        map.start_armageddon(AT);
        while map.armageddon_match().is_some_and(|m| !m.fighting()) {
            assert!(map.units.iter().all(|u| matches!(u.action, Action::Idle | Action::Landing)), "{:?}", map.units.iter().map(|u| u.action.name()).collect::<Vec<_>>());
            map.tick();
        }
    }

    #[test]
    fn once_fighting_the_standing_shamans_are_shielded_and_the_fighters_charge() {
        let mut map = two_tribes();
        map.spawn(TribeId::Blue, UnitKind::Warrior, (0, 0));
        map.spawn(TribeId::Red, UnitKind::Warrior, (0, 0));
        map.start_armageddon(AT);
        past_gathering(&mut map);
        map.tick();
        assert!(map.armageddon_match().is_some_and(ArmageddonMatch::fighting));
        assert!([TribeId::Blue, TribeId::Red].iter().all(|&t| her(&map, t).statuses.shield.is_on()));
        map.run(Ticks::secs(10));
        assert!([TribeId::Blue, TribeId::Red].iter().all(|&t| followers(&map, t).iter().all(|u| !u.is_alive() || matches!(u.action, Action::Attacking { .. } | Action::Challenge { .. } | Action::Melee { .. }))), "fighting once in the middle");
    }

    #[test]
    fn nobody_goes_for_a_shaman_and_no_shaman_goes_for_anyone() {
        let mut map = two_tribes();
        for _ in 0..3 {
            map.spawn(TribeId::Blue, UnitKind::Warrior, (0, 0));
            map.spawn(TribeId::Red, UnitKind::Firewarrior, (0, 0));
        }
        map.start_armageddon(AT);
        past_gathering(&mut map);
        let shamans: Vec<UnitId> = map.units.iter().filter(|u| u.kind == UnitKind::Shaman).map(|u| u.id).collect();
        let red = her(&map, TribeId::Red).id;
        let near = (map.unit(red).unwrap().x, map.unit(red).unwrap().z + 200);
        map.spawn(TribeId::Blue, UnitKind::Warrior, near);
        for _ in 0..Ticks::secs(20).get() {
            map.tick();
            for u in map.units.iter().filter(|u| u.is_alive()) {
                if let Action::Attacking { target: crate::unit::Target::Unit(t), .. } = u.action {
                    assert!(!shamans.contains(&t), "{:?} went for a shaman", u.kind);
                }
                if u.kind == UnitKind::Shaman {
                    assert!(matches!(u.action, Action::Idle | Action::Holding { .. }), "a shaman fighting: {:?}", u.action);
                }
            }
        }
    }

    #[test]
    fn a_tribe_without_fighters_is_struck_down_and_the_other_wins() {
        let mut map = two_tribes();
        map.spawn(TribeId::Blue, UnitKind::Brave, (0, 0));
        map.start_armageddon(AT);
        past_gathering(&mut map);
        map.tick();
        assert_eq!(map.outcome.map(|o| (o.winner, o.reason)), Some((Some(TribeId::Blue), Reason::Armageddon)));
        assert!(map.effects.iter().any(|(_, e)| matches!(e.kind, crate::effect::EffectKind::Lightning(_))), "a bolt to draw");
        map.tick();
        assert!(matches!(her(&map, TribeId::Red).action, Action::Dying { .. }), "struck down: {:?}", her(&map, TribeId::Red).action);
        assert!(her(&map, TribeId::Blue).is_alive(), "the winner untouched");
        assert!(map.coordinators.armageddons.is_empty());
    }

    #[test]
    fn a_shield_from_before_does_not_save_a_fallen_tribes_shaman() {
        let mut map = two_tribes();
        map.spawn(TribeId::Blue, UnitKind::Brave, (0, 0));
        map.start_armageddon(AT);
        past_gathering(&mut map);
        let i = map.units.iter().position(|u| u.owner == Owner::Tribe(TribeId::Red) && u.kind == UnitKind::Shaman).unwrap();
        map.units[i].statuses.shield = crate::status::Timed::steps(crate::status::SHIELD_STEPS);
        map.run(Ticks::new(2));
        assert!(!her(&map, TribeId::Red).is_alive(), "her shield dropped first");
    }

    #[test]
    fn allies_win_together() {
        let mut map = two_tribes();
        map.tribes = Tribes::from_owners([TribeId::Blue, TribeId::Red, TribeId::Yellow]).with_allies([0b011, 0b011, 0b100, 0]);
        map.spawn(TribeId::Blue, UnitKind::Brave, (0, 0));
        map.spawn(TribeId::Red, UnitKind::Brave, (0, 0));
        map.start_armageddon(AT);
        past_gathering(&mut map);
        map.tick();
        assert_eq!(map.outcome.unwrap().winner, Some(TribeId::Blue), "the side of 0 and 1");
    }

    #[test]
    fn no_fighters_anywhere_is_a_draw_that_strikes_nobody() {
        let mut map = two_tribes();
        map.start_armageddon(AT);
        past_gathering(&mut map);
        map.tick();
        assert_eq!(map.outcome.unwrap().winner, None);
        assert!(map.units.iter().all(Unit::is_alive));
    }

    #[test]
    fn still_undecided_at_the_stalemate_is_a_draw() {
        let mut map = two_tribes();
        map.spawn(TribeId::Blue, UnitKind::Brave, (0, 0));
        map.spawn(TribeId::Red, UnitKind::Brave, (0, 0));
        map.start_armageddon(AT);
        past_gathering(&mut map);
        let (_, m) = map.coordinators.armageddons.iter_mut().next().unwrap();
        m.phase = Phase::Fighting(Countdown::new(Ticks::new(1)));
        map.tick();
        assert_eq!(map.outcome.map(|o| o.winner), Some(None));
    }

    #[test]
    fn a_fight_plays_out_to_a_result_and_the_same_seed_gives_the_same_one() {
        let run = || {
            let mut map = GameMap::generate(1).seeded(9);
            for _ in 0..3 {
                map.spawn(TribeId::Blue, UnitKind::Warrior, (0, 0));
            }
            map.spawn(TribeId::Red, UnitKind::Brave, (0, 0));
            map.start_armageddon(AT);
            map.run(Ticks::secs(60));
            map.outcome
        };
        let outcome = run();
        assert_eq!(outcome.map(|o| o.winner), Some(Some(TribeId::Blue)), "3 warriors beat a brave");
        assert_eq!(run(), outcome);
    }

    #[test]
    fn the_arena_is_a_square_floor_with_a_walkable_slope_each_side_and_a_cliff_rim() {
        let heights = arena_heights((0, 0), 100, 128);
        let at = |p: (i32, i32)| heights.iter().find(|(q, _)| *q == p).map(|&(_, h)| h).unwrap_or(0);
        assert_eq!((at((0, 0)), at((FLOOR_HALF, FLOOR_HALF)), at((128 - FLOOR_HALF, 0))), (100, 100, 100), "flat, wrapped too");
        for slope in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let up: Vec<u16> = (FLOOR_HALF..=ARENA_HALF_CELLS).map(|d| at(((slope.0 * d).rem_euclid(128), (slope.1 * d).rem_euclid(128)))).collect();
            assert!(up.windows(2).all(|w| w[1] > w[0] && w[1] - w[0] < crate::path::MAX_CLIMB), "{slope:?}: rising, walkable: {up:?}");
            let rim = at(((slope.0 * (ARENA_HALF_CELLS + 1)).rem_euclid(128), (slope.1 * (ARENA_HALF_CELLS + 1)).rem_euclid(128)));
            assert!(rim - up[up.len() - 1] > crate::path::MAX_CLIMB, "a cliff behind it");
        }
        assert!(heights.iter().all(|((x, z), _)| { let d = |v: i32| v.min(128 - v); d(*x).max(d(*z)) <= OUTER_HALF_CELLS }), "nothing outside touched");
    }

    #[test]
    fn every_tribe_with_people_fights_even_off_the_roster() {
        let mut map = GameMap::sandbox_spells();
        assert_eq!(map.tribes.all().collect::<Vec<_>>(), vec![TribeId::Blue], "test setup: a roster of one");
        let centre = (64 * 512 + 256, 64 * 512 + 256);
        map.start_armageddon(centre);
        for t in [TribeId::Blue, TribeId::Red, TribeId::Yellow] {
            assert!(!followers(&map, t).is_empty(), "{t:?} in it");
        }
        while map.armageddon_match().is_some_and(|m| matches!(m.phase, Phase::Arriving { .. })) {
            map.tick();
        }
        let c = arena_centre(centre);
        let off = |u: &Unit| (u.x as i32 - c.0 as i32).abs().max((u.z as i32 - c.1 as i32).abs());
        assert!(map.units.iter().all(|u| !crate::unit::is_sea(&map.terrain, u.cell()) && off(u) > FLOOR_HALF * CELL && off(u) < ARENA_HALF_CELLS * CELL), "all on the slopes");
        map.run(Ticks::secs(120));
        let o = map.outcome.expect("settled");
        assert!(o.winner.is_some(), "fought to a winner");
        let losers: usize = [TribeId::Blue, TribeId::Red, TribeId::Yellow].iter().filter(|&&t| Some(t) != o.winner).map(|&t| followers(&map, t).iter().filter(|u| u.is_alive()).count()).sum();
        assert_eq!(losers, 0, "the others fought to death");
    }

    #[test]
    fn they_arrive_one_per_turn_shamans_first_landing_on_their_spot() {
        let mut map = GameMap::sandbox_spells();
        let at = arena_centre((64 * 512, 64 * 512));
        map.start_armageddon(at);
        let inside = |m: &GameMap| m.units.iter().filter(|u| { let (dx, dz) = offset(at, (u.x, u.z)); dx.abs().max(dz.abs()) < ARENA_HALF_CELLS * CELL }).count();
        let mut count = inside(&map);
        map.tick();
        let shaman = her(&map, TribeId::Blue);
        assert!(matches!(shaman.action, Action::Landing), "she arrives first, dropping in: {:?}", shaman.action);
        let (dx, dz) = offset(at, (shaman.x, shaman.z));
        assert_eq!(dx.abs(), dz.abs(), "at a corner");
        while map.armageddon_match().is_some_and(|m| matches!(m.phase, Phase::Arriving { .. })) {
            let now = inside(&map);
            assert!(now <= count + 1, "one at a time");
            count = now;
            map.tick();
        }
        assert_eq!(inside(&map), map.units.len(), "all of them in the end");
    }

    #[test]
    fn the_order_is_shamans_then_one_of_each_side_in_turn() {
        let people = [(Some(1), vec![10, 11, 12]), (None, vec![20]), (Some(3), vec![])];
        assert_eq!(arrival_order(&people), vec![1, 3, 10, 20, 11, 12]);
    }

    #[test]
    fn every_building_becomes_one_brave_of_its_owner() {
        let mut map = GameMap::sandbox_spells();
        let huts = map.buildings.iter().filter(|b| b.owner == Owner::Tribe(TribeId::Red)).count();
        let braves = |m: &GameMap| m.units.iter().filter(|u| u.owner == Owner::Tribe(TribeId::Red) && u.kind == UnitKind::Brave && u.is_alive()).count();
        let before = braves(&map);
        map.start_armageddon((64 * 512, 64 * 512));
        assert!(huts > 0);
        assert_eq!(braves(&map), before + huts, "inside ones kept, one more per building");
    }

    #[test]
    fn the_shamans_stand_at_the_arenas_top_corners() {
        let corners: Vec<(i32, i32)> = (0..4).map(|s| shaman_offset(s, 4)).collect();
        assert!(corners.iter().all(|&(x, z)| x.abs() == z.abs() && x.abs() > FLOOR_HALF * CELL && x.abs() < ARENA_HALF_CELLS * CELL));
        assert!(corners.iter().enumerate().all(|(i, a)| corners[i + 1..].iter().all(|b| a != b)), "one each");
    }

    #[test]
    fn a_whole_match_never_leaves_anyone_stranded() {
        let mut map = GameMap::sandbox_spells();
        map.start_armageddon((64 * 512, 64 * 512));
        while map.outcome.is_none() {
            map.tick();
            assert!(map.units.iter().all(|u| !matches!(u.action, Action::Stranded { .. })), "arms up at {:?}", map.now);
        }
    }

    #[test]
    fn the_fight_begins_with_everyone_running_into_the_middle() {
        let mut map = GameMap::sandbox_spells();
        let at = arena_centre((64 * 512, 64 * 512));
        map.start_armageddon(at);
        past_gathering(&mut map);
        let dist = |u: &Unit| { let (dx, dz) = offset(at, (u.x, u.z)); dx.abs().max(dz.abs()) };
        let fighters: Vec<&Unit> = map.units.iter().filter(|u| UnitKind::FOLLOWERS.contains(&u.kind)).collect();
        assert!(fighters.iter().all(|u| matches!(u.action, Action::Walking { to } if dist(&Unit::new(u.id, u.owner, u.kind, to)) < dist(u))), "all running inwards");
    }

    #[test]
    fn anyone_thrown_past_nine_cells_is_struck_down() {
        let mut map = two_tribes();
        map.spawn(TribeId::Blue, UnitKind::Brave, (0, 0));
        map.spawn(TribeId::Red, UnitKind::Brave, (0, 0));
        let at = arena_centre(AT);
        map.start_armageddon(at);
        past_gathering(&mut map);
        let out = map.spawn(TribeId::Red, UnitKind::Warrior, (at.0 + (ARENA_HALF_CELLS * CELL + CELL / 2) as u16, at.1)).id;
        map.run(Ticks::new(2));
        assert!(!map.unit(out).is_some_and(Unit::is_alive), "past the slopes' top");
    }

    #[test]
    fn the_centre_snaps_to_the_nearest_height_point() {
        assert_eq!(arena_centre((64 * 512 + 256, 64 * 512 + 255)), (65 * 512, 64 * 512));
        assert_eq!(arena_centre((65_535, 0)), (0, 0), "wraps");
    }

    #[test]
    fn each_side_stands_on_its_own_slope_the_shaman_behind() {
        let side_of = |(x, z): (i32, i32)| if x.abs() > z.abs() { if x > 0 { 1 } else { 3 } } else if z > 0 { 0 } else { 2 };
        let spots: Vec<_> = (0..4).flat_map(|w| (0..30).map(move |k| (w, arena_offset(w, 4, k, 30)))).collect();
        assert!(spots.iter().enumerate().all(|(i, (_, a))| spots[i + 1..].iter().all(|(_, b)| a != b)), "distinct");
        assert!(spots.iter().all(|&(w, p)| side_of(p) == w), "each on its own side");
        let depth = |(x, z): (i32, i32)| x.abs().max(z.abs());
        assert!(spots.iter().all(|&(_, p)| depth(p) > FLOOR_HALF * CELL && depth(p) < ARENA_HALF_CELLS * CELL), "on the slope");
        assert!(depth(shaman_offset(0, 4)) > spots.iter().map(|&(_, p)| depth(p)).max().unwrap(), "behind them");
        assert_eq!((side_of(arena_offset(0, 2, 0, 5)), side_of(arena_offset(1, 2, 0, 5))), (0, 2), "two face each other");
    }
}
