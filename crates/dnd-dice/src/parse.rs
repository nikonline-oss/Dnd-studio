//! winnow grammar for the P0 dice notation (ADR-0005) and limit validation.
//!
//! ```text
//! expr     = ws [sign ws] term { ws sign ws term } ws
//! term     = dice | uint | field
//! dice     = [uint] ("d" | "D") (uint | "%") [modifier]
//! modifier = ws ("kh" | "kl" | "dh" | "dl") uint     (case-insensitive)
//!          | ws1 ("adv" | "dis")                     (whitespace required)
//! field    = "@" ident { "." ident },  ident = [A-Za-z_][A-Za-z0-9_]*
//! ```
//!
//! The grammar keeps raw `u64` numbers (saturating); limits are checked afterwards so they
//! produce typed errors instead of generic syntax errors.

use winnow::ascii::{Caseless, digit1, multispace0, multispace1};
use winnow::combinator::{alt, cut_err, opt, preceded, separated};
use winnow::error::{StrContext, StrContextValue};
use winnow::prelude::*;
use winnow::token::{one_of, take_while};

use crate::ast::{Dice, Expr, Modifier, Sides, Sign, SignedTerm, Term};
use crate::error::DiceError;
use crate::limits::{MAX_DICE, MAX_EXPR_LEN, MAX_SAFE_INTEGER, MAX_SIDES};

#[derive(Debug, Clone, Copy)]
enum RawSides {
    Number(u64),
    Percent,
}

#[derive(Debug, Clone, Copy)]
enum RawModifier {
    Keep(&'static str, u64),
    Advantage,
    Disadvantage,
}

#[derive(Debug)]
enum RawTerm {
    Dice {
        count: Option<u64>,
        sides: RawSides,
        modifier: Option<RawModifier>,
    },
    Constant(u64),
    Field(String),
}

fn expected(what: &'static str) -> StrContext {
    StrContext::Expected(StrContextValue::Description(what))
}

fn number(i: &mut &str) -> ModalResult<u64> {
    // Only overflow can fail here; saturate and let validation report the limit.
    digit1
        .map(|s: &str| s.parse::<u64>().unwrap_or(u64::MAX))
        .parse_next(i)
}

fn sign(i: &mut &str) -> ModalResult<Sign> {
    alt(('+'.value(Sign::Plus), '-'.value(Sign::Minus))).parse_next(i)
}

fn sides(i: &mut &str) -> ModalResult<RawSides> {
    alt(('%'.value(RawSides::Percent), number.map(RawSides::Number)))
        .context(expected("number of sides or `%`"))
        .parse_next(i)
}

fn keep_drop(i: &mut &str) -> ModalResult<RawModifier> {
    let kind = alt((
        Caseless("kh").value("kh"),
        Caseless("kl").value("kl"),
        Caseless("dh").value("dh"),
        Caseless("dl").value("dl"),
    ))
    .parse_next(i)?;
    let n = cut_err(number.context(expected("number of dice to keep or drop"))).parse_next(i)?;
    Ok(RawModifier::Keep(kind, n))
}

fn modifier(i: &mut &str) -> ModalResult<RawModifier> {
    alt((
        preceded(multispace0, keep_drop),
        preceded(
            multispace1,
            alt((
                Caseless("adv").value(RawModifier::Advantage),
                Caseless("dis").value(RawModifier::Disadvantage),
            )),
        ),
    ))
    .parse_next(i)
}

fn dice(i: &mut &str) -> ModalResult<RawTerm> {
    let count = opt(number).parse_next(i)?;
    one_of(['d', 'D']).parse_next(i)?;
    let sides = cut_err(sides).parse_next(i)?;
    let modifier = opt(modifier).parse_next(i)?;
    Ok(RawTerm::Dice {
        count,
        sides,
        modifier,
    })
}

fn ident<'i>(i: &mut &'i str) -> ModalResult<&'i str> {
    (
        one_of(|c: char| c.is_ascii_alphabetic() || c == '_'),
        take_while(0.., |c: char| c.is_ascii_alphanumeric() || c == '_'),
    )
        .take()
        .parse_next(i)
}

fn field(i: &mut &str) -> ModalResult<RawTerm> {
    '@'.parse_next(i)?;
    let path = cut_err(
        separated::<_, _, (), _, _, _, _>(1.., ident, '.')
            .take()
            .context(expected("field name")),
    )
    .parse_next(i)?;
    Ok(RawTerm::Field(path.to_owned()))
}

fn term(i: &mut &str) -> ModalResult<RawTerm> {
    alt((dice, number.map(RawTerm::Constant), field))
        .context(expected("dice, number or @field"))
        .parse_next(i)
}

fn expression(i: &mut &str) -> ModalResult<Vec<(Sign, RawTerm)>> {
    multispace0.parse_next(i)?;
    let first_sign = opt((sign, multispace0))
        .map(|s| s.map_or(Sign::Plus, |(s, _)| s))
        .parse_next(i)?;
    let mut terms = vec![(first_sign, term.parse_next(i)?)];
    loop {
        multispace0.parse_next(i)?;
        if i.is_empty() {
            return Ok(terms);
        }
        let s = sign
            .context(expected("`+`, `-` or end of expression"))
            .parse_next(i)?;
        multispace0.parse_next(i)?;
        terms.push((s, cut_err(term).parse_next(i)?));
    }
}

pub(crate) fn parse(input: &str) -> Result<Expr, DiceError> {
    let len = input.chars().count();
    if len > MAX_EXPR_LEN {
        return Err(DiceError::TooLong { len });
    }
    if input.trim().is_empty() {
        return Err(DiceError::Empty);
    }
    let raw = expression.parse(input).map_err(|e| DiceError::Syntax {
        offset: e.offset(),
        message: e.inner().to_string().replace('\n', "; "),
    })?;
    validate(raw)
}

fn small(n: u64) -> u32 {
    // Callers check `n` against limits far below u32::MAX first.
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn validate(raw: Vec<(Sign, RawTerm)>) -> Result<Expr, DiceError> {
    let mut rolled_total: u64 = 0;
    let mut terms = Vec::with_capacity(raw.len());
    for (sign, raw_term) in raw {
        let term = match raw_term {
            RawTerm::Constant(value) => {
                if value > MAX_SAFE_INTEGER.unsigned_abs() {
                    return Err(DiceError::ConstantOutOfRange { value });
                }
                Term::Constant(
                    i64::try_from(value).map_err(|_| DiceError::ConstantOutOfRange { value })?,
                )
            }
            RawTerm::Field(path) => Term::Field(path),
            RawTerm::Dice {
                count,
                sides,
                modifier,
            } => {
                let count = count.unwrap_or(1);
                if count == 0 {
                    return Err(DiceError::ZeroDice);
                }
                if count > u64::from(MAX_DICE) {
                    return Err(DiceError::TooManyDice { count });
                }
                let sides = match sides {
                    RawSides::Percent => Sides::Percent,
                    RawSides::Number(n) if n == 0 || n > u64::from(MAX_SIDES) => {
                        return Err(DiceError::InvalidSides { sides: n });
                    }
                    RawSides::Number(n) => Sides::Number(small(n)),
                };
                let modifier = match modifier {
                    None => None,
                    Some(RawModifier::Advantage | RawModifier::Disadvantage) if count != 1 => {
                        return Err(DiceError::AdvantageNeedsSingleDie { count });
                    }
                    Some(RawModifier::Advantage) => Some(Modifier::Advantage),
                    Some(RawModifier::Disadvantage) => Some(Modifier::Disadvantage),
                    Some(RawModifier::Keep(kind, n)) => {
                        let keep = kind.starts_with('k');
                        let ok = if keep {
                            (1..=count).contains(&n)
                        } else {
                            (1..count).contains(&n)
                        };
                        if !ok {
                            return Err(DiceError::KeepDropOutOfRange {
                                modifier: kind,
                                n,
                                count,
                            });
                        }
                        let n = small(n);
                        Some(match kind {
                            "kh" => Modifier::KeepHighest(n),
                            "kl" => Modifier::KeepLowest(n),
                            "dh" => Modifier::DropHighest(n),
                            _ => Modifier::DropLowest(n),
                        })
                    }
                };
                let dice = Dice {
                    count: small(count),
                    sides,
                    modifier,
                };
                rolled_total += u64::from(dice.rolled());
                if rolled_total > u64::from(MAX_DICE) {
                    return Err(DiceError::TooManyDice {
                        count: rolled_total,
                    });
                }
                Term::Dice(dice)
            }
        };
        terms.push(SignedTerm { sign, term });
    }
    Ok(Expr::new(terms))
}
