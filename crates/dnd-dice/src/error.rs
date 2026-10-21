//! Typed errors of parsing and rolling.

use crate::fields::LookupError;
use crate::limits::{MAX_DICE, MAX_EXPR_LEN, MAX_SAFE_INTEGER, MAX_SIDES};

/// Everything that can go wrong with a dice expression.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DiceError {
    #[error("dice expression is empty")]
    Empty,
    #[error("dice expression is too long: {len} characters (max {MAX_EXPR_LEN})")]
    TooLong { len: usize },
    /// `offset` is a byte offset into the original input.
    #[error("syntax error at position {offset}: {message}")]
    Syntax { offset: usize, message: String },
    #[error("number of dice must be at least 1")]
    ZeroDice,
    #[error("too many dice: {count} (max {MAX_DICE} per expression)")]
    TooManyDice { count: u64 },
    #[error("a die must have from 1 to {MAX_SIDES} sides, got {sides}")]
    InvalidSides { sides: u64 },
    /// Keep needs `1..=count`, drop needs `1..count` (at least one die stays).
    #[error("`{modifier}{n}` is out of range for {count} dice")]
    KeepDropOutOfRange {
        modifier: &'static str,
        n: u64,
        count: u64,
    },
    #[error("advantage/disadvantage applies to a single die, got {count} dice")]
    AdvantageNeedsSingleDie { count: u64 },
    #[error("constant {value} exceeds the safe integer range (max {MAX_SAFE_INTEGER})")]
    ConstantOutOfRange { value: u64 },
    #[error("field @{path}: {source}")]
    Field { path: String, source: LookupError },
    #[error("field @{path} = {value} is outside the safe integer range")]
    FieldOutOfRange { path: String, value: i64 },
    #[error("roll total is outside the safe integer range (±{MAX_SAFE_INTEGER})")]
    TotalOutOfRange,
    #[error("random number generator failed to produce an unbiased value")]
    RngExhausted,
}
