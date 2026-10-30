//! The interpreter abstraction (docs/specs/ai-scripts.md "How a script runs", "What this means
//! here"): a tree-walking runner for the control-flow grammar only. No `DO` command is given real
//! behaviour here — each one is handed to `Env::do_call` with its `Field` arguments already resolved
//! to plain numbers (command identity and argument shape otherwise untouched), so this crate stays "a
//! parser and an interpreter abstraction", not the AI itself.

use crate::ast::{AssignOp, BinOp, Call, Cond, Script, Stmt};
use crate::field::{Field, InternalVar, UserVar};
use crate::flyby::Flyby;
use crate::raw::USER_VAR_COUNT;
use crate::token::{Arg, CmpOp, Team, TeamWild};

/// The seam a future `game-core` consumer implements: everything the interpreter itself cannot
/// know (live game state) or must not decide (what a `DO` command means).
pub trait Env {
    /// Reads an internal variable or `INT_ATTR_*` attribute (field type 2).
    fn int_var(&self, var: InternalVar) -> i32;
    /// Writes an `INT_ATTR_*` attribute (`SET`/`INCREMENT`/`DECREMENT`/`MULTIPLY`/`DIVIDE` on one).
    fn write_attr(&mut self, var: InternalVar, value: i32);
    /// Writes a query command's result into a user variable (its last parameter, a `var_field`):
    /// `COUNT_PEOPLE_IN_MARKER`, `GET_HEIGHT_AT_POS`, `NAV_CHECK`... Default: discarded, same spirit
    /// as `do_call`'s own "undecided" default.
    fn write_user_var(&mut self, _var: UserVar, _value: i32) {}
    /// `INT_RANDOM_100`: a random 0..99 (spec: "Here it must come from `map::Lcg`").
    fn random_100(&mut self) -> i32;
    /// A `DO` call, with every `Field`-bearing argument already resolved to a plain
    /// `Field::Const` (a `$variable` or `INT_ATTR_*` reference read through `int_var`/the
    /// interpreter's own user-variable storage, since `Env` itself owns neither) — command identity
    /// and argument shape are otherwise exactly as parsed, undecided by this crate.
    fn do_call(&mut self, call: &Call);
    /// A recognized `FLYBY_CREATE_NEW..=FLYBY_START` run, with every keyframe field already resolved
    /// to a plain number (a `Flyby`'s fields can be user variables; only the interpreter can read
    /// those, so `Env` never sees a raw `Field`). Default: ignored.
    fn on_flyby(&mut self, _flyby: &ResolvedFlyby) {}
}

/// A `PosKeyframe` with every `Field` resolved to a plain number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPos {
    pub x: i32,
    pub z: i32,
    pub start: i32,
    pub duration: i32,
}

/// An `AngleKeyframe` with every `Field` resolved to a plain number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAngle {
    pub angle: i32,
    pub start: i32,
    pub duration: i32,
}

/// A `ZoomKeyframe` with every `Field` resolved to a plain number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedZoom {
    pub zoom: i32,
    pub start: i32,
    pub duration: i32,
}

/// A `TooltipKeyframe` with every `Field` resolved to a plain number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTooltip {
    pub x: i32,
    pub z: i32,
    pub text: i32,
    pub start: i32,
    pub duration: i32,
}

/// An `EndTarget` with every `Field` resolved to a plain number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEndTarget {
    pub x: i32,
    pub z: i32,
    pub angle: i32,
    pub unknown: i32,
}

/// A `Flyby` with every keyframe's `Field`s resolved to plain numbers, built by `Interpreter` right
/// before calling `Env::on_flyby` (see `exec_stmt`'s `Stmt::Flyby` arm).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedFlyby {
    pub allow_interrupt: bool,
    pub positions: Vec<ResolvedPos>,
    pub angles: Vec<ResolvedAngle>,
    pub zooms: Vec<ResolvedZoom>,
    pub tooltips: Vec<ResolvedTooltip>,
    pub end_target: Option<ResolvedEndTarget>,
}

impl ResolvedFlyby {
    /// Turns from the flyby's own start until the scripted path is over: the latest `start +
    /// duration` across every channel (0 if it has none at all).
    pub fn total_turns(&self) -> i32 {
        self.positions
            .iter()
            .map(|k| k.start + k.duration)
            .chain(self.angles.iter().map(|k| k.start + k.duration))
            .chain(self.zooms.iter().map(|k| k.start + k.duration))
            .chain(self.tooltips.iter().map(|k| k.start + k.duration))
            .max()
            .unwrap_or(0)
    }
}

/// Runs a `Script`'s control flow, one call per game turn. Owns only what the format itself says
/// is per-script persistent storage (the "user variable storage" of `src/raw.rs`, which "keep[s]
/// their values between turns").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interpreter {
    pub user_vars: [i32; USER_VAR_COUNT],
}

impl Default for Interpreter {
    fn default() -> Self {
        Interpreter { user_vars: [0; USER_VAR_COUNT] }
    }
}

impl Interpreter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluates the whole script once, the way the original does every game turn (spec: "Every
    /// game turn, the whole script is evaluated once per computer player"). `turn` is used for
    /// `EVERY`'s gating; a condition that reads `INT_GAME_TURN` gets its answer from `env` instead,
    /// so the two must agree (the caller's responsibility, same as the original engine's own turn
    /// counter and its `INT_GAME_TURN` variable being the same number).
    pub fn run_turn(&mut self, turn: u32, script: &Script, env: &mut impl Env) {
        self.exec_block(&script.block, turn, env);
    }

    fn exec_block(&mut self, block: &crate::ast::Block, turn: u32, env: &mut impl Env) {
        for stmt in &block.0 {
            self.exec_stmt(stmt, turn, env);
        }
    }

    fn exec_stmt(&mut self, stmt: &Stmt, turn: u32, env: &mut impl Env) {
        match stmt {
            Stmt::Do(call) => {
                let resolved = self.resolve_call(env, call);
                env.do_call(&resolved);
            }
            Stmt::Flyby(flyby) => {
                let resolved = self.resolve_flyby(env, flyby);
                env.on_flyby(&resolved);
            }
            Stmt::Every { period, offset, block } => {
                if every_fires(turn, *period, *offset) {
                    self.exec_block(block, turn, env);
                }
            }
            Stmt::Assign { op, dest, src } => {
                let src_value = self.resolve(env, *src);
                let value = match op {
                    AssignOp::Set => src_value,
                    AssignOp::Increment => self.resolve(env, *dest) + src_value,
                    AssignOp::Decrement => self.resolve(env, *dest) - src_value,
                };
                self.write(env, *dest, value);
            }
            Stmt::AssignBinary { op, dest, a, b } => {
                let a = self.resolve(env, *a);
                let b = self.resolve(env, *b);
                // Rounding and division by zero are undocumented (ai-scripts.md "Assignments");
                // 0 is a guess, not a traced rule.
                let value = match op {
                    BinOp::Multiply => a * b,
                    BinOp::Divide => a.checked_div(b).unwrap_or(0),
                };
                self.write(env, *dest, value);
            }
            Stmt::If { cond, then_, else_ } => {
                if self.eval_cond(cond, env) {
                    self.exec_block(then_, turn, env);
                } else if let Some(else_) = else_ {
                    self.exec_block(else_, turn, env);
                }
            }
        }
    }

    fn eval_cond(&self, cond: &Cond, env: &impl Env) -> bool {
        match cond {
            Cond::Cmp { op, a, b } => {
                let a = self.resolve(env, *a);
                let b = self.resolve(env, *b);
                match op {
                    CmpOp::Gt => a > b,
                    CmpOp::Lt => a < b,
                    CmpOp::Eq => a == b,
                    CmpOp::Ne => a != b,
                    CmpOp::Ge => a >= b,
                    CmpOp::Le => a <= b,
                }
            }
            Cond::And(l, r) => self.eval_cond(l, env) && self.eval_cond(r, env),
            Cond::Or(l, r) => self.eval_cond(l, env) || self.eval_cond(r, env),
        }
    }

    fn resolve(&self, env: &impl Env, field: Field) -> i32 {
        match field {
            Field::Const(v) => v,
            Field::UserVar(UserVar(i)) => self.user_vars[i as usize],
            Field::Internal(var) => env.int_var(var),
        }
    }

    fn write(&mut self, env: &mut impl Env, dest: Field, value: i32) {
        match dest {
            Field::UserVar(UserVar(i)) => self.user_vars[i as usize] = value,
            Field::Internal(var) => env.write_attr(var, value),
            Field::Const(_) => unreachable!("the parser only ever produces a user var or INT_ATTR_* destination"),
        }
    }

    /// `call` with every `Field` it carries (including one nested in a `Team`/`TeamWild` argument)
    /// resolved to a plain `Field::Const`, the same way `resolve_flyby` does for flyby keyframes.
    /// `Arg::VarField` is left alone: it names where a query command's result goes, not a value to
    /// read.
    fn resolve_call(&self, env: &impl Env, call: &Call) -> Call {
        Call { id: call.id, args: call.args.iter().map(|&a| self.resolve_arg(env, a)).collect() }
    }

    fn resolve_arg(&self, env: &impl Env, arg: Arg) -> Arg {
        match arg {
            Arg::Field(f) => Arg::Field(Field::Const(self.resolve(env, f))),
            Arg::Team(Team::Field(f)) => Arg::Team(Team::Field(Field::Const(self.resolve(env, f)))),
            Arg::TeamWild(TeamWild::Team(Team::Field(f))) => Arg::TeamWild(TeamWild::Team(Team::Field(Field::Const(self.resolve(env, f))))),
            other => other,
        }
    }

    fn resolve_flyby(&self, env: &impl Env, flyby: &Flyby) -> ResolvedFlyby {
        ResolvedFlyby {
            allow_interrupt: flyby.allow_interrupt,
            positions: flyby
                .positions
                .iter()
                .map(|k| ResolvedPos { x: self.resolve(env, k.x), z: self.resolve(env, k.z), start: self.resolve(env, k.start), duration: self.resolve(env, k.duration) })
                .collect(),
            angles: flyby
                .angles
                .iter()
                .map(|k| ResolvedAngle { angle: self.resolve(env, k.angle), start: self.resolve(env, k.start), duration: self.resolve(env, k.duration) })
                .collect(),
            zooms: flyby
                .zooms
                .iter()
                .map(|k| ResolvedZoom { zoom: self.resolve(env, k.zoom), start: self.resolve(env, k.start), duration: self.resolve(env, k.duration) })
                .collect(),
            tooltips: flyby
                .tooltips
                .iter()
                .map(|k| ResolvedTooltip {
                    x: self.resolve(env, k.x),
                    z: self.resolve(env, k.z),
                    text: self.resolve(env, k.text),
                    start: self.resolve(env, k.start),
                    duration: self.resolve(env, k.duration),
                })
                .collect(),
            end_target: flyby.end_target.as_ref().map(|e| ResolvedEndTarget { x: self.resolve(env, e.x), z: self.resolve(env, e.z), angle: self.resolve(env, e.angle), unknown: self.resolve(env, e.unknown) }),
        }
    }
}

/// `EVERY period offset` fires on turns where `(turn + offset) % period == 0` (spec: "`EVERY n m`
/// fires on the turns where `(turn + m) % n == 0`"; `period`/`offset` here are already the real,
/// `+1`-corrected values the file's `period - 1` / `offset - 1` storage decodes to). The spec flags
/// the exact off-by-one and any per-tribe phase as unverified — this is its own best-known guess,
/// not a new one invented here.
fn every_fires(turn: u32, period: i32, offset: Option<i32>) -> bool {
    let period = period as u32;
    let offset = offset.unwrap_or(0) as u32;
    (turn + offset).is_multiple_of(period)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Block;
    use crate::token::CommandId;

    #[derive(Default)]
    struct MockEnv {
        game_turn: i32,
        calls: Vec<CommandId>,
        last_call_args: Vec<Arg>,
        flybys: usize,
        last_flyby: Option<ResolvedFlyby>,
    }

    impl Env for MockEnv {
        fn int_var(&self, var: InternalVar) -> i32 {
            if var == InternalVar(0) { self.game_turn } else { 0 }
        }
        fn write_attr(&mut self, _var: InternalVar, _value: i32) {}
        fn random_100(&mut self) -> i32 {
            0
        }
        fn do_call(&mut self, call: &Call) {
            self.calls.push(call.id);
            self.last_call_args = call.args.clone();
        }
        fn on_flyby(&mut self, flyby: &ResolvedFlyby) {
            self.flybys += 1;
            self.last_flyby = Some(flyby.clone());
        }
    }

    fn do_named(name: &str) -> Stmt {
        Stmt::Do(Call { id: CommandId::from_name(name).unwrap(), args: vec![] })
    }

    /// The shape every real script has (ai-scripts.md "How a script runs"): `IF (INT_GAME_TURN ==
    /// 0) { setup } ELSE { EVERY 256 12 { ... } } ENDIF`, using the wiki's own worked example.
    fn turn_zero_script() -> Script {
        Script {
            version: 12,
            block: Block(vec![Stmt::If {
                cond: Cond::Cmp { op: CmpOp::Eq, a: Field::Internal(InternalVar(0)), b: Field::Const(0) },
                then_: Block(vec![do_named("SET_REINCARNATION")]),
                else_: Some(Block(vec![Stmt::Every {
                    period: 256,
                    offset: Some(12),
                    block: Block(vec![do_named("DELAY_MAIN_DRUM_TOWER")]),
                }])),
            }]),
        }
    }

    #[test]
    fn turn_zero_runs_the_setup_branch_only() {
        let script = turn_zero_script();
        let mut env = MockEnv { game_turn: 0, ..Default::default() };
        let mut interp = Interpreter::new();
        interp.run_turn(0, &script, &mut env);
        assert_eq!(env.calls, vec![CommandId::from_name("SET_REINCARNATION").unwrap()]);
    }

    #[test]
    fn every_256_12_first_fires_at_turn_244_per_the_wiki_example() {
        let script = turn_zero_script();
        let mut interp = Interpreter::new();

        // One turn early: the ELSE branch runs, but EVERY doesn't fire yet.
        let mut env = MockEnv { game_turn: 243, ..Default::default() };
        interp.run_turn(243, &script, &mut env);
        assert_eq!(env.calls, vec![]);

        let mut env = MockEnv { game_turn: 244, ..Default::default() };
        interp.run_turn(244, &script, &mut env);
        assert_eq!(env.calls, vec![CommandId::from_name("DELAY_MAIN_DRUM_TOWER").unwrap()]);
    }

    #[test]
    fn increment_persists_across_turns() {
        let script = Script {
            version: 12,
            block: Block(vec![Stmt::Assign {
                op: AssignOp::Increment,
                dest: Field::UserVar(UserVar(0)),
                src: Field::Const(1),
            }]),
        };
        let mut interp = Interpreter::new();
        let mut env = MockEnv::default();
        interp.run_turn(1, &script, &mut env);
        interp.run_turn(2, &script, &mut env);
        assert_eq!(interp.user_vars[0], 2);
    }

    #[test]
    fn multiply_computes_dest_equals_a_times_b() {
        let script = Script {
            version: 12,
            block: Block(vec![Stmt::AssignBinary {
                op: BinOp::Multiply,
                dest: Field::UserVar(UserVar(0)),
                a: Field::Const(6),
                b: Field::Const(7),
            }]),
        };
        let mut interp = Interpreter::new();
        interp.run_turn(1, &script, &mut MockEnv::default());
        assert_eq!(interp.user_vars[0], 42);
    }

    #[test]
    fn a_flyby_reaches_the_env_hook() {
        let script = Script { version: 12, block: Block(vec![Stmt::Flyby(Flyby::default())]) };
        let mut interp = Interpreter::new();
        let mut env = MockEnv::default();
        interp.run_turn(1, &script, &mut env);
        assert_eq!(env.flybys, 1);
    }

    #[test]
    fn a_do_calls_field_arguments_are_resolved_not_handed_over_raw() {
        let script = Script {
            version: 12,
            block: Block(vec![Stmt::Do(Call {
                id: CommandId::from_name("TRAIN_PEOPLE_NOW").unwrap(),
                args: vec![Arg::Field(Field::UserVar(UserVar(0))), Arg::Field(Field::Internal(InternalVar(1201)))],
            })]),
        };
        let mut interp = Interpreter::new();
        interp.user_vars[0] = 5;
        let mut env = MockEnv { game_turn: 0, ..Default::default() };
        interp.run_turn(0, &script, &mut env);
        assert_eq!(env.last_call_args, vec![Arg::Field(Field::Const(5)), Arg::Field(Field::Const(0))], "a user var and an internal var both resolved to plain constants");
    }

    #[test]
    fn a_flybys_fields_are_resolved_not_handed_over_raw() {
        use crate::flyby::{Flyby, PosKeyframe};
        let flyby = Flyby {
            positions: vec![PosKeyframe { x: Field::UserVar(UserVar(0)), z: Field::Const(5), start: Field::Const(1), duration: Field::Const(2) }],
            ..Flyby::default()
        };
        let script = Script { version: 12, block: Block(vec![Stmt::Flyby(flyby)]) };
        let mut interp = Interpreter::new();
        interp.user_vars[0] = 42;
        let mut env = MockEnv::default();
        interp.run_turn(1, &script, &mut env);
        assert_eq!(env.last_flyby.unwrap().positions, vec![ResolvedPos { x: 42, z: 5, start: 1, duration: 2 }]);
    }

    #[test]
    fn total_turns_is_the_latest_start_plus_duration_across_every_channel() {
        let flyby = ResolvedFlyby {
            positions: vec![ResolvedPos { x: 0, z: 0, start: 4, duration: 80 }, ResolvedPos { x: 0, z: 0, start: 81, duration: 44 }],
            angles: vec![ResolvedAngle { angle: 0, start: 219, duration: 45 }],
            ..ResolvedFlyby::default()
        };
        assert_eq!(flyby.total_turns(), 264);
        assert_eq!(ResolvedFlyby::default().total_turns(), 0);
    }
}
