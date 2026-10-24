//! Embedded meta-schemas (`schemas/*.json`), compiled once.
//!
//! Draft 2020-12, local `$ref`s only: the `jsonschema` crate is built without remote or file
//! resolution, so a document can never make the validator fetch anything.

use std::sync::LazyLock;

use jsonschema::Validator;
use serde_json::Value;

use crate::error::PluginError;

/// How many schema violations are reported for one document.
const MAX_REPORTED: usize = 5;

#[derive(Clone, Copy, Debug)]
pub(crate) enum MetaSchema {
    Manifest,
    System,
    SheetSchema,
    Layout,
    Compendium,
}

type Compiled = Result<Validator, String>;

static MANIFEST: LazyLock<Compiled> =
    LazyLock::new(|| compile(include_str!("../schemas/manifest.schema.json")));
static SYSTEM: LazyLock<Compiled> =
    LazyLock::new(|| compile(include_str!("../schemas/system.schema.json")));
static SHEET_SCHEMA: LazyLock<Compiled> =
    LazyLock::new(|| compile(include_str!("../schemas/sheet-schema.schema.json")));
static LAYOUT: LazyLock<Compiled> =
    LazyLock::new(|| compile(include_str!("../schemas/layout.schema.json")));
static COMPENDIUM: LazyLock<Compiled> =
    LazyLock::new(|| compile(include_str!("../schemas/compendium.schema.json")));

fn compile(source: &str) -> Compiled {
    let schema: Value = serde_json::from_str(source).map_err(|e| e.to_string())?;
    jsonschema::draft202012::new(&schema).map_err(|e| e.to_string())
}

impl MetaSchema {
    fn validator(self) -> &'static Compiled {
        match self {
            Self::Manifest => &MANIFEST,
            Self::System => &SYSTEM,
            Self::SheetSchema => &SHEET_SCHEMA,
            Self::Layout => &LAYOUT,
            Self::Compendium => &COMPENDIUM,
        }
    }

    /// `Ok(Err(text))` lists the violations (`/json/pointer: message; ...`); the outer error is a
    /// meta-schema that does not compile, which is a bug in this crate.
    pub(crate) fn check(self, document: &Value) -> Result<Result<(), String>, PluginError> {
        let validator = self
            .validator()
            .as_ref()
            .map_err(|reason| PluginError::Internal {
                reason: format!("meta-schema {self:?}: {reason}"),
            })?;
        let errors: Vec<String> = validator
            .iter_errors(document)
            .take(MAX_REPORTED)
            .map(|e| {
                let at = e.instance_path().as_str();
                format!("{}: {e}", if at.is_empty() { "/" } else { at })
            })
            .collect();
        Ok(if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_meta_schemas_compile() {
        for schema in [
            MetaSchema::Manifest,
            MetaSchema::System,
            MetaSchema::SheetSchema,
            MetaSchema::Layout,
            MetaSchema::Compendium,
        ] {
            assert!(
                schema.validator().is_ok(),
                "{schema:?}: {:?}",
                schema.validator().as_ref().err()
            );
        }
    }
}
