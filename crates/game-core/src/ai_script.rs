//! A computer tribe's own script (`cpscrNNN.dat`, docs/specs/ai-scripts.md), bridged to `GameMap`
//! through `pop3_ai_script::interp::Env`. `SET_REINCARNATION` (`ScriptEnv::do_call`) and
//! `FLYBY_CREATE_NEW..=FLYBY_START` (`ScriptEnv::on_flyby`) have real behaviour so far; every other
//! `DO` command reaches the bridge and is deliberately ignored, same as `pop3-ai-script`'s own "an
//! interpreter abstraction, not the AI" framing.

use crate::map::GameMap;
use crate::time::Tick;
use pop3_ai_script::ast::{self, Call, Script};
use pop3_ai_script::field::InternalVar;
use pop3_ai_script::interp::{Env, Interpreter, ResolvedFlyby};
use pop3_ai_script::raw::RawScript;
use pop3_ai_script::token::Arg;
use pop3_format::header::TRIBES;
use std::path::Path;

/// `INT_GAME_TURN`, the only internal variable `ScriptEnv` backs with real state so far.
const INT_GAME_TURN: InternalVar = InternalVar(0);

#[derive(Debug, Clone)]
pub(crate) struct TribeScript {
    pub owner: u8,
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
            Some(TribeScript { owner: owner as u8, script, interp: Interpreter::new() })
        })
        .collect()
}

/// Bridges one tribe's `Interpreter` to the map it's deciding for.
struct ScriptEnv<'a> {
    map: &'a mut GameMap,
    owner: u8,
}

impl Env for ScriptEnv<'_> {
    fn int_var(&self, var: InternalVar) -> i32 {
        if var == INT_GAME_TURN { self.map.now.to_wire() as i32 } else { 0 }
    }

    fn write_attr(&mut self, _var: InternalVar, _value: i32) {
        // No computer-player attribute storage yet (TODO.md "Computer players (AI)").
    }

    fn random_100(&mut self) -> i32 {
        (self.map.rng.next_u32() % 100) as i32
    }

    fn do_call(&mut self, call: &Call) {
        if call.id.name() == "SET_REINCARNATION"
            && let Some(Arg::OnOff(on)) = call.args.first()
        {
            let event = if *on { crate::event::WorldEvent::RestoreSite { tribe: self.owner } } else { crate::event::WorldEvent::RemoveSite { tribe: self.owner } };
            self.map.apply_events(vec![event]);
        }
        // Every other DO command: not implemented yet, left as a no-op on purpose.
    }

    fn on_flyby(&mut self, flyby: &ResolvedFlyby) {
        self.map.apply_events(vec![crate::event::WorldEvent::StartFlyby(flyby.clone())]);
    }
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
        GameMap::bare("t", terrain, vec![ReincarnationSite::at_cell(0, (10, 10), height)])
    }

    fn run_once(map: &mut GameMap, on: bool) {
        map.tribe_scripts = vec![TribeScript { owner: 0, script: set_reincarnation(on), interp: Interpreter::new() }];
        map.tend_ai_scripts();
    }

    #[test]
    fn turn_zero_off_removes_the_site() {
        let mut map = map_with_a_site(100);
        run_once(&mut map, false);
        assert!(map.site_of(0).is_none());
    }

    #[test]
    fn turn_zero_on_is_a_no_op_restore_with_nothing_removed() {
        let mut map = map_with_a_site(100);
        run_once(&mut map, true);
        assert!(map.site_of(0).is_some(), "ON with nothing disabled changes nothing");
    }

    #[test]
    fn off_then_on_restores_at_the_same_position_using_the_live_terrain_height() {
        let mut map = map_with_a_site(100);
        run_once(&mut map, false);
        assert!(map.site_of(0).is_none());
        // The ground changed while the site was off (a spell, erosion...) - restoring should read it
        // fresh, not reuse the stale height from before it was disabled.
        map.terrain.set(10, 10, 150);
        run_once(&mut map, true);
        let restored = map.site_of(0).unwrap();
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
        assert_eq!(map.site_of(0).unwrap().height, 100);
    }

    #[test]
    fn an_unimplemented_command_reaches_do_call_and_changes_nothing() {
        let mut map = map_with_a_site(100);
        let script = Script {
            version: 12,
            block: Block(vec![Stmt::Do(Call { id: CommandId::from_name("DELAY_MAIN_DRUM_TOWER").unwrap(), args: vec![] })]),
        };
        map.tribe_scripts = vec![TribeScript { owner: 0, script, interp: Interpreter::new() }];
        map.tend_ai_scripts();
        assert!(map.site_of(0).is_some(), "an unimplemented DO command leaves the site untouched");
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
        map.tribe_scripts = vec![TribeScript { owner: 1, script, interp: Interpreter::new() }];
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
}
