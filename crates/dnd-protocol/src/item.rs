//! Items: campaign objects that may carry a plugin type, a free description, a state and the
//! adjustments they give their owner (ADR-0062). A template is a reusable blueprint; an instance
//! is an independently mutable copy owned by a character.
use crate::*;
use serde::{Deserialize, Serialize};

/// What an inventory entry is (ADR-0062): an object, or a skill the same machinery carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ItemKind {
    /// A thing: a ring, a potion, a scroll.
    #[default]
    Item,
    /// A skill, spell or talent: no weight, no state of being broken, but the same effects,
    /// grants, custom values and duplication.
    Skill,
}

/// A named integer an item carries for the rules to read (ADR-0062): `item.<key>` in expressions
/// sums it over the character's active items.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ItemValue {
    /// Plain key: `fire_damage`, `armor_bonus`.
    pub key: String,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub value: i64,
}

/// Most custom values on one item.
pub const MAX_ITEM_VALUES: usize = 32;
/// Longest custom value key.
pub const MAX_ITEM_VALUE_KEY: usize = 40;

/// Most effects on one item.
pub const MAX_ITEM_EFFECTS: usize = 16;
/// Most conditions one item grants.
pub const MAX_ITEM_GRANTS: usize = 8;
/// Most item states one campaign defines.
pub const MAX_ITEM_STATES: usize = 24;
/// Longest item state id and label.
pub const MAX_ITEM_STATE_LEN: usize = 40;

/// One numeric adjustment an active item gives its owner (ADR-0062): the same shape and
/// semantics as a condition's `Parameter` modifier, so the two add up the same way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ItemEffect {
    /// Sheet field path, e.g. `ac` or `abilities.str`.
    pub path: String,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub add: i64,
    #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Number>))]
    pub minimum: Option<i64>,
    #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Number>))]
    pub maximum: Option<i64>,
}

/// A state an item may be in (broken, fragment, activated, ...). Only states with `active` let the
/// item's effects and grants reach its owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ItemStateDef {
    pub id: String,
    pub label: Label,
    #[serde(default = "default_active")]
    pub active: bool,
}

fn default_active() -> bool {
    true
}

impl ItemStateDef {
    /// The states a campaign starts with. A campaign that never edits them stores none.
    #[must_use]
    pub fn builtin() -> Vec<Self> {
        let label = |ru: &str, en: &str| {
            Label::Localized(LocalizedText {
                ru: Some(ru.to_owned()),
                en: Some(en.to_owned()),
            })
        };
        vec![
            Self {
                id: "ok".into(),
                label: label("Целый", "Intact"),
                active: true,
            },
            Self {
                id: "broken".into(),
                label: label("Сломан", "Broken"),
                active: false,
            },
            Self {
                id: "fragment".into(),
                label: label("Обломок", "Fragment"),
                active: false,
            },
            Self {
                id: "activated".into(),
                label: label("Активирован", "Activated"),
                active: true,
            },
            Self {
                id: "deactivated".into(),
                label: label("Деактивирован", "Deactivated"),
                active: false,
            },
        ]
    }

    /// The state an item is in when it names none: the first active state of the campaign.
    #[must_use]
    pub fn default_id(states: &[Self]) -> Option<String> {
        states
            .iter()
            .find(|state| state.active)
            .or_else(|| states.first())
            .map(|state| state.id.clone())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ItemTypeDef {
    pub id: String,
    pub label: Label,
    #[cfg_attr(feature="specta", specta(type=specta_typescript::Unknown))]
    pub sheet_schema: serde_json::Value,
    #[cfg_attr(feature="specta", specta(type=specta_typescript::Unknown))]
    pub sheet_layout: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ItemDto {
    pub id: ItemId,
    /// An object or a skill (ADR-0062); missing in old data: an item.
    #[serde(default)]
    pub kind: ItemKind,
    pub name: String,
    /// Plugin item type; `None` for a free-form item with no plugin schema (ADR-0062).
    #[serde(default)]
    pub type_id: Option<String>,
    pub is_template: bool,
    pub template: Option<ItemId>,
    pub owner: Option<CharacterId>,
    pub visible_to_players: bool,
    /// Free text the table reads next to the name.
    #[serde(default)]
    pub description: String,
    /// Campaign state id; `None` means the campaign's default state.
    #[serde(default)]
    pub state: Option<String>,
    /// Numeric adjustments while the item sits in an active state.
    #[serde(default)]
    pub effects: Vec<ItemEffect>,
    /// Conditions the owner has while the item is active (a curse, a blessing).
    #[serde(default)]
    pub grants: Vec<String>,
    /// Custom integers the rules read as `@item.<key>` while the item works.
    #[serde(default)]
    pub values: Vec<ItemValue>,
    /// Plugin-provided data; `{}` for a free-form item.
    pub sheet_json: String,
    pub version: Version,
}
