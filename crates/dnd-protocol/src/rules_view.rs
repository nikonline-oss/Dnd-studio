//! Declarative presentation and authoritative derived rule views.
use crate::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LifeRules {
    #[serde(default)]
    pub dead: String,
    #[serde(default)]
    pub npc_dead: String,
    #[serde(default)]
    pub unconscious: String,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum LifeStatus {
    #[default]
    Alive,
    Unconscious,
    Dead,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ActionGroup {
    pub id: String,
    pub label: Label,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CharacterRuleView {
    pub character: CharacterId,
    pub life: LifeStatus,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ActionAvailability {
    #[serde(default)]
    pub sheet_action: Option<SheetActionView>,
    pub id: String,
    pub enabled: bool,
    pub reason: Option<String>,
    pub range_cells: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SheetActionView {
    pub collection: String,
    pub index: u16,
    pub action: PluginAction,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TokenActionView {
    pub token: TokenId,
    pub actions: Vec<ActionAvailability>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RulesView {
    #[serde(default)]
    pub characters: Vec<CharacterRuleView>,
    #[serde(default)]
    pub tokens: Vec<TokenActionView>,
    #[serde(default)]
    pub calendar: Option<CalendarView>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CalendarMonth {
    pub label: Label,
    pub days: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DayPeriod {
    pub label: Label,
    pub start_second: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CalendarDef {
    pub epoch_year: u32,
    pub day_seconds: u32,
    pub round_seconds: u32,
    pub months: Vec<CalendarMonth>,
    pub weekdays: Vec<Label>,
    pub periods: Vec<DayPeriod>,
    #[serde(default)]
    pub leap_interval: u16,
    #[serde(default)]
    pub leap_except: u16,
    #[serde(default)]
    pub leap_include: u16,
    #[serde(default)]
    pub leap_month: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CalendarView {
    #[cfg_attr(feature="specta", specta(type=specta_typescript::Number))]
    pub year: i64,
    pub month: Label,
    pub month_index: u16,
    pub day: u16,
    pub weekday: Label,
    pub hour: u16,
    pub minute: u8,
    pub second: u8,
    pub period: Label,
    #[cfg_attr(feature="specta", specta(type=specta_typescript::Number))]
    pub elapsed_days: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum CalendarUnit {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
    Round,
}
