//! The "flyby" builder (docs/specs/ai-scripts.md "Level events" "Flybys", "Worked example: level
//! 1's opening flyby"): `FLYBY_CREATE_NEW` through `FLYBY_START` is a run of `DO` calls that
//! together build one camera sequence, not independent statements — "reading order is
//! build-then-start: every `FLYBY_SET_EVENT_*` after `FLYBY_CREATE_NEW` queues one timed keyframe
//! ..., and nothing animates until `FLYBY_START`". `reduce_flybys` folds a recognized run into one
//! `Stmt::Flyby` node.

use crate::ast::{Block, Call, Stmt};
use crate::field::Field;
use crate::token::Arg;

const CREATE_NEW: u16 = 1205;
const START: u16 = 1206;
const ALLOW_INTERRUPT: u16 = 1208;
const SET_EVENT_POS: u16 = 1209;
const SET_EVENT_ANGLE: u16 = 1210;
const SET_EVENT_ZOOM: u16 = 1211;
const SET_EVENT_TOOLTIP: u16 = 1213;
const SET_END_TARGET: u16 = 1214;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosKeyframe {
    pub x: Field,
    pub z: Field,
    pub start: Field,
    pub duration: Field,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AngleKeyframe {
    pub angle: Field,
    pub start: Field,
    pub duration: Field,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoomKeyframe {
    pub zoom: Field,
    pub start: Field,
    pub duration: Field,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TooltipKeyframe {
    pub x: Field,
    pub z: Field,
    pub text: Field,
    pub start: Field,
    pub duration: Field,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndTarget {
    pub x: Field,
    pub z: Field,
    pub angle: Field,
    /// The 4th field: unknown meaning (spec: "possibly a marker index to snap to instead").
    pub unknown: Field,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Flyby {
    pub allow_interrupt: bool,
    pub positions: Vec<PosKeyframe>,
    pub angles: Vec<AngleKeyframe>,
    pub zooms: Vec<ZoomKeyframe>,
    pub tooltips: Vec<TooltipKeyframe>,
    pub end_target: Option<EndTarget>,
}

fn call_in(stmt: &Stmt) -> Option<&Call> {
    match stmt {
        Stmt::Do(call) => Some(call),
        _ => None,
    }
}

/// Recursively folds every `FLYBY_CREATE_NEW..=FLYBY_START` run found in `block` or any of its
/// nested `EVERY`/`IF` blocks into a single `Stmt::Flyby`.
pub fn reduce_flybys(block: &mut Block) {
    for stmt in block.0.iter_mut() {
        match stmt {
            Stmt::Every { block: inner, .. } => reduce_flybys(inner),
            Stmt::If { then_, else_, .. } => {
                reduce_flybys(then_);
                if let Some(e) = else_ {
                    reduce_flybys(e);
                }
            }
            _ => {}
        }
    }
    fold_run(block);
}

fn fold_run(block: &mut Block) {
    let mut i = 0;
    while i < block.0.len() {
        let is_create_new = call_in(&block.0[i]).is_some_and(|c| c.id.0 == CREATE_NEW);
        if is_create_new
            && let Some(end) = run_end(&block.0, i)
        {
            let flyby = build_flyby(&block.0[i..=end]);
            block.0.splice(i..=end, std::iter::once(Stmt::Flyby(flyby)));
        }
        i += 1;
    }
}

/// `start` is the index of `FLYBY_CREATE_NEW`; returns `FLYBY_START`'s index if every statement in
/// between is a recognized flyby-building call, `None` otherwise (an unrecognized statement aborts
/// folding this run only — best-effort, not a traced rule).
fn run_end(stmts: &[Stmt], start: usize) -> Option<usize> {
    let mut j = start + 1;
    while j < stmts.len() {
        let id = call_in(&stmts[j])?.id.0;
        match id {
            START => return Some(j),
            ALLOW_INTERRUPT | SET_EVENT_POS | SET_EVENT_ANGLE | SET_EVENT_ZOOM | SET_EVENT_TOOLTIP | SET_END_TARGET => {
                j += 1;
            }
            _ => return None,
        }
    }
    None
}

fn field_arg(args: &[Arg], i: usize) -> Field {
    match args[i] {
        Arg::Field(f) => f,
        _ => unreachable!("every flyby command's params are all `field`"),
    }
}

fn bool_arg(args: &[Arg], i: usize) -> bool {
    match args[i] {
        Arg::OnOff(b) => b,
        _ => unreachable!("FLYBY_ALLOW_INTERRUPT's only param is on_off"),
    }
}

fn build_flyby(stmts: &[Stmt]) -> Flyby {
    let mut flyby = Flyby::default();
    for stmt in stmts {
        let Some(call) = call_in(stmt) else { continue };
        match call.id.0 {
            CREATE_NEW | START => {}
            ALLOW_INTERRUPT => flyby.allow_interrupt = bool_arg(&call.args, 0),
            SET_EVENT_POS => flyby.positions.push(PosKeyframe {
                x: field_arg(&call.args, 0),
                z: field_arg(&call.args, 1),
                start: field_arg(&call.args, 2),
                duration: field_arg(&call.args, 3),
            }),
            SET_EVENT_ANGLE => flyby.angles.push(AngleKeyframe {
                angle: field_arg(&call.args, 0),
                start: field_arg(&call.args, 1),
                duration: field_arg(&call.args, 2),
            }),
            SET_EVENT_ZOOM => flyby.zooms.push(ZoomKeyframe {
                zoom: field_arg(&call.args, 0),
                start: field_arg(&call.args, 1),
                duration: field_arg(&call.args, 2),
            }),
            SET_EVENT_TOOLTIP => flyby.tooltips.push(TooltipKeyframe {
                x: field_arg(&call.args, 0),
                z: field_arg(&call.args, 1),
                text: field_arg(&call.args, 2),
                start: field_arg(&call.args, 3),
                duration: field_arg(&call.args, 4),
            }),
            SET_END_TARGET => {
                flyby.end_target = Some(EndTarget {
                    x: field_arg(&call.args, 0),
                    z: field_arg(&call.args, 1),
                    angle: field_arg(&call.args, 2),
                    unknown: field_arg(&call.args, 3),
                })
            }
            _ => unreachable!("run_end only accepts recognized ids"),
        }
    }
    flyby
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::CommandId;

    fn call(word: u16, args: Vec<Arg>) -> Stmt {
        Stmt::Do(Call { id: CommandId::from_word(word).unwrap(), args })
    }

    fn call_named(name: &str, args: Vec<Arg>) -> Stmt {
        Stmt::Do(Call { id: CommandId::from_name(name).unwrap(), args })
    }

    #[test]
    fn folds_a_create_new_to_start_run() {
        let mut block = Block(vec![
            call_named("SET_BASE_MARKER", vec![Arg::Field(Field::Const(3))]), // unrelated, before the run
            call(CREATE_NEW, vec![]),
            call(ALLOW_INTERRUPT, vec![Arg::OnOff(true)]),
            call(
                SET_EVENT_POS,
                vec![Arg::Field(Field::Const(8)), Arg::Field(Field::Const(28)), Arg::Field(Field::Const(4)), Arg::Field(Field::Const(80))],
            ),
            call(
                SET_EVENT_POS,
                vec![Arg::Field(Field::Const(2)), Arg::Field(Field::Const(28)), Arg::Field(Field::Const(81)), Arg::Field(Field::Const(44))],
            ),
            call(
                SET_END_TARGET,
                vec![Arg::Field(Field::Const(20)), Arg::Field(Field::Const(216)), Arg::Field(Field::Const(1438)), Arg::Field(Field::Const(0))],
            ),
            call(START, vec![]),
        ]);
        reduce_flybys(&mut block);
        assert_eq!(
            block.0,
            vec![
                call_named("SET_BASE_MARKER", vec![Arg::Field(Field::Const(3))]),
                Stmt::Flyby(Flyby {
                    allow_interrupt: true,
                    positions: vec![
                        PosKeyframe { x: Field::Const(8), z: Field::Const(28), start: Field::Const(4), duration: Field::Const(80) },
                        PosKeyframe { x: Field::Const(2), z: Field::Const(28), start: Field::Const(81), duration: Field::Const(44) },
                    ],
                    angles: vec![],
                    zooms: vec![],
                    tooltips: vec![],
                    end_target: Some(EndTarget { x: Field::Const(20), z: Field::Const(216), angle: Field::Const(1438), unknown: Field::Const(0) }),
                }),
            ]
        );
    }

    #[test]
    fn leaves_a_block_without_a_flyby_run_untouched() {
        let mut block = Block(vec![call_named("SET_BASE_MARKER", vec![Arg::Field(Field::Const(3))])]);
        let before = block.clone();
        reduce_flybys(&mut block);
        assert_eq!(block, before);
    }

    #[test]
    fn an_unrecognized_call_inside_the_run_aborts_folding() {
        let mut block = Block(vec![
            call(CREATE_NEW, vec![]),
            call_named("SET_BASE_MARKER", vec![Arg::Field(Field::Const(3))]), // not a flyby command
            call(START, vec![]),
        ]);
        let before = block.clone();
        reduce_flybys(&mut block);
        assert_eq!(block, before, "an unrecognized call inside the run should leave it as raw Do statements");
    }

    #[test]
    fn recurses_into_nested_every_and_if_blocks() {
        let mut inner = Block(vec![call(CREATE_NEW, vec![]), call(START, vec![])]);
        reduce_flybys(&mut inner);
        assert!(matches!(inner.0.as_slice(), [Stmt::Flyby(_)]));
    }
}
