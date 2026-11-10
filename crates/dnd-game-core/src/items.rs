//! Items use plugin schemas when they name a plugin type, and stand alone when they do not
//! (ADR-0062): a free item has a name, a description, a state, its own adjustments and the
//! conditions it grants. States gate both: a broken item still exists, it just stops working.
use crate::{
    GameState,
    apply::{bump, valid_name},
};
use dnd_protocol::*;

/// Whether an item with no explicit state works: the campaign's default state must be active.
#[must_use]
pub(crate) fn default_state_active(states: &[ItemStateDef]) -> bool {
    ItemStateDef::default_id(states)
        .and_then(|id| states.iter().find(|s| s.id == id).map(|s| s.active))
        .unwrap_or(false)
}

/// A campaign item state id, bounded and plain.
fn valid_state_id(state: &GameState, id: &str) -> Result<String, RejectReason> {
    let id = id.trim();
    if id.chars().count() > MAX_ITEM_STATE_LEN
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        || !state.campaign.item_states().iter().any(|s| s.id == id)
    {
        return Err(RejectReason::invalid("state"));
    }
    Ok(id.to_owned())
}

/// A sheet field path an item may adjust: plain segments, and the field must exist in the sheet.
fn valid_effect_path(state: &GameState, path: &str) -> Result<String, RejectReason> {
    let plain = !path.is_empty()
        && path
            .split('.')
            .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
    let Some(rules) = state.rules() else {
        return Err(RejectReason::invalid("effects"));
    };
    let mut schema = &rules.sheet_schema;
    for segment in path.split('.') {
        let Some(node) = schema.get("properties").and_then(|p| p.get(segment)) else {
            return Err(RejectReason::invalid("effects"));
        };
        schema = node;
    }
    if !plain {
        return Err(RejectReason::invalid("effects"));
    }
    Ok(path.to_owned())
}

fn valid_effects(
    state: &GameState,
    effects: &[ItemEffect],
) -> Result<Vec<ItemEffect>, RejectReason> {
    if effects.len() > MAX_ITEM_EFFECTS {
        return Err(RejectReason::invalid("effects"));
    }
    effects
        .iter()
        .map(|effect| {
            if !is_js_safe(effect.add)
                || effect.minimum.is_some_and(|v| !is_js_safe(v))
                || effect.maximum.is_some_and(|v| !is_js_safe(v))
                || effect
                    .minimum
                    .is_some_and(|min| effect.maximum.is_some_and(|max| min > max))
            {
                return Err(RejectReason::invalid("effects"));
            }
            Ok(ItemEffect {
                path: valid_effect_path(state, &effect.path)?,
                ..effect.clone()
            })
        })
        .collect()
}

/// Custom values: bounded, plain unique keys, JS-safe integers (ADR-0062).
fn valid_values(values: &[ItemValue]) -> Result<Vec<ItemValue>, RejectReason> {
    if values.len() > MAX_ITEM_VALUES {
        return Err(RejectReason::invalid("values"));
    }
    let mut keys = std::collections::BTreeSet::new();
    values
        .iter()
        .map(|value| {
            let key = value.key.trim();
            let plain = !key.is_empty()
                && key.chars().count() <= MAX_ITEM_VALUE_KEY
                && key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
            if !plain || !keys.insert(key.to_owned()) || !is_js_safe(value.value) {
                return Err(RejectReason::invalid("values"));
            }
            Ok(ItemValue {
                key: key.to_owned(),
                value: value.value,
            })
        })
        .collect()
}

fn valid_grants(state: &GameState, grants: &[String]) -> Result<Vec<String>, RejectReason> {
    if grants.len() > MAX_ITEM_GRANTS {
        return Err(RejectReason::invalid("grants"));
    }
    let known = state.rules();
    grants
        .iter()
        .map(|id| {
            let ok = known.is_some_and(|rules| rules.conditions.iter().any(|c| c.id == *id));
            if ok {
                Ok(id.clone())
            } else {
                Err(RejectReason::invalid("grants"))
            }
        })
        .collect()
}
/// The plugin schema of `kind`, when the item names a plugin type.
fn item_type<'a>(state: &'a GameState, kind: Option<&str>) -> Option<&'a ItemTypeDef> {
    state.rules().and_then(|r| {
        r.mechanics
            .item_types
            .iter()
            .find(|t| Some(t.id.as_str()) == kind)
    })
}

/// Sheet data of an item: validated against its plugin type, or `{}` for a free-form item.
fn sheet(
    state: &GameState,
    kind: Option<&str>,
    input: Option<&str>,
) -> Result<String, RejectReason> {
    let Some(kind) = kind else {
        // A free item carries no plugin data; storing a sheet would silently do nothing.
        return match input.map(str::trim) {
            None | Some("") | Some("{}") => Ok("{}".to_owned()),
            Some(_) => Err(RejectReason::invalid("sheet_json")),
        };
    };
    let def = item_type(state, Some(kind)).ok_or_else(|| RejectReason::invalid("type_id"))?;
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
            item_kind,
            name,
            type_id,
            description,
            sheet_json,
            visible_to_players,
        } => {
            if state.items.len() >= 5000 {
                return Err(RejectReason::invalid("items"));
            }
            let description = description.trim();
            if description.chars().count() > crate::apply::MAX_DESCRIPTION_LEN {
                return Err(RejectReason::invalid("description"));
            }
            Ok(vec![Event::ItemCreated {
                item: ItemDto {
                    id: *id,
                    kind: *item_kind,
                    name: valid_name(name, "name")?,
                    type_id: type_id.clone(),
                    is_template: true,
                    template: None,
                    owner: None,
                    visible_to_players: *visible_to_players,
                    description: description.to_owned(),
                    // A new item starts in the campaign's default state.
                    state: None,
                    effects: Vec::new(),
                    grants: Vec::new(),
                    values: Vec::new(),
                    sheet_json: sheet(state, type_id.as_deref(), sheet_json.as_deref())?,
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
            item.sheet_json = sheet(state, item.type_id.as_deref(), Some(&item.sheet_json))?;
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
            description,
            sheet_json,
            visible_to_players,
            state: item_state,
            effects,
            grants,
            values,
        } => {
            let mut item = state.items.get(id).cloned().ok_or(RejectReason::NotFound)?;
            if let Some(name) = name {
                item.name = valid_name(name, "name")?;
            }
            if let Some(description) = description {
                let description = description.trim();
                if description.chars().count() > crate::apply::MAX_DESCRIPTION_LEN {
                    return Err(RejectReason::invalid("description"));
                }
                item.description = description.to_owned();
            }
            if let Some(input) = sheet_json {
                item.sheet_json = sheet(state, item.type_id.as_deref(), Some(input))?;
            }
            if let Some(visible) = visible_to_players {
                item.visible_to_players = *visible;
            }
            if let Some(state_id) = item_state {
                // An empty id is how the table puts an item back to the campaign default.
                item.state = if state_id.trim().is_empty() {
                    None
                } else {
                    Some(valid_state_id(state, state_id)?)
                };
            }
            if let Some(effects) = effects {
                // Only a GM configures what an item does; permissions enforce that (ADR-0062).
                item.effects = valid_effects(state, effects)?;
            }
            if let Some(grants) = grants {
                item.grants = valid_grants(state, grants)?;
            }
            if let Some(values) = values {
                item.values = valid_values(values)?;
            }
            item.version = bump(item.version)?;
            Ok(vec![Event::ItemUpdated { item }])
        }
        Intent::DuplicateItem { id, new_id } => {
            if state.items.len() >= 5000 {
                return Err(RejectReason::invalid("items"));
            }
            let mut copy = state.items.get(id).cloned().ok_or(RejectReason::NotFound)?;
            copy.id = *new_id;
            // The copy of an instance stays an instance of the same template; the copy of a
            // template is another template. Nothing else about it is shared.
            copy.version = Version::INITIAL;
            Ok(vec![Event::ItemCreated { item: copy }])
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
    // Owners go dangling only when their character is deleted, and item events change owners;
    // without either the batch cannot orphan an item, so the state clone is skipped (ADR-0053).
    let touches_items = events.iter().any(|event| {
        matches!(
            event,
            Event::CharacterDeleted { .. }
                | Event::ItemCreated { .. }
                | Event::ItemUpdated { .. }
                | Event::ItemDeleted { .. }
        )
    });
    if !touches_items {
        return Ok(Vec::new());
    }
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
