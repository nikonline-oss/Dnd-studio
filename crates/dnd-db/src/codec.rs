//! Conversions between protocol types and column values.

use serde::Serialize;
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::error::{DbError, DbResult};

/// Unit-like enums are stored as their serde (snake_case) name, so the database and the wire
/// format never drift apart.
pub fn enum_to_str<T: Serialize>(value: &T) -> DbResult<String> {
    match serde_json::to_value(value)? {
        serde_json::Value::String(s) => Ok(s),
        other => Err(DbError::InvalidData {
            column: "enum",
            details: format!("not a unit enum: {other}"),
        }),
    }
}

pub fn enum_from_str<T: DeserializeOwned>(column: &'static str, value: &str) -> DbResult<T> {
    serde_json::from_value(serde_json::Value::String(value.to_owned())).map_err(|e| {
        DbError::InvalidData {
            column,
            details: format!("{value:?}: {e}"),
        }
    })
}

pub fn uuid_from_str(column: &'static str, value: &str) -> DbResult<Uuid> {
    Uuid::parse_str(value).map_err(|e| DbError::InvalidData {
        column,
        details: format!("{value:?}: {e}"),
    })
}

pub fn opt_uuid_from_str(column: &'static str, value: Option<String>) -> DbResult<Option<Uuid>> {
    value.map(|v| uuid_from_str(column, &v)).transpose()
}
