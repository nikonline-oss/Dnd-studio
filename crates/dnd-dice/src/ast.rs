//! Parsed dice expression and its canonical form.
//!
//! An [`Expr`] can only be obtained from [`Expr::parse`] (or deserialization, which parses), so
//! every `Expr` satisfies the limits in [`crate::limits`].

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::DiceError;

/// Sign in front of a term.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sign {
    Plus,
    Minus,
}

/// Number of sides: a plain number or `%` (d100, kept apart for the canonical form).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sides {
    Number(u32),
    Percent,
}

impl Sides {
    #[must_use]
    pub fn get(self) -> u32 {
        match self {
            Self::Number(n) => n,
            Self::Percent => 100,
        }
    }
}

/// Keep/drop or advantage modifier of a dice term.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "n", rename_all = "snake_case")]
pub enum Modifier {
    /// `khN`
    KeepHighest(u32),
    /// `klN`
    KeepLowest(u32),
    /// `dhN`
    DropHighest(u32),
    /// `dlN`
    DropLowest(u32),
    /// ` adv`: roll the die twice, keep the higher.
    Advantage,
    /// ` dis`: roll the die twice, keep the lower.
    Disadvantage,
}

/// `NdX` with an optional modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Dice {
    pub count: u32,
    pub sides: Sides,
    pub modifier: Option<Modifier>,
}

impl Dice {
    /// How many dice are physically rolled (advantage/disadvantage roll twice).
    #[must_use]
    pub fn rolled(&self) -> u32 {
        match self.modifier {
            Some(Modifier::Advantage | Modifier::Disadvantage) => 2,
            _ => self.count,
        }
    }

    /// How many rolled dice count towards the subtotal.
    #[must_use]
    pub fn kept(&self) -> u32 {
        match self.modifier {
            None => self.count,
            Some(Modifier::KeepHighest(n) | Modifier::KeepLowest(n)) => n,
            Some(Modifier::DropHighest(n) | Modifier::DropLowest(n)) => self.count - n,
            Some(Modifier::Advantage | Modifier::Disadvantage) => 1,
        }
    }
}

/// One operand of the sum.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Term {
    Dice(Dice),
    /// Non-negative constant; the sign lives in [`SignedTerm`].
    Constant(i64),
    /// Dotted field path without the `@`.
    Field(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SignedTerm {
    pub sign: Sign,
    pub term: Term,
}

/// A validated dice expression: a non-empty signed sum of terms.
///
/// Serializes as its canonical string; deserialization re-parses it, so limits always hold.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Expr {
    terms: Vec<SignedTerm>,
}

impl Expr {
    pub(crate) fn new(terms: Vec<SignedTerm>) -> Self {
        Self { terms }
    }

    /// Parses and validates a dice expression.
    ///
    /// # Errors
    /// See [`DiceError`] for syntax and limit errors.
    pub fn parse(input: &str) -> Result<Self, DiceError> {
        crate::parse::parse(input)
    }

    #[must_use]
    pub fn terms(&self) -> &[SignedTerm] {
        &self.terms
    }

    /// Total number of dice rolled by this expression.
    #[must_use]
    pub fn dice_rolled(&self) -> u32 {
        self.terms
            .iter()
            .map(|t| match &t.term {
                Term::Dice(d) => d.rolled(),
                _ => 0,
            })
            .sum()
    }
}

impl FromStr for Expr {
    type Err = DiceError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<String> for Expr {
    type Error = DiceError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::parse(&s)
    }
}

impl From<Expr> for String {
    fn from(e: Expr) -> Self {
        e.to_string()
    }
}

impl fmt::Display for Modifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KeepHighest(n) => write!(f, "kh{n}"),
            Self::KeepLowest(n) => write!(f, "kl{n}"),
            Self::DropHighest(n) => write!(f, "dh{n}"),
            Self::DropLowest(n) => write!(f, "dl{n}"),
            Self::Advantage => f.write_str(" adv"),
            Self::Disadvantage => f.write_str(" dis"),
        }
    }
}

impl fmt::Display for Dice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.count != 1 {
            write!(f, "{}", self.count)?;
        }
        match self.sides {
            Sides::Number(n) => write!(f, "d{n}")?,
            Sides::Percent => f.write_str("d%")?,
        }
        if let Some(m) = self.modifier {
            write!(f, "{m}")?;
        }
        Ok(())
    }
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dice(d) => d.fmt(f),
            Self::Constant(c) => write!(f, "{c}"),
            Self::Field(p) => write!(f, "@{p}"),
        }
    }
}

/// Canonical form: compact (no spaces except before `adv`/`dis`, count omitted when 1,
/// lowercase). It is never longer than any input that parses to the same expression, so it
/// always fits the length limit and `parse(display(e)) == e`.
impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, t) in self.terms.iter().enumerate() {
            match (i, t.sign) {
                (0, Sign::Plus) => {}
                (_, Sign::Plus) => f.write_str("+")?,
                (_, Sign::Minus) => f.write_str("-")?,
            }
            t.term.fmt(f)?;
        }
        Ok(())
    }
}
