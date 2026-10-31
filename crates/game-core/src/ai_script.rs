//! A computer tribe's own script (`cpscrNNN.dat`, docs/specs/ai-scripts.md), bridged to `GameMap`
//! through `pop3_ai_script::interp::Env`. `SET_REINCARNATION`/`SET_NO_BLUE_REINC`,
//! `FLYBY_CREATE_NEW..=FLYBY_START` (`ScriptEnv::on_flyby`), every `STATE_*` toggle, `SET_DRUM_TOWER_POS`/
//! `BUILD_DRUM_TOWER`/`DELAY_MAIN_DRUM_TOWER`/`BUILD_MAIN_DRUM_TOWER`, `TRAIN_PEOPLE_NOW`, `ATTACK`
//! (including its own `damage` withdraw threshold, `ai.rs`'s `Sortie`), `SET_SPELL_ENTRY`, the level-event
//! gifts (`GIVE_MANA_TO_PLAYER`/`GIVE_ONE_SHOT`/`GIVE_PLAYER_SPELL`/`REMOVE_PLAYER_THING`),
//! `TRIGGER_LEVEL_WON/LOST`, the timer trio and a handful of queries
//! (`I_HAVE_ONE_SHOT`/`IS_SHAMAN_AVAILABLE_FOR_ATTACK`/`COUNT_ANGELS`/`IS_BUILDING_NEAR`) have real
//! behaviour; every other `DO` command (markers/guard groups, vehicles, messages/camera/tutorial UI)
//! still reaches the bridge and is deliberately ignored (TODO.md "Computer players (AI)"). A query
//! command returns its result instead (`Env::do_call`'s own `Option<i32>`): the interpreter writes it
//! into the script's own `var_field` user variable, since `Env` owns no variable storage itself.
//! `INT_ATTR_*` attributes round-trip through `crate::ai::NativeAi` (`write_attr`/`int_var`); any other
//! internal variable this bridge doesn't model game state for (`int_var`'s fallback) reads back as its
//! own numeric token id rather than 0 — a safe, never-colliding default for the many purely
//! identifier-like ones (`INT_WARRIOR`, `INT_TEMPLE`, `INT_NO_SPECIFIC_BUILDING`...), which is what lets
//! `do_call` below recover "which model/unit kind was named" from an already-resolved `Field::Const`
//! (the interpreter resolves every `Field` before `do_call` ever sees it, so a raw `InternalVar` is
//! never available there to match by name).

use crate::ai::AttackGoal;
use crate::tribes::TribeId;
use crate::build_book::BuildAvailability;
use crate::building::BuildingKind;
use crate::map::GameMap;
use crate::mana::Mana;
use crate::outcome::Reason;
use crate::spell_book::{Availability, SpellKind};
use crate::time::Tick;
use crate::unit::UnitKind;
use pop3_ai_script::ast::{self, Call, Script};
use pop3_ai_script::field::InternalVar;
use pop3_ai_script::interp::{Env, Interpreter, ResolvedFlyby};
use pop3_ai_script::raw::RawScript;
use pop3_ai_script::token::{Arg, OneShotType, Team, TargetType};
use pop3_format::header::TRIBES;
use std::path::Path;

/// `INT_GAME_TURN`, `INT_MY_NUM_PEOPLE`: the only non-attribute internal variables `ScriptEnv` backs
/// with real state so far (field.rs's `INTERNAL_VAR_NAMES`).
const INT_GAME_TURN: InternalVar = InternalVar(0);
const INT_MY_NUM_PEOPLE: InternalVar = InternalVar(1);

#[derive(Debug, Clone)]
pub(crate) struct TribeScript {
    pub owner: TribeId,
    script: Script,
    interp: Interpreter,
}

/// A flyby playing or just finished (`GameMap::active_flyby`): `start`/`interrupted_at` are ticks, so
/// every peer agrees on exactly when it started and (if cancelled) when. The client reads this to pose
/// its own camera and show the "Flyby"/"Cancelled flyby" banner — `game-core` never touches a camera.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveFlyby {
    pub flyby: ResolvedFlyby,
    pub start: Tick,
    pub interrupted_at: Option<Tick>,
}

/// Loads and parses each computer tribe's assigned script, next to `dat` (same directory as the
/// level, like `constant.dat`). A tribe with no script (`ai_scripts[owner] == 0`), a missing file, or
/// one that doesn't parse is silently skipped — not fatal to loading the level, same forgiving spirit
/// as `GameMap::load_original`'s own `Constants::load`.
pub(crate) fn load_tribe_scripts(dat: &Path, ai_scripts: [u8; TRIBES]) -> Vec<TribeScript> {
    ai_scripts
        .iter()
        .enumerate()
        .filter_map(|(owner, &n)| {
            if n == 0 {
                return None;
            }
            let path = dat.with_file_name(format!("cpscr{n:03}.dat"));
            let data = std::fs::read(path).ok()?;
            let raw = RawScript::parse(&data).ok()?;
            let script = ast::parse(&raw).ok()?;
            Some(TribeScript { owner: TribeId::from_u8(owner as u8)?, script, interp: Interpreter::new() })
        })
        .collect()
}

/// Bridges one tribe's `Interpreter` to the map it's deciding for.
struct ScriptEnv<'a> {
    map: &'a mut GameMap,
    owner: TribeId,
}

impl Env for ScriptEnv<'_> {
    fn int_var(&self, var: InternalVar) -> i32 {
        if var == INT_GAME_TURN {
            self.map.now.to_wire() as i32
        } else if var == INT_MY_NUM_PEOPLE {
            self.map.headcount(self.owner).total() as i32
        } else if var.is_attribute() {
            self.map.ai_attr(self.owner, var)
        } else {
            // Not modelled as live game state here (TODO.md "Computer players (AI)"): the token's own
            // numeric id, not 0 — see the module doc for why that matters for `do_call` below.
            var.0 as i32
        }
    }

    fn write_attr(&mut self, var: InternalVar, value: i32) {
        self.map.ai_set_attr(self.owner, var, value);
    }

    fn random_100(&mut self) -> i32 {
        (self.map.rng.next_u32() % 100) as i32
    }

    fn do_call(&mut self, call: &Call) -> Option<i32> {
        let name = call.id.name();
        if let Some(bit) = crate::ai::state::bit_for(name) {
            if let Some(Arg::OnOff(on)) = call.args.first() {
                self.map.ai_set_state(self.owner, bit, *on);
            }
            return None;
        }
        match name {
            "SET_REINCARNATION" => {
                if let Some(Arg::OnOff(on)) = call.args.first() {
                    let event = if *on { crate::event::WorldEvent::RestoreSite { tribe: self.owner } } else { crate::event::WorldEvent::RemoveSite { tribe: self.owner } };
                    self.map.apply_events(vec![event]);
                }
                None
            }
            "SET_NO_BLUE_REINC" => {
                // The human player's equivalent of `SET_REINCARNATION OFF`: one-way, always targets
                // blue (owner 0) regardless of which computer tribe's script calls it (ai-scripts.md
                // "this AI gets a reincarnation site at the start. SET_NO_BLUE_REINC is the human
                // equivalent").
                self.map.apply_events(vec![crate::event::WorldEvent::RemoveSite { tribe: TribeId::Blue }]);
                None
            }
            "SET_DRUM_TOWER_POS" => {
                if let [Arg::Field(x), Arg::Field(z)] = call.args[..] {
                    self.map.ai_set_drum_tower_pos(self.owner, field_pos(x, z));
                }
                None
            }
            "BUILD_DRUM_TOWER" => {
                if let [Arg::Field(x), Arg::Field(z)] = call.args[..] {
                    self.map.ai_build_at(self.owner, BuildingKind::DrumTower, field_pos(x, z));
                }
                None
            }
            "DELAY_MAIN_DRUM_TOWER" => {
                self.map.ai_set_delay_main_drum_tower(self.owner, true);
                None
            }
            "BUILD_MAIN_DRUM_TOWER" => {
                self.map.ai_set_delay_main_drum_tower(self.owner, false);
                None
            }
            "TRAIN_PEOPLE_NOW" => {
                if let [Arg::Field(n), Arg::Field(model)] = call.args[..]
                    && let Some(kind) = unit_kind_from_model(model)
                {
                    self.map.ai_train_now(self.owner, kind, field_i32(n).max(0) as usize);
                }
                None
            }
            "ATTACK" => {
                if let (Some(Arg::Team(team)), Some(Arg::Field(count)), Some(Arg::TargetType(target_type)), Some(Arg::Field(model)), Some(Arg::Field(damage))) =
                    (call.args.first(), call.args.get(1), call.args.get(2), call.args.get(3), call.args.get(4))
                {
                    let goal = match target_type {
                        TargetType::Person => Some(AttackGoal::Shaman),
                        TargetType::Building => Some(AttackGoal::Building(building_kind_from_model(*model))),
                        TargetType::Marker => None, // needs the level header's Markers[256] table, not read anywhere yet
                    };
                    if let Some(goal) = goal {
                        let target_team = team_owner(*team);
                        self.map.ai_attack(self.owner, target_team, field_i32(*count), goal, field_i32(*damage));
                    }
                }
                None
            }
            "SET_SPELL_ENTRY" => {
                if let [Arg::Field(entry), Arg::Field(spell), Arg::Field(_mana), Arg::Field(frequency), Arg::Field(min_people), Arg::Field(base)] = call.args[..]
                    && let Some(kind) = spell_kind_from_model(spell)
                {
                    self.map.ai_set_spell_entry(self.owner, field_i32(entry).max(0) as usize, kind, field_i32(frequency), field_i32(min_people), field_i32(base) != 0);
                }
                None
            }
            "GIVE_MANA_TO_PLAYER" => {
                if let (Some(Arg::Team(team)), Some(Arg::Field(n))) = (call.args.first(), call.args.get(1)) {
                    let player = team_owner(*team);
                    if let Some(pool) = self.map.mana.get_mut(player as usize) {
                        pool.add(Mana(field_i32(*n).max(0) as u32));
                    }
                }
                None
            }
            "GIVE_ONE_SHOT" => {
                if let (Some(Arg::Field(spell)), Some(Arg::Team(team))) = (call.args.first(), call.args.get(1))
                    && let Some(kind) = spell_kind_from_model(*spell)
                {
                    let player = team_owner(*team);
                    if let Some(book) = self.map.spell_books.get_mut(player as usize) {
                        crate::worship::one_more_shot(book, kind);
                    }
                }
                None
            }
            "GIVE_PLAYER_SPELL" => {
                if let (Some(Arg::Team(team)), Some(Arg::Field(spell))) = (call.args.first(), call.args.get(1))
                    && let Some(kind) = spell_kind_from_model(*spell)
                {
                    let player = team_owner(*team);
                    if let Some(book) = self.map.spell_books.get_mut(player as usize) {
                        book.set(kind, Availability::Known);
                    }
                }
                None
            }
            "REMOVE_PLAYER_THING" => {
                if let (Some(Arg::Team(team)), Some(Arg::Field(model))) = (call.args.first(), call.args.get(1)) {
                    let player = team_owner(*team);
                    if let Some(kind) = spell_kind_from_model(*model) {
                        if let Some(book) = self.map.spell_books.get_mut(player as usize) {
                            book.set(kind, Availability::Hidden);
                        }
                    } else if let Some(kind) = building_kind_from_model(*model)
                        && let Some(book) = self.map.build_books.get_mut(player as usize)
                    {
                        book.set(kind, BuildAvailability::Hidden);
                    }
                }
                None
            }
            "TRIGGER_LEVEL_WON" => {
                // The level's campaign outcome, not an elimination: the human player wins
                // (ai-scripts.md "Level events"). Our own reading of "ends the level" as "declares it",
                // not traced against the exe.
                self.map.declare_outcome(Some(TribeId::Blue), Reason::Scripted);
                None
            }
            "TRIGGER_LEVEL_LOST" => {
                // The mirror case: the human loses, so this script's own tribe is left "the winner"
                // (`Outcome::winner`'s own meaning) — our own reading, same caveat as above.
                self.map.declare_outcome(Some(self.owner), Reason::Scripted);
                None
            }
            "SET_TIMER_GOING" => {
                if let Some(Arg::Field(seconds)) = call.args.first() {
                    self.map.ai_set_timer(self.owner, field_i32(*seconds));
                }
                None
            }
            "REMOVE_TIMER" => {
                self.map.ai_remove_timer(self.owner);
                None
            }
            "HAS_TIMER_REACHED_ZERO" => Some(self.map.ai_timer_reached_zero(self.owner) as i32),
            "I_HAVE_ONE_SHOT" => match call.args.first() {
                Some(Arg::OneShotType(OneShotType::Spell)) => {
                    let has = if let Some(Arg::Field(spell)) = call.args.get(1)
                        && let Some(kind) = spell_kind_from_model(*spell)
                    {
                        self.map.spell_books.get(self.owner as usize).and_then(|b| b.slot(kind)).is_some_and(|s| matches!(s.availability, Availability::Provided { shots } if shots > 0))
                    } else {
                        false
                    };
                    Some(has as i32)
                }
                // The BUILDING_TYPE form's meaning is unknown (ai-scripts.md "?"): always 0, not guessed.
                _ => Some(0),
            },
            "IS_SHAMAN_AVAILABLE_FOR_ATTACK" => {
                let available = self.map.shaman_of(self.owner).is_some_and(crate::unit::Unit::is_alive) && !self.map.shaman_locked(self.owner);
                Some(available as i32)
            }
            "COUNT_ANGELS" => {
                let team = call.args.first().and_then(|a| if let Arg::Team(t) = a { Some(team_owner(*t)) } else { None });
                let count = self.map.units.iter().filter(|u| u.kind == UnitKind::AngelOfDeath && u.is_alive() && team.is_none_or(|t| u.owner == t)).count();
                Some(count as i32)
            }
            "IS_BUILDING_NEAR" => {
                if let (Some(Arg::Field(model)), Some(Arg::Field(x)), Some(Arg::Field(z)), Some(Arg::Team(team)), Some(Arg::Field(radius))) =
                    (call.args.first(), call.args.get(1), call.args.get(2), call.args.get(3), call.args.get(4))
                {
                    let kind = building_kind_from_model(*model);
                    let team = team_owner(*team);
                    let at = field_pos(*x, *z);
                    let r = (field_i32(*radius).max(0) as i64) * pop3_format::WORLD_UNITS_PER_CELL as i64;
                    let near = self.map.buildings.iter().any(|b| b.owner == team && kind.is_none_or(|k| b.kind == k) && crate::map::torus_dist2(at, (b.x, b.z)) <= r * r);
                    Some(near as i32)
                } else {
                    Some(0)
                }
            }
            _ => None, // Every other DO command: not implemented yet, left as a no-op on purpose.
        }
    }

    fn on_flyby(&mut self, flyby: &ResolvedFlyby) {
        self.map.apply_events(vec![crate::event::WorldEvent::StartFlyby(flyby.clone())]);
    }
}

/// A `Field` argument of a `DO` call, already resolved to a plain number by the interpreter
/// (`pop3_ai_script::interp::Interpreter::resolve_call`) before `do_call` ever sees it.
fn field_i32(field: pop3_ai_script::field::Field) -> i32 {
    match field {
        pop3_ai_script::field::Field::Const(n) => n,
        // Never reached in practice (the interpreter only ever hands `do_call` a `Field::Const`), but
        // reading a stray raw field as 0 is safer than panicking on it.
        _ => 0,
    }
}

/// `x z` fields are map cells 0..255 (ai-scripts.md "Shared conventions"), converted to world units
/// (`pop3_format::WORLD_UNITS_PER_CELL` per cell) the way every other cell-based position in this crate
/// already is.
fn field_pos(x: pop3_ai_script::field::Field, z: pop3_ai_script::field::Field) -> (u16, u16) {
    let cell = pop3_format::WORLD_UNITS_PER_CELL as i32;
    ((field_i32(x) * cell) as u16, (field_i32(z) * cell) as u16)
}

/// `team` resolved to an owner number: a literal colour, or an already-resolved field (ai-scripts.md
/// "A team is a keyword or a number with blue 0, red 1, yellow 2, green 3").
fn team_owner(team: Team) -> TribeId {
    use pop3_ai_script::token::TeamColor;
    match team {
        Team::Color(TeamColor::Blue) => TribeId::Blue,
        Team::Color(TeamColor::Red) => TribeId::Red,
        Team::Color(TeamColor::Yellow) => TribeId::Yellow,
        Team::Color(TeamColor::Green) => TribeId::Green,
        Team::Field(f) => TribeId::from_u8(field_i32(f).clamp(0, 3) as u8).unwrap_or(TribeId::Blue),
    }
}

/// `INT_BRAVE`..`INT_SHAMAN` (1201..1206, field.rs's `INTERNAL_VAR_NAMES`): a `model` argument naming
/// which kind of follower, from its `int_var` fallback's own numeric id (see the module doc).
fn unit_kind_from_model(model: pop3_ai_script::field::Field) -> Option<UnitKind> {
    Some(match field_i32(model) {
        1201 => UnitKind::Brave,
        1202 => UnitKind::Warrior,
        1203 => UnitKind::Preacher, // INT_RELIGIOUS
        1204 => UnitKind::Spy,
        1205 => UnitKind::Firewarrior,
        1206 => UnitKind::Shaman,
        _ => return None,
    })
}

/// `INT_BLAST`..`INT_BLOODLUST` (field.rs's `INTERNAL_VAR_NAMES`): a `spell`/`model` argument naming
/// which spell, from its `int_var` fallback's own numeric id (see the module doc). `INT_BURN` (1184,
/// firewarriors' own fire, not a book spell) and `INT_WRATH_OF_GOD` as anything but Armageddon's own
/// internal name are not reachable here.
fn spell_kind_from_model(model: pop3_ai_script::field::Field) -> Option<SpellKind> {
    Some(match field_i32(model) {
        1185 => SpellKind::Blast,
        1186 => SpellKind::Lightning,
        1187 => SpellKind::Whirlwind, // INT_TORNADO
        1188 => SpellKind::Swarm,
        1189 => SpellKind::Invisibility,
        1190 => SpellKind::Hypnotism,
        1191 => SpellKind::Firestorm,
        1192 => SpellKind::GhostArmy,
        1193 => SpellKind::Erosion,
        1194 => SpellKind::Swamp,
        1195 => SpellKind::LandBridge,
        1196 => SpellKind::AngelOfDeath,
        1197 => SpellKind::Earthquake,
        1198 => SpellKind::Flatten,
        1199 => SpellKind::Volcano,
        1200 => SpellKind::Armageddon, // INT_WRATH_OF_GOD
        1243 => SpellKind::MagicalShield, // INT_SHIELD
        1244 => SpellKind::Convert,
        1245 => SpellKind::Teleport,
        1246 => SpellKind::Bloodlust,
        _ => return None,
    })
}

/// `INT_SMALL_HUT`..`INT_AIRSHIP_HUT_2` (1207..1221): a `model` argument naming which building kind, or
/// `None` for `INT_NO_SPECIFIC_BUILDING` (1223) and anything else not recognized — `ai_attack_target`
/// then falls back to "the nearest of any kind", matching "model is a building, or
/// `INT_NO_SPECIFIC_BUILDING` for the nearest one".
fn building_kind_from_model(model: pop3_ai_script::field::Field) -> Option<BuildingKind> {
    Some(match field_i32(model) {
        1207 => BuildingKind::Hut { size: 1 },
        1208 => BuildingKind::Hut { size: 2 },
        1209 => BuildingKind::Hut { size: 3 },
        1210 => BuildingKind::DrumTower,
        1211 => BuildingKind::Temple,
        1212 => BuildingKind::SpyTraining,
        1213 => BuildingKind::WarriorTraining,
        1214 => BuildingKind::FirewarriorTraining,
        1215 => BuildingKind::Reconversion,
        1216 => BuildingKind::WallPiece,
        1217 => BuildingKind::Gate,
        1218 | 1219 => BuildingKind::BoatHut,
        1220 | 1221 => BuildingKind::AirshipHut,
        _ => return None,
    })
}

impl GameMap {
    /// Runs every computer tribe's own script for the current turn (`self.now`). Called once at
    /// `load_original` time while `self.now` is still `Tick::ZERO` ("turn 0", a script's setup block),
    /// then once per later `tick()` (the `ELSE { EVERY ... }` branch) — the same function either way,
    /// since the interpreter itself has no special case for turn 0.
    pub(crate) fn tend_ai_scripts(&mut self) {
        let mut tribe_scripts = std::mem::take(&mut self.tribe_scripts);
        let turn = self.now.to_wire();
        for ts in tribe_scripts.iter_mut() {
            let mut env = ScriptEnv { map: self, owner: ts.owner };
            ts.interp.run_turn(turn, &ts.script, &mut env);
        }
        self.tribe_scripts = tribe_scripts;
    }

    /// Marks the current flyby cancelled (`Command::CancelFlyby`, Space): a no-op unless one is
    /// active, interruptible, and not already cancelled.
    pub(crate) fn cancel_flyby(&mut self) {
        if let Some(active) = &mut self.active_flyby
            && active.flyby.allow_interrupt
            && active.interrupted_at.is_none()
        {
            active.interrupted_at = Some(self.now);
        }
    }

    /// Clears a flyby once it's over: finished on its own (the camera is already sitting at its last
    /// keyframe, nothing to restore) or cancelled and past its grace period
    /// (`FLYBY_CANCEL_GRACE_TICKS`) — the client takes `active_flyby` disappearing as "restore the
    /// pre-flyby camera and give control back".
    pub(crate) fn tend_flyby(&mut self) {
        let Some(active) = &self.active_flyby else { return };
        let done = match active.interrupted_at {
            Some(at) => self.now.since(at) >= FLYBY_CANCEL_GRACE_TICKS.get() as i32,
            None => self.now.since(active.start) >= active.flyby.total_turns(),
        };
        if done {
            self.active_flyby = None;
        }
    }
}

/// How long "Cancelled flyby" shows before the camera restores and the player regains control
/// (`GameMap::tend_flyby`): undocumented in the original, our own guess for "a few ticks".
pub const FLYBY_CANCEL_GRACE_TICKS: crate::time::Ticks = crate::time::Ticks::secs(1);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tribes::Owner;
    use crate::site::ReincarnationSite;
    use crate::terrain::Heightmap;
    use pop3_ai_script::ast::{Block, Cond, Stmt};
    use pop3_ai_script::field::Field;
    use pop3_ai_script::token::CommandId;

    fn set_reincarnation(on: bool) -> Script {
        // IF (INT_GAME_TURN == 0) { DO SET_REINCARNATION <on_off> } — the shape every real script
        // uses for its one-off setup (ai-scripts.md "How a script runs").
        Script {
            version: 12,
            block: Block(vec![Stmt::If {
                cond: Cond::Cmp { op: pop3_ai_script::token::CmpOp::Eq, a: Field::Internal(INT_GAME_TURN), b: Field::Const(0) },
                then_: Block(vec![Stmt::Do(Call {
                    id: CommandId::from_name("SET_REINCARNATION").unwrap(),
                    args: vec![Arg::OnOff(on)],
                })]),
                else_: None,
            }]),
        }
    }

    /// A site at (10, 10), with the terrain's own height there set to match (like `sites_from_level`
    /// builds a real one): `height` isn't just a stored number, the ground really is that high.
    fn map_with_a_site(height: u16) -> GameMap {
        let mut terrain = Heightmap::new(32);
        terrain.set(10, 10, height);
        GameMap::bare("t", terrain, vec![ReincarnationSite::at_cell(TribeId::Blue, (10, 10), height)])
    }

    fn run_once(map: &mut GameMap, on: bool) {
        map.tribe_scripts = vec![TribeScript { owner: TribeId::Blue, script: set_reincarnation(on), interp: Interpreter::new() }];
        map.tend_ai_scripts();
    }

    #[test]
    fn turn_zero_off_removes_the_site() {
        let mut map = map_with_a_site(100);
        run_once(&mut map, false);
        assert!(map.site_of(TribeId::Blue).is_none());
    }

    #[test]
    fn turn_zero_on_is_a_no_op_restore_with_nothing_removed() {
        let mut map = map_with_a_site(100);
        run_once(&mut map, true);
        assert!(map.site_of(TribeId::Blue).is_some(), "ON with nothing disabled changes nothing");
    }

    #[test]
    fn off_then_on_restores_at_the_same_position_using_the_live_terrain_height() {
        let mut map = map_with_a_site(100);
        run_once(&mut map, false);
        assert!(map.site_of(TribeId::Blue).is_none());
        // The ground changed while the site was off (a spell, erosion...) - restoring should read it
        // fresh, not reuse the stale height from before it was disabled.
        map.terrain.set(10, 10, 150);
        run_once(&mut map, true);
        let restored = map.site_of(TribeId::Blue).unwrap();
        assert_eq!(restored.cell(), (10, 10), "same position");
        assert_eq!(restored.height, 150, "re-reads the live terrain, not the pre-disable height");
    }

    #[test]
    fn off_then_on_over_water_keeps_its_last_dry_height() {
        let mut map = map_with_a_site(100);
        run_once(&mut map, false);
        // The ground flooded while the site was off: restoring must not reincarnate the shaman into
        // the sea, so it keeps the height it last had instead of adopting the live (water) one.
        map.terrain.set(10, 10, crate::terrain::SEA_LEVEL);
        run_once(&mut map, true);
        assert_eq!(map.site_of(TribeId::Blue).unwrap().height, 100);
    }

    #[test]
    fn set_no_blue_reinc_removes_blues_site_regardless_of_the_scripts_own_owner() {
        // A green tribe's script (owner 2) calling SET_NO_BLUE_REINC disables blue's (tribe 0) site,
        // not its own - unlike SET_REINCARNATION, which always targets self.owner.
        let script = Script {
            version: 12,
            block: Block(vec![Stmt::If {
                cond: Cond::Cmp { op: pop3_ai_script::token::CmpOp::Eq, a: Field::Internal(INT_GAME_TURN), b: Field::Const(0) },
                then_: Block(vec![Stmt::Do(Call { id: CommandId::from_name("SET_NO_BLUE_REINC").unwrap(), args: vec![] })]),
                else_: None,
            }]),
        };
        let mut map = map_with_a_site(100);
        map.tribe_scripts = vec![TribeScript { owner: TribeId::Yellow, script, interp: Interpreter::new() }];
        map.tend_ai_scripts();
        assert!(map.site_of(TribeId::Blue).is_none());
    }

    #[test]
    fn an_unimplemented_command_reaches_do_call_and_changes_nothing() {
        let mut map = map_with_a_site(100);
        let script = Script {
            version: 12,
            block: Block(vec![Stmt::Do(Call { id: CommandId::from_name("DELAY_MAIN_DRUM_TOWER").unwrap(), args: vec![] })]),
        };
        map.tribe_scripts = vec![TribeScript { owner: TribeId::Blue, script, interp: Interpreter::new() }];
        map.tend_ai_scripts();
        assert!(map.site_of(TribeId::Blue).is_some(), "an unimplemented DO command leaves the site untouched");
    }

    #[test]
    fn a_flyby_statement_sets_active_flyby_resolved() {
        use pop3_ai_script::flyby::{Flyby, PosKeyframe};
        let flyby = Flyby {
            allow_interrupt: true,
            positions: vec![PosKeyframe { x: Field::Const(8), z: Field::Const(28), start: Field::Const(4), duration: Field::Const(80) }],
            ..Flyby::default()
        };
        let script = Script {
            version: 12,
            block: Block(vec![Stmt::If {
                cond: Cond::Cmp { op: pop3_ai_script::token::CmpOp::Eq, a: Field::Internal(INT_GAME_TURN), b: Field::Const(0) },
                then_: Block(vec![Stmt::Flyby(flyby)]),
                else_: None,
            }]),
        };
        let mut map = map_with_a_site(100);
        map.tribe_scripts = vec![TribeScript { owner: TribeId::Red, script, interp: Interpreter::new() }];
        map.tend_ai_scripts();
        let active = map.active_flyby.as_ref().unwrap();
        assert!(active.flyby.allow_interrupt);
        assert_eq!(active.flyby.positions, vec![pop3_ai_script::interp::ResolvedPos { x: 8, z: 28, start: 4, duration: 80 }]);
        assert_eq!((active.start, active.interrupted_at), (map.now, None));
    }

    #[test]
    fn tend_flyby_clears_once_finished_not_before() {
        use pop3_ai_script::interp::{ResolvedFlyby, ResolvedPos};
        let flyby = ResolvedFlyby { positions: vec![ResolvedPos { x: 0, z: 0, start: 0, duration: 10 }], ..ResolvedFlyby::default() };
        let mut map = map_with_a_site(100);
        map.active_flyby = Some(ActiveFlyby { flyby, start: Tick::from_wire(5), interrupted_at: None });
        map.now = Tick::from_wire(14); // 9 turns elapsed, total_turns is 10
        map.tend_flyby();
        assert!(map.active_flyby.is_some(), "not finished yet");
        map.now = Tick::from_wire(15); // 10 turns elapsed
        map.tend_flyby();
        assert!(map.active_flyby.is_none());
    }

    #[test]
    fn tend_flyby_clears_after_the_cancel_grace_period_not_before() {
        use pop3_ai_script::interp::ResolvedFlyby;
        let flyby = ResolvedFlyby { allow_interrupt: true, ..ResolvedFlyby::default() };
        let mut map = map_with_a_site(100);
        map.active_flyby = Some(ActiveFlyby { flyby, start: Tick::from_wire(0), interrupted_at: Some(Tick::from_wire(5)) });
        map.now = Tick::from_wire(5 + FLYBY_CANCEL_GRACE_TICKS.get() - 1);
        map.tend_flyby();
        assert!(map.active_flyby.is_some(), "grace period not over yet");
        map.now = Tick::from_wire(5 + FLYBY_CANCEL_GRACE_TICKS.get());
        map.tend_flyby();
        assert!(map.active_flyby.is_none());
    }

    #[test]
    fn cancel_flyby_is_a_no_op_without_allow_interrupt_or_nothing_active() {
        use pop3_ai_script::interp::ResolvedFlyby;
        let mut map = map_with_a_site(100);
        map.cancel_flyby(); // nothing active
        assert!(map.active_flyby.is_none());
        map.active_flyby = Some(ActiveFlyby { flyby: ResolvedFlyby { allow_interrupt: false, ..ResolvedFlyby::default() }, start: Tick::ZERO, interrupted_at: None });
        map.cancel_flyby();
        assert_eq!(map.active_flyby.as_ref().unwrap().interrupted_at, None, "not interruptible");
    }

    /// `IF (INT_GAME_TURN == 0) { DO <call> }`, the same one-off setup shape `set_reincarnation` uses,
    /// for a script that only needs to run one command once.
    fn turn_zero_do(call: Call) -> Script {
        Script {
            version: 12,
            block: Block(vec![Stmt::If {
                cond: Cond::Cmp { op: pop3_ai_script::token::CmpOp::Eq, a: Field::Internal(INT_GAME_TURN), b: Field::Const(0) },
                then_: Block(vec![Stmt::Do(call)]),
                else_: None,
            }]),
        }
    }

    /// A tribe `owner` with a native AI state alongside its script, like `GameMap::load_original` sets
    /// up for every tribe `LevelHeader::ai_scripts` assigns one to.
    fn map_with_ai_and_script(owner: TribeId, script: Script) -> GameMap {
        let mut map = map_with_a_site(100);
        map.ai.push(crate::ai::NativeAi::new(owner));
        map.tribe_scripts = vec![TribeScript { owner, script, interp: Interpreter::new() }];
        map
    }

    #[test]
    fn a_state_toggle_command_flips_the_matching_native_ai_bit() {
        let on = turn_zero_do(Call { id: CommandId::from_name("STATE_AUTO_ATTACK").unwrap(), args: vec![Arg::OnOff(true)] });
        let mut map = map_with_ai_and_script(TribeId::Blue, on);
        assert!(!map.ai_state(TribeId::Blue, crate::ai::state::AUTO_ATTACK));
        map.tend_ai_scripts();
        assert!(map.ai_state(TribeId::Blue, crate::ai::state::AUTO_ATTACK));
    }

    #[test]
    fn set_and_increment_round_trip_through_native_ai_attributes() {
        let script = Script {
            version: 12,
            block: Block(vec![
                Stmt::Assign { op: pop3_ai_script::ast::AssignOp::Set, dest: Field::Internal(InternalVar(1025)), src: Field::Const(3) },
                Stmt::Assign { op: pop3_ai_script::ast::AssignOp::Increment, dest: Field::Internal(InternalVar(1025)), src: Field::Const(4) },
            ]),
        };
        let mut map = map_with_ai_and_script(TribeId::Blue, script);
        map.tend_ai_scripts();
        assert_eq!(map.ai_attr(TribeId::Blue, InternalVar(1025)), 7, "INT_ATTR_MAX_ATTACKS set then incremented");
    }

    /// A fully flat, dry 32x32 map (unlike `map_with_a_site`, which only levels the one site cell):
    /// building placement needs its whole footprint and surroundings on dry, even ground.
    fn flat_map(owner: TribeId) -> GameMap {
        let mut terrain = Heightmap::new(32);
        for z in 0..32 {
            for x in 0..32 {
                terrain.set(x, z, 100);
            }
        }
        GameMap::bare("t", terrain, vec![ReincarnationSite::at_cell(owner, (10, 10), 100)])
    }

    #[test]
    fn set_drum_tower_pos_places_a_plan_there_and_build_drum_tower_assigns_workers() {
        let mut map = flat_map(TribeId::Blue);
        map.ai.push(crate::ai::NativeAi::new(TribeId::Blue));
        map.spawn(TribeId::Blue, UnitKind::Brave, (20 * 512, 20 * 512));
        let script = turn_zero_do(Call { id: CommandId::from_name("BUILD_DRUM_TOWER").unwrap(), args: vec![Arg::Field(Field::Const(20)), Arg::Field(Field::Const(20))] });
        map.tribe_scripts = vec![TribeScript { owner: TribeId::Blue, script, interp: Interpreter::new() }];
        map.tend_ai_scripts();
        let tower = map.buildings.iter().find(|b| b.owner == Owner::Tribe(TribeId::Blue) && b.kind == BuildingKind::DrumTower).expect("a drum tower plan");
        assert_eq!((tower.x, tower.z), (20 * 512, 20 * 512));
        assert!(map.workers(tower.id).count() > 0, "the idle brave was put to work");
    }

    #[test]
    fn train_people_now_sends_idle_braves_into_a_built_training_building() {
        let mut map = flat_map(TribeId::Blue);
        map.ai.push(crate::ai::NativeAi::new(TribeId::Blue));
        let tower = map.place_building(TribeId::Blue, BuildingKind::WarriorTraining, (20 * 512, 20 * 512), 0).unwrap();
        let b = map.building_index(tower).unwrap();
        map.buildings[b].flat = true;
        map.buildings[b].used = map.buildings[b].kind.wood_cost();
        map.spawn(TribeId::Blue, UnitKind::Brave, (20 * 512 - 512, 20 * 512));
        let script = turn_zero_do(Call {
            id: CommandId::from_name("TRAIN_PEOPLE_NOW").unwrap(),
            args: vec![Arg::Field(Field::Const(1)), Arg::Field(Field::Internal(InternalVar(1202)))], // 1 INT_WARRIOR
        });
        map.tribe_scripts = vec![TribeScript { owner: TribeId::Blue, script, interp: Interpreter::new() }];
        map.tend_ai_scripts();
        for _ in 0..400 {
            map.tick();
        }
        assert!(map.units.iter().any(|u| u.owner == Owner::Tribe(TribeId::Blue) && u.inside.is_some_and(|i| i.building == tower)), "the brave was sent to train");
    }

    #[test]
    fn attack_sends_fighters_at_the_named_teams_shaman_once_auto_attack_and_max_attacks_allow_it() {
        let mut terrain = Heightmap::new(32);
        terrain.set(10, 10, 100);
        terrain.set(20, 10, 100);
        let mut map = GameMap::bare("t", terrain, vec![ReincarnationSite::at_cell(TribeId::Blue, (10, 10), 100), ReincarnationSite::at_cell(TribeId::Red, (20, 10), 100)]);
        map.tribes = crate::tribes::Tribes::from_sites(&map.sites);
        map.ai.push(crate::ai::NativeAi::new(TribeId::Blue));
        let enemy_shaman = map.spawn(TribeId::Red, UnitKind::Shaman, (20 * 512, 10 * 512)).id;
        map.spawn(TribeId::Blue, UnitKind::Warrior, (10 * 512, 10 * 512));
        let script = turn_zero_do(Call {
            id: CommandId::from_name("ATTACK").unwrap(),
            args: vec![Arg::Team(Team::Color(pop3_ai_script::token::TeamColor::Red)), Arg::Field(Field::Const(5)), Arg::TargetType(TargetType::Person), Arg::Field(Field::Internal(InternalVar(1225))), Arg::Field(Field::Const(999))],
        });
        map.tribe_scripts = vec![TribeScript { owner: TribeId::Blue, script, interp: Interpreter::new() }];
        // STATE_AUTO_ATTACK off and INT_ATTR_MAX_ATTACKS 0 by default: the attack does nothing yet.
        map.tend_ai_scripts();
        assert!(map.units.iter().all(|u| !matches!(u.action, crate::unit::Action::Attacking { .. })));
        map.ai_set_state(TribeId::Blue, crate::ai::state::AUTO_ATTACK, true);
        map.ai_set_attr(TribeId::Blue, InternalVar(1025), 999);
        map.tend_ai_scripts();
        let attacker = map.units.iter().find(|u| u.owner == Owner::Tribe(TribeId::Blue)).unwrap();
        assert!(matches!(attacker.action, crate::unit::Action::Attacking { target: crate::unit::Target::Unit(id), .. } if id == enemy_shaman));
    }

    #[test]
    fn set_spell_entry_reaches_the_native_ai() {
        let script = turn_zero_do(Call {
            id: CommandId::from_name("SET_SPELL_ENTRY").unwrap(),
            args: vec![Arg::Field(Field::Const(0)), Arg::Field(Field::Internal(InternalVar(1185))), Arg::Field(Field::Const(0)), Arg::Field(Field::Const(8)), Arg::Field(Field::Const(2)), Arg::Field(Field::Const(1))],
        });
        let mut map = map_with_ai_and_script(TribeId::Blue, script);
        map.spawn(TribeId::Blue, UnitKind::Shaman, (10 * 512, 10 * 512));
        map.sites.push(ReincarnationSite::at_cell(TribeId::Red, (12, 10), 100));
        map.tribes = crate::tribes::Tribes::from_sites(&map.sites);
        for _ in 0..2 {
            map.spawn(TribeId::Red, UnitKind::Brave, (12 * 512, 10 * 512));
        }
        map.tend_ai_scripts();
        let before = map.spell_books[TribeId::Blue as usize].slot(SpellKind::Blast).unwrap().charges;
        for _ in 0..20 {
            map.tick();
        }
        assert!(map.spell_books[TribeId::Blue as usize].slot(SpellKind::Blast).unwrap().charges < before, "the script's slot reached the shaman's own auto-cast");
    }

    #[test]
    fn give_mana_to_player_adds_to_the_named_players_mana_pool() {
        let script = turn_zero_do(Call { id: CommandId::from_name("GIVE_MANA_TO_PLAYER").unwrap(), args: vec![Arg::Team(Team::Color(pop3_ai_script::token::TeamColor::Blue)), Arg::Field(Field::Const(5000))] });
        let mut map = map_with_ai_and_script(TribeId::Blue, script);
        map.tend_ai_scripts();
        assert_eq!(map.mana[TribeId::Blue as usize].incoming, crate::mana::Mana(5000));
    }

    #[test]
    fn give_one_shot_grants_a_free_cast_of_a_spell_not_already_known() {
        let script = turn_zero_do(Call { id: CommandId::from_name("GIVE_ONE_SHOT").unwrap(), args: vec![Arg::Field(Field::Internal(InternalVar(1186))), Arg::Team(Team::Color(pop3_ai_script::token::TeamColor::Blue))] }); // INT_LIGHTNING
        let mut map = map_with_ai_and_script(TribeId::Blue, script);
        map.tend_ai_scripts();
        assert_eq!(map.spell_books[TribeId::Blue as usize].slot(SpellKind::Lightning).unwrap().availability, Availability::Provided { shots: 1 });
    }

    #[test]
    fn give_player_spell_makes_it_known() {
        let script = turn_zero_do(Call { id: CommandId::from_name("GIVE_PLAYER_SPELL").unwrap(), args: vec![Arg::Team(Team::Color(pop3_ai_script::token::TeamColor::Blue)), Arg::Field(Field::Internal(InternalVar(1186)))] }); // INT_LIGHTNING
        let mut map = map_with_ai_and_script(TribeId::Blue, script);
        map.tend_ai_scripts();
        assert_eq!(map.spell_books[TribeId::Blue as usize].slot(SpellKind::Lightning).unwrap().availability, Availability::Known);
    }

    #[test]
    fn remove_player_thing_hides_a_known_spell() {
        let script = turn_zero_do(Call { id: CommandId::from_name("REMOVE_PLAYER_THING").unwrap(), args: vec![Arg::Team(Team::Color(pop3_ai_script::token::TeamColor::Blue)), Arg::Field(Field::Internal(InternalVar(1185)))] }); // INT_BLAST, Known by demo()
        let mut map = map_with_ai_and_script(TribeId::Blue, script);
        assert_eq!(map.spell_books[TribeId::Blue as usize].slot(SpellKind::Blast).unwrap().availability, Availability::Known);
        map.tend_ai_scripts();
        assert_eq!(map.spell_books[TribeId::Blue as usize].slot(SpellKind::Blast).unwrap().availability, Availability::Hidden);
    }

    #[test]
    fn trigger_level_won_declares_blue_the_winner() {
        let script = turn_zero_do(Call { id: CommandId::from_name("TRIGGER_LEVEL_WON").unwrap(), args: vec![] });
        let mut map = map_with_ai_and_script(TribeId::Blue, script);
        map.tend_ai_scripts();
        assert_eq!(map.outcome.map(|o| o.winner), Some(Some(TribeId::Blue)));
    }

    #[test]
    fn trigger_level_lost_declares_the_scripts_own_tribe_the_winner() {
        let script = turn_zero_do(Call { id: CommandId::from_name("TRIGGER_LEVEL_LOST").unwrap(), args: vec![] });
        let mut map = map_with_ai_and_script(TribeId::Red, script);
        map.tend_ai_scripts();
        assert_eq!(map.outcome.map(|o| o.winner), Some(Some(TribeId::Red)));
    }

    #[test]
    fn set_timer_going_and_has_timer_reached_zero_round_trip() {
        let script = turn_zero_do(Call { id: CommandId::from_name("SET_TIMER_GOING").unwrap(), args: vec![Arg::Field(Field::Const(1))] });
        let mut map = map_with_ai_and_script(TribeId::Blue, script);
        map.tend_ai_scripts();
        assert!(!map.ai_timer_reached_zero(TribeId::Blue), "not yet");
        for _ in 0..13 {
            map.tick();
        }
        assert!(map.ai_timer_reached_zero(TribeId::Blue));
    }

    #[test]
    fn i_have_one_shot_reports_a_provided_spell_through_its_var_field() {
        let query = Call { id: CommandId::from_name("I_HAVE_ONE_SHOT").unwrap(), args: vec![Arg::OneShotType(OneShotType::Spell), Arg::Field(Field::Internal(InternalVar(1199))), Arg::VarField(pop3_ai_script::field::UserVar(0))] }; // INT_VOLCANO, Provided{shots:1} in demo()
        let script = Script {
            version: 12,
            block: Block(vec![Stmt::Do(query), Stmt::Assign { op: pop3_ai_script::ast::AssignOp::Set, dest: Field::Internal(InternalVar(1025)), src: Field::UserVar(pop3_ai_script::field::UserVar(0)) }]),
        };
        let mut map = map_with_ai_and_script(TribeId::Blue, script);
        map.tend_ai_scripts();
        assert_eq!(map.ai_attr(TribeId::Blue, InternalVar(1025)), 1);
    }

    #[test]
    fn is_shaman_available_for_attack_true_when_alive_and_not_locked() {
        let query = Call { id: CommandId::from_name("IS_SHAMAN_AVAILABLE_FOR_ATTACK").unwrap(), args: vec![Arg::VarField(pop3_ai_script::field::UserVar(0))] };
        let script = Script {
            version: 12,
            block: Block(vec![Stmt::Do(query), Stmt::Assign { op: pop3_ai_script::ast::AssignOp::Set, dest: Field::Internal(InternalVar(1025)), src: Field::UserVar(pop3_ai_script::field::UserVar(0)) }]),
        };
        let mut map = map_with_ai_and_script(TribeId::Blue, script);
        map.spawn(TribeId::Blue, UnitKind::Shaman, (10 * 512, 10 * 512));
        map.tend_ai_scripts();
        assert_eq!(map.ai_attr(TribeId::Blue, InternalVar(1025)), 1);
    }

    #[test]
    fn count_angels_counts_only_the_named_tribes_own_living_angels() {
        let query = Call { id: CommandId::from_name("COUNT_ANGELS").unwrap(), args: vec![Arg::Team(Team::Color(pop3_ai_script::token::TeamColor::Red)), Arg::VarField(pop3_ai_script::field::UserVar(0))] };
        let script = Script {
            version: 12,
            block: Block(vec![Stmt::Do(query), Stmt::Assign { op: pop3_ai_script::ast::AssignOp::Set, dest: Field::Internal(InternalVar(1025)), src: Field::UserVar(pop3_ai_script::field::UserVar(0)) }]),
        };
        let mut map = map_with_ai_and_script(TribeId::Blue, script);
        map.spawn(TribeId::Red, UnitKind::AngelOfDeath, (10 * 512, 10 * 512));
        map.spawn(TribeId::Blue, UnitKind::AngelOfDeath, (10 * 512, 10 * 512));
        map.tend_ai_scripts();
        assert_eq!(map.ai_attr(TribeId::Blue, InternalVar(1025)), 1);
    }

    #[test]
    fn is_building_near_finds_a_building_of_the_named_kind_within_radius() {
        let mut map = flat_map(TribeId::Blue);
        map.ai.push(crate::ai::NativeAi::new(TribeId::Blue));
        // Away from the site at (10, 10): `SPAWN_FLAT_RADIUS` (3 cells) around it refuses any placement.
        let placed = map.place_building(TribeId::Blue, BuildingKind::DrumTower, (21 * 512, 20 * 512), 0);
        assert!(placed.is_some(), "test setup: the drum tower must actually get placed");
        let query = Call {
            id: CommandId::from_name("IS_BUILDING_NEAR").unwrap(),
            args: vec![Arg::Field(Field::Internal(InternalVar(1210))), Arg::Field(Field::Const(20)), Arg::Field(Field::Const(20)), Arg::Team(Team::Color(pop3_ai_script::token::TeamColor::Blue)), Arg::Field(Field::Const(2)), Arg::VarField(pop3_ai_script::field::UserVar(0))],
        }; // INT_DRUM_TOWER near (20, 20), radius 2 cells
        let script = Script {
            version: 12,
            block: Block(vec![Stmt::Do(query), Stmt::Assign { op: pop3_ai_script::ast::AssignOp::Set, dest: Field::Internal(InternalVar(1025)), src: Field::UserVar(pop3_ai_script::field::UserVar(0)) }]),
        };
        map.tribe_scripts = vec![TribeScript { owner: TribeId::Blue, script, interp: Interpreter::new() }];
        map.tend_ai_scripts();
        assert_eq!(map.ai_attr(TribeId::Blue, InternalVar(1025)), 1);
    }
}
