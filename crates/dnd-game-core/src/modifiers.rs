//! Pure evaluation of plugin-declared condition consequences. No game-system identifiers.
use crate::plugins::{CharacterFields, SheetContext};
use crate::{GameRng, GameState};
use dnd_expr::{Context, LookupError, Value};
use dnd_protocol::*;
use std::collections::{BTreeMap, BTreeSet};

fn invalid() -> RejectReason {
    RejectReason::invalid("condition_rules")
}
pub(crate) fn feature_enabled(state: &GameState, id: Option<&str>) -> bool {
    id.is_none_or(|id| {
        state
            .system_plugin
            .as_ref()
            .and_then(|p| state.plugins.get(p))
            .is_some_and(|p| {
                p.system.as_ref().is_some_and(|r| {
                    r.features.iter().any(|f| {
                        f.id == id
                            && p.feature_settings
                                .get(id)
                                .copied()
                                .unwrap_or(f.default_enabled)
                    })
                })
            })
    })
}
/// Items of `c` that work right now (ADR-0062): owned, instances, in an active state. A state id
/// the campaign no longer defines counts as inactive, so a stray id cannot keep a bonus alive.
pub(crate) fn active_items<'a>(state: &'a GameState, c: &CharacterDto) -> Vec<&'a ItemDto> {
    let states = state.campaign.item_states();
    let active = |item: &ItemDto| match &item.state {
        None => crate::items::default_state_active(&states),
        Some(id) => states.iter().any(|s| s.id == *id && s.active),
    };
    state
        .items
        .values()
        .filter(|item| item.owner == Some(c.id) && !item.is_template && active(item))
        .collect()
}

fn base_conditions(state: &GameState, c: &CharacterDto) -> BTreeSet<String> {
    c.conditions
        .iter()
        .cloned()
        .chain(
            active_items(state, c)
                .into_iter()
                .flat_map(|item| item.grants.clone()),
        )
        .chain(
            state
                .effects
                .values()
                .filter(|e| {
                    state
                        .tokens
                        .get(&e.token)
                        .is_some_and(|t| t.character_id == Some(c.id))
                })
                .filter_map(|e| e.condition.clone()),
        )
        .collect()
}
struct RuleContext<'a> {
    actor: &'a CharacterFields,
    target: Option<&'a CharacterFields>,
    actor_conditions: &'a BTreeSet<String>,
    target_conditions: Option<&'a BTreeSet<String>>,
    distance: Option<i64>,
}
impl Context for RuleContext<'_> {
    fn lookup(&self, path: &str) -> Result<Value, LookupError> {
        if let Some(id) = path.strip_prefix("actor.condition.") {
            return Ok(Value::from(self.actor_conditions.contains(id)));
        }
        if let Some(id) = path.strip_prefix("target.condition.") {
            return self
                .target_conditions
                .map(|c| Value::from(c.contains(id)))
                .ok_or(LookupError::Unknown);
        }
        if path == "event.distance_cells" {
            return self.distance.map(Value::from).ok_or(LookupError::Unknown);
        }
        if let Some(path) = path.strip_prefix("actor.") {
            return Context::lookup(self.actor, path);
        }
        if let Some(path) = path.strip_prefix("target.") {
            return self
                .target
                .ok_or(LookupError::Unknown)
                .and_then(|t| Context::lookup(t, path));
        }
        Context::lookup(self.actor, path)
    }
}
fn holds(context: &impl Context, expression: &str) -> bool {
    expression.is_empty()
        || dnd_expr::Expr::parse(expression)
            .and_then(|e| e.eval(context))
            .is_ok_and(|v| {
                v.as_bool()
                    .unwrap_or_else(|| v.as_integer().is_ok_and(|n| n != 0))
            })
}
/// Conditions a character has: stored ones, those of its token effects, and those its active
/// items grant (ADR-0062). Plugins decide what each condition does.
pub fn active_conditions(state: &GameState, c: &CharacterDto) -> BTreeSet<String> {
    let mut set = base_conditions(state, c);
    let Some(rules) = state.rules() else {
        return set;
    };
    let fields = CharacterFields::for_character(state, c);
    for _ in 0..=rules.conditions.len() {
        let context = RuleContext {
            actor: &fields,
            target: None,
            actor_conditions: &set,
            target_conditions: None,
            distance: None,
        };
        let implied: Vec<String> = rules
            .mechanics
            .condition_rules
            .iter()
            .filter(|r| {
                r.subject == RuleSubject::Actor
                    && set.contains(&r.condition)
                    && feature_enabled(state, r.feature.as_deref())
                    && holds(&context, &r.when)
            })
            .flat_map(|r| {
                r.effects.iter().filter_map(|e| {
                    if let ConditionModifier::Imply { condition } = e {
                        Some(condition.clone())
                    } else {
                        None
                    }
                })
            })
            .collect();
        let old = set.len();
        set.extend(implied);
        if set.len() == old {
            break;
        }
    }
    set
}
pub(crate) fn allowed(state: &GameState, c: &CharacterDto, capability: RuleCapability) -> bool {
    let Some(rules) = state.rules() else {
        return true;
    };
    let set = active_conditions(state, c);
    let fields = CharacterFields::for_character(state, c);
    let context = RuleContext {
        actor: &fields,
        target: None,
        actor_conditions: &set,
        target_conditions: None,
        distance: None,
    };
    !rules.mechanics.condition_rules.iter().any(|r| {
        r.subject == RuleSubject::Actor
            && set.contains(&r.condition)
            && feature_enabled(state, r.feature.as_deref())
            && holds(&context, &r.when)
            && r.effects
                .iter()
                .any(|e| matches!(e,ConditionModifier::Deny{capability:c}if *c==capability))
    })
}

#[derive(Clone, Copy, Debug, Default)]
struct Adjustment {
    add: i64,
    minimum: Option<i64>,
    maximum: Option<i64>,
}
impl Adjustment {
    fn apply(self, n: i64) -> i64 {
        let mut n = n.saturating_add(self.add);
        if let Some(min) = self.minimum {
            n = n.max(min);
        }
        if let Some(max) = self.maximum {
            n = n.min(max);
        }
        n
    }
}
fn adjustments(state: &GameState, c: &CharacterDto) -> BTreeMap<String, Adjustment> {
    let Some(rules) = state.rules() else {
        return BTreeMap::new();
    };
    let set = active_conditions(state, c);
    let fields = CharacterFields::for_character(state, c);
    let context = RuleContext {
        actor: &fields,
        target: None,
        actor_conditions: &set,
        target_conditions: None,
        distance: None,
    };
    let mut result: BTreeMap<String, Adjustment> = BTreeMap::new();
    // Active items add up exactly like condition rules: summed, then bounded (ADR-0062).
    for item in active_items(state, c) {
        for effect in &item.effects {
            let a = result.entry(effect.path.clone()).or_default();
            a.add = a.add.saturating_add(effect.add);
            if let Some(min) = effect.minimum {
                a.minimum = Some(a.minimum.map_or(min, |v| v.max(min)));
            }
            if let Some(max) = effect.maximum {
                a.maximum = Some(a.maximum.map_or(max, |v| v.min(max)));
            }
        }
    }
    for rule in rules.mechanics.condition_rules.iter().filter(|r| {
        r.subject == RuleSubject::Actor
            && set.contains(&r.condition)
            && feature_enabled(state, r.feature.as_deref())
            && holds(&context, &r.when)
    }) {
        for effect in &rule.effects {
            if let ConditionModifier::Parameter {
                path,
                add,
                minimum,
                maximum,
            } = effect
            {
                let a = result.entry(path.clone()).or_default();
                a.add = a.add.saturating_add(*add);
                if let Some(min) = minimum {
                    a.minimum = Some(a.minimum.map_or(*min, |v| v.max(*min)));
                }
                if let Some(max) = maximum {
                    a.maximum = Some(a.maximum.map_or(*max, |v| v.min(*max)));
                }
            }
        }
    }
    result
}

/// Effective numeric fields; stored sheet values remain unchanged. Dependencies see adjusted values.
pub fn effective_fields(
    state: &GameState,
    c: &CharacterDto,
) -> Result<CharacterFields, RejectReason> {
    let Some(rules) = state.rules() else {
        return Ok(CharacterFields::for_character(state, c));
    };
    let raw: serde_json::Value = serde_json::from_str(&c.sheet_json).map_err(|_| invalid())?;
    let mut sheet = sheet_schema::apply_defaults(&rules.sheet_schema, &raw);
    let adjustments = adjustments(state, c);
    for (path, adjust) in &adjustments {
        if rules.computed.contains_key(path) {
            continue;
        }
        let mut node = &mut sheet;
        for segment in path.split('.') {
            node = node.get_mut(segment).ok_or_else(invalid)?;
        }
        let value = adjust.apply(node.as_i64().ok_or_else(invalid)?);
        if !is_js_safe(value) {
            return Err(invalid());
        }
        *node = value.into();
    }
    let computed = dnd_expr::Computed::resolve_adjusted(
        &rules.computed,
        &SheetContext(&sheet),
        |name, value| adjustments.get(name).map_or(value, |a| a.apply(value)),
    )
    .map_err(|_| invalid())?
    .into_values();
    if computed.values().any(|n| !is_js_safe(*n)) {
        return Err(invalid());
    }
    let mut fields = CharacterFields::for_character(state, c);
    fields.sheet = sheet;
    fields.computed = computed;
    Ok(fields)
}

/// Labels of active actor rules, suitable for explaining effective values in the sheet.
pub fn active_modifier_labels(state: &GameState, c: &CharacterDto) -> Vec<Label> {
    let Some(rules) = state.rules() else {
        return Vec::new();
    };
    let set = active_conditions(state, c);
    let fields = CharacterFields::for_character(state, c);
    let context = RuleContext {
        actor: &fields,
        target: None,
        actor_conditions: &set,
        target_conditions: None,
        distance: None,
    };
    let mut labels: Vec<Label> = active_items(state, c)
        .into_iter()
        .filter(|item| !item.effects.is_empty() || !item.grants.is_empty())
        .map(|item| Label::Text(item.name.clone()))
        .collect();
    labels.extend(
        rules
            .mechanics
            .condition_rules
            .iter()
            .filter(|r| {
                r.subject == RuleSubject::Actor
                    && set.contains(&r.condition)
                    && feature_enabled(state, r.feature.as_deref())
                    && holds(&context, &r.when)
            })
            .map(|r| r.label.clone()),
    );
    labels
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RollContext<'a> {
    pub actor: &'a CharacterDto,
    pub target: Option<&'a CharacterDto>,
    pub kind: RollKind,
    pub tags: &'a [String],
    pub distance: Option<i64>,
}
pub(crate) fn roll_mode(state: &GameState, input: RollContext<'_>) -> (bool, bool) {
    let Some(rules) = state.rules() else {
        return (false, false);
    };
    let actor_set = active_conditions(state, input.actor);
    let target_set = input.target.map(|c| active_conditions(state, c));
    let actor = CharacterFields::for_character(state, input.actor);
    let target = input
        .target
        .map(|c| CharacterFields::for_character(state, c));
    let context = RuleContext {
        actor: &actor,
        target: target.as_ref(),
        actor_conditions: &actor_set,
        target_conditions: target_set.as_ref(),
        distance: input.distance,
    };
    let (mut adv, mut dis) = (false, false);
    for rule in &rules.mechanics.condition_rules {
        let subject = match rule.subject {
            RuleSubject::Actor => Some(&actor_set),
            RuleSubject::Target => target_set.as_ref(),
        };
        if !subject.is_some_and(|s| s.contains(&rule.condition))
            || !feature_enabled(state, rule.feature.as_deref())
            || !holds(&context, &rule.when)
        {
            continue;
        }
        for effect in &rule.effects {
            if let ConditionModifier::Roll { kind, tags, mode } = effect
                && *kind == input.kind
                && tags.iter().all(|tag| input.tags.contains(tag))
            {
                match mode {
                    RollMode::Advantage => adv = true,
                    RollMode::Disadvantage => dis = true,
                }
            }
        }
    }
    (adv, dis)
}

pub(crate) fn roll(
    state: &GameState,
    dice: &str,
    input: RollContext<'_>,
    rng: &mut dyn GameRng,
) -> Result<dnd_dice::Roll, RejectReason> {
    let (adv, dis) = roll_mode(state, input);
    let fields = effective_fields(state, input.actor)?;
    let expr = dnd_dice::Expr::parse(dice)
        .and_then(|e| e.with_d20_mode(adv, dis))
        .map_err(|_| RejectReason::invalid("dice"))?;
    expr.roll(&fields, &mut crate::chat::DiceRngAdapter(rng))
        .map_err(|_| RejectReason::invalid("dice"))
}

pub(crate) fn validate(rules: &SystemRulesDto) -> Result<(), RejectReason> {
    let defs = &rules.mechanics.condition_rules;
    if defs.is_empty() {
        return Ok(());
    }
    if defs.len() > 128 {
        return Err(invalid());
    }
    let mut ids = BTreeSet::new();
    let defaults = crate::default_sheet(rules);
    let context = SheetContext(&defaults);
    let computed = dnd_expr::Computed::resolve(&rules.computed, &context).map_err(|_| invalid())?;
    let path_exists = |path: &str| {
        computed.get(path).is_some()
            || Context::lookup(&context, path).is_ok_and(|v| v.as_integer().is_ok())
    };
    let mut resource_fields: BTreeSet<String> = rules
        .resources
        .iter()
        .flat_map(|r| [&r.value, &r.max])
        .flat_map(|expression| {
            dnd_expr::Expr::parse(expression)
                .map(|e| {
                    e.references()
                        .into_iter()
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .collect();
    for _ in 0..=rules.computed.len() {
        let additions: Vec<String> = resource_fields
            .iter()
            .filter_map(|name| rules.computed.get(name))
            .flat_map(|expression| {
                dnd_expr::Expr::parse(expression)
                    .map(|e| {
                        e.references()
                            .into_iter()
                            .map(str::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            })
            .collect();
        let before = resource_fields.len();
        resource_fields.extend(additions);
        if before == resource_fields.len() {
            break;
        }
    }
    for rule in defs {
        if rule.id.is_empty()
            || rule.id.len() > 64
            || !rule
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || !ids.insert(&rule.id)
            || rule.label.get("en").is_empty()
            || rule.label.get("en").len() > 800
            || !rules.conditions.iter().any(|c| c.id == rule.condition)
            || rule
                .feature
                .as_ref()
                .is_some_and(|id| !rules.features.iter().any(|f| f.id == *id))
            || rule.effects.is_empty()
            || rule.effects.len() > 16
            || rule.when.len() > 1024
        {
            return Err(invalid());
        }
        if !rule.when.is_empty() {
            let expr = dnd_expr::Expr::parse(&rule.when).map_err(|_| invalid())?;
            if rule
                .effects
                .iter()
                .any(|e| !matches!(e, ConditionModifier::Roll { .. }))
                && expr
                    .references()
                    .iter()
                    .any(|path| path.starts_with("target.") || path.starts_with("event."))
            {
                return Err(invalid());
            }
            for path in expr.references() {
                if path == "event.distance_cells" {
                    continue;
                }
                if let Some(id) = path
                    .strip_prefix("actor.condition.")
                    .or_else(|| path.strip_prefix("target.condition."))
                {
                    if !rules.conditions.iter().any(|c| c.id == id) {
                        return Err(invalid());
                    }
                    continue;
                }
                if !path_exists(
                    path.strip_prefix("actor.")
                        .or_else(|| path.strip_prefix("target."))
                        .unwrap_or(path),
                ) {
                    return Err(invalid());
                }
            }
        }
        for effect in &rule.effects {
            match effect {
                ConditionModifier::Imply { condition } => {
                    if rule.subject != RuleSubject::Actor
                        || !rules.conditions.iter().any(|c| c.id == *condition)
                    {
                        return Err(invalid());
                    }
                }
                ConditionModifier::Deny { .. } => {
                    if rule.subject != RuleSubject::Actor {
                        return Err(invalid());
                    }
                }
                ConditionModifier::Roll { tags, .. } => {
                    if tags.len() > 16 || tags.iter().any(|t| t.is_empty() || t.len() > 64) {
                        return Err(invalid());
                    }
                }
                ConditionModifier::Parameter {
                    path,
                    add,
                    minimum,
                    maximum,
                } => {
                    if rule.subject != RuleSubject::Actor
                        || !path_exists(path)
                        || add.unsigned_abs() > 1_000_000
                        || minimum.is_some_and(|v| !is_js_safe(v))
                        || maximum.is_some_and(|v| !is_js_safe(v))
                    {
                        return Err(invalid());
                    }
                    if !rule.when.is_empty()
                        && dnd_expr::Expr::parse(&rule.when)
                            .map_err(|_| invalid())?
                            .references()
                            .iter()
                            .any(|p| p.starts_with("target.") || p.starts_with("event."))
                    {
                        return Err(invalid());
                    }
                    if resource_fields.contains(path) {
                        return Err(invalid());
                    }
                }
            }
        }
    }
    Ok(())
}
