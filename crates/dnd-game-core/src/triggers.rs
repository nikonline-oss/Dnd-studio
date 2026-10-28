//! Triggers (ADR-0024): "when → if → then" records the core runs after an intent. Data, not code:
//! conditions are `dnd-expr`, dice are `dnd-dice`, actions come from a fixed list. Their events
//! join the intent's batch, so the local game and the session server run them alike. Events
//! caused by triggers do not set off other triggers.

use std::collections::BTreeMap;

use dnd_expr::{Context, Expr, LookupError, Value as ExprValue};
use dnd_protocol::{
    BoardVisibility, CharacterDto, CharacterId, ChatKind, ChatMessageDto, ChatVisibility, Event,
    Intent, JournalVisibility, MAX_TRIGGER_STEPS, MAX_TRIGGER_TEXT, MAX_TRIGGERS,
    MAX_TRIGGERS_PER_INTENT, RejectReason, RevealTarget, SystemMessage, TokenId, TriggerAction,
    TriggerDef, TriggerDto, TriggerEvent, TriggerFiredDto, TriggerId, Version,
};
use serde_json::Value;

use crate::apply::{MAX_NAME_LEN, bump, valid_name};
use crate::chat::{DiceRngAdapter, message_id, posted, roll_dto, system};
use crate::evolve::evolve;
use crate::plugins::{CharacterFields, give_item, inventory_names, normalize_sheet, sheet_values};
use crate::rng::GameRng;
use crate::state::GameState;

/// Longest condition of a trigger or a step.
const MAX_EXPRESSION_LEN: usize = 1024;
/// Most tokens a trigger may be limited to.
const MAX_TRIGGER_TOKENS: usize = 100;

/// Events for a trigger intent; `None` if `intent` is not about triggers.
pub(crate) fn apply(
    state: &GameState,
    intent: &Intent,
    rng: &mut dyn GameRng,
) -> Option<Result<Vec<Event>, RejectReason>> {
    let result = match intent {
        Intent::CreateTrigger { id, definition } => (|| {
            if state.triggers.len() >= MAX_TRIGGERS {
                return Err(RejectReason::invalid("id"));
            }
            Ok(vec![Event::TriggerCreated {
                trigger: TriggerDto {
                    id: *id,
                    definition: valid_definition(state, definition)?,
                    fired: false,
                    version: Version::INITIAL,
                },
            }])
        })(),
        Intent::UpdateTrigger { id, definition } => (|| {
            let mut trigger = state
                .triggers
                .get(id)
                .cloned()
                .ok_or(RejectReason::NotFound)?;
            trigger.definition = valid_definition(state, definition)?;
            // Edited: armed again.
            trigger.fired = false;
            trigger.version = bump(trigger.version)?;
            Ok(vec![Event::TriggerUpdated { trigger }])
        })(),
        Intent::DeleteTrigger { id } => (|| {
            if !state.triggers.contains_key(id) {
                return Err(RejectReason::NotFound);
            }
            Ok(vec![Event::TriggerDeleted { id: *id }])
        })(),
        Intent::FireTrigger { id, token } => (|| {
            let trigger = state.triggers.get(id).ok_or(RejectReason::NotFound)?;
            if trigger.definition.event != TriggerEvent::Manual || trigger.fired {
                return Err(RejectReason::invalid("id"));
            }
            let character = match token {
                Some(token) => {
                    state
                        .tokens
                        .get(token)
                        .ok_or_else(|| RejectReason::invalid("token"))?
                        .character_id
                }
                None => None,
            };
            let mut work = state.clone();
            Ok(run(
                &mut work,
                *id,
                &Firing {
                    character,
                    token: *token,
                    total: None,
                },
                rng,
            ))
        })(),
        _ => return None,
    };
    Some(result)
}

/// Who set a trigger off and with what roll.
struct Firing {
    character: Option<CharacterId>,
    token: Option<TokenId>,
    total: Option<i64>,
}

/// The events of the triggers that `events` (the intent's own) set off, in trigger order.
pub(crate) fn react(
    before: &GameState,
    intent: &Intent,
    events: &[Event],
    rng: &mut dyn GameRng,
) -> Vec<Event> {
    let armed = |kind: fn(&TriggerEvent) -> bool| {
        before
            .triggers
            .values()
            .any(|t| t.definition.enabled && !t.fired && kind(&t.definition.event))
    };
    let roll = match intent {
        Intent::RollDice {
            character_id: Some(character),
            roll: Some(roll),
            ..
        } => Some((*character, roll.as_str())),
        _ => None,
    };
    let moves = events.iter().any(|e| matches!(e, Event::TokenMoved { .. }));
    let items = events
        .iter()
        .any(|e| matches!(e, Event::CharacterUpdated { .. }));
    let wanted = (roll.is_some() && armed(|e| matches!(e, TriggerEvent::RollInZone { .. })))
        || (moves && armed(|e| matches!(e, TriggerEvent::TokenEnteredZone { .. })))
        || (items && armed(|e| matches!(e, TriggerEvent::ItemReceived { .. })));
    if !wanted {
        return Vec::new();
    }

    let mut after = before.clone();
    for event in events {
        evolve(&mut after, event);
    }
    let firings = matches(before, &after, roll, events);
    let mut work = after;
    let mut out = Vec::new();
    for (id, firing) in firings.into_iter().take(MAX_TRIGGERS_PER_INTENT) {
        out.extend(run(&mut work, id, &firing, rng));
    }
    out
}

/// The triggers `events` set off, with who set them off.
fn matches(
    before: &GameState,
    after: &GameState,
    roll: Option<(CharacterId, &str)>,
    events: &[Event],
) -> Vec<(TriggerId, Firing)> {
    let mut found = Vec::new();
    let total = events.iter().find_map(|e| match e {
        Event::ChatMessagePosted { message } => message.roll.as_ref().map(|r| r.total),
        _ => None,
    });
    for trigger in after.triggers.values() {
        let def = &trigger.definition;
        if !def.enabled || trigger.fired {
            continue;
        }
        let allowed = |token: TokenId| def.tokens.is_empty() || def.tokens.contains(&token);
        let firing = match &def.event {
            TriggerEvent::RollInZone { zone, rolls } => (|| {
                let (character, roll) = roll?;
                if !rolls.is_empty() && !rolls.iter().any(|r| r == roll) {
                    return None;
                }
                let zone = after.zones.get(zone)?;
                let token = after.tokens_on(zone.map_id).find(|t| {
                    t.character_id == Some(character)
                        && allowed(t.id)
                        && zone.shape.contains(t.x, t.y)
                })?;
                Some(Firing {
                    character: Some(character),
                    token: Some(token.id),
                    total,
                })
            })(),
            TriggerEvent::TokenEnteredZone { zone } => (|| {
                let zone = after.zones.get(zone)?;
                events.iter().find_map(|e| {
                    let Event::TokenMoved { id, x, y } = e else {
                        return None;
                    };
                    let token = after.tokens.get(id)?;
                    let was = before.tokens.get(id)?;
                    let entered = token.map_id == zone.map_id
                        && zone.shape.contains(*x, *y)
                        && !zone.shape.contains(was.x, was.y);
                    let eligible = if def.tokens.is_empty() {
                        token.character_id.is_some()
                    } else {
                        def.tokens.contains(id)
                    };
                    (entered && eligible).then_some(Firing {
                        character: token.character_id,
                        token: Some(*id),
                        total: None,
                    })
                })
            })(),
            TriggerEvent::ItemReceived { name } => (|| {
                let rules = after.rules()?;
                events.iter().find_map(|e| {
                    let Event::CharacterUpdated { character } = e else {
                        return None;
                    };
                    let old = before
                        .characters
                        .get(&character.id)
                        .map(|c| inventory_names(rules, &c.sheet_json))
                        .unwrap_or_default();
                    let new = inventory_names(rules, &character.sheet_json);
                    let count = |names: &[String]| {
                        names
                            .iter()
                            // Ignoring case in any script ("Ключ" = "ключ").
                            .filter(|n| n.to_lowercase() == name.to_lowercase())
                            .count()
                    };
                    let token = if def.tokens.is_empty() {
                        None
                    } else {
                        // Limited to tokens: the character must have one of them.
                        Some(
                            def.tokens
                                .iter()
                                .find(|t| {
                                    after
                                        .tokens
                                        .get(t)
                                        .is_some_and(|t| t.character_id == Some(character.id))
                                })
                                .copied()?,
                        )
                    };
                    (count(&new) > count(&old)).then_some(Firing {
                        character: Some(character.id),
                        token,
                        total: None,
                    })
                })
            })(),
            TriggerEvent::Manual => None,
        };
        if let Some(firing) = firing
            && condition_holds(after, def.condition.as_deref(), &firing, &BTreeMap::new())
        {
            found.push((trigger.id, firing));
        }
    }
    found
}

/// Runs a trigger's steps against `work`, which follows every event produced. Returns the events.
fn run(work: &mut GameState, id: TriggerId, firing: &Firing, rng: &mut dyn GameRng) -> Vec<Event> {
    let Some(trigger) = work.triggers.get(&id).cloned() else {
        return Vec::new();
    };
    if trigger.fired {
        return Vec::new();
    }
    let mut events = vec![Event::TriggerFired {
        fired: TriggerFiredDto {
            trigger: id,
            name: trigger.definition.name.clone(),
            character: firing.character,
            token: firing.token,
        },
    }];
    if trigger.definition.once
        && let Ok(version) = bump(trigger.version)
    {
        events.push(Event::TriggerUpdated {
            trigger: TriggerDto {
                fired: true,
                version,
                ..trigger.clone()
            },
        });
    }
    for event in &events {
        evolve(work, event);
    }
    let mut vars = BTreeMap::new();
    if let Some(total) = firing.total {
        vars.insert("total".to_owned(), total);
    }
    for step in &trigger.definition.steps {
        if !condition_holds(work, step.when.as_deref(), firing, &vars) {
            continue;
        }
        for event in perform(work, &trigger, &step.action, firing, &mut vars, rng) {
            evolve(work, &event);
            events.push(event);
        }
    }
    events
}

/// One action's events (none when it does not apply, e.g. no character to act on).
fn perform(
    work: &GameState,
    trigger: &TriggerDto,
    action: &TriggerAction,
    firing: &Firing,
    vars: &mut BTreeMap<String, i64>,
    rng: &mut dyn GameRng,
) -> Vec<Event> {
    let character = firing
        .character
        .and_then(|id| work.characters.get(&id))
        .cloned();
    match action {
        TriggerAction::Reveal { target } => reveal(work, *target),
        TriggerAction::SetNodeVisibility { node, visibility } => work
            .board_nodes
            .get(node)
            .filter(|n| n.visibility != *visibility)
            .and_then(|n| {
                let mut node = n.clone();
                node.visibility = *visibility;
                node.version = bump(node.version).ok()?;
                Some(vec![Event::BoardNodeUpdated { node }])
            })
            .unwrap_or_default(),
        TriggerAction::Chat { text, gm_only } => {
            let mut event = system(SystemMessage::Narration { text: text.clone() }, rng);
            if *gm_only && let Event::ChatMessagePosted { message } = &mut event {
                message.visibility = ChatVisibility::Gm;
            }
            vec![event]
        }
        TriggerAction::Roll { dice, label } => {
            let Some(roll) = roll_dice(work, dice, character.as_ref(), vars, rng) else {
                return Vec::new();
            };
            vars.insert("roll".to_owned(), roll.total);
            let name = if label.trim().is_empty() {
                trigger.definition.name.clone()
            } else {
                label.clone()
            };
            vec![posted(ChatMessageDto {
                id: message_id(rng),
                kind: ChatKind::Roll,
                sender: None,
                sender_name: name,
                body: dice.trim().to_owned(),
                roll: Some(roll_dto(&roll)),
                system: None,
                visibility: ChatVisibility::All,
                at_ms: None,
            })]
        }
        TriggerAction::ChangeResource {
            resource,
            dice,
            heal,
        } => (|| {
            let character = character?;
            let path = resource_path(work, resource)?;
            let amount = roll_dice(work, dice, Some(&character), vars, rng)?
                .total
                .max(0);
            let (_, current, max) = sheet_values(work, &character.sheet_json)?
                .resources
                .into_iter()
                .find(|(id, _, _)| id == resource)?;
            let next = if *heal {
                current.saturating_add(amount).min(max.max(0))
            } else {
                current.saturating_sub(amount).max(0)
            };
            let sheet = set_integer(&character.sheet_json, &path, next)?;
            let updated = updated_character(work, &character, |c| c.sheet_json = sheet)?;
            Some(vec![
                updated,
                system(
                    SystemMessage::ResourceChanged {
                        name: character.name.clone(),
                        resource: resource.clone(),
                        delta: next - current,
                    },
                    rng,
                ),
            ])
        })()
        .unwrap_or_default(),
        TriggerAction::AddCondition { condition }
        | TriggerAction::RemoveCondition { condition } => (|| {
            let character = character?;
            let rules = work.rules()?;
            let add = matches!(action, TriggerAction::AddCondition { .. });
            if add == character.conditions.contains(condition) {
                return None;
            }
            let conditions: Vec<String> = rules
                .conditions
                .iter()
                .filter(|c| {
                    if c.id == *condition {
                        add
                    } else {
                        character.conditions.contains(&c.id)
                    }
                })
                .map(|c| c.id.clone())
                .collect();
            let updated = updated_character(work, &character, |c| c.conditions = conditions)?;
            Some(vec![
                updated,
                system(
                    SystemMessage::ConditionChanged {
                        name: character.name.clone(),
                        condition: condition.clone(),
                        added: add,
                    },
                    rng,
                ),
            ])
        })()
        .unwrap_or_default(),
        TriggerAction::GiveItem { name } => (|| {
            let character = character?;
            let sheet = give_item(work, &character.sheet_json, name).ok()?;
            let updated = updated_character(work, &character, |c| c.sheet_json = sheet)?;
            Some(vec![
                updated,
                system(
                    SystemMessage::ItemGiven {
                        name: character.name.clone(),
                        item: name.clone(),
                    },
                    rng,
                ),
            ])
        })()
        .unwrap_or_default(),
    }
}

/// Shows `target` to players; nothing if it is already shown or gone.
fn reveal(work: &GameState, target: RevealTarget) -> Vec<Event> {
    let event = match target {
        RevealTarget::Node { id } => work
            .board_nodes
            .get(&id)
            .filter(|n| n.visibility != BoardVisibility::Players)
            .and_then(|n| {
                let mut node = n.clone();
                node.visibility = BoardVisibility::Players;
                node.version = bump(node.version).ok()?;
                Some(Event::BoardNodeUpdated { node })
            }),
        RevealTarget::Board { id } => work
            .boards
            .get(&id)
            .filter(|b| !b.visible_to_players)
            .and_then(|b| {
                let mut board = b.clone();
                board.visible_to_players = true;
                board.version = bump(board.version).ok()?;
                Some(Event::BoardUpdated { board })
            }),
        RevealTarget::Map { id } => work
            .maps
            .get(&id)
            .filter(|m| !m.visible_to_players)
            .and_then(|m| {
                let mut map = m.clone();
                map.visible_to_players = true;
                map.version = bump(map.version).ok()?;
                Some(Event::MapUpdated { map })
            }),
        RevealTarget::Journal { id } => work
            .journal
            .get(&id)
            .filter(|e| e.visibility != JournalVisibility::Players)
            .and_then(|e| {
                let mut entry = e.clone();
                entry.visibility = JournalVisibility::Players;
                entry.version = bump(entry.version).ok()?;
                Some(Event::JournalEntryUpdated { entry })
            }),
        RevealTarget::Token { id } => work.tokens.get(&id).filter(|t| !t.visible).and_then(|t| {
            let mut token = t.clone();
            token.visible = true;
            token.version = bump(token.version).ok()?;
            Some(Event::TokenUpdated { token })
        }),
    };
    event.into_iter().collect()
}

fn updated_character(
    work: &GameState,
    character: &CharacterDto,
    edit: impl FnOnce(&mut CharacterDto),
) -> Option<Event> {
    let mut next = work.characters.get(&character.id)?.clone();
    edit(&mut next);
    next.sheet_json = normalize_sheet(work, &next.sheet_json).ok()?;
    next.version = bump(next.version).ok()?;
    Some(Event::CharacterUpdated { character: next })
}

/// The sheet path a resource's value reads (`@hp.current` → `hp.current`), if it is a plain
/// field; computed resources cannot be changed.
fn resource_path(state: &GameState, resource: &str) -> Option<String> {
    let def = state.rules()?.resources.iter().find(|r| r.id == resource)?;
    let path = def.value.trim().strip_prefix('@')?;
    let plain = !path.is_empty()
        && path
            .split('.')
            .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
    plain.then(|| path.to_owned())
}

fn set_integer(sheet_json: &str, path: &str, value: i64) -> Option<String> {
    let mut sheet: Value = serde_json::from_str(sheet_json).ok()?;
    let mut node = &mut sheet;
    for segment in path.split('.') {
        node = node
            .as_object_mut()?
            .entry(segment)
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
    }
    *node = Value::from(value);
    serde_json::to_string(&sheet).ok()
}

/// What `@name` means in trigger expressions: `@total`, `@roll`, then the character's fields.
struct Scope<'a> {
    fields: Option<CharacterFields>,
    vars: &'a BTreeMap<String, i64>,
}

impl<'a> Scope<'a> {
    fn new(
        state: &GameState,
        character: Option<&CharacterDto>,
        vars: &'a BTreeMap<String, i64>,
    ) -> Self {
        Self {
            fields: character.map(|c| CharacterFields::new(state, Some(&c.sheet_json))),
            vars,
        }
    }
}

impl Context for Scope<'_> {
    fn lookup(&self, path: &str) -> Result<ExprValue, LookupError> {
        if let Some(value) = self.vars.get(path) {
            return Ok(ExprValue::from(*value));
        }
        match &self.fields {
            Some(fields) => Context::lookup(fields, path),
            None => Err(LookupError::Unknown),
        }
    }
}

impl dnd_dice::FieldLookup for Scope<'_> {
    fn lookup(&self, path: &str) -> Result<i64, dnd_dice::LookupError> {
        if let Some(value) = self.vars.get(path) {
            return Ok(*value);
        }
        match &self.fields {
            Some(fields) => dnd_dice::FieldLookup::lookup(fields, path),
            None => Err(dnd_dice::LookupError::Unknown),
        }
    }
}

/// No condition, or one that evaluates to `true` (or a non-zero number). Errors count as false.
fn condition_holds(
    state: &GameState,
    condition: Option<&str>,
    firing: &Firing,
    vars: &BTreeMap<String, i64>,
) -> bool {
    let Some(condition) = condition.filter(|c| !c.trim().is_empty()) else {
        return true;
    };
    let mut all = vars.clone();
    if let Some(total) = firing.total {
        all.entry("total".to_owned()).or_insert(total);
    }
    let character = firing.character.and_then(|id| state.characters.get(&id));
    let scope = Scope::new(state, character, &all);
    match Expr::parse(condition).and_then(|e| e.eval(&scope)) {
        Ok(value) => value
            .as_bool()
            .unwrap_or_else(|| value.as_integer().is_ok_and(|n| n != 0)),
        Err(_) => false,
    }
}

fn roll_dice(
    state: &GameState,
    dice: &str,
    character: Option<&CharacterDto>,
    vars: &BTreeMap<String, i64>,
    rng: &mut dyn GameRng,
) -> Option<dnd_dice::Roll> {
    let scope = Scope::new(state, character, vars);
    dnd_dice::Expr::parse(dice)
        .ok()?
        .roll(&scope, &mut DiceRngAdapter(rng))
        .ok()
}

/// A definition whose references exist and whose expressions and dice parse; names trimmed.
fn valid_definition(state: &GameState, def: &TriggerDef) -> Result<TriggerDef, RejectReason> {
    let mut def = def.clone();
    def.name = valid_name(&def.name, "name")?;
    let rules = state.rules();
    match &mut def.event {
        TriggerEvent::RollInZone { zone, rolls } => {
            if !state.zones.contains_key(zone) {
                return Err(RejectReason::invalid("event"));
            }
            let known = |id: &String| rules.is_some_and(|r| r.rolls.iter().any(|d| d.id == *id));
            if !rolls.iter().all(known) {
                return Err(RejectReason::invalid("event"));
            }
        }
        TriggerEvent::TokenEnteredZone { zone } => {
            if !state.zones.contains_key(zone) {
                return Err(RejectReason::invalid("event"));
            }
        }
        TriggerEvent::Manual => {}
        TriggerEvent::ItemReceived { name } => {
            *name = valid_name(name, "event")?;
        }
    }
    if def.tokens.len() > MAX_TRIGGER_TOKENS
        || !def.tokens.iter().all(|t| state.tokens.contains_key(t))
    {
        return Err(RejectReason::invalid("tokens"));
    }
    if let Some(condition) = &def.condition {
        valid_expression(condition, "condition")?;
    }
    if def.steps.is_empty() || def.steps.len() > MAX_TRIGGER_STEPS {
        return Err(RejectReason::invalid("steps"));
    }
    for step in &mut def.steps {
        if let Some(when) = &step.when {
            valid_expression(when, "steps")?;
        }
        valid_action(state, &mut step.action)?;
    }
    Ok(def)
}

fn valid_expression(expression: &str, field: &str) -> Result<(), RejectReason> {
    if expression.len() > MAX_EXPRESSION_LEN || Expr::parse(expression).is_err() {
        return Err(RejectReason::invalid(field));
    }
    Ok(())
}

fn valid_action(state: &GameState, action: &mut TriggerAction) -> Result<(), RejectReason> {
    let invalid = || RejectReason::invalid("steps");
    let dice_ok = |dice: &str| dice.len() <= 256 && dnd_dice::Expr::parse(dice).is_ok();
    let rules = state.rules();
    let ok = match action {
        TriggerAction::Reveal { target } => match target {
            RevealTarget::Node { id } => state.board_nodes.contains_key(id),
            RevealTarget::Board { id } => state.boards.contains_key(id),
            RevealTarget::Map { id } => state.maps.contains_key(id),
            RevealTarget::Journal { id } => state.journal.contains_key(id),
            RevealTarget::Token { id } => state.tokens.contains_key(id),
        },
        TriggerAction::SetNodeVisibility { node, .. } => state.board_nodes.contains_key(node),
        TriggerAction::Chat { text, .. } => {
            *text = text.trim().to_owned();
            !text.is_empty() && text.chars().count() <= MAX_TRIGGER_TEXT
        }
        TriggerAction::Roll { dice, label } => {
            *label = label.trim().to_owned();
            dice_ok(dice) && label.chars().count() <= MAX_NAME_LEN
        }
        TriggerAction::ChangeResource { resource, dice, .. } => {
            dice_ok(dice) && resource_path(state, resource).is_some()
        }
        TriggerAction::AddCondition { condition }
        | TriggerAction::RemoveCondition { condition } => {
            rules.is_some_and(|r| r.conditions.iter().any(|c| c.id == *condition))
        }
        TriggerAction::GiveItem { name } => {
            *name = valid_name(name, "steps")?;
            rules.is_some_and(|r| r.inventory.is_some())
        }
    };
    if ok { Ok(()) } else { Err(invalid()) }
}
