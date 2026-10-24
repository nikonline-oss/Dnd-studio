//! Values: exact rationals over `i64` and booleans.

use std::cmp::Ordering;
use std::fmt;

use crate::error::ExprError;

/// An exact fraction `numer / denom`, always normalized: `denom > 0` and `gcd(numer, denom) == 1`.
///
/// Every operation that would leave the `i64` range returns `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rational {
    num: i64,
    den: i64,
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

impl Rational {
    pub const ZERO: Self = Self { num: 0, den: 1 };

    #[must_use]
    pub const fn integer(n: i64) -> Self {
        Self { num: n, den: 1 }
    }

    /// `None` when `den == 0` or the normalized fraction does not fit into `i64`.
    #[must_use]
    pub fn new(num: i64, den: i64) -> Option<Self> {
        Self::from_i128(i128::from(num), i128::from(den))
    }

    fn from_i128(num: i128, den: i128) -> Option<Self> {
        if den == 0 {
            return None;
        }
        let (num, den) = if den < 0 {
            (num.checked_neg()?, den.checked_neg()?)
        } else {
            (num, den)
        };
        let g = gcd(num.unsigned_abs(), den.unsigned_abs());
        let g = i128::try_from(g).ok()?;
        Some(Self {
            num: i64::try_from(num / g).ok()?,
            den: i64::try_from(den / g).ok()?,
        })
    }

    #[must_use]
    pub const fn numer(self) -> i64 {
        self.num
    }

    #[must_use]
    pub const fn denom(self) -> i64 {
        self.den
    }

    #[must_use]
    pub const fn is_integer(self) -> bool {
        self.den == 1
    }

    #[must_use]
    pub const fn to_integer(self) -> Option<i64> {
        if self.den == 1 { Some(self.num) } else { None }
    }

    fn parts(self) -> (i128, i128) {
        (i128::from(self.num), i128::from(self.den))
    }

    #[must_use]
    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        let ((a, b), (c, d)) = (self.parts(), rhs.parts());
        Self::from_i128(a.checked_mul(d)?.checked_add(c.checked_mul(b)?)?, b * d)
    }

    #[must_use]
    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        let ((a, b), (c, d)) = (self.parts(), rhs.parts());
        Self::from_i128(a.checked_mul(d)?.checked_sub(c.checked_mul(b)?)?, b * d)
    }

    #[must_use]
    pub fn checked_mul(self, rhs: Self) -> Option<Self> {
        let ((a, b), (c, d)) = (self.parts(), rhs.parts());
        Self::from_i128(a * c, b * d)
    }

    /// `None` on division by zero or overflow.
    #[must_use]
    pub fn checked_div(self, rhs: Self) -> Option<Self> {
        let ((a, b), (c, d)) = (self.parts(), rhs.parts());
        Self::from_i128(a * d, b * c)
    }

    /// Floored remainder of two integers (the result has the sign of `rhs`, as in Python):
    /// `a % b == a - b * floor(a / b)`. `None` for non-integers or `rhs == 0`.
    #[must_use]
    pub fn checked_rem(self, rhs: Self) -> Option<Self> {
        let (a, b) = (
            i128::from(self.to_integer()?),
            i128::from(rhs.to_integer()?),
        );
        if b == 0 {
            return None;
        }
        let mut r = a % b;
        if r != 0 && (r < 0) != (b < 0) {
            r += b;
        }
        Self::from_i128(r, 1)
    }

    #[must_use]
    pub fn checked_neg(self) -> Option<Self> {
        Some(Self {
            num: self.num.checked_neg()?,
            den: self.den,
        })
    }

    #[must_use]
    pub fn checked_abs(self) -> Option<Self> {
        Some(Self {
            num: self.num.checked_abs()?,
            den: self.den,
        })
    }

    #[must_use]
    pub fn floor(self) -> Self {
        // den > 0, so div_euclid rounds towards negative infinity and cannot overflow.
        Self::integer(self.num.div_euclid(self.den))
    }

    #[must_use]
    pub fn checked_ceil(self) -> Option<Self> {
        let (n, d) = self.parts();
        Self::from_i128(-(-n).div_euclid(d), 1)
    }

    /// Rounds to the nearest integer, halves away from zero (`2.5 → 3`, `-2.5 → -3`).
    #[must_use]
    pub fn checked_round(self) -> Option<Self> {
        let (n, d) = self.parts();
        let magnitude = (2 * n.abs() + d) / (2 * d);
        Self::from_i128(if n < 0 { -magnitude } else { magnitude }, 1)
    }
}

impl Ord for Rational {
    fn cmp(&self, other: &Self) -> Ordering {
        let ((a, b), (c, d)) = (self.parts(), other.parts());
        (a * d).cmp(&(c * b))
    }
}

impl PartialOrd for Rational {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl From<i64> for Rational {
    fn from(n: i64) -> Self {
        Self::integer(n)
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
    }
}

/// The type of a [`Value`], for error messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueType {
    Number,
    Bool,
}

impl fmt::Display for ValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Number => "number",
            Self::Bool => "boolean",
        })
    }
}

/// Result of an expression and value of a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Value {
    Number(Rational),
    Bool(bool),
}

impl Value {
    #[must_use]
    pub const fn value_type(self) -> ValueType {
        match self {
            Self::Number(_) => ValueType::Number,
            Self::Bool(_) => ValueType::Bool,
        }
    }

    /// The value as an integer.
    ///
    /// # Errors
    /// [`ExprError::NotAnInteger`] for fractions and booleans.
    pub fn as_integer(self) -> Result<i64, ExprError> {
        match self {
            Self::Number(r) => r.to_integer(),
            Self::Bool(_) => None,
        }
        .ok_or(ExprError::NotAnInteger { value: self })
    }

    #[must_use]
    pub const fn as_bool(self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(b),
            Self::Number(_) => None,
        }
    }

    #[must_use]
    pub const fn as_number(self) -> Option<Rational> {
        match self {
            Self::Number(r) => Some(r),
            Self::Bool(_) => None,
        }
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Self::Number(Rational::integer(n))
    }
}

impl From<Rational> for Value {
    fn from(r: Rational) -> Self {
        Self::Number(r)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(r) => r.fmt(f),
            Self::Bool(b) => b.fmt(f),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // test code

    use super::*;

    fn r(n: i64, d: i64) -> Rational {
        Rational::new(n, d).unwrap()
    }

    #[test]
    fn normalizes() {
        assert_eq!(r(4, -6), r(-2, 3));
        assert_eq!(r(-2, 3).numer(), -2);
        assert_eq!(r(-2, 3).denom(), 3);
        assert_eq!(r(0, -5), Rational::ZERO);
        assert_eq!(Rational::new(1, 0), None);
        assert_eq!(Rational::new(i64::MIN, -1), None);
        assert_eq!(r(i64::MIN, 2), Rational::integer(i64::MIN / 2));
    }

    #[test]
    fn rounding() {
        let cases = [
            // (num, den, floor, ceil, round)
            (5, 2, 2, 3, 3),
            (-5, 2, -3, -2, -3),
            (7, 3, 2, 3, 2),
            (-7, 3, -3, -2, -2),
            (5, 3, 1, 2, 2),
            (4, 1, 4, 4, 4),
            (-1, 2, -1, 0, -1),
            (1, 2, 0, 1, 1),
        ];
        for (n, d, fl, ce, ro) in cases {
            let x = r(n, d);
            assert_eq!(x.floor(), Rational::integer(fl), "floor {x}");
            assert_eq!(x.checked_ceil(), Some(Rational::integer(ce)), "ceil {x}");
            assert_eq!(x.checked_round(), Some(Rational::integer(ro)), "round {x}");
        }
        let min = Rational::integer(i64::MIN);
        assert_eq!(min.floor(), min);
        assert_eq!(min.checked_ceil(), Some(min));
        assert_eq!(min.checked_round(), Some(min));
        assert_eq!(min.checked_abs(), None);
        assert_eq!(min.checked_neg(), None);
    }

    #[test]
    fn floored_remainder() {
        let rem = |a: i64, b: i64| Rational::integer(a).checked_rem(Rational::integer(b));
        assert_eq!(rem(7, 3), Some(Rational::integer(1)));
        assert_eq!(rem(-7, 3), Some(Rational::integer(2)));
        assert_eq!(rem(7, -3), Some(Rational::integer(-2)));
        assert_eq!(rem(-7, -3), Some(Rational::integer(-1)));
        assert_eq!(rem(i64::MIN, -1), Some(Rational::ZERO));
        assert_eq!(rem(1, 0), None);
        assert_eq!(r(1, 2).checked_rem(Rational::integer(1)), None);
    }

    #[test]
    fn overflow_is_none() {
        let max = Rational::integer(i64::MAX);
        assert_eq!(max.checked_add(Rational::integer(1)), None);
        assert_eq!(max.checked_mul(Rational::integer(2)), None);
        assert_eq!(r(1, i64::MAX).checked_mul(r(1, 2)), None);
        assert_eq!(
            max.checked_sub(max).unwrap().checked_div(Rational::ZERO),
            None
        );
        assert_eq!(r(1, 3).checked_add(r(1, 6)), Some(r(1, 2)));
    }

    #[test]
    fn ordering() {
        assert!(r(1, 3) < r(1, 2));
        assert!(r(-1, 2) < r(-1, 3));
        assert!(Rational::integer(i64::MIN) < r(i64::MAX, i64::MAX - 1));
    }
}
