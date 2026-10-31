//! Computer tribes' autonomous behaviour (pop3-rev-analysis.md "Native AI: how a computer-controlled
//! tribe actually decides what to do"; TODO.md "Computer players (AI)"): the same per-tribe state a
//! script's `DO` commands tune (`ai_script.rs`) also feeds this engine, so a script and the tribe's own
//! standing behaviour are two inputs to one system, not two parallel "AI" implementations.
//!
//! Deliberately scoped down from the original's full 12-category priority list and 24 native task
//! types to the handful that matter most for a working single-player opponent: construct essential
//! buildings, train people, and post a defender — in that order, one action per sweep, exactly the
//! original's own "position 0 is always construct buildings" shape. Left out for a future session (not
//! modelled at all here): shaman positioning and auto-cast, garrison/vehicle tasks, scripted patrol
//! groups and one-off guards (`SET_MARKER_ENTRY`, `GUARD_AT_MARKER`...: these need the level header's
//! `Markers[256]` table, not read anywhere yet), the reactive counter-attack, and the original's own
//! muster-then-march sequence for an attack (ours launches attackers directly at their target instead
//! of gathering them at the drum tower first).
//!
//! Decoupling (TODO.md "ideally the AI engine is well decoupled and can be switched with another one
//! easily"): every decision below reaches the simulation only through `GameMap`'s own public
//! builder/order helpers (`place_building`, `build_orders`, `send_into`, `set_patrol`, `apply`...),
//! exactly as a human player's input would — never by reaching into unit/building fields directly. A
//! different engine only has to produce the same kind of calls, so swapping this module out touches
//! nothing else in the simulation: one field on `GameMap` (`ai`), one call site in `GameMap::tick`
//! (`tend_native_ai`), and the handful of `pub(crate)` entry points `ai_script.rs` calls into.

use crate::building::{BuildingKind, Stage};
use crate::command::Command;
use crate::headcount::State as HeadcountState;
use crate::map::{torus_dist2, GameMap};
use crate::send_in::Destination;
use crate::time::Every;
use crate::tribes::TribeId;
use crate::unit::{Order, Target, UnitId, UnitKind};
use pop3_ai_script::field::InternalVar;

/// The 48 `INT_ATTR_*` slots a script can tune (`InternalVar::ATTR_RANGE`).
const ATTR_COUNT: usize = 48;

/// Which `STATE_*` flag (editor token `1028 + i`, ai-scripts.md "States") is which, `i` indexing
/// `NativeAi::states`. Not every bit is read by this session's engine (most aren't implemented yet,
/// see the module doc); they are still all named here since the script side (`ai_script.rs`) stores
/// every one of them regardless of whether this engine acts on it yet.
pub(crate) mod state {
    pub const CONSTRUCT_BUILDING: u32 = 0;
    pub const FETCH_WOOD: u32 = 1;
    pub const SHAMAN_GET_WILDS: u32 = 2;
    pub const HOUSE_A_PERSON: u32 = 3;
    pub const SEND_GHOSTS: u32 = 4;
    pub const BRING_NEW_PEOPLE_BACK: u32 = 5;
    pub const TRAIN_PEOPLE: u32 = 6;
    pub const POPULATE_DRUM_TOWER: u32 = 7;
    pub const DEFEND: u32 = 8;
    pub const DEFEND_BASE: u32 = 9;
    pub const SPELL_DEFENCE: u32 = 10;
    pub const PREACH: u32 = 11;
    pub const BUILD_WALLS: u32 = 12;
    pub const SABOTAGE: u32 = 13;
    pub const SPELL_OFFENSIVE: u32 = 14;
    pub const FIREWARRIOR_DEFEND: u32 = 15;
    pub const BUILD_VEHICLE: u32 = 16;
    pub const FETCH_LOST_PEOPLE: u32 = 17;
    pub const FETCH_LOST_VEHICLE: u32 = 18;
    pub const FETCH_FAR_VEHICLE: u32 = 19;
    pub const AUTO_ATTACK: u32 = 20;
    pub const FLATTEN_BASE: u32 = 22;
    pub const BUILD_OUTER_DEFENCES: u32 = 23;

    /// The bit `STATE_<name>` (an `ai_script.rs` `DO` command's editor spelling) switches, if it's one
    /// of the ones named above.
    pub fn bit_for(name: &str) -> Option<u32> {
        Some(match name {
            "STATE_CONSTRUCT_BUILDING" => CONSTRUCT_BUILDING,
            "STATE_FETCH_WOOD" => FETCH_WOOD,
            "STATE_SHAMAN_GET_WILDS" => SHAMAN_GET_WILDS,
            "STATE_HOUSE_A_PERSON" => HOUSE_A_PERSON,
            "STATE_SEND_GHOSTS" => SEND_GHOSTS,
            "STATE_STATE_BRING_NEW_PEOPLE_BACK" => BRING_NEW_PEOPLE_BACK,
            "STATE_TRAIN_PEOPLE" => TRAIN_PEOPLE,
            "STATE_POPULATE_DRUM_TOWER" => POPULATE_DRUM_TOWER,
            "STATE_DEFEND" => DEFEND,
            "STATE_DEFEND_BASE" => DEFEND_BASE,
            "STATE_SPELL_DEFENCE" => SPELL_DEFENCE,
            "STATE_PREACH" => PREACH,
            "STATE_BUILD_WALLS" => BUILD_WALLS,
            "STATE_SABOTAGE" => SABOTAGE,
            "STATE_SPELL_OFFENSIVE" => SPELL_OFFENSIVE,
            "STATE_FIREWARRIOR_DEFEND" => FIREWARRIOR_DEFEND,
            "STATE_BUILD_VEHICLE" => BUILD_VEHICLE,
            "STATE_FETCH_LOST_PEOPLE" => FETCH_LOST_PEOPLE,
            "STATE_FETCH_LOST_VEHICLE" => FETCH_LOST_VEHICLE,
            "STATE_FETCH_FAR_VEHICLE" => FETCH_FAR_VEHICLE,
            "STATE_AUTO_ATTACK" => AUTO_ATTACK,
            "STATE_FLATTEN_BASE" => FLATTEN_BASE,
            "STATE_BUILD_OUTER_DEFENCES" => BUILD_OUTER_DEFENCES,
            _ => return None,
        })
    }
}

/// One computer tribe's standing state: the script-tunable bits (ai-scripts.md "States", "attributes")
/// plus what this engine itself needs to remember between sweeps. Persistent the same way a script's
/// own settings are ("States, attributes, marker entries and spell entries are persistent AI
/// settings").
#[derive(Clone, Debug)]
pub(crate) struct NativeAi {
    pub owner: TribeId,
    attrs: [i32; ATTR_COUNT],
    states: u32,
    /// `SET_DRUM_TOWER_POS`/`BUILD_DRUM_TOWER`: where this tribe's base is, read-only state from the
    /// script's own setup, not something `GameMap::ai_base` ever needs to search for once it's there.
    drum_tower_pos: Option<(u16, u16)>,
    /// `DELAY_MAIN_DRUM_TOWER`/`BUILD_MAIN_DRUM_TOWER`: while set, `ai_next_building` never picks the
    /// drum tower on its own (ai-scripts.md: "The tower is built automatically unless
    /// `DELAY_MAIN_DRUM_TOWER` was called").
    delay_main_drum_tower: bool,
}

impl NativeAi {
    pub fn new(owner: TribeId) -> Self {
        NativeAi { owner, attrs: default_attrs(), states: 0, drum_tower_pos: None, delay_main_drum_tower: false }
    }

    fn attr(&self, var: InternalVar) -> i32 {
        attr_index(var).map_or(0, |i| self.attrs[i])
    }

    fn set_attr(&mut self, var: InternalVar, value: i32) {
        if let Some(i) = attr_index(var) {
            self.attrs[i] = value;
        }
    }

    fn state(&self, bit: u32) -> bool {
        self.states & (1 << bit) != 0
    }

    fn set_state(&mut self, bit: u32, on: bool) {
        if on {
            self.states |= 1 << bit;
        } else {
            self.states &= !(1 << bit);
        }
    }
}

fn attr_index(var: InternalVar) -> Option<usize> {
    InternalVar::ATTR_RANGE.contains(&var.0).then(|| (var.0 - InternalVar::ATTR_RANGE.start) as usize)
}

/// Our own guessed starting shares (AGENTS.md: "a code-level default... written by us, not copied from
/// the real numbers"), close enough to the original's own tutorial-level values to read as a plausible
/// opponent before any script's `SET INT_ATTR_*` overrides them. Indices are `INT_ATTR_*` minus 1000
/// (`field.rs`'s `INTERNAL_VAR_NAMES`): 1007 `PREF_WARRIOR_PEOPLE`, 1008 `PREF_FIREWARRIOR_PEOPLE`, 1006
/// `PREF_RELIGIOUS_PEOPLE`, 1005 `PREF_SPY_PEOPLE` (percentages of the tribe's population),
/// 1015 `MAX_DEFENSIVE_ACTIONS` (reused here as "how many posted defenders to keep", our own guess at
/// its exact original effect), 1025 `MAX_ATTACKS` (0: `DO ATTACK` does nothing until a script raises
/// it, matching "0 ignores every attack").
fn default_attrs() -> [i32; ATTR_COUNT] {
    let mut attrs = [0; ATTR_COUNT];
    attrs[7] = 20; // PREF_WARRIOR_PEOPLE
    attrs[8] = 10; // PREF_FIREWARRIOR_PEOPLE
    attrs[6] = 10; // PREF_RELIGIOUS_PEOPLE
    attrs[5] = 5; // PREF_SPY_PEOPLE
    attrs[15] = 2; // MAX_DEFENSIVE_ACTIONS (our reuse: posted-defender quota)
    attrs
}

/// Whichever `DO ATTACK` command the script named as the target (ai-scripts.md "Attacks and spells").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttackGoal {
    /// The target tribe's shaman (`ATTACK_PERSON`: "in practice always the enemy shaman").
    Shaman,
    /// The nearest building of `0` (any kind, `INT_NO_SPECIFIC_BUILDING`) or a specific kind, owned by
    /// the target tribe.
    Building(Option<BuildingKind>),
}

/// How often a tribe's priority sweep runs: "about once every 64 turns per tribe, a little over 5
/// seconds" (pop3-rev-analysis.md "Native AI"), phased by owner so tribes sharing a cadence don't all
/// act on the same tick.
const SWEEP_EVERY: Every = Every::new(64);

impl GameMap {
    fn ai_index(&self, owner: TribeId) -> Option<usize> {
        self.ai.iter().position(|a| a.owner == owner)
    }

    pub(crate) fn ai_state(&self, owner: TribeId, bit: u32) -> bool {
        self.ai_index(owner).is_some_and(|i| self.ai[i].state(bit))
    }

    pub(crate) fn ai_set_state(&mut self, owner: TribeId, bit: u32, on: bool) {
        if let Some(i) = self.ai_index(owner) {
            self.ai[i].set_state(bit, on);
        }
    }

    pub(crate) fn ai_attr(&self, owner: TribeId, var: InternalVar) -> i32 {
        self.ai_index(owner).map_or(0, |i| self.ai[i].attr(var))
    }

    pub(crate) fn ai_set_attr(&mut self, owner: TribeId, var: InternalVar, value: i32) {
        if let Some(i) = self.ai_index(owner) {
            self.ai[i].set_attr(var, value);
        }
    }

    pub(crate) fn ai_set_drum_tower_pos(&mut self, owner: TribeId, at: (u16, u16)) {
        if let Some(i) = self.ai_index(owner) {
            self.ai[i].drum_tower_pos = Some(at);
        }
    }

    pub(crate) fn ai_set_delay_main_drum_tower(&mut self, owner: TribeId, delay: bool) {
        if let Some(i) = self.ai_index(owner) {
            self.ai[i].delay_main_drum_tower = delay;
        }
    }

    /// `BUILD_DRUM_TOWER x z` (and any other direct scripted placement, same shape): places `kind`
    /// exactly at `at`, if it can stand there, and sets idle braves to building it right away — unlike
    /// `ai_construct_building`'s own randomized site search, the script already chose the spot.
    pub(crate) fn ai_build_at(&mut self, owner: TribeId, kind: BuildingKind, at: (u16, u16)) -> bool {
        let Some(id) = self.place_building(owner, kind, at, 0) else { return false };
        let Some(site) = self.building_index(id) else { return false };
        let workers: Vec<UnitId> = self.idle_braves(owner).into_iter().take(kind.max_braves() as usize).collect();
        if !workers.is_empty() {
            for c in self.build_orders(owner, &workers, site) {
                self.apply(&c);
            }
        }
        true
    }

    /// `TRAIN_PEOPLE_NOW n model`: sends up to `n` idle braves into a built training building for
    /// `kind` right away, overriding the sweep's own `INT_ATTR_PREF_*_PEOPLE` pacing for this one call.
    pub(crate) fn ai_train_now(&mut self, owner: TribeId, kind: UnitKind, n: usize) {
        let Some(building_kind) = training_building_for(kind) else { return };
        let workers: Vec<UnitId> = self.idle_braves(owner).into_iter().take(n).collect();
        if workers.is_empty() {
            return;
        }
        for c in self.send_into(owner, &workers, Destination::Kind(building_kind)) {
            self.apply(&c);
        }
    }

    /// Where this tribe's base is, for siting new buildings and posting defenders: its scripted drum
    /// tower spot if one was set, else its reincarnation site, else any building it already owns.
    /// `None` only for a computer tribe with nothing placed at all yet (just spawned, no script setup
    /// ran) — `construct_building`/`post_defender` simply do nothing that sweep.
    fn ai_base(&self, owner: TribeId) -> Option<(u16, u16)> {
        let drum_tower = self.ai_index(owner).and_then(|i| self.ai[i].drum_tower_pos);
        drum_tower.or_else(|| self.site_of(owner).map(|s| (s.x, s.z))).or_else(|| self.buildings.iter().find(|b| b.owner == owner).map(|b| (b.x, b.z)))
    }

    /// Every computer tribe's priority sweep, one line in `GameMap::tick` next to `tend_ai_scripts`
    /// (ai-scripts.md's own "a script never bypasses the engine's own continuous economy/AI loop"): the
    /// first category below that can act, does — construction always tried first, matching "position 0
    /// is always construct essential buildings" (pop3-rev-analysis.md "Native AI").
    pub(crate) fn tend_native_ai(&mut self) {
        let owners: Vec<TribeId> = self.ai.iter().map(|a| a.owner).collect();
        for owner in owners {
            if !SWEEP_EVERY.fires(self.now, owner.as_u8() as u32) {
                continue;
            }
            if self.ai_construct_building(owner) {
                continue;
            }
            if self.ai_train_people(owner) {
                continue;
            }
            self.ai_post_defender(owner);
        }
    }

    /// Idle braves of `owner`: alive, nothing to do, not aboard a vehicle — the pool every task below
    /// draws from (pop3-rev-analysis.md "one shared 'find N eligible idle workers' search", our own
    /// simplified version of it: no "borrow from a building with some to spare" case yet).
    fn idle_braves(&self, owner: TribeId) -> Vec<UnitId> {
        self.units.iter().filter(|u| u.owner == owner && u.kind == UnitKind::Brave && u.is_alive() && u.is_free() && u.work.is_none() && u.aboard.is_none() && u.queued_patrol().is_empty()).map(|u| u.id).collect()
    }

    /// Task 0, "construct essential buildings" (pop3-rev-analysis.md): the next building this tribe's
    /// economy needs, if any, gets a plan (reusing an unfinished one of the same kind rather than
    /// placing a second) and whatever idle braves are around. Our own guessed thresholds, not the
    /// original's real `INT_ATTR_HOUSE_PERCENTAGE`-driven rule (TODO.md).
    fn ai_construct_building(&mut self, owner: TribeId) -> bool {
        if !self.ai_state(owner, state::CONSTRUCT_BUILDING) {
            return false;
        }
        let Some(kind) = self.ai_next_building(owner) else { return false };
        let site = match self.buildings.iter().position(|b| b.owner == owner && b.kind == kind && b.stage() != Stage::Built) {
            Some(site) => Some(site),
            None => self.ai_place_building(owner, kind),
        };
        let Some(site) = site else { return false };
        let workers: Vec<UnitId> = self.idle_braves(owner).into_iter().take(kind.max_braves() as usize).collect();
        if !workers.is_empty() {
            for c in self.build_orders(owner, &workers, site) {
                self.apply(&c);
            }
        }
        true
    }

    /// Our own guessed building priority: a hut once housing is nearly full, else the next essential
    /// kind this tribe doesn't have yet, gated by a population floor so a brand new tribe builds a drum
    /// tower before a temple (AGENTS.md: "our own economy" defaults, not the real numbers).
    fn ai_next_building(&self, owner: TribeId) -> Option<BuildingKind> {
        let has = |k: BuildingKind| self.buildings.iter().any(|b| b.owner == owner && b.kind == k);
        let delayed_drum_tower = self.ai_index(owner).is_some_and(|i| self.ai[i].delay_main_drum_tower);
        if !delayed_drum_tower && !has(BuildingKind::DrumTower) {
            // Always the very first building, matching "position 0... confirms building construction
            // really is the AI's standing top priority" (pop3-rev-analysis.md "Native AI").
            return Some(BuildingKind::DrumTower);
        }
        let housing = self.housing(owner);
        if housing.population + 2 >= housing.room() && housing.room() < crate::headcount::MAX_POPULATION {
            return Some(BuildingKind::Hut { size: 1 });
        }
        let wants: [(u16, BuildingKind); 3] = [(8, BuildingKind::WarriorTraining), (10, BuildingKind::Temple), (14, BuildingKind::FirewarriorTraining)];
        wants.into_iter().find(|&(min_population, kind)| housing.population >= min_population && !has(kind)).map(|(_, kind)| kind)
    }

    /// A bounded number of randomized nearby spots around this tribe's base (pop3-rev-analysis.md: "a
    /// bounded number of randomized nearby guesses, not a flood-fill or real pathfinding search"), the
    /// first that `can_place` accepts.
    fn ai_place_building(&mut self, owner: TribeId, kind: BuildingKind) -> Option<usize> {
        let base = self.ai_base(owner)?;
        let size = self.terrain.size() as i32;
        for _ in 0..24 {
            let dx = (self.rng.next_u32() % 17) as i32 - 8;
            let dz = (self.rng.next_u32() % 17) as i32 - 8;
            let cell = ((base.0 as i32 / 512 + dx).rem_euclid(size), (base.1 as i32 / 512 + dz).rem_euclid(size));
            let at = ((cell.0 * 512) as u16, (cell.1 * 512) as u16);
            if let Some(id) = self.place_building(owner, kind, at, 0) {
                return self.building_index(id);
            }
        }
        None
    }

    /// Task 26, "train people" (pop3-rev-analysis.md): tops idle braves up into whichever trained kind
    /// is furthest under its `INT_ATTR_PREF_*_PEOPLE` share of the tribe's population, through a built
    /// training building of the matching kind — the same `send_into` a human's Build-tab click uses.
    fn ai_train_people(&mut self, owner: TribeId) -> bool {
        if !self.ai_state(owner, state::TRAIN_PEOPLE) {
            return false;
        }
        let Some(kind) = self.ai_next_trainee(owner) else { return false };
        let Some(building_kind) = training_building_for(kind) else { return false };
        let workers: Vec<UnitId> = self.idle_braves(owner).into_iter().take(2).collect();
        if workers.is_empty() {
            return false;
        }
        let commands = self.send_into(owner, &workers, Destination::Kind(building_kind));
        if commands.is_empty() {
            return false;
        }
        for c in commands {
            self.apply(&c);
        }
        true
    }

    fn ai_next_trainee(&self, owner: TribeId) -> Option<UnitKind> {
        let Some(i) = self.ai_index(owner) else { return None };
        let headcount = self.headcount(owner);
        let population = headcount.total().max(1) as i32;
        let count = |k: UnitKind| HeadcountState::ALL.iter().map(|&s| headcount.get(s, k) as i32).sum::<i32>();
        let wants = [
            (InternalVar(1007), UnitKind::Warrior),       // INT_ATTR_PREF_WARRIOR_PEOPLE
            (InternalVar(1008), UnitKind::Firewarrior),   // INT_ATTR_PREF_FIREWARRIOR_PEOPLE
            (InternalVar(1006), UnitKind::Preacher),      // INT_ATTR_PREF_RELIGIOUS_PEOPLE
            (InternalVar(1005), UnitKind::Spy),           // INT_ATTR_PREF_SPY_PEOPLE
        ];
        wants.into_iter().find(|&(var, kind)| count(kind) * 100 < self.ai[i].attr(var).clamp(0, 100) * population).map(|(_, kind)| kind)
    }

    /// Task 15, "post a spare defender, gated by a defender quota" (pop3-rev-analysis.md): below quota
    /// (`INT_ATTR_MAX_DEFENSIVE_ACTIONS`, our own reuse of that attribute), an idle fighter is sent to
    /// stand guard near the base — a single-point patrol, the stationary guard post `patrol.rs` already
    /// describes.
    fn ai_post_defender(&mut self, owner: TribeId) -> bool {
        if !self.ai_state(owner, state::DEFEND_BASE) {
            return false;
        }
        let Some(i) = self.ai_index(owner) else { return false };
        let quota = self.ai[i].attr(InternalVar(1015)).max(0) as usize;
        let posted = self.units.iter().filter(|u| u.owner == owner && u.is_alive() && !u.queued_patrol().is_empty()).count();
        if posted >= quota {
            return false;
        }
        let Some(base) = self.ai_base(owner) else { return false };
        let Some(defender) = self.units.iter().find(|u| u.owner == owner && u.is_alive() && matches!(u.kind, UnitKind::Warrior | UnitKind::Firewarrior) && u.is_free() && u.work.is_none() && u.aboard.is_none() && u.queued_patrol().is_empty()).map(|u| u.id) else {
            return false;
        };
        let size = self.terrain.size() as i32;
        let dx = (self.rng.next_u32() % 5) as i32 - 2;
        let dz = (self.rng.next_u32() % 5) as i32 - 2;
        let cell = ((base.0 as i32 / 512 + dx).rem_euclid(size), (base.1 as i32 / 512 + dz).rem_euclid(size));
        let at = ((cell.0 * 512) as u16, (cell.1 * 512) as u16);
        let point = self.add_hidden_point(at.0, at.1);
        self.set_patrol(owner, &[defender], crate::patrol::PatrolAction::GoTo(point));
        true
    }

    /// `DO ATTACK` (ai-scripts.md "Attacks and spells"): the only task the native engine never enqueues
    /// on its own (pop3-rev-analysis.md: "the native AI genuinely never decides to attack on its own").
    /// Simplified from the original's own muster-then-march sequence: the chosen fighters set off for
    /// their target right away instead of gathering at the drum tower first (TODO.md).
    pub(crate) fn ai_attack(&mut self, owner: TribeId, target_team: TribeId, count: i32, goal: AttackGoal) {
        if !self.ai_state(owner, state::AUTO_ATTACK) || self.ai_attr(owner, InternalVar(1025)) <= 0 {
            return; // STATE_AUTO_ATTACK off, or INT_ATTR_MAX_ATTACKS 0 ("0 ignores every attack")
        }
        let Some(target) = self.ai_attack_target(owner, target_team, goal) else { return };
        let fighters: Vec<UnitId> = self
            .units
            .iter()
            .filter(|u| u.owner == owner && u.is_alive() && u.kind.stats().fights && u.is_free() && u.work.is_none() && u.aboard.is_none() && u.queued_patrol().is_empty())
            .take(count.max(0) as usize)
            .map(|u| u.id)
            .collect();
        for unit in fighters {
            self.apply(&Command::OrderUnit { player: owner, unit, order: Order::Attack { target } });
        }
    }

    fn ai_attack_target(&self, owner: TribeId, target_team: TribeId, goal: AttackGoal) -> Option<Target> {
        match goal {
            AttackGoal::Shaman => self.units.iter().find(|u| u.owner == target_team && u.kind == UnitKind::Shaman && u.is_alive()).map(|u| Target::Unit(u.id)),
            AttackGoal::Building(kind) => {
                let base = self.ai_base(owner).unwrap_or((0, 0));
                self.buildings
                    .iter()
                    .filter(|b| b.owner == target_team && crate::wreck::wear_limit(b.kind).is_some())
                    .filter(|b| kind.is_none_or(|k| b.kind == k))
                    .min_by_key(|b| torus_dist2(base, (b.x, b.z)))
                    .map(|b| Target::Building(b.id))
            }
        }
    }
}

fn training_building_for(kind: UnitKind) -> Option<BuildingKind> {
    match kind {
        UnitKind::Warrior => Some(BuildingKind::WarriorTraining),
        UnitKind::Firewarrior => Some(BuildingKind::FirewarriorTraining),
        UnitKind::Preacher => Some(BuildingKind::Temple),
        UnitKind::Spy => Some(BuildingKind::SpyTraining),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::Heightmap;

    fn map_with_ai(owner: TribeId) -> GameMap {
        let mut terrain = Heightmap::new(64);
        for dz in -10..=10 {
            for dx in -10..=10 {
                terrain.set(32 + dx, 32 + dz, 100);
            }
        }
        let mut map = GameMap::bare("ai-test", terrain, vec![crate::site::ReincarnationSite::at_cell(owner, (32, 32), 100)]);
        map.ai.push(NativeAi::new(owner));
        map.ai_set_state(owner, state::CONSTRUCT_BUILDING, true);
        map.ai_set_state(owner, state::TRAIN_PEOPLE, true);
        map.ai_set_state(owner, state::DEFEND_BASE, true);
        map.ai_set_state(owner, state::AUTO_ATTACK, true);
        map.ai_set_attr(owner, InternalVar(1025), 999); // INT_ATTR_MAX_ATTACKS
        map
    }

    #[test]
    fn a_state_off_by_default_keeps_the_sweep_from_acting() {
        let mut map = map_with_ai(TribeId::Blue);
        map.ai_set_state(TribeId::Blue, state::CONSTRUCT_BUILDING, false);
        let before = map.buildings.len();
        for _ in 0..200 {
            map.tick();
        }
        assert_eq!(map.buildings.len(), before, "construction never starts with the state off");
    }

    #[test]
    fn a_computer_tribe_builds_a_drum_tower_and_assigns_idle_braves() {
        let mut map = map_with_ai(TribeId::Blue);
        for _ in 0..5 {
            map.spawn(TribeId::Blue, UnitKind::Brave, (32 * 512, 32 * 512));
        }
        for _ in 0..SWEEP_EVERY.period() as usize + 5 {
            map.tick();
        }
        let tower = map.buildings.iter().find(|b| b.owner == TribeId::Blue && b.kind == BuildingKind::DrumTower);
        assert!(tower.is_some(), "a drum tower plan appears");
        assert!(map.workers(tower.unwrap().id).count() > 0, "idle braves were put to work on it");
    }

    #[test]
    fn a_computer_tribe_trains_towards_its_preferred_people_once_housed() {
        let mut map = map_with_ai(TribeId::Blue);
        map.ai_set_state(TribeId::Blue, state::CONSTRUCT_BUILDING, false); // isolate training from the construction category
        map.terrain.set(40, 32, 100);
        let tower = map.place_building(TribeId::Blue, BuildingKind::WarriorTraining, (40 * 512, 32 * 512), 0).unwrap();
        let b = map.building_index(tower).unwrap();
        map.buildings[b].flat = true;
        map.buildings[b].used = map.buildings[b].kind.wood_cost();
        for _ in 0..5 {
            map.spawn(TribeId::Blue, UnitKind::Brave, (32 * 512, 32 * 512));
        }
        for _ in 0..SWEEP_EVERY.period() as usize + 400 {
            map.tick();
        }
        assert!(map.units.iter().any(|u| u.owner == TribeId::Blue && u.inside.is_some_and(|i| i.building == tower)), "a brave went in to train");
    }

    #[test]
    fn a_computer_tribe_posts_a_defender_up_to_its_quota() {
        let mut map = map_with_ai(TribeId::Blue);
        map.ai_set_state(TribeId::Blue, state::CONSTRUCT_BUILDING, false);
        map.ai_set_state(TribeId::Blue, state::TRAIN_PEOPLE, false);
        map.ai_set_attr(TribeId::Blue, InternalVar(1015), 1); // quota 1
        map.spawn(TribeId::Blue, UnitKind::Warrior, (32 * 512, 32 * 512));
        map.spawn(TribeId::Blue, UnitKind::Warrior, (32 * 512, 32 * 512));
        for _ in 0..SWEEP_EVERY.period() as usize * 2 {
            map.tick();
        }
        let posted = map.units.iter().filter(|u| u.owner == TribeId::Blue && !u.queued_patrol().is_empty()).count();
        assert_eq!(posted, 1, "exactly the quota, not every warrior");
    }

    #[test]
    fn ai_attack_sends_fighters_at_the_target_tribes_shaman() {
        let mut map = map_with_ai(TribeId::Blue);
        let mut terrain = Heightmap::new(64);
        terrain.set(32, 32, 100);
        terrain.set(40, 32, 100);
        map.terrain = terrain;
        map.sites.push(crate::site::ReincarnationSite::at_cell(TribeId::Red, (40, 32), 100));
        map.tribes = crate::tribes::Tribes::from_sites(&map.sites);
        let enemy_shaman = map.spawn(TribeId::Red, UnitKind::Shaman, (40 * 512, 32 * 512)).id;
        map.spawn(TribeId::Blue, UnitKind::Warrior, (32 * 512, 32 * 512));
        map.ai_attack(TribeId::Blue, TribeId::Red, 1, AttackGoal::Shaman);
        let attacker = map.units.iter().find(|u| u.owner == TribeId::Blue && u.kind == UnitKind::Warrior).unwrap();
        assert!(matches!(attacker.action, crate::unit::Action::Attacking { target: Target::Unit(id), .. } if id == enemy_shaman), "{:?}", attacker.action);
    }

    #[test]
    fn ai_attack_does_nothing_without_auto_attack_or_max_attacks() {
        let mut map = map_with_ai(TribeId::Blue);
        map.sites.push(crate::site::ReincarnationSite::at_cell(TribeId::Red, (40, 32), 100));
        map.tribes = crate::tribes::Tribes::from_sites(&map.sites);
        map.spawn(TribeId::Red, UnitKind::Shaman, (40 * 512, 32 * 512));
        map.spawn(TribeId::Blue, UnitKind::Warrior, (32 * 512, 32 * 512));
        map.ai_set_state(TribeId::Blue, state::AUTO_ATTACK, false);
        map.ai_attack(TribeId::Blue, TribeId::Red, 5, AttackGoal::Shaman);
        assert!(map.units.iter().all(|u| !matches!(u.action, crate::unit::Action::Attacking { .. })), "state off: nothing happens");
        map.ai_set_state(TribeId::Blue, state::AUTO_ATTACK, true);
        map.ai_set_attr(TribeId::Blue, InternalVar(1025), 0);
        map.ai_attack(TribeId::Blue, TribeId::Red, 5, AttackGoal::Shaman);
        assert!(map.units.iter().all(|u| !matches!(u.action, crate::unit::Action::Attacking { .. })), "MAX_ATTACKS 0: nothing happens");
    }
}
