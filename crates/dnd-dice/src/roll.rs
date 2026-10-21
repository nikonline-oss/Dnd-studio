//! Evaluating an [`Expr`] into a structured [`Roll`].

use serde::{Deserialize, Serialize};

use crate::ast::{Dice, Expr, Modifier, Sides, Sign, Term};
use crate::error::DiceError;
use crate::fields::FieldLookup;
use crate::limits::MAX_SAFE_INTEGER;
use crate::rng::{DiceRng, uniform};

/// One physical die.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Die {
    pub value: u32,
    /// `false` if dropped by keep/drop or advantage/disadvantage.
    pub kept: bool,
}

/// What a term was and what it produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TermDetail {
    Dice {
        /// Canonical notation of the term, e.g. `4d6kh3`.
        notation: String,
        count: u32,
        sides: u32,
        /// Written as `d%`.
        percentile: bool,
        modifier: Option<Modifier>,
        /// In roll order.
        dice: Vec<Die>,
    },
    Constant {
        value: i64,
    },
    Field {
        path: String,
        value: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermRoll {
    pub sign: Sign,
    /// Value of the term before its sign is applied.
    pub subtotal: i64,
    pub term: TermDetail,
}

/// Result of rolling an expression. All numbers are within ±(2^53−1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Roll {
    pub expression: Expr,
    pub terms: Vec<TermRoll>,
    pub total: i64,
}

fn in_safe_range(v: i64) -> bool {
    (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&v)
}

/// Rolls the dice of an expression.
///
/// All field references are resolved before the first die is rolled, so a lookup error does
/// not consume randomness.
///
/// # Errors
/// [`DiceError::Field`] / [`DiceError::FieldOutOfRange`] for bad references,
/// [`DiceError::TotalOutOfRange`] if the sum leaves ±(2^53−1),
/// [`DiceError::RngExhausted`] for a broken RNG.
pub fn roll<L, R>(expr: &Expr, fields: &L, rng: &mut R) -> Result<Roll, DiceError>
where
    L: FieldLookup + ?Sized,
    R: DiceRng + ?Sized,
{
    let mut resolved = Vec::new();
    for t in expr.terms() {
        if let Term::Field(path) = &t.term {
            let value = fields.lookup(path).map_err(|source| DiceError::Field {
                path: path.clone(),
                source,
            })?;
            if !in_safe_range(value) {
                return Err(DiceError::FieldOutOfRange {
                    path: path.clone(),
                    value,
                });
            }
            resolved.push(value);
        }
    }
    let mut resolved = resolved.into_iter();

    let mut total: i64 = 0;
    let mut terms = Vec::with_capacity(expr.terms().len());
    for t in expr.terms() {
        let (subtotal, term) = match &t.term {
            Term::Constant(value) => (*value, TermDetail::Constant { value: *value }),
            Term::Field(path) => {
                let value = resolved.next().ok_or(DiceError::TotalOutOfRange)?;
                (
                    value,
                    TermDetail::Field {
                        path: path.clone(),
                        value,
                    },
                )
            }
            Term::Dice(d) => roll_dice(d, rng)?,
        };
        let signed = match t.sign {
            Sign::Plus => Some(subtotal),
            Sign::Minus => subtotal.checked_neg(),
        };
        total = signed
            .and_then(|s| total.checked_add(s))
            .filter(|v| in_safe_range(*v))
            .ok_or(DiceError::TotalOutOfRange)?;
        terms.push(TermRoll {
            sign: t.sign,
            subtotal,
            term,
        });
    }
    Ok(Roll {
        expression: expr.clone(),
        terms,
        total,
    })
}

fn roll_dice<R: DiceRng + ?Sized>(d: &Dice, rng: &mut R) -> Result<(i64, TermDetail), DiceError> {
    let sides = d.sides.get();
    let mut dice = Vec::with_capacity(d.rolled() as usize);
    for _ in 0..d.rolled() {
        dice.push(Die {
            value: uniform(rng, sides)?,
            kept: true,
        });
    }

    // Ties: the earlier die wins (is kept by keep, dropped by drop).
    let mut ascending: Vec<usize> = (0..dice.len()).collect();
    ascending.sort_by_key(|&i| (dice[i].value, i));
    let mut descending: Vec<usize> = (0..dice.len()).collect();
    descending.sort_by_key(|&i| (std::cmp::Reverse(dice[i].value), i));
    let (keep, order, n) = match d.modifier {
        None => (true, &ascending, dice.len()),
        Some(Modifier::KeepHighest(n)) => (true, &descending, n as usize),
        Some(Modifier::KeepLowest(n)) => (true, &ascending, n as usize),
        Some(Modifier::DropHighest(n)) => (false, &descending, n as usize),
        Some(Modifier::DropLowest(n)) => (false, &ascending, n as usize),
        Some(Modifier::Advantage) => (true, &descending, 1),
        Some(Modifier::Disadvantage) => (true, &ascending, 1),
    };
    if keep {
        for die in &mut dice {
            die.kept = false;
        }
    }
    for &i in order.iter().take(n) {
        if let Some(die) = dice.get_mut(i) {
            die.kept = keep;
        }
    }

    // At most 1000 dice × 1 000 000 sides: far below 2^53.
    let subtotal = dice
        .iter()
        .filter(|d| d.kept)
        .map(|d| i64::from(d.value))
        .sum();
    Ok((
        subtotal,
        TermDetail::Dice {
            notation: d.to_string(),
            count: d.count,
            sides,
            percentile: d.sides == Sides::Percent,
            modifier: d.modifier,
            dice,
        },
    ))
}
