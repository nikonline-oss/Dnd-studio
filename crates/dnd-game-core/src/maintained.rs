//! System-neutral ownership and lifecycle of maintained token effects.
use crate::{GameRng, GameState, evolve};
use dnd_expr::{Context, LookupError, Value};
use dnd_protocol::*;
use std::collections::{BTreeMap, BTreeSet};

fn invalid() -> RejectReason {
    RejectReason::invalid("maintained_effect")
}
fn emit(state: &mut GameState, out: &mut Vec<Event>, events: Vec<Event>) {
    for event in events {
        evolve(state, &event);
        out.push(event);
    }
}
fn runtime(state: &GameState) -> Event {
    Event::PluginRuntimeUpdated {
        runtime: Box::new(state.plugin_runtime.clone()),
    }
}
fn rule<'a>(state: &'a GameState, id: &str) -> Option<&'a MaintainedRule> {
    state
        .rules()?
        .mechanics
        .maintained_rules
        .iter()
        .find(|r| r.id == id)
}
struct Scope<'a> {
    fields: &'a dyn Context,
    conditions: &'a BTreeSet<String>,
    known: &'a BTreeSet<String>,
    vars: BTreeMap<String, i64>,
}
impl Context for Scope<'_> {
    fn lookup(&self, path: &str) -> Result<Value, LookupError> {
        if let Some(id) = path.strip_prefix("condition.") {
            return if self.known.contains(id) {
                Ok(Value::Bool(self.conditions.contains(id)))
            } else {
                Err(LookupError::Unknown)
            };
        }
        self.vars
            .get(path)
            .map_or_else(|| self.fields.lookup(path), |n| Ok(Value::from(*n)))
    }
}
fn holds(expression: &str, context: &dyn Context) -> Result<bool, RejectReason> {
    if expression.is_empty() {
        return Ok(true);
    }
    let value = dnd_expr::Expr::parse(expression)
        .and_then(|e| e.eval(context))
        .map_err(|_| invalid())?;
    Ok(value
        .as_bool()
        .unwrap_or_else(|| value.as_integer().is_ok_and(|n| n != 0)))
}
fn kept(state: &GameState, owner: &CharacterDto, policy: &MaintainedRule) -> bool {
    if !crate::modifiers::feature_enabled(state, policy.feature.as_deref()) {
        return false;
    }
    let Ok(fields) = crate::modifiers::effective_fields(state, owner) else {
        return false;
    };
    let known = state
        .rules()
        .map(|r| r.conditions.iter().map(|c| c.id.clone()).collect())
        .unwrap_or_default();
    let conditions = crate::modifiers::active_conditions(state, owner);
    holds(
        &policy.keep_condition,
        &Scope {
            fields: &fields,
            conditions: &conditions,
            known: &known,
            vars: BTreeMap::new(),
        },
    )
    .unwrap_or(false)
}

pub(crate) fn end(state: &GameState, id: EffectId) -> Result<Vec<Event>, RejectReason> {
    let entry = state
        .plugin_runtime
        .maintained
        .iter()
        .find(|m| m.id == id)
        .ok_or(RejectReason::NotFound)?;
    let mut work = state.clone();
    let mut out = Vec::new();
    for id in &entry.effects {
        if work.effects.contains_key(id) {
            emit(
                &mut work,
                &mut out,
                vec![Event::TokenEffectRemoved {
                    id: *id,
                    expired: false,
                }],
            );
        }
    }
    work.plugin_runtime.maintained.retain(|m| m.id != id);
    out.push(runtime(&work));
    Ok(out)
}

/// A replacement begins at declaration even when the new action is later countered.
pub(crate) fn release_for_action(
    state: &GameState,
    source: &TokenDto,
    action: &PluginAction,
) -> Result<Vec<Event>, RejectReason> {
    let Some(def) = &action.maintain else {
        return Ok(Vec::new());
    };
    let policy = rule(state, &def.rule).ok_or_else(invalid)?;
    let owner = source
        .character_id
        .and_then(|id| state.characters.get(&id))
        .ok_or_else(invalid)?;
    if !kept(state, owner, policy) {
        return Err(invalid());
    }
    let ids: Vec<_> = state
        .plugin_runtime
        .maintained
        .iter()
        .filter(|e| {
            e.owner == owner.id
                && rule(state, &e.rule).is_some_and(|r| r.exclusive_group == policy.exclusive_group)
        })
        .map(|e| e.id)
        .collect();
    let mut work = state.clone();
    let mut out = Vec::new();
    for id in ids {
        let events = end(&work, id)?;
        emit(&mut work, &mut out, events);
    }
    Ok(out)
}

pub(crate) fn start(
    state: &GameState,
    source: &TokenDto,
    action: &PluginAction,
    targets: &[TokenId],
    at_ms: i64,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    let Some(def) = &action.maintain else {
        return Ok(Vec::new());
    };
    let policy = rule(state, &def.rule).ok_or_else(invalid)?;
    let owner = source
        .character_id
        .and_then(|id| state.characters.get(&id))
        .ok_or_else(invalid)?;
    if targets.is_empty() || !kept(state, owner, policy) {
        return Err(invalid());
    }
    let mut work = state.clone();
    let mut out = Vec::new();
    let previous: Vec<_> = state
        .plugin_runtime
        .maintained
        .iter()
        .filter(|entry| {
            entry.owner == owner.id
                && rule(state, &entry.rule)
                    .is_some_and(|r| r.exclusive_group == policy.exclusive_group)
        })
        .map(|e| e.id)
        .collect();
    for id in previous {
        let events = end(&work, id)?;
        emit(&mut work, &mut out, events);
    }
    if work.plugin_runtime.maintained.len() >= 128 {
        return Err(invalid());
    }
    let duration = match def.duration {
        EffectDurationDef::Rounds { rounds } => EffectDuration::Rounds { remaining: rounds },
        EffectDurationDef::Turns { turns } => EffectDuration::Turns { remaining: turns },
        EffectDurationDef::GameTime { seconds } => EffectDuration::GameTime {
            until: state
                .campaign
                .game_clock
                .seconds
                .saturating_add(i64::from(seconds)),
        },
        EffectDurationDef::RealTime { minutes } => EffectDuration::RealTime {
            until_ts: at_ms.saturating_add(i64::from(minutes) * 60_000),
        },
    };
    let mut effects = Vec::new();
    for token in targets {
        if work.effects.values().filter(|e| e.token == *token).count() >= MAX_EFFECTS_PER_TOKEN {
            return Err(invalid());
        }
        let effect = TokenEffectDto {
            id: EffectId(crate::chat::message_id(rng).0),
            token: *token,
            label: action.label.get("en").to_owned(),
            condition: Some(def.condition.clone()),
            resource: None,
            dice: None,
            heal: false,
            reveal: Vec::new(),
            duration,
            version: Version::INITIAL,
        };
        effects.push(effect.id);
        emit(
            &mut work,
            &mut out,
            vec![Event::TokenEffectApplied { effect }],
        );
    }
    work.plugin_runtime.maintained.push(MaintainedEffect {
        id: EffectId(crate::chat::message_id(rng).0),
        owner: owner.id,
        source: source.id,
        rule: def.rule.clone(),
        label: action.label.clone(),
        effects,
    });
    out.push(runtime(&work));
    Ok(out)
}

/// Only typed damage calls this; the amount includes damage absorbed by buffers.
pub(crate) fn on_damage(
    state: &GameState,
    owner_id: CharacterId,
    resource: &str,
    amount: i64,
    at_ms: i64,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    if amount <= 0 {
        return Ok(Vec::new());
    }
    let entries: Vec<_> = state
        .plugin_runtime
        .maintained
        .iter()
        .filter(|e| {
            e.owner == owner_id
                && rule(state, &e.rule).is_some_and(|r| r.damage_check.resource == resource)
        })
        .cloned()
        .collect();
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let mut work = state.clone();
    let mut out = Vec::new();
    for entry in entries {
        let policy = rule(&work, &entry.rule).ok_or_else(invalid)?.clone();
        let owner = work
            .characters
            .get(&entry.owner)
            .ok_or_else(invalid)?
            .clone();
        if !kept(&work, &owner, &policy) {
            let events = end(&work, entry.id)?;
            emit(&mut work, &mut out, events);
            continue;
        }
        let fields = crate::modifiers::effective_fields(&work, &owner)?;
        let known = work
            .rules()
            .map(|r| r.conditions.iter().map(|c| c.id.clone()).collect())
            .unwrap_or_default();
        let conditions = crate::modifiers::active_conditions(&work, &owner);
        let mut context = Scope {
            fields: &fields,
            conditions: &conditions,
            known: &known,
            vars: BTreeMap::from([("damage".into(), amount)]),
        };
        let difficulty = dnd_expr::Expr::parse(&policy.damage_check.difficulty)
            .and_then(|e| e.eval_integer(&context))
            .map_err(|_| invalid())?;
        if !(0..=dnd_protocol::validate::MAX_SAFE_INTEGER).contains(&difficulty) {
            return Err(invalid());
        }
        let roll = crate::modifiers::roll(
            &work,
            &policy.damage_check.dice,
            crate::modifiers::RollContext {
                actor: &owner,
                target: None,
                kind: RollKind::Save,
                tags: &policy.damage_check.tags,
                distance: None,
            },
            rng,
        )?;
        let token = work.tokens.get(&entry.source).ok_or_else(invalid)?;
        out.push(crate::mechanics::roll_event(
            &work,
            token,
            None,
            &policy.label,
            &roll,
            at_ms,
            rng,
        ));
        context.vars.insert("difficulty".into(), difficulty);
        context.vars.insert("total".into(), roll.total);
        if !holds(&policy.damage_check.success, &context)? {
            let events = end(&work, entry.id)?;
            emit(&mut work, &mut out, events);
        }
    }
    Ok(out)
}

/// Runs before triggers and again afterwards so state changes cannot leave orphan bonuses.
pub(crate) fn cleanup(state: &GameState, events: &[Event]) -> Vec<Event> {
    if state.plugin_runtime.maintained.is_empty() && !events.iter().any(|event| {
        matches!(event, Event::PluginRuntimeUpdated { runtime } if !runtime.maintained.is_empty())
    }) { return Vec::new(); }
    let mut work = state.clone();
    for event in events {
        evolve(&mut work, event);
    }
    let changed_rules = state.rules() != work.rules();
    let mut out = Vec::new();
    for entry in work.plugin_runtime.maintained.clone() {
        let valid = !changed_rules
            && work
                .tokens
                .get(&entry.source)
                .is_some_and(|t| t.character_id == Some(entry.owner))
            && work
                .characters
                .get(&entry.owner)
                .is_some_and(|c| rule(&work, &entry.rule).is_some_and(|r| kept(&work, c, r)));
        if !valid {
            if let Ok(events) = end(&work, entry.id) {
                emit(&mut work, &mut out, events);
            }
        } else {
            let existing: Vec<_> = entry
                .effects
                .iter()
                .filter(|id| work.effects.contains_key(id))
                .copied()
                .collect();
            if existing != entry.effects {
                if existing.is_empty() {
                    work.plugin_runtime.maintained.retain(|e| e.id != entry.id);
                } else if let Some(record) = work
                    .plugin_runtime
                    .maintained
                    .iter_mut()
                    .find(|e| e.id == entry.id)
                {
                    record.effects = existing;
                }
                out.push(runtime(&work));
            }
        }
    }
    out
}

pub(crate) fn validate(rules: &SystemRulesDto, base: &dyn Context) -> Result<(), RejectReason> {
    let known: BTreeSet<_> = rules.conditions.iter().map(|c| c.id.clone()).collect();
    let empty = BTreeSet::new();
    let context = Scope {
        fields: base,
        conditions: &empty,
        known: &known,
        vars: BTreeMap::from([
            ("damage".into(), 10),
            ("difficulty".into(), 10),
            ("total".into(), 10),
        ]),
    };
    for policy in &rules.mechanics.maintained_rules {
        holds(&policy.keep_condition, &context)?;
        holds(&policy.damage_check.success, &context)?;
        let difficulty = dnd_expr::Expr::parse(&policy.damage_check.difficulty)
            .and_then(|e| e.eval_integer(&context))
            .map_err(|_| invalid())?;
        if !(0..=dnd_protocol::validate::MAX_SAFE_INTEGER).contains(&difficulty) {
            return Err(invalid());
        }
        // Dice use only actor fields; damage/total/difficulty are not dice variables.
        dnd_dice::Expr::parse(&policy.damage_check.dice)
            .and_then(|e| {
                e.roll(
                    &dnd_dice::ExprFields(base),
                    &mut dnd_dice::SplitMix64::new(0),
                )
            })
            .map_err(|_| invalid())?;
    }
    Ok(())
}
