//! Declarative, system-neutral actions and movement hooks. No script execution.
use crate::{Label, TokenId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PluginMechanics {
    #[serde(default)]
    pub calendar: Option<crate::CalendarDef>,
    #[serde(default)]
    pub life: crate::LifeRules,
    #[serde(default)]
    pub action_groups: Vec<crate::ActionGroup>,
    #[serde(default)]
    pub sheet_actions: Vec<SheetActionCollection>,
    #[serde(default)]
    pub item_types: Vec<crate::ItemTypeDef>,
    #[serde(default)]
    pub turn_budgets: Vec<TurnBudget>,
    #[serde(default)]
    pub interruption_rules: Vec<InterruptionRule>,
    #[serde(default)]
    pub maintained_rules: Vec<MaintainedRule>,
    #[serde(default)]
    pub damage_profiles: Vec<DamageProfile>,
    #[serde(default)]
    pub gm_panel: Vec<GmPanelSection>,
    #[serde(default)]
    pub condition_rules: Vec<crate::ConditionRule>,
    #[serde(default)]
    pub movement_condition: String,
    #[serde(default)]
    pub actions: Vec<PluginAction>,
    #[serde(default)]
    pub movement_rules: Vec<MovementRule>,
    #[serde(default)]
    pub turn_resources: Vec<String>,
}
/// A bounded sheet list whose rows instantiate trusted plugin actions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SheetActionCollection {
    pub id: String,
    pub path: String,
    pub name_field: String,
    /// Dice formula; @row.* references the current list row.
    pub attack_dice: Option<String>,
    pub action_field: Option<String>,
    pub actions: Vec<String>,
    #[serde(default)]
    pub bindings: std::collections::BTreeMap<SheetActionField, String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum SheetActionField {
    DamageDice,
    RangeCells,
    RadiusCells,
    DamageType,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PluginAction {
    #[serde(default)]
    pub save_prompt: bool,
    #[serde(default)]
    pub save_failure: String,
    #[serde(default)]
    pub sheet_group: Option<String>,
    #[serde(default)]
    pub spatial: SpatialRequirements,
    #[serde(default)]
    pub timing: Option<ActionTiming>,
    #[serde(default)]
    pub interrupt: Option<InterruptEffect>,
    #[serde(default)]
    pub maintain: Option<MaintainedAction>,
    /// Tags label the roll/action for declarative selectors; they are not game logic.
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub activation: Option<crate::RuleCapability>,
    #[serde(default)]
    pub target_self: bool,
    /// Self-applied condition lasting until the current turn ends. Requires one's own active turn.
    #[serde(default)]
    pub turn_condition: Option<String>,
    /// Inspector rows the turn condition opens while it lasts (ADR-0061).
    #[serde(default)]
    pub turn_reveal: Vec<crate::model::TokenField>,
    pub id: String,
    pub label: Label,
    pub feature: Option<String>,
    /// Actor eligibility, using actor sheet fields.
    pub condition: String,
    /// Maximum distance in grid cells; zero means self-centered area.
    pub range_cells: String,
    pub distance_metric: DistanceMetric,
    pub reaction_only: bool,
    pub critical_if: Option<String>,
    pub critical_dice: Option<String>,
    /// Optional circular area around the selected point.
    pub radius_cells: Option<u16>,
    pub attack_dice: Option<String>,
    /// Checked against the target's fields, with @total from attack/save.
    pub hit: Option<String>,
    pub save_dice: Option<String>,
    pub save_dc: Option<String>,
    /// On a successful save, divide damage by this positive integer.
    pub save_divisor: u8,
    pub damage_dice: String,
    #[serde(default)]
    pub damage_type: Option<String>,
    #[serde(default)]
    pub damage_profile: Option<String>,
    pub resource: String,
    /// Resource spent by the actor on acceptance, one unit.
    pub cost: Option<String>,
    /// Multiple resource costs; mutually exclusive with the legacy unit cost.
    #[serde(default)]
    pub costs: Vec<ActionCost>,
    pub visual: ActionVisual,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ActionVisual {
    pub color: String,
    pub duration_ms: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum DistanceMetric {
    Euclidean,
    Chebyshev,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MovementRule {
    pub id: String,
    pub feature: String,
    pub action: String,
    pub reach_cells: String,
    pub metric: DistanceMetric,
    pub source_condition: String,
    pub target_condition: String,
    pub hostile_only: bool,
    pub movement_modes: Vec<MovementMode>,
    pub combat_only: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum MovementMode {
    #[default]
    Voluntary,
    Forced,
    Teleport,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PluginRuntime {
    #[serde(default)]
    pub saves: Vec<PendingSaves>,
    #[serde(default)]
    pub trigger_runs: Vec<PendingTriggerRun>,
    #[serde(default)]
    pub action_stacks: Vec<ActionStack>,
    #[serde(default)]
    pub maintained: Vec<MaintainedEffect>,
    #[serde(default)]
    pub visuals: Vec<ActionPresentation>,
    pub movements: Vec<PendingMovement>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PendingMovement {
    pub id: crate::IntentId,
    pub token: TokenId,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub destination_x: i64,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub destination_y: i64,
    pub mode: MovementMode,
    pub offers: Vec<ReactionOffer>,
    /// Hooks already offered during this movement, including declined ones.
    pub handled: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ReactionOffer {
    pub ready: bool,
    pub source: TokenId,
    pub rule: String,
    pub action: String,
    pub label: Label,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ActionPresentation {
    pub source: TokenId,
    pub map: crate::MapId,
    pub label: Label,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub x: i64,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub y: i64,
    pub radius_cells: u16,
    pub visual: ActionVisual,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub at_ms: i64,
}

/// A positive, bounded resource expense evaluated before any action effects.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ActionCost {
    #[serde(default)]
    pub scope: CostScope,
    #[serde(default)]
    pub feature: Option<String>,
    /// Reserve at declaration; pay only if the action resolves.
    #[serde(default)]
    pub on_resolution: bool,
    pub resource: String,
    pub amount: String,
}

/// Ordered, declarative cards in the GM dashboard; never executable markup.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct GmPanelSection {
    pub id: String,
    pub label: Label,
    pub description: Label,
    pub kind: GmPanelKind,
    #[serde(default)]
    pub items: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum GmPanelKind {
    Clock,
    Overview,
    Characters,
    Reactions,
    Maintained,
    Rules,
    Actions,
}

/// Ordered target-side transformations followed by resource buffers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DamageProfile {
    pub id: String,
    pub label: Label,
    pub feature: Option<String>,
    pub modifiers: Vec<DamageAdjustment>,
    pub absorb: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DamageAdjustment {
    pub id: String,
    /// Empty matches every type; type IDs are opaque system-defined identifiers.
    pub types: Vec<String>,
    /// Target sheet fields. @damage is the current amount, not a stored field.
    pub condition: String,
    pub amount: String,
}

/// System-neutral policy for an exclusive group of ongoing effects.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MaintainedRule {
    pub id: String,
    pub label: Label,
    pub feature: Option<String>,
    pub exclusive_group: String,
    pub keep_condition: String,
    pub damage_check: MaintainedCheck,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MaintainedCheck {
    pub resource: String,
    pub dice: String,
    pub difficulty: String,
    pub success: String,
    pub tags: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MaintainedAction {
    pub rule: String,
    pub condition: String,
    pub duration: crate::EffectDurationDef,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MaintainedEffect {
    pub id: crate::EffectId,
    pub owner: crate::CharacterId,
    pub source: TokenId,
    pub rule: String,
    pub label: Label,
    pub effects: Vec<crate::EffectId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InterruptionRule {
    pub id: String,
    pub feature: Option<String>,
    pub action: String,
    pub tags: Vec<String>,
    pub any_tags: Vec<String>,
    pub range_cells: String,
    pub metric: DistanceMetric,
    pub source_condition: String,
    pub target_condition: String,
    pub hostile_only: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InterruptEffect {
    pub save_dice: String,
    pub save_dc: String,
    pub cancel_if: String,
    pub tags: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DeferredCost {
    pub resource: String,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub amount: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ActionStack {
    pub id: crate::IntentId,
    pub map: crate::MapId,
    pub frames: Vec<PendingAction>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PendingAction {
    /// Frozen sheet action: editing or reordering the list cannot change a declared cast.
    #[serde(default)]
    pub definition: Option<PluginAction>,
    pub awaiting_reactions: bool,
    pub id: crate::IntentId,
    pub source: TokenId,
    pub owner: crate::CharacterId,
    pub controller: Option<crate::UserId>,
    pub action: String,
    pub label: Label,
    pub target: Option<TokenId>,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub x: i64,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub y: i64,
    pub deferred: Vec<DeferredCost>,
    pub offers: Vec<ReactionOffer>,
    pub ready: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ActionResolution {
    Completed,
    Countered,
    Cancelled,
    Invalid,
}

/// Resource refill policy. Amount is the desired balance, not an additive grant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TurnBudget {
    pub resource: String,
    pub label: Label,
    pub feature: Option<String>,
    pub refresh: BudgetRefresh,
    #[serde(default)]
    pub amount: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum BudgetRefresh {
    OwnTurn,
    EveryTurn,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum CostScope {
    #[default]
    Always,
    Combat,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ActionTiming {
    pub turn: RequiredTurn,
    pub feature: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum RequiredTurn {
    Own,
    Other,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TurnEconomyView {
    pub in_combat: bool,
    pub own_turn: bool,
    pub turn_started: bool,
    pub budgets: Vec<TurnBudgetValue>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TurnBudgetValue {
    pub resource: String,
    pub label: Label,
    pub refresh: BudgetRefresh,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub remaining: i64,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub reserved: i64,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub limit: i64,
}

/// Frozen position and variables of a trigger waiting for its plugin action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PendingTriggerRun {
    /// Nonempty for a manual group; empty for a plugin action window (legacy default).
    #[serde(default)]
    pub rolls: Vec<crate::RollRequestId>,
    pub action: crate::IntentId,
    pub trigger: crate::TriggerId,
    pub version: crate::Version,
    pub map: Option<crate::MapId>,
    pub character: Option<crate::CharacterId>,
    pub token: Option<TokenId>,
    pub next_step: u16,
    #[cfg_attr(feature = "specta", specta(type = std::collections::BTreeMap<String, specta_typescript::Number>))]
    pub variables: std::collections::BTreeMap<String, i64>,
}

/// Geometry required by an action, independent of system-specific senses and cover bonuses.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SpatialRequirements {
    #[serde(default)]
    pub feature: Option<String>,
    #[serde(default)]
    pub sight: bool,
    #[serde(default)]
    pub line_of_effect: bool,
    #[serde(default)]
    pub area_line_of_effect: bool,
}

/// A frozen damage roll awaiting saves by the affected controllers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PendingSaves {
    pub id: crate::IntentId,
    pub map: crate::MapId,
    pub source: Option<TokenId>,
    pub owner: crate::CharacterId,
    pub action: PluginAction,
    #[cfg_attr(feature="specta", specta(type=specta_typescript::Number))]
    pub damage: i64,
    #[cfg_attr(feature="specta", specta(type=specta_typescript::Number))]
    pub difficulty: i64,
    pub targets: Vec<SaveTarget>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SaveTarget {
    pub token: TokenId,
    pub character: crate::CharacterId,
    pub status: SaveStatus,
    #[cfg_attr(feature="specta", specta(type=Option<specta_typescript::Number>))]
    pub total: Option<i64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum SaveStatus {
    Pending,
    Succeeded,
    Failed,
    AutomaticFailure,
}
