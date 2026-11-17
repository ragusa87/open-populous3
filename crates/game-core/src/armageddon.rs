//! Armageddon (docs/plans/spells.md "Armageddon", the user's own design over the trace's one line, pop3-rev-analysis.md
//! "Per spell" 18): on cast, every shaman dying, dead or gone who still has a reincarnation site comes back first
//! (`revive_shamans`), then every site is removed for good; each tribe's followers are counted by kind; every
//! building, totem, vehicle and every unit but the shamans is removed, and the trees within `ARENA_RADIUS_CELLS` of the cast point; then as many fresh
//! fighters of each kind are spawned per tribe in the arena, a wedge each (allies' wedges side by side), its shaman
//! just behind them. The match (`ArmageddonMatch`, a coordinator no unit links to) holds combat off for `GATHERING`,
//! then lets them fight, sending every idle fighter at the nearest enemy one; no shaman is auto-attacked, nor
//! auto-attacks (`spotting::first_seen`). While a tribe's own fighters live,
//! its shaman stays shielded (topped up every turn); the turn they are all dead, her shield drops and lightning
//! strikes her, with no site left to come back to. The last side (alliance) with fighters wins; sides falling together, or no winner
//! after `STALEMATE`, is a draw. The numbers are `[ours]`, none traced. Not done: the trace's "anyone past 9 cells
//! away dies"; whoever's place falls in the sea stands on the nearest free land instead (`land_near`).

use crate::coordinator::{Coordinator, Step};
use crate::effect::DamageKind;
use crate::event::WorldEvent;
use crate::id::Id;
use crate::map::{GameMap, Lcg};
use crate::outcome::Reason;
use crate::time::{Countdown, Tick, Ticks};
use crate::tribes::{Owner, TribeId};
use crate::unit::{Action, Unit, UnitId, UnitKind};
use crate::slots::{spot_cell, spot_centre, spot_of, Spot, PER_CELL};
use pop3_format::WORLD_UNITS_PER_CELL;

/// The arena's radius around the cast point: trees inside are cleared (the trace's own 9 cells).
pub const ARENA_RADIUS_CELLS: i32 = 9;
/// Fighters stand idle this long after the gathering, for the camera's reveal (`[ours]`).
pub const GATHERING: Ticks = Ticks::secs(5);
/// A match still undecided this long after the fighting began is a draw (`[ours]`).
pub const STALEMATE: Ticks = Ticks::secs(180);
/// A fighting shaman's shield, topped up to this many steps every turn: "permanent" (`[ours]`).
pub const SHIELD_STEPS: u8 = 2;
/// The wedge's first row, from the centre, and the gap between rows and fighters (world units, `[ours]`).
const FRONT: i32 = 2 * WORLD_UNITS_PER_CELL as i32;
const SPACING: i32 = WORLD_UNITS_PER_CELL as i32 / 2;
/// Fighters per row of a wedge (`[ours]`).
const ROW: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Gathering(Countdown),
    Fighting(Countdown),
}

/// Armageddon's match: its centre (the client camera's target too), its phase, and which tribes are still in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArmageddonMatch {
    pub at: (u16, u16),
    pub phase: Phase,
    in_match: [bool; crate::totem::TRIBES],
}

impl ArmageddonMatch {
    fn new(at: (u16, u16), tribes: impl Iterator<Item = TribeId>) -> Self {
        let mut in_match = [false; crate::totem::TRIBES];
        tribes.for_each(|t| in_match[t.as_usize()] = true);
        ArmageddonMatch { at, phase: Phase::Gathering(Countdown::new(GATHERING)), in_match }
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
            Phase::Gathering(left) => {
                if left.tick() {
                    self.phase = Phase::Fighting(Countdown::new(STALEMATE));
                }
                return Step::default();
            }
            Phase::Fighting(left) => left.tick(),
        };
        let still: Vec<TribeId> = world.tribes.all().filter(|t| self.in_match[t.as_usize()]).collect();
        let (standing, fallen): (Vec<TribeId>, Vec<TribeId>) = still.into_iter().partition(|&t| fighters(world, t) > 0);
        if standing.is_empty() {
            return Step { finished: true, events: vec![WorldEvent::Declare { winner: None, reason: Reason::Armageddon }] };
        }
        let mut events: Vec<WorldEvent> = fallen.iter().flat_map(|&t| struck_down(world, t)).collect();
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

/// A tribe whose fighters are all dead: its shaman's shield drops, lightning strikes her (no site to come back to).
fn struck_down(world: &GameMap, tribe: TribeId) -> Vec<WorldEvent> {
    let Some(unit) = shaman(world, tribe).map(|u| u.id) else { return Vec::new() };
    vec![WorldEvent::Shield { unit, steps: 0 }, WorldEvent::Damage { unit, amount: u16::MAX, kind: DamageKind::Lightning, from: None }]
}

/// Every idle fighter sent at the nearest enemy fighter it may attack: sight alone would leave the wedges apart.
fn charges(world: &GameMap) -> Vec<WorldEvent> {
    let fighter = |u: &Unit| UnitKind::FOLLOWERS.contains(&u.kind) && u.is_alive();
    world
        .units
        .iter()
        .filter(|u| fighter(u) && u.action == Action::Idle)
        .filter_map(|u| {
            let foe = |t: &Unit| fighter(t) && !world.tribes.same_team(u.owner, t.owner) && crate::attack::may_attack(u, t);
            crate::nearest::nearest_n(&world.units, (u.x, u.z), 1, foe).first().map(|&at| WorldEvent::StrikeBack { unit: u.id, at })
        })
        .collect()
}

/// Where the `k`-th fighter of wedge `wedge` (of `wedges`) stands, off the arena's centre in world units: rows of
/// `ROW` across three quarters of the wedge's arc, the first `FRONT` out, each row `SPACING` behind the last.
pub fn arena_offset(wedge: usize, wedges: usize, k: usize) -> (i32, i32) {
    let (row, col) = ((k / ROW) as i32, (k % ROW) as i64);
    let width = crate::angle::TURN as i64 / wedges.max(1) as i64;
    let spread = width * 3 / 4;
    let angle = (wedge as i64 * width - spread / 2 + spread * (2 * col + 1) / (2 * ROW as i64)).rem_euclid(crate::angle::TURN as i64) as u32;
    polar(angle, FRONT + row * SPACING)
}

/// Where wedge `wedge`'s shaman stands: on its middle, a row behind its `count` fighters.
pub fn shaman_offset(wedge: usize, wedges: usize, count: usize) -> (i32, i32) {
    let width = crate::angle::TURN as usize / wedges.max(1);
    polar((wedge * width) as u32, FRONT + (count.div_ceil(ROW) as i32 + 1) * SPACING)
}

/// The nearest spot to `at` on land and not in `taken`, ring by ring out to twice the arena (`at` itself if none):
/// a place in the arena that falls in the sea.
pub fn land_near(terrain: &crate::terrain::Heightmap, at: (u16, u16), taken: &std::collections::BTreeSet<Spot>) -> Spot {
    let size = terrain.size() as i32 * PER_CELL;
    let start = spot_of(at);
    let wrap = |(x, z): Spot| (x.rem_euclid(size), z.rem_euclid(size));
    let fits = |s: Spot| !taken.contains(&s) && !crate::unit::is_sea(terrain, spot_cell(s));
    (0..=2 * ARENA_RADIUS_CELLS * PER_CELL)
        .flat_map(|r| (-r..=r).flat_map(move |dz| (-r..=r).filter(move |dx| dx.abs() == r || dz.abs() == r).map(move |dx| (start.0 + dx, start.1 + dz))))
        .map(wrap)
        .find(|&s| fits(s))
        .unwrap_or(start)
}

fn polar(angle: u32, r: i32) -> (i32, i32) {
    (r * crate::angle::sin(angle) / crate::angle::ONE, r * crate::angle::cos(angle) / crate::angle::ONE)
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

    /// Armageddon takes effect at `at` (module doc): the count, the wipe, the gathering into the arena, the match.
    pub(crate) fn start_armageddon(&mut self, at: (u16, u16)) {
        self.revive_shamans();
        let tribes: Vec<TribeId> = self.sites.iter().map(|s| s.owner).collect();
        tribes.into_iter().for_each(|t| self.remove_reincarnation(t));
        let order: Vec<TribeId> = self.tribes.teams().into_iter().flatten().collect();
        let tally: Vec<(TribeId, Vec<UnitKind>)> = order
            .iter()
            .map(|&t| (t, self.units.iter().filter(|u| u.owner == Owner::Tribe(t) && UnitKind::FOLLOWERS.contains(&u.kind) && !u.statuses.ghost && u.is_alive()).map(|u| u.kind).collect()))
            .collect();
        self.wipe_for_armageddon(at);
        let size = (self.terrain.size() * WORLD_UNITS_PER_CELL as usize) as i32;
        let mut taken = std::collections::BTreeSet::new();
        let mut place = |terrain: &crate::terrain::Heightmap, (dx, dz): (i32, i32)| {
            let spot = land_near(terrain, (((at.0 as i32 + dx).rem_euclid(size)) as u16, ((at.1 as i32 + dz).rem_euclid(size)) as u16), &taken);
            taken.insert(spot);
            spot_centre(spot)
        };
        for (wedge, (tribe, kinds)) in tally.iter().enumerate() {
            for (k, &kind) in kinds.iter().enumerate() {
                let spot = place(&self.terrain, arena_offset(wedge, tally.len(), k));
                self.spawn(*tribe, kind, spot);
            }
            let spot = place(&self.terrain, shaman_offset(wedge, tally.len(), kinds.len()));
            if let Some(u) = self.units.iter_mut().find(|u| u.owner == Owner::Tribe(*tribe) && u.kind == UnitKind::Shaman && !u.statuses.ghost && u.is_alive()) {
                u.gather_at(spot);
            }
        }
        let id = self.new_id_of();
        self.coordinators.armageddons.insert(id, ArmageddonMatch::new(at, order.into_iter()));
    }

    /// Every tribe with a reincarnation site gets its shaman back, alive and whole, at the site: whether she is
    /// dying, dead waiting to come back, or gone from the map.
    fn revive_shamans(&mut self) {
        let sites: Vec<_> = self.sites.iter().filter(|s| s.enabled).cloned().collect();
        for site in sites {
            let life = crate::health::Health::full(self.balance.units.life(UnitKind::Shaman));
            match self.units.iter_mut().find(|u| u.owner == Owner::Tribe(site.owner) && u.kind == UnitKind::Shaman && !u.statuses.ghost) {
                Some(u) if u.is_alive() => {}
                Some(u) => u.revive_at(&site, life),
                None => {
                    self.spawn(site.owner, UnitKind::Shaman, site.spawn_point());
                }
            }
        }
    }

    /// Everything but the shamans and the ground: buildings, totems, vehicles, every other unit, any fight or
    /// formation in progress, and the trees within `ARENA_RADIUS_CELLS` of `at`.
    fn wipe_for_armageddon(&mut self, at: (u16, u16)) {
        self.units.retain(|u| u.kind == UnitKind::Shaman && !u.statuses.ghost);
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
            dx * dx + dz * dz <= ARENA_RADIUS_CELLS * ARENA_RADIUS_CELLS
        };
        self.trees.retain(|t| !near(t.cell()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
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
        map.run(GATHERING);
        assert!(map.armageddon_match().is_none_or(ArmageddonMatch::fighting), "test setup: gathering over");
    }

    #[test]
    fn casting_wipes_the_map_and_gathers_the_same_headcount_into_the_arena() {
        let mut map = two_tribes();
        for _ in 0..3 {
            map.spawn(TribeId::Blue, UnitKind::Warrior, (1000, 1000));
        }
        map.spawn(TribeId::Red, UnitKind::Brave, (60_000, 60_000));
        map.spawn(TribeId::Red, UnitKind::Firewarrior, (60_000, 60_000));
        map.start_armageddon(AT);
        assert!(map.buildings.is_empty() && map.totems.is_empty() && map.vehicles.is_empty());
        assert!(map.trees.iter().all(|t| cells_off(&map, &Unit::new(Id::from_raw(0), TribeId::Blue, UnitKind::Brave, (t.x, t.z))) > ARENA_RADIUS_CELLS - 1));
        let kinds = |t| { let mut k: Vec<_> = followers(&map, t).iter().map(|u| u.kind).collect(); k.sort_by_key(|k| *k as u8); k };
        assert_eq!(kinds(TribeId::Blue).len(), 3);
        assert_eq!(kinds(TribeId::Red), vec![UnitKind::Brave, UnitKind::Firewarrior]);
        assert!(map.units.iter().all(|u| cells_off(&map, u) <= ARENA_RADIUS_CELLS), "everyone in the arena, the shamans too");
        assert!(map.armageddon_match().is_some_and(|m| !m.fighting()));
    }

    #[test]
    fn every_shaman_is_brought_back_then_every_site_goes() {
        let mut map = two_tribes();
        let red = her(&map, TribeId::Red).id;
        map.damage(red, u16::MAX, DamageKind::Lightning, None);
        map.units.retain(|u| !(u.owner == Owner::Tribe(TribeId::Blue) && u.kind == UnitKind::Shaman));
        map.start_armageddon(AT);
        assert!([TribeId::Blue, TribeId::Red].iter().all(|&t| her(&map, t).is_alive() && cells_off(&map, her(&map, t)) <= ARENA_RADIUS_CELLS));
        assert_eq!(her(&map, TribeId::Red).id, red, "the same shaman, not a new one");
        assert_eq!(her(&map, TribeId::Red).health.current(), map.balance.units.life(UnitKind::Shaman));
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
        let mut map = two_tribes();
        map.spawn(TribeId::Blue, UnitKind::Warrior, (0, 0));
        map.spawn(TribeId::Red, UnitKind::Warrior, (0, 0));
        map.start_armageddon(AT);
        map.run(Ticks::new(GATHERING.get() - 1));
        assert!(map.units.iter().all(|u| u.action == Action::Idle));
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
        assert!([TribeId::Blue, TribeId::Red].iter().all(|&t| followers(&map, t).iter().all(|u| matches!(u.action, Action::Attacking { .. } | Action::Challenge { .. } | Action::Melee { .. }))));
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
        map.tick();
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
    fn a_place_in_the_sea_moves_to_the_nearest_free_land() {
        let mut terrain = crate::terrain::Heightmap::new(128);
        for x in 0..128 {
            for z in 12..128 {
                terrain.set(x, z, 100);
            }
        }
        let sea = (10 * 512 + 256, 5 * 512 + 256);
        let first = land_near(&terrain, sea, &Default::default());
        assert!(!crate::unit::is_sea(&terrain, spot_cell(first)) && spot_cell(first).1 >= 11, "{first:?}");
        let second = land_near(&terrain, sea, &[first].into());
        assert_ne!(second, first);
        let ashore = (10 * 512 + 256, 40 * 512 + 256);
        assert_eq!(land_near(&terrain, ashore, &Default::default()), spot_of(ashore), "already on land: stays");
    }

    #[test]
    fn wedges_hold_distinct_spots_and_the_shaman_stands_behind() {
        let spots: Vec<_> = (0..2).flat_map(|w| (0..20).map(move |k| arena_offset(w, 2, k))).collect();
        assert!(spots.iter().enumerate().all(|(i, a)| spots[i + 1..].iter().all(|b| a != b)));
        let r = |(x, z): (i32, i32)| ((x as i64 * x as i64 + z as i64 * z as i64) as f64).sqrt() as i32;
        assert!(r(shaman_offset(0, 2, 20)) > r(arena_offset(0, 2, 19)));
        assert!(arena_offset(0, 2, 0).1 > 0 && arena_offset(1, 2, 0).1 < 0, "facing each other across the centre");
    }
}
