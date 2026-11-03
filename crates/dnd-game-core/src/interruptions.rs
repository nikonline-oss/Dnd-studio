//! Bounded, persisted declaration/reaction/resolution stack. No system identifiers.
use crate::mechanics::{
    ExecutionPhase, action_costs, character, eligible, eligible_base, emit, execute_body, number,
    runtime_event,
};
use crate::{Actor, GameRng, GameState, evolve};
use dnd_protocol::*;
use std::collections::BTreeMap;

fn invalid() -> RejectReason {
    RejectReason::invalid("action_stack")
}
fn action<'a>(s: &'a GameState, id: &str) -> Option<&'a PluginAction> {
    s.rules()?.mechanics.actions.iter().find(|a| a.id == id)
}
pub(crate) fn reserved(s: &GameState, owner: CharacterId, resource: &str) -> i64 {
    s.plugin_runtime
        .action_stacks
        .iter()
        .flat_map(|s| &s.frames)
        .filter(|f| f.owner == owner)
        .flat_map(|f| &f.deferred)
        .filter(|c| c.resource == resource)
        .map(|c| c.amount)
        .sum()
}
pub(crate) fn busy(s: &GameState, source: TokenId) -> bool {
    s.tokens.get(&source).is_some_and(|t| {
        s.plugin_runtime.action_stacks.iter().any(|stack| {
            stack.map == t.map_id || stack.frames.iter().any(|f| Some(f.owner) == t.character_id)
        })
    })
}
fn actor(f: &PendingAction) -> Actor {
    Actor {
        role: if f.controller.is_some() {
            Role::Player
        } else {
            Role::Gm
        },
        user_id: f.controller.unwrap_or(UserId(f.source.0)),
    }
}
pub(crate) fn fresh_frame_id(
    s: &GameState,
    rng: &mut dyn GameRng,
) -> Result<IntentId, RejectReason> {
    for _ in 0..16 {
        let id = IntentId(crate::chat::message_id(rng).0);
        if !s.plugin_runtime.saves.iter().any(|g| g.id == id)
            && !s
                .plugin_runtime
                .action_stacks
                .iter()
                .any(|stack| stack.id == id || stack.frames.iter().any(|f| f.id == id))
            && !s
                .plugin_runtime
                .trigger_runs
                .iter()
                .any(|run| run.action == id)
        {
            return Ok(id);
        }
    }
    Err(invalid())
}
fn deferred(
    s: &GameState,
    source: &TokenDto,
    a: &PluginAction,
) -> Result<Vec<DeferredCost>, RejectReason> {
    let c = character(s, source)?;
    a.costs
        .iter()
        .filter(|cost| cost.on_resolution && crate::economy::cost_applies(s, c, cost))
        .map(|cost| {
            Ok(DeferredCost {
                resource: cost.resource.clone(),
                amount: number(s, c, &cost.amount)?,
            })
        })
        .collect()
}
fn notice(
    s: &GameState,
    frame: &PendingAction,
    outcome: ActionResolution,
    rng: &mut dyn GameRng,
) -> Event {
    let mut event = crate::chat::system_for_character(
        frame.owner,
        SystemMessage::PluginActionResolved {
            action: Some(frame.id),
            label: frame.label.clone(),
            outcome,
        },
        rng,
    );
    if !crate::visibility::PlayersSee::of(s)
        .tokens
        .contains(&frame.source)
        && let Event::ChatMessagePosted { message } = &mut event
    {
        message.visibility = ChatVisibility::Gm;
    }
    event
}
fn put(s: &mut GameState, stack: &ActionStack) {
    s.plugin_runtime.action_stacks.retain(|p| p.id != stack.id);
    if !stack.frames.is_empty() {
        s.plugin_runtime.action_stacks.push(stack.clone());
    }
}
fn rule_matches(
    s: &GameState,
    rule: &InterruptionRule,
    reactor: &TokenDto,
    caster: &TokenDto,
    casting: &PluginAction,
) -> bool {
    if reactor.id == caster.id
        || reactor.character_id == caster.character_id
        || reactor.map_id != caster.map_id
    {
        return false;
    }
    if !crate::modifiers::feature_enabled(s, rule.feature.as_deref())
        || !rule.tags.iter().all(|tag| casting.tags.contains(tag))
        || (!rule.any_tags.is_empty()
            && !rule.any_tags.iter().any(|tag| casting.tags.contains(tag)))
    {
        return false;
    }
    let (Ok(rc), Ok(cc)) = (character(s, reactor), character(s, caster)) else {
        return false;
    };
    let Some(reaction) = action(s, &rule.action) else {
        return false;
    };
    if !eligible(s, reactor, reaction)
        || !crate::spatial::permits(s, reaction, reactor, (caster.x, caster.y))
    {
        return false;
    }
    if rule.hostile_only
        && (reactor.disposition == caster.disposition
            || reactor.disposition == TokenDisposition::Neutral
            || caster.disposition == TokenDisposition::Neutral)
    {
        return false;
    }
    let see = crate::visibility::PlayersSee::of(s);
    let gm_controlled = rc.owner.is_none_or(|id| {
        s.users
            .get(&id)
            .is_some_and(|u| crate::sees_everything(u.role))
    });
    if !gm_controlled && (!see.tokens.contains(&reactor.id) || !see.tokens.contains(&caster.id)) {
        return false;
    }
    let Some(map) = s.maps.get(&caster.map_id) else {
        return false;
    };
    let dx = (reactor.x - caster.x).abs() as f64;
    let dy = (reactor.y - caster.y).abs() as f64;
    let distance = match rule.metric {
        DistanceMetric::Euclidean => dx.hypot(dy),
        DistanceMetric::Chebyshev => dx.max(dy),
    };
    let Ok(range) = number(s, rc, &rule.range_cells) else {
        return false;
    };
    if !(0..=1000).contains(&range) {
        return false;
    }
    if distance
        > (range as f64 + ((reactor.scale + caster.scale) / 2.0 - 1.0).max(0.0))
            * map.grid.cell_size
            * 16.0
            + 1.0
    {
        return false;
    }
    let Ok(action_range) = number(s, rc, &reaction.range_cells) else {
        return false;
    };
    let action_distance = match reaction.distance_metric {
        DistanceMetric::Euclidean => dx.hypot(dy),
        DistanceMetric::Chebyshev => dx.max(dy),
    };
    if !(0..=1000).contains(&action_range)
        || action_distance
            > (action_range as f64 + ((reactor.scale + caster.scale) / 2.0 - 1.0).max(0.0))
                * map.grid.cell_size
                * 16.0
                + 1.0
    {
        return false;
    }
    crate::triggers::expr_holds(s, rc, &BTreeMap::new(), &rule.source_condition)
        && crate::triggers::expr_holds(s, cc, &BTreeMap::new(), &rule.target_condition)
}
fn offers(s: &GameState, source: &TokenDto, a: &PluginAction) -> Vec<ReactionOffer> {
    let Some(rules) = s.rules() else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for token in s.tokens.values() {
        for rule in &rules.mechanics.interruption_rules {
            if rule_matches(s, rule, token, source, a) {
                result.push(ReactionOffer {
                    ready: result.is_empty(),
                    source: token.id,
                    rule: rule.id.clone(),
                    action: rule.action.clone(),
                    label: action(s, &rule.action)
                        .map_or_else(|| a.label.clone(), |a| a.label.clone()),
                });
                if result.len() == 128 {
                    return result;
                }
            }
        }
    }
    result
}
fn refresh(s: &GameState, frame: &mut PendingAction) {
    let Some(source) = s.tokens.get(&frame.source) else {
        frame.offers.clear();
        return;
    };
    let Some(a) = action(s, &frame.action) else {
        frame.offers.clear();
        return;
    };
    frame.offers.retain(|offer| {
        s.rules()
            .and_then(|r| {
                r.mechanics
                    .interruption_rules
                    .iter()
                    .find(|r| r.id == offer.rule && r.action == offer.action)
            })
            .zip(s.tokens.get(&offer.source))
            .is_some_and(|(r, reactor)| rule_matches(s, r, reactor, source, a))
    });
    frame.awaiting_reactions = !frame.offers.is_empty();
    for (i, offer) in frame.offers.iter_mut().enumerate() {
        offer.ready = i == 0;
    }
}
#[allow(clippy::too_many_arguments)]
fn prepare(
    s: &GameState,
    source: TokenId,
    id: &str,
    target: Option<TokenId>,
    x: i64,
    y: i64,
    actor: &Actor,
    at: i64,
    rng: &mut dyn GameRng,
) -> Result<(Vec<Event>, PendingAction), RejectReason> {
    let a = action(s, id).ok_or_else(invalid)?;
    let token = s.tokens.get(&source).ok_or_else(invalid)?;
    let frame = PendingAction {
        awaiting_reactions: false,
        id: fresh_frame_id(s, rng)?,
        source,
        owner: character(s, token)?.id,
        controller: (actor.role == Role::Player).then_some(actor.user_id),
        action: id.to_owned(),
        label: a.label.clone(),
        target: if a.target_self { Some(source) } else { target },
        x,
        y,
        deferred: deferred(s, token, a)?,
        offers: Vec::new(),
        ready: true,
    };
    let events = execute_body(
        s,
        source,
        id,
        target,
        x,
        y,
        actor,
        at,
        rng,
        ExecutionPhase::Prepare,
    )?;
    Ok((events, frame))
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn begin(
    s: &GameState,
    source: TokenId,
    id: &str,
    target: Option<TokenId>,
    x: i64,
    y: i64,
    actor: &Actor,
    at: i64,
    rng: &mut dyn GameRng,
) -> Result<Option<Vec<Event>>, RejectReason> {
    let a = action(s, id).ok_or_else(invalid)?;
    if a.reaction_only {
        return Ok(None);
    }
    let token = s.tokens.get(&source).ok_or_else(invalid)?;
    let reactions = offers(s, token, a);
    if reactions.is_empty() {
        return Ok(None);
    }
    if s.plugin_runtime.action_stacks.len() >= 16
        || s.plugin_runtime.movements.iter().any(|m| {
            s.tokens
                .get(&m.token)
                .is_some_and(|t| t.map_id == token.map_id)
        })
    {
        return Err(invalid());
    }
    let (events, mut frame) = prepare(s, source, id, target, x, y, actor, at, rng)?;
    let mut work = s.clone();
    let mut out = Vec::new();
    emit(&mut work, &mut out, events);
    frame.awaiting_reactions = !reactions.is_empty();
    frame.offers = reactions;
    let stack = ActionStack {
        id: frame.id,
        map: token.map_id,
        frames: vec![frame],
    };
    put(&mut work, &stack);
    out.push(runtime_event(&work));
    Ok(Some(out))
}
fn commit(
    s: &GameState,
    f: &PendingAction,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    let a = action(s, &f.action).ok_or_else(invalid)?;
    let token = s.tokens.get(&f.source).ok_or_else(invalid)?;
    if !eligible_base(s, token, a) {
        return Err(invalid());
    }
    let mut payable = a.clone();
    payable.cost = None;
    payable.costs = f
        .deferred
        .iter()
        .map(|c| ActionCost {
            scope: CostScope::Always,
            feature: None,
            resource: c.resource.clone(),
            amount: c.amount.to_string(),
            on_resolution: false,
        })
        .collect();
    let c = character(s, token)?;
    let costs = action_costs(s, c, &payable)?;
    let mut work = s.clone();
    let mut out = Vec::new();
    for (id, amount) in costs {
        let current = work.characters.get(&c.id).ok_or_else(invalid)?;
        let (mut events, delta) =
            crate::triggers::apply_resource_change_delta(&work, current, &id, amount, false, rng);
        if delta != Some(-amount) {
            return Err(invalid());
        }
        if !crate::visibility::PlayersSee::of(s)
            .tokens
            .contains(&f.source)
        {
            for e in &mut events {
                if let Event::ChatMessagePosted { message } = e {
                    message.visibility = ChatVisibility::Gm;
                }
            }
        }
        emit(&mut work, &mut out, events);
    }
    Ok(out)
}
struct CheckContext<'a> {
    fields: &'a dyn dnd_expr::Context,
    vars: &'a BTreeMap<String, i64>,
}
impl dnd_expr::Context for CheckContext<'_> {
    fn lookup(&self, path: &str) -> Result<dnd_expr::Value, dnd_expr::LookupError> {
        self.vars.get(path).map_or_else(
            || self.fields.lookup(path),
            |n| Ok(dnd_expr::Value::from(*n)),
        )
    }
}
fn counter(
    s: &GameState,
    f: &PendingAction,
    target: &PendingAction,
    at: i64,
    rng: &mut dyn GameRng,
) -> Result<(Vec<Event>, bool), RejectReason> {
    let def = action(s, &f.action)
        .and_then(|a| a.interrupt.as_ref())
        .ok_or_else(invalid)?;
    let source = s.tokens.get(&f.source).ok_or_else(invalid)?;
    let token = s.tokens.get(&target.source).ok_or_else(invalid)?;
    let caster = character(s, token)?;
    let difficulty = number(s, character(s, source)?, &def.save_dc)?;
    if !(0..=dnd_protocol::validate::MAX_SAFE_INTEGER).contains(&difficulty) {
        return Err(invalid());
    }
    let roll = crate::modifiers::roll(
        s,
        &def.save_dice,
        crate::modifiers::RollContext {
            actor: caster,
            target: Some(character(s, source)?),
            kind: RollKind::Save,
            tags: &def.tags,
            distance: None,
        },
        rng,
    )?;
    let vars = BTreeMap::from([
        ("total".into(), roll.total),
        ("difficulty".into(), difficulty),
    ]);
    let fields = crate::modifiers::effective_fields(s, caster)?;
    let context = CheckContext {
        fields: &fields,
        vars: &vars,
    };
    let value = dnd_expr::Expr::parse(&def.cancel_if)
        .and_then(|e| e.eval(&context))
        .map_err(|_| invalid())?;
    let cancelled = value
        .as_bool()
        .unwrap_or_else(|| value.as_integer().is_ok_and(|n| n != 0));
    let mut work = s.clone();
    let a = action(s, &f.action).ok_or_else(invalid)?;
    work.plugin_runtime
        .visuals
        .retain(|v| at.saturating_sub(v.at_ms) < 5000);
    work.plugin_runtime.visuals.push(ActionPresentation {
        source: source.id,
        map: source.map_id,
        label: a.label.clone(),
        x: source.x,
        y: source.y,
        radius_cells: 0,
        visual: a.visual.clone(),
        at_ms: at,
    });
    if work.plugin_runtime.visuals.len() > 20 {
        work.plugin_runtime.visuals.remove(0);
    }
    Ok((
        vec![
            crate::mechanics::roll_event(s, source, Some(token), &f.label, &roll, at, rng),
            runtime_event(&work),
        ],
        cancelled,
    ))
}
fn drain(
    s: &GameState,
    mut stack: ActionStack,
    at: i64,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    let mut work = s.clone();
    let mut out = Vec::new();
    for _ in 0..16 {
        let Some(last) = stack.frames.last_mut() else {
            break;
        };
        last.ready = true;
        refresh(&work, last);
        if !last.offers.is_empty() {
            put(&mut work, &stack);
            out.push(runtime_event(&work));
            return Ok(out);
        }
        let frame = stack.frames.pop().ok_or_else(invalid)?;
        put(&mut work, &stack); // Release only this frame's reservations before committing it.
        if action(&work, &frame.action).is_some_and(|a| a.interrupt.is_some()) {
            let Some(parent) = stack.frames.last().cloned() else {
                return Err(invalid());
            };
            let path_valid = action(&work, &frame.action).is_some_and(|a| {
                work.tokens.get(&frame.source).is_some_and(|source| {
                    work.tokens.get(&parent.source).is_some_and(|target| {
                        source.map_id == target.map_id
                            && crate::spatial::permits(&work, a, source, (target.x, target.y))
                    })
                })
            });
            if !path_valid {
                out.push(notice(&work, &frame, ActionResolution::Invalid, rng));
                continue;
            }
            let costs = commit(&work, &frame, rng)?;
            emit(&mut work, &mut out, costs);
            let (events, cancelled) = counter(&work, &frame, &parent, at, rng)?;
            emit(&mut work, &mut out, events);
            out.push(notice(&work, &frame, ActionResolution::Completed, rng));
            if cancelled {
                stack.frames.pop();
                put(&mut work, &stack);
                out.push(notice(&work, &parent, ActionResolution::Countered, rng));
            }
        } else {
            let result = execute_body(
                &work,
                frame.source,
                &frame.action,
                frame.target,
                frame.x,
                frame.y,
                &actor(&frame),
                at,
                rng,
                ExecutionPhase::Resume(frame.deferred.clone(), frame.id),
            );
            match result {
                Ok(events) => {
                    emit(&mut work, &mut out, events);
                    if !work.plugin_runtime.saves.iter().any(|g| g.id == frame.id) {
                        out.push(notice(&work, &frame, ActionResolution::Completed, rng));
                    }
                }
                Err(_) => out.push(notice(&work, &frame, ActionResolution::Invalid, rng)),
            }
        }
    }
    put(&mut work, &stack);
    out.push(runtime_event(&work));
    Ok(out)
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn answer(
    s: &GameState,
    stack_id: IntentId,
    frame_id: IntentId,
    source: TokenId,
    accept: bool,
    actor: &Actor,
    at: i64,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    let mut stack = s
        .plugin_runtime
        .action_stacks
        .iter()
        .find(|p| p.id == stack_id)
        .ok_or(RejectReason::NotFound)?
        .clone();
    let frame = stack
        .frames
        .last_mut()
        .filter(|f| f.id == frame_id)
        .ok_or_else(invalid)?;
    let mut checked = frame.clone();
    refresh(s, &mut checked);
    let offer = frame
        .offers
        .first()
        .filter(|o| o.source == source && o.ready)
        .ok_or_else(invalid)?
        .clone();
    if accept
        && !checked
            .offers
            .iter()
            .any(|o| o.source == source && o.rule == offer.rule)
    {
        return Err(invalid());
    }
    frame.offers.remove(0);
    let parent_source = frame.source;
    let mut work = s.clone();
    let mut out = Vec::new();
    put(&mut work, &stack);
    if accept {
        let target = work.tokens.get(&parent_source).ok_or_else(invalid)?.clone();
        let (events, mut next) = prepare(
            &work,
            source,
            &offer.action,
            Some(parent_source),
            target.x,
            target.y,
            actor,
            at,
            rng,
        )?;
        emit(&mut work, &mut out, events);
        let a = action(&work, &offer.action).ok_or_else(invalid)?;
        next.offers = offers(&work, work.tokens.get(&source).ok_or_else(invalid)?, a);
        next.awaiting_reactions = !next.offers.is_empty();
        if stack.frames.len() >= 8 {
            return Err(RejectReason::invalid("stack_limit"));
        }
        if let Some(parent) = stack.frames.last_mut() {
            parent.ready = false;
        }
        stack.frames.push(next);
        put(&mut work, &stack);
    }
    let events = drain(&work, stack, at, rng)?;
    emit(&mut work, &mut out, events);
    Ok(out)
}
pub(crate) fn continue_action(
    s: &GameState,
    stack_id: IntentId,
    frame: IntentId,
    at: i64,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    let stack = s
        .plugin_runtime
        .action_stacks
        .iter()
        .find(|p| p.id == stack_id)
        .ok_or(RejectReason::NotFound)?
        .clone();
    if !stack
        .frames
        .last()
        .is_some_and(|f| f.id == frame && f.offers.is_empty())
    {
        return Err(invalid());
    }
    drain(s, stack, at, rng)
}
pub(crate) fn cancel(
    s: &GameState,
    id: IntentId,
    outcome: ActionResolution,
    _at: i64,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    let stack = s
        .plugin_runtime
        .action_stacks
        .iter()
        .find(|s| s.id == id)
        .ok_or(RejectReason::NotFound)?;
    let mut work = s.clone();
    work.plugin_runtime.action_stacks.retain(|s| s.id != id);
    let mut out: Vec<_> = stack
        .frames
        .iter()
        .map(|f| notice(s, f, outcome, rng))
        .collect();
    out.push(runtime_event(&work));
    Ok(out)
}
pub(crate) fn cleanup(
    s: &GameState,
    events: &[Event],
    at: i64,
    rng: &mut dyn GameRng,
) -> Vec<Event> {
    if s.plugin_runtime.action_stacks.is_empty() && !events.iter().any(|e| matches!(e, Event::PluginRuntimeUpdated { runtime } if !runtime.action_stacks.is_empty())) { return Vec::new(); }
    let mut work = s.clone();
    for e in events {
        evolve(&mut work, e);
    }
    let mut out = Vec::new();
    let config_changed = s.rules() != work.rules() || events.iter().any(|e| matches!(e, Event::PluginInstalled { plugin } if Some(&plugin.id) == s.system_plugin.as_ref()));
    for mut stack in work.plugin_runtime.action_stacks.clone() {
        let valid = !config_changed
            && work.maps.contains_key(&stack.map)
            && stack.frames.iter().all(|f| {
                work.tokens.get(&f.source).is_some_and(|t| {
                    t.character_id == Some(f.owner)
                        && t.map_id == stack.map
                        && action(&work, &f.action).is_some_and(|a| eligible_base(&work, t, a))
                }) && f.controller.is_none_or(|user| {
                    work.characters
                        .get(&f.owner)
                        .is_some_and(|c| c.owner == Some(user))
                })
            });
        if !valid {
            if let Ok(events) = cancel(&work, stack.id, ActionResolution::Invalid, at, rng) {
                emit(&mut work, &mut out, events);
            }
        } else if let Some(last) = stack.frames.last_mut() {
            let old = last.offers.clone();
            refresh(&work, last);
            if old != last.offers {
                put(&mut work, &stack);
                out.push(runtime_event(&work));
            }
        }
    }
    out
}
