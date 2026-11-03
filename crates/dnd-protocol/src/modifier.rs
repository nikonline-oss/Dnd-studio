//! System-neutral condition consequences. Names and game-specific semantics belong to plugins.
use crate::Label;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum RuleCapability {
    Action,
    BonusAction,
    Reaction,
    Movement,
    Provoke,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum RollKind {
    Attack,
    Save,
    Check,
    Initiative,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum RollMode {
    Advantage,
    Disadvantage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum RuleSubject {
    Actor,
    Target,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ConditionRule {
    pub id: String,
    pub label: Label,
    pub condition: String,
    pub subject: RuleSubject,
    pub feature: Option<String>,
    /// Evaluated against unmodified fields. @actor.condition.ID and @target.condition.ID are booleans.
    #[serde(default)]
    pub when: String,
    pub effects: Vec<ConditionModifier>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ConditionModifier {
    Imply {
        condition: String,
    },
    Deny {
        capability: RuleCapability,
    },
    Roll {
        kind: RollKind,
        #[serde(default)]
        tags: Vec<String>,
        mode: RollMode,
    },
    /// Additive modifiers are summed, then lower bounds and finally upper bounds apply.
    Parameter {
        path: String,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        add: i64,
        #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Number>))]
        minimum: Option<i64>,
        #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Number>))]
        maximum: Option<i64>,
    },
}
