//! Typed documents of a `.dndplugin` package (docs/plugin-format.md).
//!
//! Every document is first checked against its embedded meta-schema (`schemas/`), then
//! deserialized into these types, so the types never see anything the format does not allow.

use std::collections::BTreeMap;

pub use dnd_protocol::{
    ConditionDef, InventoryDef, Label, LocalizedText, PluginKind, ResourceDef, RollDef,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

/// Version of the package format this crate reads (`manifest.json` → `format`).
pub const PLUGIN_FORMAT: u32 = 1;

/// Default qualitative health thresholds, in percent (CLAUDE.md §5).
pub const DEFAULT_HEALTH_THRESHOLDS: [u8; 3] = [75, 25, 0];

/// `manifest.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub id: String,
    pub name: Label,
    pub version: Version,
    pub kind: PluginKind,
    pub authors: Vec<String>,
    /// SPDX expression (not checked against the SPDX list).
    pub license: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    /// Range of app versions the plugin works with, e.g. `^0.1`.
    pub app_compat: VersionReq,
    /// Plugin id → version range.
    #[serde(default)]
    pub dependencies: BTreeMap<String, VersionReq>,
}

/// `system.json`: the rules a system plugin brings into the campaign.
///
/// Expressions (`computed`, resource `value`/`max`) and dice (`initiative`, rolls) are kept as
/// written; their syntax is checked by the game core with `dnd-expr`/`dnd-dice`, not here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemRules {
    pub sheet: SheetRef,
    /// Optional NPC presentation of the same sheet schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creature_sheet_layout: Option<String>,
    /// Name → expression. Results must be integers (the core checks).
    #[serde(default)]
    pub computed: BTreeMap<String, String>,
    /// Dice expression for initiative, e.g. `1d20 + @dex_mod`.
    pub initiative: String,
    #[serde(default)]
    pub resources: Vec<ResourceDef>,
    /// Strictly descending percents: the boundaries between Healthy / Wounded / Critical / Dead.
    #[serde(default = "default_health_thresholds")]
    pub health_thresholds: Vec<u8>,
    #[serde(default)]
    pub rolls: Vec<RollDef>,
    /// Conditions characters may have (ADR-0024).
    #[serde(default)]
    pub conditions: Vec<ConditionDef>,
    /// The sheet list of items.
    #[serde(default)]
    pub inventory: Option<InventoryDef>,
}

fn default_health_thresholds() -> Vec<u8> {
    DEFAULT_HEALTH_THRESHOLDS.to_vec()
}

/// Paths of the character sheet files inside the package (both under `sheet/`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SheetRef {
    pub schema: String,
    pub layout: String,
}

/// A node of the declarative sheet layout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayoutNode {
    Tabs {
        tabs: Vec<LayoutTab>,
    },
    Section {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<Label>,
        children: Vec<LayoutNode>,
    },
    Row {
        children: Vec<LayoutNode>,
    },
    Grid {
        columns: u8,
        children: Vec<LayoutNode>,
    },
    /// An editable field; `path` is dotted, relative to the sheet data (or to the list item).
    Field {
        path: String,
        label: Label,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        widget: Option<Widget>,
    },
    Computed {
        name: String,
        label: Label,
    },
    Roll {
        roll: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<Label>,
    },
    Resource {
        resource: String,
    },
    /// An array of objects; `item` nodes address fields relative to one element.
    List {
        path: String,
        label: Label,
        item: Vec<LayoutNode>,
    },
    Text {
        text: Label,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutTab {
    pub label: Label,
    pub children: Vec<LayoutNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Widget {
    Number,
    Text,
    Checkbox,
    Select,
    Textarea,
}

/// `compendia/<name>.json`. Parsed and validated here, shown in stage 3b.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Compendium {
    pub id: String,
    pub name: Label,
    pub entry_type: String,
    pub entries: Vec<CompendiumEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompendiumEntry {
    pub id: String,
    pub name: Label,
    pub data: serde_json::Map<String, serde_json::Value>,
}
