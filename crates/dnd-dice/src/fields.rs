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
