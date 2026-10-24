//! Field references (`@str_mod`, `@hp.max`) are resolved by the caller.

use std::collections::{BTreeMap, HashMap};

/// Why a field reference could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LookupError {
    #[error("unknown field")]
    Unknown,
    #[error("field is not an integer")]
    NotAnInteger,
    #[error("{0}")]
    Other(String),
}

/// Resolves a dotted field path (without the leading `@`) to an integer.
pub trait FieldLookup {
    fn lookup(&self, path: &str) -> Result<i64, LookupError>;
}

/// A lookup with no fields: every reference is [`LookupError::Unknown`].
#[derive(Debug, Clone, Copy, Default)]
pub struct NoFields;

impl FieldLookup for NoFields {
    fn lookup(&self, _path: &str) -> Result<i64, LookupError> {
        Err(LookupError::Unknown)
    }
}

impl FieldLookup for BTreeMap<String, i64> {
    fn lookup(&self, path: &str) -> Result<i64, LookupError> {
        self.get(path).copied().ok_or(LookupError::Unknown)
    }
}

impl<S: std::hash::BuildHasher> FieldLookup for HashMap<String, i64, S> {
    fn lookup(&self, path: &str) -> Result<i64, LookupError> {
        self.get(path).copied().ok_or(LookupError::Unknown)
    }
}

impl<L: FieldLookup + ?Sized> FieldLookup for &L {
    fn lookup(&self, path: &str) -> Result<i64, LookupError> {
        (**self).lookup(path)
    }
}

/// Serves a `dnd_expr` context (a sheet, or computed fields over it via
/// `dnd_expr::Computed::with_sheet`) to dice expressions. Non-integer values are
/// [`LookupError::NotAnInteger`].
#[derive(Debug, Clone, Copy)]
pub struct ExprFields<C>(pub C);

impl<C: dnd_expr::Context> FieldLookup for ExprFields<C> {
    fn lookup(&self, path: &str) -> Result<i64, LookupError> {
        match self.0.lookup(path) {
            Ok(value) => value.as_integer().map_err(|_| LookupError::NotAnInteger),
            Err(dnd_expr::LookupError::Unknown) => Err(LookupError::Unknown),
            Err(dnd_expr::LookupError::Other(message)) => Err(LookupError::Other(message)),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // test code

    use super::*;
    use dnd_expr::{Computed, Rational, Value};

    #[test]
    fn expr_fields_serve_computed_values_over_the_sheet() {
        let sheet = BTreeMap::from([
            ("str".to_owned(), Value::from(16)),
            ("half".to_owned(), Value::from(Rational::new(1, 2).unwrap())),
        ]);
        let defs = BTreeMap::from([("str_mod".to_owned(), "floor((@str - 10) / 2)".to_owned())]);
        let computed = Computed::resolve(&defs, &sheet).unwrap();
        let fields = ExprFields(computed.with_sheet(&sheet));
        assert_eq!(fields.lookup("str_mod"), Ok(3));
        assert_eq!(fields.lookup("str"), Ok(16));
        assert_eq!(fields.lookup("half"), Err(LookupError::NotAnInteger));
        assert_eq!(fields.lookup("dex"), Err(LookupError::Unknown));
        let roll = crate::Expr::parse("d1 + @str_mod")
            .unwrap()
            .roll(&fields, &mut crate::SplitMix64::new(1))
            .unwrap();
        assert_eq!(roll.total, 4);
    }
}
