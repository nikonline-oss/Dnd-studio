//! Field references (`@str`, `@hp.max`) are resolved by the caller through a [`Context`].

use std::collections::{BTreeMap, HashMap};

use crate::value::Value;

/// Why a field reference could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LookupError {
    #[error("unknown field")]
    Unknown,
    #[error("{0}")]
    Other(String),
}

/// Resolves a dotted field path (without the leading `@`) to a value.
pub trait Context {
    /// # Errors
    /// [`LookupError::Unknown`] when there is no such field; anything else as [`LookupError::Other`].
    fn lookup(&self, path: &str) -> Result<Value, LookupError>;
}

/// A context with no fields: every reference is [`LookupError::Unknown`].
#[derive(Debug, Clone, Copy, Default)]
pub struct NoContext;

impl Context for NoContext {
    fn lookup(&self, _path: &str) -> Result<Value, LookupError> {
        Err(LookupError::Unknown)
    }
}

impl Context for BTreeMap<String, Value> {
    fn lookup(&self, path: &str) -> Result<Value, LookupError> {
        self.get(path).copied().ok_or(LookupError::Unknown)
    }
}

impl Context for BTreeMap<String, i64> {
    fn lookup(&self, path: &str) -> Result<Value, LookupError> {
        self.get(path)
            .map(|&n| Value::from(n))
            .ok_or(LookupError::Unknown)
    }
}

impl<S: std::hash::BuildHasher> Context for HashMap<String, Value, S> {
    fn lookup(&self, path: &str) -> Result<Value, LookupError> {
        self.get(path).copied().ok_or(LookupError::Unknown)
    }
}

impl<S: std::hash::BuildHasher> Context for HashMap<String, i64, S> {
    fn lookup(&self, path: &str) -> Result<Value, LookupError> {
        self.get(path)
            .map(|&n| Value::from(n))
            .ok_or(LookupError::Unknown)
    }
}

impl<C: Context + ?Sized> Context for &C {
    fn lookup(&self, path: &str) -> Result<Value, LookupError> {
        (**self).lookup(path)
    }
}
