//! The statement tree (docs/specs/ai-scripts.md "Code"): a recursive-descent parser over a
//! `RawScript`'s code words, resolving operands through its field table.

use crate::field::{Field, FieldError};
use crate::flyby::{self, Flyby};
use crate::raw::RawScript;
use crate::token::{
    word, Arg, AttackType, CmpOp, CommandId, GuardType, OneShotType, ParamKind, TargetType, Team, TeamColor, TeamWild,
    Token, VehicleType,
};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    pub version: u16,
    pub block: Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Block(pub Vec<Stmt>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Set,
    Increment,
    Decrement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Multiply,
    Divide,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub id: CommandId,
    pub args: Vec<Arg>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Do(Call),
    /// `EVERY period [offset] { ... }`. The file stores `period - 1` and `offset - 1` (a bit mask
    /// for the power-of-two period); these are the real (already `+1`-corrected) values.
    Every { period: i32, offset: Option<i32>, block: Block },
    Assign { op: AssignOp, dest: Field, src: Field },
    AssignBinary { op: BinOp, dest: Field, a: Field, b: Field },
    If { cond: Cond, then_: Block, else_: Option<Block> },
    /// A recognized `FLYBY_CREATE_NEW .. FLYBY_START` run, folded by `flyby::reduce_flybys`.
    Flyby(Flyby),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cond {
    Cmp { op: CmpOp, a: Field, b: Field },
    And(Box<Cond>, Box<Cond>),
    Or(Box<Cond>, Box<Cond>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AstError {
    Truncated,
    UnexpectedWord { at: usize, word: u16 },
    BadVersion(u16),
    BadDestination(Field),
    /// `EVERY`'s period/offset must be constants (spec: "Both operands must be constants").
    NonConstantEvery,
    BadArg { command: CommandId, index: usize },
    TrailingData,
    Field(FieldError),
}

impl From<FieldError> for AstError {
    fn from(e: FieldError) -> Self {
        AstError::Field(e)
    }
}

impl fmt::Display for AstError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AstError::Truncated => write!(f, "code stream ended early"),
            AstError::UnexpectedWord { at, word } => write!(f, "unexpected word {word} at {at}"),
            AstError::BadVersion(v) => write!(f, "expected version 12, found {v}"),
            AstError::BadDestination(field) => write!(f, "{field:?} is not a valid assignment destination"),
            AstError::NonConstantEvery => write!(f, "EVERY's period/offset must be constants"),
            AstError::BadArg { command, index } => write!(f, "bad argument {index} for {}", command.name()),
            AstError::TrailingData => write!(f, "nonzero word after SCRIPT_END"),
            AstError::Field(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AstError {}

struct Parser<'a> {
    words: &'a [u16],
    fields: &'a [crate::raw::RawField],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u16> {
        self.words.get(self.pos).copied()
    }

    fn next(&mut self) -> Result<u16, AstError> {
        let w = self.words.get(self.pos).copied().ok_or(AstError::Truncated)?;
        self.pos += 1;
        Ok(w)
    }

    fn expect(&mut self, token: Token) -> Result<(), AstError> {
        let at = self.pos;
        let word = self.next()?;
        if Token::from_word(word) == Some(token) {
            Ok(())
        } else {
            Err(AstError::UnexpectedWord { at, word })
        }
    }

    fn operand(&mut self) -> Result<Field, AstError> {
        let at = self.pos;
        let word = self.next()?;
        if word >= word::TOKEN_BASE {
            return Err(AstError::UnexpectedWord { at, word });
        }
        let raw = *self.fields.get(word as usize).ok_or(AstError::UnexpectedWord { at, word })?;
        Ok(Field::decode(raw)?)
    }

    fn const_operand(&mut self) -> Result<i32, AstError> {
        match self.operand()? {
            Field::Const(v) => Ok(v),
            _ => Err(AstError::NonConstantEvery),
        }
    }

    fn fdest(&mut self) -> Result<Field, AstError> {
        let field = self.operand()?;
        match field {
            Field::UserVar(_) => Ok(field),
            Field::Internal(iv) if iv.is_attribute() => Ok(field),
            other => Err(AstError::BadDestination(other)),
        }
    }

    fn parse_script(&mut self) -> Result<Script, AstError> {
        let version = self.next()?;
        if version != 12 {
            return Err(AstError::BadVersion(version));
        }
        let block = self.parse_block()?;
        self.expect(Token::ScriptEnd)?;
        if self.words[self.pos..].iter().any(|&w| w != 0) {
            return Err(AstError::TrailingData);
        }
        Ok(Script { version, block })
    }

    fn parse_block(&mut self) -> Result<Block, AstError> {
        self.expect(Token::Begin)?;
        let mut stmts = Vec::new();
        while self.peek() != Some(word::END) {
            stmts.push(self.parse_stmt()?);
        }
        self.expect(Token::End)?;
        Ok(Block(stmts))
    }

    fn parse_stmt(&mut self) -> Result<Stmt, AstError> {
        let at = self.pos;
        let word = self.next()?;
        match Token::from_word(word) {
            Some(Token::Do) => Ok(Stmt::Do(self.parse_call()?)),
            Some(Token::Every) => {
                let period = self.const_operand()?;
                let offset = if self.peek() == Some(word::BEGIN) { None } else { Some(self.const_operand()?) };
                let block = self.parse_block()?;
                Ok(Stmt::Every { period: period + 1, offset: offset.map(|o| o + 1), block })
            }
            Some(op @ (Token::Set | Token::Increment | Token::Decrement)) => {
                let dest = self.fdest()?;
                let src = self.operand()?;
                let op = match op {
                    Token::Set => AssignOp::Set,
                    Token::Increment => AssignOp::Increment,
                    Token::Decrement => AssignOp::Decrement,
                    _ => unreachable!(),
                };
                Ok(Stmt::Assign { op, dest, src })
            }
            Some(op @ (Token::Multiply | Token::Divide)) => {
                let dest = self.fdest()?;
                let a = self.operand()?;
                let b = self.operand()?;
                let op = if op == Token::Multiply { BinOp::Multiply } else { BinOp::Divide };
                Ok(Stmt::AssignBinary { op, dest, a, b })
            }
            Some(Token::If) => {
                let cond = self.parse_cond()?;
                let then_ = self.parse_block()?;
                let else_ = if self.peek() == Some(word::ELSE) {
                    self.next()?;
                    Some(self.parse_block()?)
                } else {
                    None
                };
                self.expect(Token::Endif)?;
                Ok(Stmt::If { cond, then_, else_ })
            }
            _ => Err(AstError::UnexpectedWord { at, word }),
        }
    }

    fn parse_cond(&mut self) -> Result<Cond, AstError> {
        let at = self.pos;
        let word = self.next()?;
        match Token::from_word(word) {
            Some(Token::And) => Ok(Cond::And(Box::new(self.parse_cond()?), Box::new(self.parse_cond()?))),
            Some(Token::Or) => Ok(Cond::Or(Box::new(self.parse_cond()?), Box::new(self.parse_cond()?))),
            Some(Token::Cmp(op)) => {
                let a = self.operand()?;
                let b = self.operand()?;
                Ok(Cond::Cmp { op, a, b })
            }
            _ => Err(AstError::UnexpectedWord { at, word }),
        }
    }

    fn parse_call(&mut self) -> Result<Call, AstError> {
        let at = self.pos;
        let word = self.next()?;
        let id = CommandId::from_word(word).ok_or(AstError::UnexpectedWord { at, word })?;
        let mut args = Vec::with_capacity(id.params().len());
        for (index, &kind) in id.params().iter().enumerate() {
            args.push(self.parse_arg(id, index, kind)?);
        }
        Ok(Call { id, args })
    }

    fn parse_arg(&mut self, command: CommandId, index: usize, kind: ParamKind) -> Result<Arg, AstError> {
        let bad = || AstError::BadArg { command, index };
        Ok(match kind {
            ParamKind::Field => Arg::Field(self.operand()?),
            ParamKind::VarField => match self.operand()? {
                Field::UserVar(v) => Arg::VarField(v),
                _ => return Err(bad()),
            },
            ParamKind::OnOff => {
                let word = self.next()?;
                match Token::from_word(word) {
                    Some(Token::On) => Arg::OnOff(true),
                    Some(Token::Off) => Arg::OnOff(false),
                    _ => return Err(bad()),
                }
            }
            ParamKind::Team => Arg::Team(self.parse_team()?.ok_or_else(bad)?),
            ParamKind::TeamWild => {
                let w = self.peek().ok_or(AstError::Truncated)?;
                if w == word::COUNT_WILD {
                    self.next()?;
                    Arg::TeamWild(TeamWild::Wild)
                } else {
                    Arg::TeamWild(TeamWild::Team(self.parse_team()?.ok_or_else(bad)?))
                }
            }
            ParamKind::TargetType => {
                let word = self.next()?;
                Arg::TargetType(TargetType::from_word(word).ok_or_else(bad)?)
            }
            ParamKind::AttackType => {
                let word = self.next()?;
                Arg::AttackType(AttackType::from_word(word).ok_or_else(bad)?)
            }
            ParamKind::GuardType => {
                let word = self.next()?;
                Arg::GuardType(GuardType::from_word(word).ok_or_else(bad)?)
            }
            ParamKind::OneShotType => {
                let word = self.next()?;
                Arg::OneShotType(OneShotType::from_word(word).ok_or_else(bad)?)
            }
            ParamKind::VehicleType => {
                let word = self.next()?;
                Arg::VehicleType(VehicleType::from_word(word).ok_or_else(bad)?)
            }
        })
    }

    /// A `team`: a keyword token, or an operand.
    fn parse_team(&mut self) -> Result<Option<Team>, AstError> {
        let w = self.peek().ok_or(AstError::Truncated)?;
        if w >= word::TOKEN_BASE {
            self.next()?;
            Ok(TeamColor::from_word(w).map(Team::Color))
        } else {
            Ok(Some(Team::Field(self.operand()?)))
        }
    }
}

pub fn parse(raw: &RawScript) -> Result<Script, AstError> {
    let mut parser = Parser { words: &raw.code, fields: &raw.fields, pos: 0 };
    let mut script = parser.parse_script()?;
    flyby::reduce_flybys(&mut script.block);
    Ok(script)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::{InternalVar, UserVar};
    use crate::raw::RawField;

    fn fields(pairs: &[(u32, i32)]) -> Vec<RawField> {
        pairs.iter().map(|&(ty, value)| RawField { ty, value }).collect()
    }

    fn script_of(code: &[u16], fields: Vec<RawField>) -> RawScript {
        let mut words = vec![0u16; crate::raw::CODE_WORDS];
        words[..code.len()].copy_from_slice(code);
        RawScript { version: words[0], code: words, fields, user_vars: [0; crate::raw::USER_VAR_COUNT] }
    }

    #[test]
    fn parses_the_spec_worked_example() {
        // docs/specs/ai-scripts.md "Code": cpscr010's opening words.
        // F0 = INT_GAME_TURN, F1 = const 0, F2 = user var 0.
        let fields = fields(&[(2, 0), (0, 0), (1, 0)]);
        let set_reincarnation = CommandId::from_name("SET_REINCARNATION").unwrap();
        let delay_main_drum_tower = CommandId::from_name("DELAY_MAIN_DRUM_TOWER").unwrap();
        // outer { IF (F0==F1) { DO SET_REINCARNATION OFF; DO DELAY_MAIN_DRUM_TOWER; SET F2 F1 } ENDIF }, SCRIPT_END
        let code = [
            12,
            word::BEGIN,
            word::IF,
            word::EQ,
            0,
            1,
            word::BEGIN,
            word::DO,
            set_reincarnation.0,
            word::OFF,
            word::DO,
            delay_main_drum_tower.0,
            word::SET,
            2,
            1,
            word::END,
            word::ENDIF,
            word::END,
            word::SCRIPT_END,
        ];
        let script = parse(&script_of(&code, fields)).unwrap();
        assert_eq!(
            script.block,
            Block(vec![Stmt::If {
                cond: Cond::Cmp { op: CmpOp::Eq, a: Field::Internal(InternalVar(0)), b: Field::Const(0) },
                then_: Block(vec![
                    Stmt::Do(Call { id: set_reincarnation, args: vec![Arg::OnOff(false)] }),
                    Stmt::Do(Call { id: delay_main_drum_tower, args: vec![] }),
                    Stmt::Assign { op: AssignOp::Set, dest: Field::UserVar(UserVar(0)), src: Field::Const(0) },
                ]),
                else_: None,
            }])
        );
    }

    #[test]
    fn every_decodes_period_and_offset_minus_one_storage() {
        // EVERY 256 12 { }  ->  stored period 255, stored offset 11.
        let fields = fields(&[(0, 255), (0, 11)]);
        let code = [12, word::BEGIN, word::EVERY, 0, 1, word::BEGIN, word::END, word::END, word::SCRIPT_END];
        let script = parse(&script_of(&code, fields)).unwrap();
        assert_eq!(script.block.0[0], Stmt::Every { period: 256, offset: Some(12), block: Block(vec![]) });
    }

    #[test]
    fn every_without_offset_goes_straight_to_the_block() {
        let fields = fields(&[(0, 255)]);
        let code = [12, word::BEGIN, word::EVERY, 0, word::BEGIN, word::END, word::END, word::SCRIPT_END];
        let script = parse(&script_of(&code, fields)).unwrap();
        assert_eq!(script.block.0[0], Stmt::Every { period: 256, offset: None, block: Block(vec![]) });
    }

    #[test]
    fn and_or_chains_are_left_nested() {
        // OR AND c1 c2 c3  ->  (c1 && c2) || c3, three comparisons against the same two fields.
        let fields = fields(&[(0, 1), (0, 2)]);
        let code = [word::OR, word::AND, word::EQ, 0, 1, word::EQ, 0, 1, word::EQ, 0, 1];
        let mut parser = Parser { words: &code, fields: &fields, pos: 0 };
        let cond = parser.parse_cond().unwrap();
        let cmp = Cond::Cmp { op: CmpOp::Eq, a: Field::Const(1), b: Field::Const(2) };
        assert_eq!(cond, Cond::Or(Box::new(Cond::And(Box::new(cmp.clone()), Box::new(cmp.clone()))), Box::new(cmp)));
    }

    #[test]
    fn rejects_trailing_data_after_script_end() {
        let code = [12, word::BEGIN, word::END, word::SCRIPT_END, 7];
        assert_eq!(parse(&script_of(&code, vec![])), Err(AstError::TrailingData));
    }

    #[test]
    fn rejects_wrong_version() {
        let code = [13, word::BEGIN, word::END, word::SCRIPT_END];
        assert_eq!(parse(&script_of(&code, vec![])), Err(AstError::BadVersion(13)));
    }
}
