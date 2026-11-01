//! Triggers (ADR-0024): "when → if → then" records the core runs after an intent. Data, not code:
//! conditions are `dnd-expr`, dice are `dnd-dice`, actions come from a fixed list. Their events
//! join the intent's batch, so the local game and the session server run them alike. Events
//! caused by triggers do not set off other triggers.

use std::collections::BTreeMap;

use dnd_expr::{Context, Expr, LookupError, Value as ExprValue};
use dnd_protocol::{
    BoardVisibility, CharacterDto, CharacterId, ChatKind, ChatMessageDto, ChatVisibility,
    CombatDto, CombatId, CombatantDto, CombatantId, Event, Intent, JournalVisibility,
    MAX_COMBATANTS, MAX_SPAWN_TOKENS, MAX_TRIGGER_STEPS, MAX_TRIGGER_TEXT, MAX_TRIGGERS,
    MAX_TRIGGERS_PER_INTENT, MapId, RejectReason, RevealTarget, SpawnPlacement, SystemMessage,
    TokenDto, TokenId, TokenLayer, TriggerAction, TriggerDef, TriggerDto, TriggerEvent,
    TriggerFiredDto, TriggerId, Version, ZoneShape, uuid_v7_from_random,
};
use serde_json::Value;

use crate::apply::{MAX_NAME_LEN, bump, valid_name};
use crate::chat::{DiceRngAdapter, message_id, posted, roll_dto, system};
use crate::combat::initiative_expression;
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
                    fired_maps: Vec::new(),
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
            trigger.fired_maps.clear();
            trigger.version = bump(trigger.version)?;
            Ok(vec![Event::TriggerUpdated { trigger }])
        })(),
        Intent::DeleteTrigger { id } => (|| {
            if !state.triggers.contains_key(id) {
                return Err(RejectReason::NotFound);
            }
            let mut events: Vec<_> = state.board_nodes.values()
                .filter(|n| matches!(n.content, dnd_protocol::BoardNodeContent::Trigger { id: target } if target == *id))
                .flat_map(|n| crate::boards::delete_node(state, n.id)).collect();
            events.push(Event::TriggerDeleted { id: *id });
            Ok(events)
        })(),
        Intent::FireTrigger { id, token } => (|| {
            let trigger = state.triggers.get(id).ok_or(RejectReason::NotFound)?;
            if trigger.definition.event != TriggerEvent::Manual {
                return Err(RejectReason::invalid("id"));
            }
            let scope = effective_maps(state, &trigger.definition);
            let selected = token
                .map(|id| {
                    state
                        .tokens
                        .get(&id)
                        .ok_or_else(|| RejectReason::invalid("token"))
                })
                .transpose()?;
            if let Some(selected) = selected
                && ((!scope.is_empty() && !scope.contains(&selected.map_id))
                    || (!trigger.definition.tokens.is_empty()
                        && !trigger.definition.tokens.contains(&selected.id)))
            {
                return Err(RejectReason::invalid("token"));
            }
            let maps: Vec<Option<MapId>> = if scope.is_empty() {
                vec![None]
            } else if let Some(selected) = selected {
                vec![Some(selected.map_id)]
            } else {
                scope
                    .into_iter()
                    .filter(|id| state.maps.contains_key(id))
                    .map(Some)
                    .collect()
            };
            let firings: Vec<_> = maps
                .into_iter()
                .filter(|map| available(trigger, *map))
                .map(|map_id| Firing {
                    character: selected.and_then(|t| t.character_id),
                    token: *token,
                    total: None,
                    map_id,
                })
                .collect();
            if firings.is_empty() {
                return Err(RejectReason::invalid("id"));
            }
            let mut work = state.clone();
            Ok(firings
                .iter()
                .flat_map(|firing| run(&mut work, *id, firing, rng))
                .collect())
        })(),
        _ => return None,
    };
    Some(result)
}

/// Who set a trigger off and with what roll.
pub(crate) struct Firing {
    pub(crate) map_id: Option<MapId>,
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
            .any(|t| t.definition.enabled && kind(&t.definition.event))
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
        if !def.enabled {
            continue;
        }
        let scope = effective_maps(after, def);
        let allowed = |token: TokenId| def.tokens.is_empty() || def.tokens.contains(&token);
        let mut candidates = Vec::new();
        match &def.event {
            TriggerEvent::RollInZone { zone, rolls } => {
                let Some((character, roll)) = roll else {
                    continue;
                };
                if !rolls.is_empty() && !rolls.iter().any(|r| r == roll) {
                    continue;
                }
                let Some(anchor) = after.zones.get(zone) else {
                    continue;
                };
                for zone in after
                    .zones
                    .values()
                    .filter(|z| scope.contains(&z.map_id) && z.name.trim() == anchor.name.trim())
                {
                    if let Some(token) = after.tokens_on(zone.map_id).find(|t| {
                        t.character_id == Some(character)
                            && allowed(t.id)
                            && zone.shape.contains(t.x, t.y)
                    }) {
                        candidates.push(Firing {
                            character: Some(character),
                            token: Some(token.id),
                            total,
                            map_id: Some(zone.map_id),
                        });
                    }
                }
            }
            TriggerEvent::TokenEnteredZone { zone } => {
                let Some(anchor) = after.zones.get(zone) else {
                    continue;
                };
                for event in events {
                    let Event::TokenMoved { id, x, y } = event else {
                        continue;
                    };
                    let (Some(token), Some(was)) = (after.tokens.get(id), before.tokens.get(id))
                    else {
                        continue;
                    };
                    if !scope.contains(&token.map_id)
                        || !allowed(*id)
                        || (def.tokens.is_empty() && token.character_id.is_none())
                    {
                        continue;
                    }
                    // A logical named zone may have several shapes on the same map.
                    let named: Vec<_> = after
                        .zones
                        .values()
                        .filter(|z| z.map_id == token.map_id && z.name.trim() == anchor.name.trim())
                        .collect();
                    if named.iter().any(|z| z.shape.contains(*x, *y))
                        && !named.iter().any(|z| z.shape.contains(was.x, was.y))
                    {
                        candidates.push(Firing {
                            character: token.character_id,
                            token: Some(*id),
                            total: None,
                            map_id: Some(token.map_id),
                        });
                    }
                }
            }
            TriggerEvent::ItemReceived { name } => {
                let Some(rules) = after.rules() else {
                    continue;
                };
                for event in events {
                    let Event::CharacterUpdated { character } = event else {
                        continue;
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
                            .filter(|n| n.to_lowercase() == name.to_lowercase())
                            .count()
                    };
                    if count(&new) <= count(&old) {
                        continue;
                    }
                    if scope.is_empty() {
                        let token = after
                            .tokens
                            .values()
                            .find(|t| t.character_id == Some(character.id) && allowed(t.id))
                            .map(|t| t.id);
                        if !def.tokens.is_empty() && token.is_none() {
                            continue;
                        }
                        candidates.push(Firing {
                            character: Some(character.id),
                            token,
                            total: None,
                            map_id: None,
                        });
                    } else {
                        for token in after.tokens.values().filter(|t| {
                            t.character_id == Some(character.id)
                                && scope.contains(&t.map_id)
                                && allowed(t.id)
                        }) {
                            candidates.push(Firing {
                                character: Some(character.id),
                                token: Some(token.id),
                                total: None,
                                map_id: Some(token.map_id),
                            });
                        }
                    }
                }
            }
            TriggerEvent::Manual => {}
        }
        let mut seen = Vec::new();
        for firing in candidates {
            if !seen.contains(&firing.map_id)
                && available(trigger, firing.map_id)
                && condition_holds(after, def.condition.as_deref(), &firing, &BTreeMap::new())
            {
                seen.push(firing.map_id);
                found.push((trigger.id, firing));
            }
        }
    }
    found
}

/// Resolve old zone definitions without mutating snapshots or replayed events.
fn effective_maps(state: &GameState, def: &TriggerDef) -> Vec<MapId> {
    if !def.map_ids.is_empty() {
        return def.map_ids.clone();
    }
    let zone = match def.event {
        TriggerEvent::RollInZone { zone, .. } | TriggerEvent::TokenEnteredZone { zone } => {
            Some(zone)
        }
        _ => None,
    };
    zone.and_then(|id| state.zones.get(&id))
        .map(|z| vec![z.map_id])
        .unwrap_or_default()
}

fn available(trigger: &TriggerDto, map_id: Option<MapId>) -> bool {
    // A legacy spent record has no map list: preserve its spent state until explicitly edited.
    if trigger.fired && trigger.fired_maps.is_empty() {
        return false;
    }
    map_id.map_or(!trigger.fired, |id| !trigger.fired_maps.contains(&id))
}

/// Runs a trigger's steps against `work`, which follows every event produced. Returns the events.
fn run(work: &mut GameState, id: TriggerId, firing: &Firing, rng: &mut dyn GameRng) -> Vec<Event> {
    let Some(trigger) = work.triggers.get(&id).cloned() else {
        return Vec::new();
    };
    if !available(&trigger, firing.map_id) {
        return Vec::new();
    }
    let mut events = vec![Event::TriggerFired {
        fired: TriggerFiredDto {
            trigger: id,
            name: trigger.definition.name.clone(),
            character: firing.character,
            token: firing.token,
            map_id: firing.map_id,
        },
    }];
    if trigger.definition.once
        && let Ok(version) = bump(trigger.version)
    {
        let mut next = trigger.clone();
        next.version = version;
        if let Some(map_id) = firing.map_id {
            next.fired_maps.push(map_id);
            next.fired_maps.sort();
            next.fired_maps.dedup();
            next.fired = effective_maps(work, &next.definition)
                .iter()
                .all(|id| next.fired_maps.contains(id));
        } else {
            next.fired = true;
        }
        events.push(Event::TriggerUpdated { trigger: next });
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
        TriggerAction::PlaySound {
            asset,
            volume,
            looped,
        } => work
            .assets
            .get(asset)
            .and_then(|a| crate::library::sound(work, a, *volume, *looped).ok())
            .map(|sound| vec![Event::SoundPlayed { sound }])
            .unwrap_or_default(),
        TriggerAction::StopSound { asset } => vec![Event::SoundStopped { asset_id: *asset }],
        TriggerAction::SpawnTokens {
            character,
            count,
            placement,
        } => spawn_tokens(work, *character, *count, *placement, firing, rng),
        TriggerAction::StartCombat => start_combat(work, firing, rng),
        TriggerAction::PlaySting { asset, volume } => work
            .assets
            .get(asset)
            .and_then(|a| crate::library::sound(work, a, *volume, false).ok())
            .map(|mut sound| {
                sound.sting = true;
                vec![Event::SoundPlayed { sound }]
            })
            .unwrap_or_default(),
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

/// A fresh id of any entity kind, drawn from the core's RNG (deterministic for a seed).
fn fresh_id(rng: &mut dyn GameRng) -> dnd_protocol::Uuid {
    let bits = (u128::from(rng.next_u64()) << 64) | u128::from(rng.next_u64());
    uuid_v7_from_random(bits)
}

/// Tokens of `character` appear on the firing map; nothing spawns without a map context.
fn spawn_tokens(
    work: &GameState,
    character: CharacterId,
    count: u8,
    placement: SpawnPlacement,
    firing: &Firing,
    rng: &mut dyn GameRng,
) -> Vec<Event> {
    let Some(map_id) = firing.map_id else {
        return Vec::new();
    };
    let Some(who) = work.characters.get(&character) else {
        return Vec::new();
    };
    let mut events = Vec::new();
    for n in 0..count {
        let Some((x, y)) = spawn_point(work, map_id, placement, n, rng) else {
            break;
        };
        events.push(Event::TokenCreated {
            token: TokenDto {
                id: TokenId(fresh_id(rng)),
                map_id,
                character_id: Some(character),
                asset_id: None,
                name: who.name.clone(),
                x,
                y,
                rotation: 0.0,
                scale: 1.0,
                layer: TokenLayer::Tokens,
                visible: true,
                version: Version::INITIAL,
            },
        });
    }
    events
}

/// Where the `n`-th spawned token lands: a fanned-out point, or a random spot inside the zone.
fn spawn_point(
    state: &GameState,
    map_id: MapId,
    placement: SpawnPlacement,
    n: u8,
    rng: &mut dyn GameRng,
) -> Option<(i64, i64)> {
    let map = state.maps.get(&map_id)?;
    let point = match placement {
        SpawnPlacement::Point { x, y } => {
            let step = (map.grid.cell_size * 16.0).max(16.0) as i64;
            let (col, row) = (i64::from(n) % 5, i64::from(n) / 5);
            (x + col * step, y + row * step)
        }
        SpawnPlacement::Zone { id } => {
            let zone = state.zones.get(&id).filter(|z| z.map_id == map_id)?;
            let (min_x, min_y, max_x, max_y) = shape_bounds(&zone.shape)?;
            let mut found = None;
            for _ in 0..32 {
                let span_x = u64::try_from(max_x - min_x).unwrap_or(0).max(1);
                let span_y = u64::try_from(max_y - min_y).unwrap_or(0).max(1);
                let px = min_x + i64::try_from(rng.next_u64() % span_x).unwrap_or(0);
                let py = min_y + i64::try_from(rng.next_u64() % span_y).unwrap_or(0);
                if zone.shape.contains(px, py) {
                    found = Some((px, py));
                    break;
                }
            }
            found.unwrap_or(((min_x + max_x) / 2, (min_y + max_y) / 2))
        }
    };
    Some((
        point.0.clamp(0, map.width * 16),
        point.1.clamp(0, map.height * 16),
    ))
}

/// The bounding box of every part of a zone shape, in Units16.
fn shape_bounds(shape: &ZoneShape) -> Option<(i64, i64, i64, i64)> {
    let mut bounds: Option<(i64, i64, i64, i64)> = None;
    for part in shape.parts() {
        let part_bounds = match &part {
            ZoneShape::Rect {
                x,
                y,
                width,
                height,
            } => (*x, *y, x.saturating_add(*width), y.saturating_add(*height)),
            ZoneShape::Polygon { points } => {
                let xs = points.as_chunks::<2>().0.iter().map(|[x, _]| *x);
                let ys = points.as_chunks::<2>().0.iter().map(|[_, y]| *y);
                match (xs.clone().min(), ys.clone().min(), xs.max(), ys.max()) {
                    (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
                    _ => continue,
                }
            }
            ZoneShape::Union { .. } => continue,
        };
        bounds = Some(match bounds {
            None => part_bounds,
            Some((a, b, c, d)) => (
                a.min(part_bounds.0),
                b.min(part_bounds.1),
                c.max(part_bounds.2),
                d.max(part_bounds.3),
            ),
        });
    }
    bounds
}

/// Opens a combat on the firing map: visible character tokens join and roll initiative at once.
fn start_combat(work: &GameState, firing: &Firing, rng: &mut dyn GameRng) -> Vec<Event> {
    let Some(map_id) = firing.map_id else {
        return Vec::new();
    };
    if work.combats.values().any(|c| c.map_id == map_id) {
        return Vec::new();
    }
    let combat_id = CombatId(fresh_id(rng));
    let mut events = vec![Event::CombatStarted {
        combat: CombatDto {
            id: combat_id,
            map_id,
            round: 1,
            turn: None,
            version: Version::INITIAL,
        },
    }];
    let mut sort_order = 0;
    for token in work.tokens.values().filter(|t| {
        t.map_id == map_id && t.visible && t.layer != TokenLayer::Gm && t.character_id.is_some()
    }) {
        if sort_order >= MAX_COMBATANTS {
            break;
        }
        let Some(character_id) = token.character_id else {
            continue;
        };
        let character = work.characters.get(&character_id);
        let name = if token.name.is_empty() {
            character.map_or_else(String::new, |c| c.name.clone())
        } else {
            token.name.clone()
        };
        let sheet = character.map(|c| c.sheet_json.as_str());
        let expression = work
            .rules()
            .map_or_else(|| initiative_expression(sheet), |r| r.initiative.as_str());
        let initiative = dnd_dice::Expr::parse(expression)
            .and_then(|e| e.roll(&CharacterFields::new(work, sheet), &mut DiceRngAdapter(rng)))
            .ok();
        if let Some(roll) = &initiative {
            events.push(posted(ChatMessageDto {
                id: message_id(rng),
                kind: ChatKind::Roll,
                sender: None,
                sender_name: name.clone(),
                body: expression.to_owned(),
                roll: Some(roll_dto(roll)),
                system: None,
                visibility: ChatVisibility::All,
                at_ms: None,
            }));
        }
        events.push(Event::CombatantAdded {
            combatant: CombatantDto {
                id: CombatantId(fresh_id(rng)),
                combat_id,
                token_id: Some(token.id),
                character_id: Some(character_id),
                name,
                initiative: initiative.map(|r| r.total),
                sort_order: i64::try_from(sort_order).unwrap_or(i64::MAX),
                version: Version::INITIAL,
            },
        });
        sort_order += 1;
    }
    events
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
    if def.map_ids.len() > 100 || def.map_ids.iter().any(|id| !state.maps.contains_key(id)) {
        return Err(RejectReason::invalid("map_ids"));
    }
    def.map_ids.sort();
    def.map_ids.dedup();
    if def.map_ids.is_empty() {
        def.map_ids = effective_maps(state, &def);
    }
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
        TriggerAction::PlaySound { asset, volume, .. } => {
            *volume <= dnd_protocol::MAX_VOLUME
                && state
                    .assets
                    .get(asset)
                    .is_some_and(|a| a.usage == dnd_protocol::AssetUsage::Audio)
        }
        TriggerAction::StopSound { asset } => asset.is_none_or(|a| {
            state
                .assets
                .get(&a)
                .is_some_and(|a| a.usage == dnd_protocol::AssetUsage::Audio)
        }),
        TriggerAction::SpawnTokens {
            character,
            count,
            placement,
        } => {
            state.characters.contains_key(character)
                && *count >= 1
                && usize::from(*count) <= MAX_SPAWN_TOKENS
                && match placement {
                    SpawnPlacement::Point { .. } => true,
                    SpawnPlacement::Zone { id } => state.zones.contains_key(id),
                }
        }
        TriggerAction::StartCombat => true,
        TriggerAction::PlaySting { asset, volume } => {
            *volume <= dnd_protocol::MAX_VOLUME
                && state
                    .assets
                    .get(asset)
                    .is_some_and(|a| a.usage == dnd_protocol::AssetUsage::Audio)
        }
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

/// Binding visuals are part of the same transaction as edits/placing a trigger card.
/// Edges between trigger and map cards express bindings; map cards themselves are never removed.
pub(crate) fn sync_board_bindings(
    state: &GameState,
    intent: &Intent,
    events: &[Event],
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    use dnd_protocol::{
        BoardEdgeDto, BoardEdgeId, BoardLink, BoardNodeContent, BoardNodeDto, BoardNodeId,
        MAX_BOARD_COORD, MAX_BOARD_EDGES, MAX_BOARD_NODES,
    };
    let trigger_id = match intent {
        Intent::CreateTrigger { id, .. } | Intent::UpdateTrigger { id, .. } => *id,
        Intent::CreateBoardNode {
            content: BoardNodeContent::Trigger { id },
            ..
        }
        | Intent::UpdateBoardNode {
            content: Some(BoardNodeContent::Trigger { id }),
            ..
        } => *id,
        _ => return Ok(Vec::new()),
    };
    let mut work = state.clone();
    for event in events {
        evolve(&mut work, event);
    }
    let Some(trigger) = work.triggers.get(&trigger_id) else {
        return Ok(Vec::new());
    };
    let maps = effective_maps(&work, &trigger.definition);
    let cards: Vec<_> = work
        .board_nodes
        .values()
        .filter(|n| matches!(n.content, BoardNodeContent::Trigger { id } if id == trigger_id))
        .cloned()
        .collect();
    let mut out = Vec::new();
    let fresh = |work: &GameState, rng: &mut dyn GameRng| {
        // Bounded retries also handle deterministic test RNGs which repeat IDs across intents.
        for _ in 0..4096 {
            let id = message_id(rng).0;
            if !crate::apply::id_in_use(work, id) {
                return Ok(id);
            }
        }
        Err(RejectReason::invalid("board_id"))
    };
    let emit = |work: &mut GameState, out: &mut Vec<Event>, event: Event| {
        evolve(work, &event);
        out.push(event);
    };
    for card in cards {
        let removed: Vec<_> = work.board_edges.values().filter(|e| e.from == card.id || e.to == card.id).filter(|e| {
            let other = if e.from == card.id { e.to } else { e.from };
            work.board_nodes.get(&other).is_some_and(|n| matches!(n.content, BoardNodeContent::Link { target: BoardLink::Map { id } } if !maps.contains(&id)))
        }).map(|e| e.id).collect();
        for id in removed {
            emit(&mut work, &mut out, Event::BoardEdgeDeleted { id });
        }
        for (index, map_id) in maps.iter().enumerate() {
            if !work.maps.contains_key(map_id) {
                continue;
            }
            let existing = work.board_nodes.values().find(|n| n.board_id == card.board_id && matches!(n.content, BoardNodeContent::Link { target: BoardLink::Map { id } } if id == *map_id)).map(|n| n.id);
            let map_card = if let Some(id) = existing {
                id
            } else {
                if work
                    .board_nodes
                    .values()
                    .filter(|n| n.board_id == card.board_id)
                    .count()
                    >= MAX_BOARD_NODES
                {
                    return Err(RejectReason::invalid("board_id"));
                }
                let id = BoardNodeId(fresh(&work, rng)?);
                emit(
                    &mut work,
                    &mut out,
                    Event::BoardNodeCreated {
                        node: BoardNodeDto {
                            id,
                            board_id: card.board_id,
                            content: BoardNodeContent::Link {
                                target: BoardLink::Map { id: *map_id },
                            },
                            x: card
                                .x
                                .saturating_add(card.width)
                                .saturating_add(640)
                                .clamp(-MAX_BOARD_COORD, MAX_BOARD_COORD),
                            y: card
                                .y
                                .saturating_add((index as i64).saturating_mul(2560))
                                .clamp(-MAX_BOARD_COORD, MAX_BOARD_COORD),
                            width: 3200,
                            height: 1920,
                            visibility: BoardVisibility::Gm,
                            version: Version::INITIAL,
                        },
                    },
                );
                id
            };
            if work.board_edges.values().any(|e| {
                (e.from == card.id && e.to == map_card) || (e.to == card.id && e.from == map_card)
            }) {
                continue;
            }
            if work
                .board_edges
                .values()
                .filter(|e| e.board_id == card.board_id)
                .count()
                >= MAX_BOARD_EDGES
            {
                return Err(RejectReason::invalid("board_id"));
            }
            let id = BoardEdgeId(fresh(&work, rng)?);
            emit(
                &mut work,
                &mut out,
                Event::BoardEdgeCreated {
                    edge: BoardEdgeDto {
                        id,
                        board_id: card.board_id,
                        from: card.id,
                        to: map_card,
                        label: String::new(),
                        directed: true,
                        visibility: BoardVisibility::Gm,
                        version: Version::INITIAL,
                    },
                },
            );
        }
    }
    Ok(out)
}
