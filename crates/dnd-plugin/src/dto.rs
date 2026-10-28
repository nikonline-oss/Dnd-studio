//! What the game core receives about an installed plugin (the types live in `dnd-protocol`).
//! Plugins are stored inside the campaign, addressed by the package's SHA-256; a system plugin's
//! rules become game state, so the session server gets them with the campaign.

pub use dnd_protocol::{PluginDto as InstalledPluginDto, SystemRulesDto};
use serde_json::Value;

use crate::package::{PluginPackage, SystemPackage};

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
            trigger_templates: self.trigger_templates.clone(),
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
            conditions: r.conditions.clone(),
            inventory: r.inventory.clone(),
        }
    }
}
