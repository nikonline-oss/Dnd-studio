//! Installed plugins as game state (ADR-0023). Plugins live inside the campaign: the package is
//! kept by its SHA-256, and a system plugin's rules travel with the state, so the session server
//! and every client apply the same rules.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Largest installed plugin description (rules, schema and layout as JSON), in bytes.
pub const MAX_PLUGIN_BYTES: usize = 512 * 1024;

/// A user-visible text: one string for every language, or per language.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Label {
    Text(String),
    Localized(LocalizedText),
}

/// At least one of the languages is present.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LocalizedText {
    #[serde(default)]
    pub ru: Option<String>,
    #[serde(default)]
    pub en: Option<String>,
}

impl Label {
    /// The text for `lang` (`"ru"`, `"en"`), falling back to English, then Russian.
    #[must_use]
    pub fn get(&self, lang: &str) -> &str {
        match self {
            Self::Text(text) => text,
            Self::Localized(l) => {
                let wanted = match lang {
                    "ru" => l.ru.as_deref(),
                    "en" => l.en.as_deref(),
                    _ => None,
                };
                wanted.or(l.en.as_deref()).or(l.ru.as_deref()).unwrap_or("")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PluginKind {
    /// Rules of the game: exactly one per campaign (ADR-0005).
    System,
    Content,
    Theme,
    Ui,
}

impl PluginKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Content => "content",
            Self::Theme => "theme",
            Self::Ui => "ui",
        }
    }
}

/// A generalized hit-point-like resource. `value` and `max` are expressions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ResourceDef {
    pub id: String,
    pub label: Label,
    pub value: String,
    pub max: String,
}

/// A condition of the system (poisoned, prone, …), shown on sheets and tokens (ADR-0024).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ConditionDef {
    pub id: String,
    pub label: Label,
}

/// Where a character's items live in the sheet: a list of objects at `path` whose `name_field`
/// is the item's name (ADR-0024).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InventoryDef {
    pub path: String,
    pub name_field: String,
}

/// A roll macro shown on the sheet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RollDef {
    pub id: String,
    pub label: Label,
    pub dice: String,
}

/// What a template parameter asks for (the GM fills it in when creating the trigger).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum TemplateParamKind {
    Zone,
    BoardNode,
    Board,
    Map,
    Journal,
    Token,
    /// A roll of the system.
    Roll,
    /// A resource of the system.
    Resource,
    /// A condition of the system.
    Condition,
    Number,
    Text,
}

/// One blank of a trigger template.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TemplateParam {
    pub id: String,
    pub label: Label,
    pub kind: TemplateParamKind,
}

/// A ready-made trigger a plugin ships (`triggers/<name>.json`, ADR-0024): a trigger definition
/// whose strings may contain `{{param}}` blanks. Data only; the trigger made from it is checked
/// by the core like any other.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TriggerTemplateDto {
    pub id: String,
    pub name: Label,
    pub description: Option<Label>,
    pub params: Vec<TemplateParam>,
    /// A `TriggerDef` with blanks (so not typed until they are filled).
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Unknown))]
    pub definition: Value,
}

/// Most trigger templates in one plugin.
pub const MAX_TRIGGER_TEMPLATES: usize = 64;

/// A plugin installed in the campaign.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PluginDto {
    pub id: String,
    /// Semver, as in the manifest.
    pub version: String,
    pub kind: PluginKind,
    pub name: Label,
    pub license: String,
    pub attribution: Option<String>,
    /// Lowercase hex SHA-256 of the `.dndplugin` file (kept in the campaign's `plugins/`).
    pub archive_sha256: String,
    /// Present exactly for `kind: system`.
    pub system: Option<SystemRulesDto>,
    /// Trigger templates (ADR-0024); plugins installed before they existed have none.
    #[serde(default)]
    pub trigger_templates: Vec<TriggerTemplateDto>,
}

/// The rules of a system plugin, self-contained (no package paths).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SystemRulesDto {
    /// Sheet data schema (JSON Schema subset, see `sheet_schema`).
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Unknown))]
    pub sheet_schema: Value,
    /// Root layout node (rendered by the clients; the core only stores it).
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Unknown))]
    pub sheet_layout: Value,
    /// Optional NPC/creature form over the same validated system data.
    #[serde(default)]
    #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Unknown>))]
    pub creature_sheet_layout: Option<Value>,
    /// Name → expression; results are integers.
    pub computed: BTreeMap<String, String>,
    /// Dice expression for initiative.
    pub initiative: String,
    pub resources: Vec<ResourceDef>,
    /// Strictly descending percents: Healthy / Wounded / Critical / Dead boundaries.
    pub health_thresholds: Vec<u8>,
    pub rolls: Vec<RollDef>,
    /// Conditions characters may have; rules installed before they existed have none.
    #[serde(default)]
    pub conditions: Vec<ConditionDef>,
    /// The sheet list of items, if the system has one.
    #[serde(default)]
    pub inventory: Option<InventoryDef>,
}
