//! Item instances use plugin schemas and the campaign's ordinary intent/event pipeline.
use crate::{
    GameState,
    apply::{bump, valid_name},
};
use dnd_protocol::*;
fn sheet(state: &GameState, kind: &str, input: Option<&str>) -> Result<String, RejectReason> {
    let def = state
        .rules()
        .and_then(|r| r.mechanics.item_types.iter().find(|t| t.id == kind))
        .ok_or_else(|| RejectReason::invalid("type_id"))?;
    let raw = input.unwrap_or("{}");
    validate_sheet_json(raw)?;
    let data: serde_json::Value =
        serde_json::from_str(raw).map_err(|_| RejectReason::invalid("sheet_json"))?;
    let data = sheet_schema::apply_defaults(&def.sheet_schema, &data);
    sheet_schema::check_data(&def.sheet_schema, &data)
        .map_err(|_| RejectReason::invalid("sheet_json"))?;
    serde_json::to_string(&data).map_err(|_| RejectReason::invalid("sheet_json"))
}
pub(crate) fn apply(
    state: &GameState,
    intent: &Intent,
) -> Option<Result<Vec<Event>, RejectReason>> {
    Some((|| match intent {
        Intent::CreateItem {
            id,
            name,
            type_id,
            sheet_json,
            visible_to_players,
        } => {
            if state.items.len() >= 5000 {
                return Err(RejectReason::invalid("items"));
            }
            Ok(vec![Event::ItemCreated {
                item: ItemDto {
                    id: *id,
                    name: valid_name(name, "name")?,
                    type_id: type_id.clone(),
                    is_template: true,
                    template: None,
                    owner: None,
                    visible_to_players: *visible_to_players,
                    sheet_json: sheet(state, type_id, sheet_json.as_deref())?,
                    version: Version::INITIAL,
                },
            }])
        }
        Intent::InstantiateItem {
            id,
            template,
            owner,
        } => {
            if state.items.len() >= 5000 {
                return Err(RejectReason::invalid("items"));
            }
            let mut item = state
                .items
                .get(template)
                .filter(|i| i.is_template)
                .cloned()
                .ok_or(RejectReason::NotFound)?;
            if owner.is_some_and(|id| !state.characters.contains_key(&id)) {
                return Err(RejectReason::NotFound);
            }
            item.sheet_json = sheet(state, &item.type_id, Some(&item.sheet_json))?;
            item.id = *id;
            item.template = Some(*template);
            item.is_template = false;
            item.owner = *owner;
            item.visible_to_players = false;
            item.version = Version::INITIAL;
            Ok(vec![Event::ItemCreated { item }])
        }
        Intent::UpdateItem {
            id,
            name,
            sheet_json,
            visible_to_players,
        } => {
            let mut item = state.items.get(id).cloned().ok_or(RejectReason::NotFound)?;
            if let Some(name) = name {
                item.name = valid_name(name, "name")?;
            }
            if let Some(input) = sheet_json {
                item.sheet_json = sheet(state, &item.type_id, Some(input))?;
            }
            if let Some(visible) = visible_to_players {
                item.visible_to_players = *visible;
            }
            item.version = bump(item.version)?;
            Ok(vec![Event::ItemUpdated { item }])
        }
        Intent::TransferItem { id, owner } => {
            let mut item = state
                .items
                .get(id)
                .filter(|i| !i.is_template)
                .cloned()
                .ok_or(RejectReason::NotFound)?;
            if owner.is_some_and(|id| !state.characters.contains_key(&id)) {
                return Err(RejectReason::NotFound);
            }
            item.owner = *owner;
            item.visible_to_players = false;
            item.version = bump(item.version)?;
            Ok(vec![Event::ItemUpdated { item }])
        }
        Intent::DeleteItem { id } => {
            if !state.items.contains_key(id) {
                return Err(RejectReason::NotFound);
            }
            Ok(vec![Event::ItemDeleted { id: *id }])
        }
        _ => Err(RejectReason::invalid("intent")),
    })())
}
pub(crate) fn cleanup(state: &GameState, events: &[Event]) -> Result<Vec<Event>, RejectReason> {
    let mut work = state.clone();
    for e in events {
        crate::evolve(&mut work, e);
    }
    work.items
        .values()
        .filter(|i| i.owner.is_some_and(|id| !work.characters.contains_key(&id)))
        .map(|i| {
            let mut item = i.clone();
            item.owner = None;
            item.visible_to_players = false;
            item.version = bump(item.version)?;
            Ok(Event::ItemUpdated { item })
        })
        .collect()
}
