//! Campaign item templates and independently mutable instances.
use crate::*;
use serde::{Deserialize, Serialize};
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
    pub name: String,
    pub type_id: String,
    pub is_template: bool,
    pub template: Option<ItemId>,
    pub owner: Option<CharacterId>,
    pub visible_to_players: bool,
    pub sheet_json: String,
    pub version: Version,
}
