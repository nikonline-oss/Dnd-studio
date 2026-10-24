//! What the game core receives about an installed plugin (wave 2 moves these into
//! `dnd-protocol`). Plugins are stored inside the campaign, addressed by the package's SHA-256;
//! a system plugin's rules become game state, so the session server gets them with the campaign.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{Label, PluginKind, ResourceDef, RollDef};
use crate::package::{PluginPackage, SystemPackage};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InstalledPluginDto {
    pub id: String,
    /// Semver, as in the manifest.
    pub version: String,
    pub kind: PluginKind,
    pub name: Label,
    pub license: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    /// Lowercase hex SHA-256 of the `.dndplugin` file.
    pub archive_sha256: String,
    /// Present exactly for `kind: system`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemRulesDto>,
}

/// The rules of a system plugin, self-contained (no package paths).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SystemRulesDto {
    /// Sheet data schema (JSON Schema subset, docs/plugin-format.md).
    pub sheet_schema: Value,
    /// Root layout node.
    pub sheet_layout: Value,
    pub computed: BTreeMap<String, String>,
    pub initiative: String,
    pub resources: Vec<ResourceDef>,
    pub health_thresholds: Vec<u8>,
    pub rolls: Vec<RollDef>,
}

impl PluginPackage {
    pub fn to_dto(&self) -> InstalledPluginDto {
        let m = &self.manifest;
        InstalledPluginDto {
            id: m.id.clone(),
            version: m.version.to_string(),
            kind: m.kind,
            name: m.name.clone(),
            license: m.license.clone(),
            attribution: m.attribution.clone(),
            archive_sha256: self.sha256.clone(),
            system: self.system.as_ref().map(SystemPackage::to_dto),
        }
    }
}

impl SystemPackage {
    pub fn to_dto(&self) -> SystemRulesDto {
        let r = &self.rules;
        SystemRulesDto {
            sheet_schema: self.sheet_schema.clone(),
            // A layout node always serializes; `Null` would only hide a bug the core then rejects.
            sheet_layout: serde_json::to_value(&self.sheet_layout).unwrap_or(Value::Null),
            computed: r.computed.clone(),
            initiative: r.initiative.clone(),
            resources: r.resources.clone(),
            health_thresholds: r.health_thresholds.clone(),
            rolls: r.rolls.clone(),
        }
    }
}
