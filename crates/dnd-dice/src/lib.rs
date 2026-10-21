//! Dice expression parser and roll engine (P0 notation of ADR-0005).
//!
//! ```
//! use dnd_dice::{Expr, NoFields, SplitMix64, roll};
//!
//! let expr = Expr::parse("4d6kh3 + 2")?;
//! assert_eq!(expr.to_string(), "4d6kh3+2");
//! let result = roll(&expr, &NoFields, &mut SplitMix64::new(42))?;
//! assert!((5..=20).contains(&result.total));
//! # Ok::<(), dnd_dice::DiceError>(())
//! ```
//!
//! See `README.md` for the notation, limits and dependency boundaries.

mod ast;
mod error;
mod fields;
mod parse;
mod rng;
mod roll;

pub mod limits {
    //! Hard limits; every [`crate::Expr`] satisfies them.

    /// Maximum expression length in characters.
    pub const MAX_EXPR_LEN: usize = 256;
    /// Maximum number of dice rolled by one expression (advantage counts as 2).
    pub const MAX_DICE: u32 = 1000;
    /// Maximum number of sides of a die.
    pub const MAX_SIDES: u32 = 1_000_000;
    /// Largest integer exactly representable in a JS number (2^53 − 1); totals stay within ±this.
    pub const MAX_SAFE_INTEGER: i64 = (1 << 53) - 1;
}

pub use ast::{Dice, Expr, Modifier, Sides, Sign, SignedTerm, Term};
pub use error::DiceError;
pub use fields::{FieldLookup, LookupError, NoFields};
pub use rng::{DiceRng, SplitMix64, uniform};
pub use roll::{Die, Roll, TermDetail, TermRoll, roll};

impl Expr {
    /// Shorthand for [`roll()`].
    ///
    /// # Errors
    /// See [`roll()`].
    pub fn roll<L, R>(&self, fields: &L, rng: &mut R) -> Result<Roll, DiceError>
    where
        L: FieldLookup + ?Sized,
        R: DiceRng + ?Sized,
    {
        roll::roll(self, fields, rng)
    }
}
