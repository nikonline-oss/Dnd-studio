//! Typed errors of parsing, evaluation and computed-field resolution.

use crate::limits::{MAX_DEPTH, MAX_EXPR_LEN};
use crate::value::{Value, ValueType};

/// Everything that can go wrong with an expression. `offset` is a byte offset into the source text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExprError {
    #[error("expression is empty")]
    Empty,
    #[error("expression is too long: {len} characters (max {MAX_EXPR_LEN})")]
    TooLong { len: usize },
    #[error("expression is nested too deeply at position {offset} (max depth {MAX_DEPTH})")]
    TooDeep { offset: usize },
    #[error("syntax error at position {offset}: {message}")]
    Syntax { offset: usize, message: String },
    #[error("integer at position {offset} is out of range")]
    IntegerOutOfRange { offset: usize },
    #[error("unknown name `{name}` at position {offset}; fields are written as `@{name}`")]
    UnknownName { offset: usize, name: String },
    #[error(
        "unknown function `{name}` at position {offset}; available: min, max, floor, ceil, round, abs"
    )]
    UnknownFunction { offset: usize, name: String },
    #[error("`{function}` at position {offset} takes {expected} argument(s), got {found}")]
    WrongArity {
        offset: usize,
        function: &'static str,
        expected: &'static str,
        found: usize,
    },
    #[error("comparisons cannot be chained (position {offset}); use parentheses and `&&`")]
    ChainedComparison { offset: usize },
    #[error("unknown field @{path} at position {offset}")]
    UnknownReference { offset: usize, path: String },
    #[error("field @{path} at position {offset}: {message}")]
    Reference {
        offset: usize,
        path: String,
        message: String,
    },
    #[error("type mismatch at position {offset}: expected {expected}, found {found}")]
    TypeMismatch {
        offset: usize,
        expected: ValueType,
        found: ValueType,
    },
    #[error("division by zero at position {offset}")]
    DivisionByZero { offset: usize },
    #[error("`%` at position {offset} needs integer operands")]
    IntegerRequired { offset: usize },
    #[error("arithmetic overflow at position {offset}")]
    Overflow { offset: usize },
    #[error("{value} is not an integer")]
    NotAnInteger { value: Value },
    #[error("`{name}` is not a valid field name (expected `name` or `name.sub`)")]
    InvalidFieldName { name: String },
    #[error("computed fields reference each other in a cycle: {}", .fields.join(" -> "))]
    Cycle { fields: Vec<String> },
    /// An error in the formula of a computed field; offsets refer to that formula.
    #[error("computed field `{field}`: {source}")]
    Computed {
        field: String,
        source: Box<ExprError>,
    },
}

impl ExprError {
    /// Stable machine-readable code (snake case). [`ExprError::Computed`] reports its source's code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::TooLong { .. } => "too_long",
            Self::TooDeep { .. } => "too_deep",
            Self::Syntax { .. } => "syntax",
            Self::IntegerOutOfRange { .. } => "integer_out_of_range",
            Self::UnknownName { .. } => "unknown_name",
            Self::UnknownFunction { .. } => "unknown_function",
            Self::WrongArity { .. } => "wrong_arity",
            Self::ChainedComparison { .. } => "chained_comparison",
            Self::UnknownReference { .. } => "unknown_reference",
            Self::Reference { .. } => "reference",
            Self::TypeMismatch { .. } => "type_mismatch",
            Self::DivisionByZero { .. } => "division_by_zero",
            Self::IntegerRequired { .. } => "integer_required",
            Self::Overflow { .. } => "overflow",
            Self::NotAnInteger { .. } => "not_an_integer",
            Self::InvalidFieldName { .. } => "invalid_field_name",
            Self::Cycle { .. } => "cycle",
            Self::Computed { source, .. } => source.code(),
        }
    }

    /// Byte offset into the source text, when the error points at one.
    #[must_use]
    pub fn offset(&self) -> Option<usize> {
        match self {
            Self::TooDeep { offset }
            | Self::Syntax { offset, .. }
            | Self::IntegerOutOfRange { offset }
            | Self::UnknownName { offset, .. }
            | Self::UnknownFunction { offset, .. }
            | Self::WrongArity { offset, .. }
            | Self::ChainedComparison { offset }
            | Self::UnknownReference { offset, .. }
            | Self::Reference { offset, .. }
            | Self::TypeMismatch { offset, .. }
            | Self::DivisionByZero { offset }
            | Self::IntegerRequired { offset }
            | Self::Overflow { offset } => Some(*offset),
            Self::Computed { source, .. } => source.offset(),
            Self::Empty
            | Self::TooLong { .. }
            | Self::NotAnInteger { .. }
            | Self::InvalidFieldName { .. }
            | Self::Cycle { .. } => None,
        }
    }
}
