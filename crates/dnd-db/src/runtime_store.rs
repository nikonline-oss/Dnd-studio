//! The saved rules process in `plugin_runtime.dto` (ADR-0051).
//!
//! The payload is the state of a possibly half-finished action: saves awaited from targets, a
//! declared action waiting on a reaction, maintained effects. Its shape therefore belongs to the
//! application version that wrote it, not to the campaign. Stored JSON carries an explicit
//! version; a campaign written by an older version is read as it is (every field of
//! [`PluginRuntime`] has a default), and a campaign written by a newer one is refused with a
//! clear error instead of being silently replaced by an empty runtime.

use dnd_protocol::PluginRuntime;
use serde::{Deserialize, Serialize};

use crate::error::DbError;

/// Version of the stored runtime written by this build. Bump it on any incompatible change of
/// [`PluginRuntime`] and add a reading path for the previous one.
pub const RUNTIME_VERSION: u32 = 1;

/// The database column, for error messages.
const COLUMN: &str = "plugin_runtime.dto";

#[derive(Serialize, Deserialize)]
struct Stored {
    version: u32,
    runtime: PluginRuntime,
}

/// Wraps the runtime for storage.
pub fn encode(runtime: &PluginRuntime) -> Result<String, DbError> {
    Ok(serde_json::to_string(&Stored {
        version: RUNTIME_VERSION,
        runtime: runtime.clone(),
    })?)
}

/// Reads the runtime written by this or an older build.
///
/// - a bare [`PluginRuntime`] is the pre-versioning format: it is accepted as it is and rewritten
///   with a version on the next save;
/// - a version this build does not know is [`DbError::RuntimeTooNew`];
/// - broken JSON is [`DbError::InvalidData`], never an empty runtime: losing a pending action
///   silently would change the game.
pub fn decode(json: &str) -> Result<PluginRuntime, DbError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|error| DbError::InvalidData {
            column: COLUMN,
            details: error.to_string(),
        })?;
    let Some(version) = value.get("version") else {
        return serde_json::from_value(value).map_err(|error| DbError::InvalidData {
            column: COLUMN,
            details: format!("unversioned runtime: {error}"),
        });
    };
    let version = version
        .as_u64()
        .and_then(|version| u32::try_from(version).ok())
        .ok_or_else(|| DbError::InvalidData {
            column: COLUMN,
            details: format!("runtime version is not a number: {version}"),
        })?;
    if version > RUNTIME_VERSION {
        return Err(DbError::RuntimeTooNew {
            version,
            supported: RUNTIME_VERSION,
        });
    }
    let runtime = value.get("runtime").ok_or_else(|| DbError::InvalidData {
        column: COLUMN,
        details: format!("runtime version {version} has no payload"),
    })?;
    serde_json::from_value(runtime.clone()).map_err(|error| DbError::InvalidData {
        column: COLUMN,
        details: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use dnd_protocol::{IntentId, MovementMode, PendingMovement, TokenId};

    fn pending() -> PluginRuntime {
        PluginRuntime {
            movements: vec![PendingMovement {
                id: IntentId::new_v7(),
                token: TokenId::new_v7(),
                destination_x: 32,
                destination_y: 64,
                mode: MovementMode::Voluntary,
                offers: Vec::new(),
                handled: Vec::new(),
            }],
            ..PluginRuntime::default()
        }
    }

    #[test]
    fn a_round_trip_keeps_a_pending_action() {
        let runtime = pending();
        assert_eq!(decode(&encode(&runtime).unwrap()).unwrap(), runtime);
    }

    #[test]
    fn an_unversioned_runtime_still_opens() {
        let runtime = pending();
        let legacy = serde_json::to_string(&runtime).unwrap();
        assert_eq!(decode(&legacy).unwrap(), runtime);
    }

    #[test]
    fn a_newer_runtime_is_refused_rather_than_emptied() {
        let newer = format!(
            "{{\"version\": {}, \"runtime\": {}}}",
            RUNTIME_VERSION + 1,
            serde_json::to_string(&pending()).unwrap()
        );
        assert!(matches!(
            decode(&newer),
            Err(DbError::RuntimeTooNew { version, supported })
                if version == RUNTIME_VERSION + 1 && supported == RUNTIME_VERSION
        ));
    }

    #[test]
    fn broken_json_is_an_error_not_an_empty_runtime() {
        assert!(matches!(
            decode("{not json"),
            Err(DbError::InvalidData { .. })
        ));
        assert!(matches!(
            decode("{\"version\": 1}"),
            Err(DbError::InvalidData { .. })
        ));
        assert!(matches!(
            decode("{\"version\": \"one\", \"runtime\": {}}"),
            Err(DbError::InvalidData { .. })
        ));
    }
}
