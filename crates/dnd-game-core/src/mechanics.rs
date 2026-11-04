//! Bounded declarative action interpreter. System-specific rules live in packages.
use crate::triggers::{apply_resource_change_delta, expr_holds, roll_dice};
use crate::{Actor, GameRng, GameState, evolve};
use dnd_protocol::*;
use std::collections::{BTreeMap, BTreeSet};

fn invalid() -> RejectReason {
    RejectReason::invalid("plugin_action")
}
fn active(state: &GameState) -> Option<&PluginDto> {
    state
        .system_plugin
        .as_ref()
        .and_then(|id| state.plugins.get(id))
}
fn enabled(state: &GameState, feature: Option<&str>) -> bool {
    feature.is_none_or(|id| {
        active(state)
            .and_then(|p| {
                p.system.as_ref().and_then(|r| {
                    r.features.iter().find(|f| f.id == id).map(|f| {
                        p.feature_settings
                            .get(id)
                            .copied()
                            .unwrap_or(f.default_enabled)
                    })
                })
            })
            .unwrap_or(false)
    })
}
pub(crate) fn character<'a>(
    state: &'a GameState,
    token: &TokenDto,
) -> Result<&'a CharacterDto, RejectReason> {
    token
        .character_id
        .and_then(|id| state.characters.get(&id))
        .ok_or_else(invalid)
}
pub(crate) fn resource(state: &GameState, c: &CharacterDto, id: &str) -> Option<(i64, i64)> {
    crate::sheet_values(state, &c.sheet_json)?
        .resources
        .into_iter()
        .find(|(key, _, _)| key == id)
        .map(|(_, v, m)| (v, m))
}
pub(crate) fn eligible_base(state: &GameState, source: &TokenDto, action: &PluginAction) -> bool {
    let Ok(c) = character(state, source) else {
        return false;
    };
    crate::modifiers::allowed(
        state,
        c,
        action.activation.unwrap_or(if action.reaction_only {
            RuleCapability::Reaction
        } else {
            RuleCapability::Action
        }),
    ) && crate::economy::timing_allows(state, source, c, action)
        && enabled(state, action.feature.as_deref())
        && expr_holds(state, c, &BTreeMap::new(), &action.condition)
}
pub(crate) fn eligible(state: &GameState, source: &TokenDto, action: &PluginAction) -> bool {
    eligible_base(state, source, action)
        && character(state, source).is_ok_and(|c| action_costs(state, c, action).is_ok())
}
pub(crate) fn action_costs(
    state: &GameState,
    c: &CharacterDto,
    action: &PluginAction,
) -> Result<BTreeMap<String, i64>, RejectReason> {
    let mut costs = BTreeMap::<String, i64>::new();
    if let Some(id) = &action.cost {
        costs.insert(id.clone(), 1);
    }
    for cost in action
        .costs
        .iter()
        .filter(|cost| crate::economy::cost_applies(state, c, cost))
    {
        let amount = number(state, c, &cost.amount)?;
        if !(1..=1_000_000).contains(&amount) {
            return Err(invalid());
        }
        let total = costs.entry(cost.resource.clone()).or_default();
        *total = total.checked_add(amount).ok_or_else(invalid)?;
    }
    for (id, amount) in &costs {
        if !resource(state, c, id).is_some_and(|(current, _)| {
            current - crate::interruptions::reserved(state, c.id, id) >= *amount
        }) {
            return Err(invalid());
        }
    }
    Ok(costs)
}
pub(crate) fn emit(work: &mut GameState, events: &mut Vec<Event>, additions: Vec<Event>) {
    for event in additions {
        evolve(work, &event);
        events.push(event);
    }
}
pub(crate) fn runtime_event(work: &GameState) -> Event {
    Event::PluginRuntimeUpdated {
        runtime: Box::new(work.plugin_runtime.clone()),
    }
}

pub(crate) fn apply(
    state: &GameState,
    envelope: &IntentEnvelope,
    actor: &Actor,
    rng: &mut dyn GameRng,
) -> Option<Result<Vec<Event>, RejectReason>> {
    let result = match &envelope.intent {
        Intent::ResolveActionSave { group, target } => {
            crate::saves::resolve(state, *group, *target, envelope.ts_ms, rng)
        }
        Intent::CancelActionSaves { group } => crate::saves::cancel(state, *group, rng),
        Intent::MoveToken { id, x, y } => movement(
            state,
            *id,
            *x,
            *y,
            MovementMode::Voluntary,
            envelope.intent_id,
            Vec::new(),
        ),
        Intent::MoveTokenWithMode { id, x, y, mode } => {
            movement(state, *id, *x, *y, *mode, envelope.intent_id, Vec::new())
        }
        Intent::ResolvePluginReaction {
            movement,
            source,
            accept,
        } => resolve(
            state,
            *movement,
            *source,
            *accept,
            actor,
            envelope.ts_ms,
            rng,
        ),
        Intent::ResolveActionReaction {
            stack,
            frame,
            source,
            accept,
        } => crate::interruptions::answer(
            state,
            *stack,
            *frame,
            *source,
            *accept,
            actor,
            envelope.ts_ms,
            rng,
        ),
        Intent::ContinuePluginAction { stack, frame } => {
            crate::interruptions::continue_action(state, *stack, *frame, envelope.ts_ms, rng)
        }
        Intent::CancelActionStack { stack } => crate::interruptions::cancel(
            state,
            *stack,
            ActionResolution::Cancelled,
            envelope.ts_ms,
            rng,
        ),
        Intent::EndMaintainedEffect { id } => crate::maintained::end(state, *id),
        Intent::CancelPluginMovement { movement } => {
            let mut work = state.clone();
            if !work
                .plugin_runtime
                .movements
                .iter()
                .any(|m| m.id == *movement)
            {
                return Some(Err(RejectReason::NotFound));
            }
            work.plugin_runtime.movements.retain(|m| m.id != *movement);
            Ok(vec![runtime_event(&work)])
        }
        Intent::ExecutePluginAction {
            source,
            action,
            target,
            x,
            y,
        } => {
            if state
                .tokens
                .get(source)
                .is_some_and(|t| crate::rules_view::map_busy(state, t.map_id))
            {
                return Some(Err(invalid()));
            }

            if state
                .plugin_runtime
                .movements
                .iter()
                .any(|m| m.token == *source || m.offers.iter().any(|o| o.source == *source))
            {
                return Some(Err(invalid()));
            }
            if crate::sheet_actions::resolve(state, *source, action)
                .is_some_and(|a| a.reaction_only)
            {
                return Some(Err(invalid()));
            }
            execute(
                state,
                *source,
                action,
                *target,
                *x,
                *y,
                actor,
                envelope.ts_ms,
                rng,
            )
        }
        _ => return None,
    };
    Some(result)
}

/// Segment exit from the closed reach region, expressed as a fraction of movement.
fn exit_fraction(
    source: &TokenDto,
    moving: &TokenDto,
    x: i64,
    y: i64,
    radius: f64,
    metric: DistanceMetric,
) -> Option<f64> {
    let ax = (moving.x - source.x) as f64;
    let ay = (moving.y - source.y) as f64;
    let bx = (x - source.x) as f64;
    let by = (y - source.y) as f64;
    let distance = |x: f64, y: f64| match metric {
        DistanceMetric::Euclidean => x.hypot(y),
        DistanceMetric::Chebyshev => x.abs().max(y.abs()),
    };
    if distance(ax, ay) > radius + 0.01 || distance(bx, by) <= radius + 0.01 {
        return None;
    }
    // Convex regions: binary search finds the first exit of the segment.
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..48 {
        let mid = (lo + hi) / 2.0;
        if distance(ax + (bx - ax) * mid, ay + (by - ay) * mid) <= radius {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(lo)
}
fn movement(
    state: &GameState,
    id: TokenId,
    x: i64,
    y: i64,
    mode: MovementMode,
    movement_id: IntentId,
    handled: Vec<String>,
) -> Result<Vec<Event>, RejectReason> {
    let token = state.tokens.get(&id).ok_or(RejectReason::NotFound)?;
    let map = state
        .maps
        .get(&token.map_id)
        .ok_or(RejectReason::NotFound)?;
    crate::apply::on_map(map, x, y)?;
    if state
        .plugin_runtime
        .movements
        .iter()
        .any(|m| m.token == id || m.offers.iter().any(|o| o.source == id))
    {
        return Err(RejectReason::invalid("pending_movement"));
    }
    if mode == MovementMode::Voluntary
        && let (Some(rules), Ok(c)) = (state.rules(), character(state, token))
        && !expr_holds(
            state,
            c,
            &BTreeMap::new(),
            &rules.mechanics.movement_condition,
        )
    {
        return Err(invalid());
    }
    if let Ok(c) = character(state, token) {
        if mode == MovementMode::Voluntary
            && !crate::modifiers::allowed(state, c, RuleCapability::Movement)
        {
            return Err(invalid());
        }
        if !crate::modifiers::allowed(state, c, RuleCapability::Provoke) {
            return Ok(vec![Event::TokenMoved { id, x, y }]);
        }
    }
    let mut candidates = Vec::new();
    if let Some(rules) = state.rules() {
        for hook in &rules.mechanics.movement_rules {
            if !enabled(state, Some(&hook.feature)) || !hook.movement_modes.contains(&mode) {
                continue;
            }
            if hook.combat_only && !state.combats.values().any(|c| c.map_id == token.map_id) {
                continue;
            }
            let Ok(target_char) = character(state, token) else {
                continue;
            };
            if !expr_holds(state, target_char, &BTreeMap::new(), &hook.target_condition) {
                continue;
            }
            let Some(action) = rules.mechanics.actions.iter().find(|a| a.id == hook.action) else {
                continue;
            };
            for source in state
                .tokens
                .values()
                .filter(|s| s.id != id && s.map_id == token.map_id)
            {
                let key = format!("{}:{}", hook.id, source.id);
                if handled.contains(&key) || !eligible(state, source, action) {
                    continue;
                }
                if hook.hostile_only
                    && !matches!(
                        (source.disposition, token.disposition),
                        (TokenDisposition::Ally, TokenDisposition::Enemy)
                            | (TokenDisposition::Enemy, TokenDisposition::Ally)
                    )
                {
                    continue;
                }
                let Ok(c) = character(state, source) else {
                    continue;
                };
                if !expr_holds(state, c, &BTreeMap::new(), &hook.source_condition) {
                    continue;
                }
                let reach = number(state, c, &hook.reach_cells)?;
                if reach > 1000 {
                    return Err(invalid());
                }
                if reach <= 0 {
                    continue;
                }
                let radius = (reach as f64 + (source.scale + token.scale) / 2.0 - 1.0)
                    * map.grid.cell_size
                    * 16.0;
                if let Some(fraction) = exit_fraction(source, token, x, y, radius, hook.metric) {
                    let boundary = (
                        token.x + ((x - token.x) as f64 * fraction).trunc() as i64,
                        token.y + ((y - token.y) as f64 * fraction).trunc() as i64,
                    );
                    if !crate::spatial::permits(state, action, source, boundary) {
                        continue;
                    }
                    candidates.push((
                        fraction,
                        source.id,
                        key,
                        ReactionOffer {
                            ready: false,
                            source: source.id,
                            rule: hook.id.clone(),
                            action: action.id.clone(),
                            label: action.label.clone(),
                        },
                    ));
                }
            }
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut events = Vec::new();
    if let Some(first) = candidates.first() {
        let fraction = first.0;
        let px = token.x + (((x - token.x) as f64 * fraction).trunc() as i64);
        let py = token.y + (((y - token.y) as f64 * fraction).trunc() as i64);
        events.push(Event::TokenMoved { id, x: px, y: py });
        let mut next = state.plugin_runtime.clone();
        let mut offers: Vec<ReactionOffer> = candidates
            .into_iter()
            .filter(|c| (c.0 - fraction).abs() < 0.00001)
            .map(|c| c.3)
            .collect();
        if let Some(first) = offers.first_mut() {
            first.ready = true;
        }
        if next.movements.len() >= 32 {
            return Err(RejectReason::invalid("pending_movement"));
        }
        next.movements.push(PendingMovement {
            id: movement_id,
            token: id,
            destination_x: x,
            destination_y: y,
            mode,
            offers,
            handled,
        });
        events.push(Event::PluginRuntimeUpdated {
            runtime: Box::new(next),
        });
    } else {
        events.push(Event::TokenMoved { id, x, y });
    }
    Ok(events)
}

#[allow(clippy::too_many_arguments)] // Shared by player actions, reactions and trigger steps.
pub(crate) fn execute(
    state: &GameState,
    source_id: TokenId,
    action_id: &str,
    target_id: Option<TokenId>,
    x: i64,
    y: i64,
    actor: &Actor,
    at_ms: i64,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    if crate::sheet_actions::resolve(state, source_id, action_id)
        .is_some_and(|a| a.interrupt.is_some())
    {
        return Err(invalid());
    }
    if crate::interruptions::busy(state, source_id) {
        return Err(RejectReason::invalid("pending_action"));
    }
    if let Some(events) = crate::interruptions::begin(
        state, source_id, action_id, target_id, x, y, actor, at_ms, rng,
    )? {
        return Ok(events);
    }
    execute_body(
        state,
        source_id,
        action_id,
        target_id,
        x,
        y,
        actor,
        at_ms,
        rng,
        ExecutionPhase::Full,
    )
}

pub(crate) enum ExecutionPhase {
    Full,
    Prepare,
    Resume(Vec<DeferredCost>, IntentId, Option<Box<PluginAction>>),
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_body(
    state: &GameState,
    source_id: TokenId,
    action_id: &str,
    target_id: Option<TokenId>,
    x: i64,
    y: i64,
    actor: &Actor,
    at_ms: i64,
    rng: &mut dyn GameRng,
    phase: ExecutionPhase,
) -> Result<Vec<Event>, RejectReason> {
    let mut action = match &phase {
        ExecutionPhase::Resume(_, _, Some(definition)) => definition.as_ref().clone(),
        _ => crate::sheet_actions::resolve(state, source_id, action_id).ok_or_else(invalid)?,
    };
    if let ExecutionPhase::Resume(costs, _, _) = &phase {
        action.cost = None;
        action.costs = costs
            .iter()
            .map(|cost| ActionCost {
                scope: CostScope::Always,
                feature: None,
                resource: cost.resource.clone(),
                amount: cost.amount.to_string(),
                on_resolution: false,
            })
            .collect();
    }
    let source = state.tokens.get(&source_id).ok_or(RejectReason::NotFound)?;
    if !eligible(state, source, &action) {
        return Err(invalid());
    }
    let map = state
        .maps
        .get(&source.map_id)
        .ok_or(RejectReason::NotFound)?;
    let target_id = if action.target_self {
        Some(source_id)
    } else {
        target_id
    };
    let target = target_id.and_then(|id| state.tokens.get(&id));
    let (x, y) = if action.radius_cells.is_none() {
        let t = target.ok_or_else(invalid)?;
        if t.map_id != source.map_id {
            return Err(invalid());
        }
        (t.x, t.y)
    } else {
        (x, y)
    };
    if !(0..=map.width * 16).contains(&x) || !(0..=map.height * 16).contains(&y) {
        return Err(invalid());
    }
    let cell = map.grid.cell_size * 16.0;
    let dx = (source.x - x).abs() as f64;
    let dy = (source.y - y).abs() as f64;
    let distance = match action.distance_metric {
        DistanceMetric::Euclidean => dx.hypot(dy),
        DistanceMetric::Chebyshev => dx.max(dy),
    };
    let allowance = if action.radius_cells.is_none() {
        target.map_or(0.0, |t| {
            ((source.scale + t.scale) / 2.0 - 1.0).max(0.0) * cell
        })
    } else {
        0.0
    };
    if distance
        > number(state, character(state, source)?, &action.range_cells)? as f64 * cell
            + allowance
            + 1.0
    {
        return Err(invalid());
    }
    if !crate::spatial::permits(state, &action, source, (x, y)) {
        return Err(invalid());
    }
    let visible = crate::players_state(state);
    if actor.role == Role::Player && target_id.is_some_and(|id| !visible.tokens.contains_key(&id)) {
        return Err(RejectReason::Forbidden);
    }
    let source_char = character(state, source)?.clone();
    let condition_turn = if action.turn_condition.is_some() {
        Some(
            state
                .combats
                .values()
                .find_map(|combat| {
                    let turn = combat.turn?;
                    state
                        .combatants
                        .get(&turn)
                        .filter(|c| {
                            c.character_id == Some(source_char.id) && combat.map_id == source.map_id
                        })
                        .map(|_| (combat.id, turn, combat.round))
                })
                .ok_or_else(invalid)?,
        )
    } else {
        None
    };
    let mut work = state.clone();
    let mut events = Vec::new();
    let vars = BTreeMap::new();
    let mut payable = action.clone();
    if matches!(phase, ExecutionPhase::Prepare) {
        payable.costs.retain(|c| !c.on_resolution);
    }
    for (cost, amount) in action_costs(state, &source_char, &payable)? {
        let current = work.characters.get(&source_char.id).ok_or_else(invalid)?;
        let (change, delta) =
            apply_resource_change_delta(&work, current, &cost, amount, false, rng);
        if delta != Some(-amount) {
            return Err(invalid());
        }
        emit(&mut work, &mut events, change);
    }
    if matches!(phase, ExecutionPhase::Prepare) {
        let released = crate::maintained::release_for_action(&work, source, &action)?;
        emit(&mut work, &mut events, released);
        if !visible.tokens.contains_key(&source.id) {
            for event in &mut events {
                if let Event::ChatMessagePosted { message } = event {
                    message.visibility = ChatVisibility::Gm;
                }
            }
        }
        return Ok(events);
    }
    let damage = roll_dice(&work, &action.damage_dice, Some(&source_char), &vars, rng)
        .ok_or_else(invalid)?;
    if (action.turn_condition.is_none() && action.maintain.is_none())
        || action.damage_dice.trim() != "0"
    {
        events.push(roll_event(
            state,
            source,
            None,
            &action.label,
            &damage,
            at_ms,
            rng,
        ));
    }
    let targets: Vec<_> = state
        .tokens
        .values()
        .filter(|t| {
            t.map_id == source.map_id
                && if let Some(radius) = action.radius_cells {
                    ((t.x - x) as f64).hypot((t.y - y) as f64) <= f64::from(radius) * cell
                } else {
                    Some(t.id) == target_id
                }
        })
        .filter(|t| crate::spatial::area_permits(state, &action, source.map_id, (x, y), (t.x, t.y)))
        .cloned()
        .collect();
    let mut seen = BTreeSet::new();
    let maintained_targets: Vec<_> = targets
        .iter()
        .filter(|t| t.character_id.is_some())
        .map(|t| t.id)
        .collect();
    if action.save_prompt {
        let root = match &phase {
            ExecutionPhase::Resume(_, id, _) => Some(*id),
            _ => None,
        };
        let pending = crate::saves::start(
            &work,
            source,
            &action,
            &targets,
            damage.total.max(0),
            root,
            at_ms,
            rng,
        )?;
        emit(&mut work, &mut events, pending);
    }
    for token in targets {
        if action.save_prompt {
            continue;
        }
        let Ok(c) = character(&work, &token).cloned() else {
            continue;
        };
        if !seen.insert(c.id) {
            continue;
        }
        let mut vars = BTreeMap::new();
        if let Some(dice) = &action.attack_dice {
            let dx = (source.x - token.x).abs() as f64;
            let dy = (source.y - token.y).abs() as f64;
            let delta = match action.distance_metric {
                DistanceMetric::Euclidean => dx.hypot(dy),
                DistanceMetric::Chebyshev => dx.max(dy),
            };
            let distance = ((delta / cell) - ((source.scale + token.scale) / 2.0 - 1.0))
                .max(0.0)
                .ceil() as i64;
            let roll = crate::modifiers::roll(
                &work,
                dice,
                crate::modifiers::RollContext {
                    actor: &source_char,
                    target: Some(&c),
                    kind: RollKind::Attack,
                    tags: &action.tags,
                    distance: Some(distance),
                },
                rng,
            )?;
            events.push(roll_event(
                state,
                source,
                Some(&token),
                &action.label,
                &roll,
                at_ms,
                rng,
            ));
            vars.insert("total".into(), roll.total);
            if let Some(natural) = roll.terms.iter().find_map(|t| match &t.term {
                dnd_dice::TermDetail::Dice { dice, .. } => {
                    dice.iter().find(|d| d.kept).map(|d| i64::from(d.value))
                }
                _ => None,
            }) {
                vars.insert("natural".into(), natural);
            }
            if !action
                .hit
                .as_ref()
                .is_none_or(|hit| expr_holds(&work, &c, &vars, hit))
            {
                continue;
            }
        }
        let mut amount = damage.total.max(0);
        if action
            .critical_if
            .as_ref()
            .is_some_and(|test| expr_holds(&work, &source_char, &vars, test))
            && let Some(dice) = &action.critical_dice
        {
            amount = amount.saturating_add(
                roll_dice(&work, dice, Some(&source_char), &vars, rng)
                    .ok_or_else(invalid)?
                    .total
                    .max(0),
            );
        }
        if let Some(dice) = &action.save_dice {
            let roll = crate::modifiers::roll(
                &work,
                dice,
                crate::modifiers::RollContext {
                    actor: &c,
                    target: Some(&source_char),
                    kind: RollKind::Save,
                    tags: &action.tags,
                    distance: None,
                },
                rng,
            )?;
            events.push(roll_event(
                state,
                source,
                Some(&token),
                &action.label,
                &roll,
                at_ms,
                rng,
            ));
            vars.insert("total".into(), roll.total);
            if let Some(natural) = roll.terms.iter().find_map(|t| match &t.term {
                dnd_dice::TermDetail::Dice { dice, .. } => {
                    dice.iter().find(|d| d.kept).map(|d| i64::from(d.value))
                }
                _ => None,
            }) {
                vars.insert("natural".into(), natural);
            }
            if let Some(dc) = &action.save_dc {
                vars.insert("difficulty".into(), number(&work, &source_char, dc)?);
            }
            if action
                .hit
                .as_ref()
                .is_some_and(|hit| expr_holds(&work, &c, &vars, hit))
            {
                amount /= i64::from(action.save_divisor.max(1));
            }
        }
        if amount == 0 {
            continue;
        }
        let mut change = apply_damage(&work, &c, &action, amount, at_ms, rng)?;
        if !visible.tokens.contains_key(&token.id) || !visible.tokens.contains_key(&source.id) {
            for event in &mut change {
                if let Event::ChatMessagePosted { message } = event {
                    message.visibility = ChatVisibility::Gm;
                }
            }
        }
        emit(&mut work, &mut events, change);
    }
    let maintained =
        crate::maintained::start(&work, source, &action, &maintained_targets, at_ms, rng)?;
    emit(&mut work, &mut events, maintained);
    if let (Some(condition), Some((combat, turn, round))) = (&action.turn_condition, condition_turn)
    {
        if work
            .effects
            .values()
            .filter(|e| e.token == source_id)
            .count()
            >= MAX_EFFECTS_PER_TOKEN
        {
            return Err(invalid());
        }
        let effect = TokenEffectDto {
            id: EffectId(crate::chat::message_id(rng).0),
            token: source_id,
            label: action.label.get("en").to_owned(),
            condition: Some(condition.clone()),
            resource: None,
            dice: None,
            heal: false,
            duration: EffectDuration::TurnEnd {
                combat,
                turn,
                round,
            },
            version: Version::INITIAL,
        };
        emit(
            &mut work,
            &mut events,
            vec![Event::TokenEffectApplied { effect }],
        );
    }
    work.plugin_runtime
        .visuals
        .retain(|v| at_ms.saturating_sub(v.at_ms) < 5000);
    work.plugin_runtime.visuals.push(ActionPresentation {
        source: source_id,
        map: source.map_id,
        label: action.label,
        x,
        y,
        radius_cells: action.radius_cells.unwrap_or(0),
        visual: action.visual,
        at_ms,
    });
    if work.plugin_runtime.visuals.len() > 20 {
        work.plugin_runtime.visuals.remove(0);
    }
    if !visible.tokens.contains_key(&source.id) {
        for event in &mut events {
            if let Event::ChatMessagePosted { message } = event {
                message.visibility = ChatVisibility::Gm;
            }
        }
    }
    events.push(runtime_event(&work));
    Ok(events)
}

fn resolve(
    state: &GameState,
    movement_id: IntentId,
    source: TokenId,
    accept: bool,
    actor: &Actor,
    at_ms: i64,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    let pending = state
        .plugin_runtime
        .movements
        .iter()
        .find(|m| m.id == movement_id)
        .ok_or(RejectReason::NotFound)?
        .clone();
    // Stable order for simultaneous reactions. GM may decline the current offer on someone's behalf.
    let offer = pending
        .offers
        .first()
        .filter(|o| o.source == source)
        .ok_or_else(invalid)?
        .clone();
    let mut work = state.clone();
    let mut events = Vec::new();
    if accept {
        let t = state
            .tokens
            .get(&pending.token)
            .ok_or(RejectReason::NotFound)?;
        if offer_valid(&work, &pending, &offer) {
            let changes = execute(
                &work,
                source,
                &offer.action,
                Some(t.id),
                t.x,
                t.y,
                actor,
                at_ms,
                rng,
            )?;
            emit(&mut work, &mut events, changes);
        } else {
            return Err(invalid());
        }
    }
    work.plugin_runtime
        .movements
        .retain(|m| m.id != movement_id);
    let mut pending = pending;
    pending.offers.remove(0);
    let parent = pending.clone();
    pending
        .offers
        .retain(|offer| offer_valid(&work, &parent, offer));
    if let Some(first) = pending.offers.first_mut() {
        first.ready = true;
    }
    pending.handled.push(format!("{}:{}", offer.rule, source));
    if !pending.offers.is_empty() {
        work.plugin_runtime.movements.push(pending);
    } else if work.tokens.get(&pending.token).is_some_and(|t| {
        pending.mode != MovementMode::Voluntary
            || character(&work, t).is_ok_and(|c| {
                work.rules().is_none_or(|r| {
                    expr_holds(&work, c, &BTreeMap::new(), &r.mechanics.movement_condition)
                })
            })
    }) {
        let changes = movement(
            &work,
            pending.token,
            pending.destination_x,
            pending.destination_y,
            pending.mode,
            pending.id,
            pending.handled,
        )?;
        emit(&mut work, &mut events, changes);
    }
    events.push(runtime_event(&work));
    Ok(events)
}

pub(crate) fn number(
    state: &GameState,
    c: &CharacterDto,
    expression: &str,
) -> Result<i64, RejectReason> {
    let fields = crate::modifiers::effective_fields(state, c)?;
    dnd_expr::Expr::parse(expression)
        .and_then(|e| e.eval_integer(&fields))
        .map_err(|_| invalid())
}

struct DamageContext<'a> {
    base: &'a dyn dnd_expr::Context,
    damage: i64,
}
impl dnd_expr::Context for DamageContext<'_> {
    fn lookup(&self, path: &str) -> Result<dnd_expr::Value, dnd_expr::LookupError> {
        if path == "damage" {
            Ok(dnd_expr::Value::from(self.damage))
        } else {
            self.base.lookup(path)
        }
    }
}
fn damage_amount(
    profile: &DamageProfile,
    damage_type: Option<&str>,
    amount: i64,
    fields: &dyn dnd_expr::Context,
) -> Result<i64, RejectReason> {
    let mut amount = amount.max(0);
    for modifier in &profile.modifiers {
        if !modifier.types.is_empty()
            && !damage_type.is_some_and(|t| modifier.types.iter().any(|id| id == t))
        {
            continue;
        }
        let context = DamageContext {
            base: fields,
            damage: amount,
        };
        let applies = if modifier.condition.is_empty() {
            true
        } else {
            let value = dnd_expr::Expr::parse(&modifier.condition)
                .and_then(|e| e.eval(&context))
                .map_err(|_| invalid())?;
            value
                .as_bool()
                .unwrap_or_else(|| value.as_integer().is_ok_and(|n| n != 0))
        };
        if applies {
            amount = dnd_expr::Expr::parse(&modifier.amount)
                .and_then(|e| e.eval_integer(&context))
                .map_err(|_| invalid())?;
            if !(0..=dnd_protocol::validate::MAX_SAFE_INTEGER).contains(&amount) {
                return Err(invalid());
            }
        }
    }
    Ok(amount)
}
pub(crate) fn apply_damage(
    state: &GameState,
    character: &CharacterDto,
    action: &PluginAction,
    amount: i64,
    at_ms: i64,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    let profile = action
        .damage_profile
        .as_ref()
        .map(|id| {
            state
                .rules()
                .and_then(|r| r.mechanics.damage_profiles.iter().find(|p| &p.id == id))
                .ok_or_else(invalid)
        })
        .transpose()?
        .filter(|p| enabled(state, p.feature.as_deref()));
    let mut remaining = if let Some(profile) = profile {
        damage_amount(
            profile,
            action.damage_type.as_deref(),
            amount,
            &crate::modifiers::effective_fields(state, character)?,
        )?
    } else {
        amount.max(0)
    };
    let taken = remaining;
    let mut work = state.clone();
    let mut events = Vec::new();
    let resources: Vec<&str> = profile
        .into_iter()
        .flat_map(|p| p.absorb.iter().map(String::as_str))
        .chain(std::iter::once(action.resource.as_str()))
        .collect();
    for id in resources {
        if remaining == 0 {
            break;
        }
        let c = work.characters.get(&character.id).ok_or_else(invalid)?;
        let (change, delta) = apply_resource_change_delta(&work, c, id, remaining, false, rng);
        let delta = delta.ok_or_else(invalid)?;
        if delta > 0 || -delta > remaining {
            return Err(invalid());
        }
        remaining += delta;
        // Empty buffers need no event or resource-change trigger.
        if delta != 0 {
            emit(&mut work, &mut events, change);
        }
    }
    let maintained =
        crate::maintained::on_damage(&work, character.id, &action.resource, taken, at_ms, rng)?;
    events.extend(maintained);
    Ok(events)
}

pub(crate) fn validate(rules: &SystemRulesDto) -> Result<(), RejectReason> {
    crate::sheet_actions::validate(rules)?;
    let m = &rules.mechanics;
    if m.calendar
        .as_ref()
        .is_some_and(|c| !crate::calendar::valid(c))
        || m.item_types.len() > 32
        || m.action_groups.len() > 32
    {
        return Err(invalid());
    }
    let mut new_ids = BTreeSet::new();
    for item in &m.item_types {
        if item.id.is_empty()
            || item.id.len() > 64
            || !new_ids.insert(&item.id)
            || dnd_protocol::sheet_schema::check_schema(&item.sheet_schema).is_err()
            || !item.sheet_layout.is_object()
        {
            return Err(invalid());
        }
    }
    new_ids.clear();
    for group in &m.action_groups {
        if group.id.is_empty() || !new_ids.insert(&group.id) {
            return Err(invalid());
        }
    }
    for formula in [&m.life.dead, &m.life.npc_dead, &m.life.unconscious] {
        if !formula.is_empty() && dnd_expr::Expr::parse(formula).is_err() {
            return Err(invalid());
        }
    }

    if m.actions.len() > 128 || m.movement_rules.len() > 128 || m.turn_resources.len() > 32 {
        return Err(invalid());
    }
    let ident = |s: &str| {
        !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    };
    let feature = |s: &str| rules.features.iter().any(|f| f.id == s);
    let resource = |s: &str| rules.resources.iter().any(|r| r.id == s);
    let expr = |s: &str| s.len() <= 1024 && (s.is_empty() || dnd_expr::Expr::parse(s).is_ok());
    let dice = |s: &str| s.len() <= 256 && dnd_dice::Expr::parse(s).is_ok();
    if !expr(&m.movement_condition) {
        return Err(invalid());
    }
    let mut interruption_ids = BTreeSet::new();
    if m.interruption_rules.len() > 128 {
        return Err(invalid());
    }
    for r in &m.interruption_rules {
        if !ident(&r.id)
            || !interruption_ids.insert(&r.id)
            || r.feature.as_deref().is_some_and(|id| !feature(id))
            || !m
                .actions
                .iter()
                .any(|a| a.id == r.action && a.reaction_only && a.interrupt.is_some())
            || r.tags.len() > 16
            || r.any_tags.len() > 16
            || r.tags.iter().chain(&r.any_tags).any(|t| !ident(t))
            || r.range_cells.is_empty()
            || !expr(&r.range_cells)
            || !expr(&r.source_condition)
            || !expr(&r.target_condition)
        {
            return Err(invalid());
        }
    }
    let mut budget_ids = BTreeSet::new();
    if m.turn_budgets.len() > 32 {
        return Err(invalid());
    }
    for b in &m.turn_budgets {
        if !resource(&b.resource)
            || !crate::economy::writable_budget(rules, &b.resource)
            || !budget_ids.insert(&b.resource)
            || b.feature.as_deref().is_some_and(|f| !feature(f))
            || b.label.get("en").is_empty()
            || ["en", "ru"]
                .iter()
                .any(|lang| b.label.get(lang).len() > 200)
            || b.amount.as_ref().is_some_and(|s| s.is_empty() || !expr(s))
            || !rules
                .resources
                .iter()
                .find(|r| r.id == b.resource)
                .is_some_and(|r| {
                    r.value.trim().strip_prefix('@').is_some_and(|p| {
                        !p.is_empty()
                            && p.bytes()
                                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'.')
                    })
                })
        {
            return Err(invalid());
        }
    }
    let mut policy_ids = BTreeSet::new();
    if m.maintained_rules.len() > 32 {
        return Err(invalid());
    }
    for policy in &m.maintained_rules {
        if !ident(&policy.id)
            || !policy_ids.insert(&policy.id)
            || !ident(&policy.exclusive_group)
            || policy.label.get("en").is_empty()
            || ["en", "ru"]
                .iter()
                .any(|lang| policy.label.get(lang).len() > 200)
            || policy.feature.as_deref().is_some_and(|id| !feature(id))
            || !expr(&policy.keep_condition)
            || !resource(&policy.damage_check.resource)
            || !dice(&policy.damage_check.dice)
            || policy.damage_check.difficulty.is_empty()
            || !expr(&policy.damage_check.difficulty)
            || policy.damage_check.success.is_empty()
            || !expr(&policy.damage_check.success)
            || policy.damage_check.tags.len() > 16
            || policy.damage_check.tags.iter().any(|t| !ident(t))
        {
            return Err(invalid());
        }
    }
    let mut profile_ids = BTreeSet::new();
    if m.damage_profiles.len() > 32 {
        return Err(invalid());
    }
    for profile in &m.damage_profiles {
        let mut modifiers = BTreeSet::new();
        let mut buffers = BTreeSet::new();
        if !ident(&profile.id)
            || !profile_ids.insert(&profile.id)
            || profile.feature.as_deref().is_some_and(|f| !feature(f))
            || profile.label.get("en").is_empty()
            || ["en", "ru"]
                .iter()
                .any(|lang| profile.label.get(lang).len() > 200)
            || profile.modifiers.len() > 64
            || profile.absorb.len() > 8
            || profile
                .absorb
                .iter()
                .any(|id| !resource(id) || !buffers.insert(id))
            || profile.modifiers.iter().any(|a| {
                !ident(&a.id)
                    || !modifiers.insert(&a.id)
                    || a.types.len() > 32
                    || a.types.iter().any(|t| !ident(t))
                    || !expr(&a.condition)
                    || a.amount.is_empty()
                    || !expr(&a.amount)
            })
        {
            return Err(invalid());
        }
    }
    let mut ids = BTreeSet::new();
    for a in &m.actions {
        if (a.save_prompt
            && (a.save_dice.is_none()
                || a.attack_dice.is_some()
                || a.maintain.is_some()
                || a.reaction_only))
            || (!a.save_failure.is_empty() && !expr(&a.save_failure))
            || a.sheet_group
                .as_ref()
                .is_some_and(|id| !m.action_groups.iter().any(|g| &g.id == id))
            || a.spatial.feature.as_deref().is_some_and(|f| !feature(f))
            || (a.spatial.area_line_of_effect && a.radius_cells.is_none())
            || a.timing
                .as_ref()
                .is_some_and(|t| t.feature.as_deref().is_some_and(|f| !feature(f)))
            || a.interrupt.as_ref().is_some_and(|r| {
                !a.reaction_only
                    || a.target_self
                    || a.radius_cells.is_some()
                    || a.maintain.is_some()
                    || a.turn_condition.is_some()
                    || a.attack_dice.is_some()
                    || a.save_dice.is_some()
                    || a.damage_dice.trim() != "0"
                    || !dice(&r.save_dice)
                    || r.save_dc.is_empty()
                    || !expr(&r.save_dc)
                    || r.cancel_if.is_empty()
                    || !expr(&r.cancel_if)
                    || r.tags.len() > 16
                    || r.tags.iter().any(|t| !ident(t))
            })
            || a.maintain.as_ref().is_some_and(|def| {
                !m.maintained_rules.iter().any(|r| r.id == def.rule)
                    || ["en", "ru"]
                        .iter()
                        .any(|lang| a.label.get(lang).chars().count() > MAX_EFFECT_LABEL)
                    || !rules.conditions.iter().any(|c| c.id == def.condition)
                    || a.attack_dice.is_some()
                    || a.save_dice.is_some()
                    || a.damage_dice.trim() != "0"
                    || a.turn_condition.is_some()
                    || a.reaction_only
                    || match def.duration {
                        EffectDurationDef::Rounds { rounds } => {
                            !(1..=MAX_EFFECT_SPAN).contains(&rounds)
                        }
                        EffectDurationDef::Turns { turns } => {
                            !(1..=MAX_EFFECT_SPAN).contains(&turns)
                        }
                        EffectDurationDef::GameTime { seconds } => {
                            !(1..=604_800).contains(&seconds)
                        }
                        EffectDurationDef::RealTime { minutes } => {
                            !(1..=MAX_EFFECT_MINUTES).contains(&minutes)
                        }
                    }
            })
            || a.costs.len() > 16
            || (a.cost.is_some() && !a.costs.is_empty())
            || a.costs.iter().any(|c| {
                !resource(&c.resource)
                    || c.feature.as_deref().is_some_and(|f| !feature(f))
                    || c.amount.is_empty()
                    || !expr(&c.amount)
            })
            || a.tags.len() > 16
            || a.tags.iter().any(|t| t.is_empty() || t.len() > 64)
            || a.activation.is_some_and(|c| {
                !matches!(
                    c,
                    RuleCapability::Action | RuleCapability::BonusAction | RuleCapability::Reaction
                )
            })
            || (a.reaction_only && a.activation.is_some_and(|c| c != RuleCapability::Reaction))
            || a.turn_condition
                .as_ref()
                .is_some_and(|id| !a.target_self || !rules.conditions.iter().any(|c| c.id == *id))
        {
            return Err(invalid());
        }
        if !ident(&a.id)
            || !ids.insert(&a.id)
            || a.feature.as_deref().is_some_and(|f| !feature(f))
            || !expr(&a.condition)
            || !expr(&a.range_cells)
            || a.range_cells.is_empty()
            || a.radius_cells.is_some_and(|r| r == 0 || r > 100)
            || !(1..=10).contains(&a.save_divisor)
            || a.damage_type.as_deref().is_some_and(|t| !ident(t))
            || a.damage_profile.as_ref().is_some_and(|id| {
                !m.damage_profiles
                    .iter()
                    .any(|p| &p.id == id && !p.absorb.contains(&a.resource))
            })
            || !dice(&a.damage_dice)
            || !resource(&a.resource)
            || a.cost.as_deref().is_some_and(|c| !resource(c))
            || a.critical_if.as_deref().is_some_and(|e| !expr(e))
            || a.critical_dice.as_deref().is_some_and(|d| !dice(d))
            || a.attack_dice.as_deref().is_some_and(|d| !dice(d))
            || a.save_dice.as_deref().is_some_and(|d| !dice(d))
            || a.hit.as_deref().is_some_and(|h| !expr(h))
            || a.save_dc.as_deref().is_some_and(|e| !expr(e))
            || (a.attack_dice.is_some() && a.save_dice.is_some())
            || (a.save_dice.is_some() && (a.save_dc.is_none() || a.hit.is_none()))
            || a.visual.duration_ms < 50
            || a.visual.duration_ms > 5000
            || a.visual.color.len() != 7
            || !a.visual.color.starts_with('#')
            || !a.visual.color[1..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(invalid());
        }
    }
    let mut ids = BTreeSet::new();
    for h in &m.movement_rules {
        if !ident(&h.id)
            || !ids.insert(&h.id)
            || !feature(&h.feature)
            || !expr(&h.reach_cells)
            || h.reach_cells.is_empty()
            || h.movement_modes.is_empty()
            || h.movement_modes.len() > 3
            || !expr(&h.source_condition)
            || !expr(&h.target_condition)
            || !m
                .actions
                .iter()
                .any(|a| a.id == h.action && a.radius_cells.is_none() && a.interrupt.is_none())
        {
            return Err(invalid());
        }
    }
    let mut section_ids = BTreeSet::new();
    if m.gm_panel.len() > 24 {
        return Err(invalid());
    }
    for section in &m.gm_panel {
        if !ident(&section.id)
            || !section_ids.insert(&section.id)
            || section.label.get("en").is_empty()
            || ["ru", "en"].iter().any(|lang| {
                section.label.get(lang).len() > 200 || section.description.get(lang).len() > 2000
            })
            || section.items.len() > 128
            || section.items.iter().any(|id| match section.kind {
                GmPanelKind::Clock => !id.parse::<u32>().is_ok_and(|n| (1..=604_800).contains(&n)),
                GmPanelKind::Rules => !feature(id),
                GmPanelKind::Actions => !m.actions.iter().any(|a| &a.id == id && !a.reaction_only),
                _ => true,
            })
        {
            return Err(invalid());
        }
    }
    if m.turn_resources.iter().any(|id| !resource(id)) {
        return Err(invalid());
    }
    Ok(())
}

/// Generic refresh at the beginning of the character's own turn; the plugin names resources.
pub(crate) fn after(state: &GameState, events: &[Event], _rng: &mut dyn GameRng) -> Vec<Event> {
    let mut work = state.clone();
    for e in events {
        evolve(&mut work, e);
    }
    let mut result = Vec::new();
    // Configuration/deletion cancels suspended work, leaving the token at its last committed position.
    if events.iter().any(|e| {
        matches!(
            e,
            Event::MapDeleted { .. }
                | Event::SystemPluginSet { .. }
                | Event::PluginInstalled { .. }
                | Event::PluginUninstalled { .. }
                | Event::TokenDeleted { .. }
                | Event::CharacterDeleted { .. }
                | Event::CombatEnded { .. }
        )
    }) && !work.plugin_runtime.movements.is_empty()
    {
        work.plugin_runtime.movements.clear();
        result.push(runtime_event(&work));
    }
    result
}

pub(crate) fn check_evaluation(
    rules: &SystemRulesDto,
    base: &dyn dnd_expr::Context,
) -> Result<(), RejectReason> {
    struct Vars<'a>(&'a dyn dnd_expr::Context);
    impl dnd_expr::Context for Vars<'_> {
        fn lookup(&self, path: &str) -> Result<dnd_expr::Value, dnd_expr::LookupError> {
            match path {
                "total" | "natural" | "difficulty" => Ok(dnd_expr::Value::from(10)),
                _ => self.0.lookup(path),
            }
        }
    }
    let vars = Vars(base);
    let test = |expression: &str| -> Result<(), RejectReason> {
        if expression.is_empty() {
            return Ok(());
        }
        dnd_expr::Expr::parse(expression)
            .and_then(|e| e.eval(&vars))
            .map(|_| ())
            .map_err(|_| invalid())
    };
    let dice = |expression: &str| -> Result<(), RejectReason> {
        dnd_dice::Expr::parse(expression)
            .and_then(|e| {
                e.roll(
                    &dnd_dice::ExprFields(&vars),
                    &mut dnd_dice::SplitMix64::new(0),
                )
            })
            .map(|_| ())
            .map_err(|_| invalid())
    };
    let distance = |expression: &str| -> Result<(), RejectReason> {
        let n = dnd_expr::Expr::parse(expression)
            .and_then(|e| e.eval_integer(base))
            .map_err(|_| invalid())?;
        if !(0..=1000).contains(&n) {
            return Err(invalid());
        }
        Ok(())
    };
    for profile in &rules.mechanics.damage_profiles {
        let context = DamageContext { base, damage: 10 };
        for modifier in &profile.modifiers {
            if !modifier.condition.is_empty() {
                dnd_expr::Expr::parse(&modifier.condition)
                    .and_then(|e| e.eval(&context))
                    .map_err(|_| invalid())?;
            }
            let amount = dnd_expr::Expr::parse(&modifier.amount)
                .and_then(|e| e.eval_integer(&context))
                .map_err(|_| invalid())?;
            if !(0..=dnd_protocol::validate::MAX_SAFE_INTEGER).contains(&amount) {
                return Err(invalid());
            }
        }
    }
    for r in &rules.mechanics.interruption_rules {
        distance(&r.range_cells)?;
        test(&r.source_condition)?;
        test(&r.target_condition)?;
    }
    for a in &rules.mechanics.actions {
        if let Some(r) = &a.interrupt {
            dice(&r.save_dice)?;
            let dc = dnd_expr::Expr::parse(&r.save_dc)
                .and_then(|e| e.eval_integer(base))
                .map_err(|_| invalid())?;
            if !(0..=dnd_protocol::validate::MAX_SAFE_INTEGER).contains(&dc) {
                return Err(invalid());
            }
            test(&r.cancel_if)?;
        }
    }
    for b in &rules.mechanics.turn_budgets {
        if let Some(e) = &b.amount {
            let amount = dnd_expr::Expr::parse(e)
                .and_then(|e| e.eval_integer(base))
                .map_err(|_| invalid())?;
            let max = rules
                .resources
                .iter()
                .find(|r| r.id == b.resource)
                .ok_or_else(invalid)?;
            let max = dnd_expr::Expr::parse(&max.max)
                .and_then(|e| e.eval_integer(base))
                .map_err(|_| invalid())?;
            if !(0..=max).contains(&amount) {
                return Err(invalid());
            }
        }
    }
    crate::maintained::validate(rules, base)?;
    test(&rules.mechanics.movement_condition)?;
    for action in &rules.mechanics.actions {
        test(&action.condition)?;
        for cost in &action.costs {
            let amount = dnd_expr::Expr::parse(&cost.amount)
                .and_then(|e| e.eval_integer(base))
                .map_err(|_| invalid())?;
            if !(1..=1_000_000).contains(&amount) {
                return Err(invalid());
            }
        }
        distance(&action.range_cells)?;
        dice(&action.damage_dice)?;
        for expression in [&action.hit, &action.save_dc, &action.critical_if]
            .into_iter()
            .flatten()
        {
            test(expression)?;
        }
        for expression in [
            &action.attack_dice,
            &action.save_dice,
            &action.critical_dice,
        ]
        .into_iter()
        .flatten()
        {
            dice(expression)?;
        }
    }
    for rule in &rules.mechanics.movement_rules {
        distance(&rule.reach_cells)?;
        test(&rule.source_condition)?;
        test(&rule.target_condition)?;
    }
    Ok(())
}

pub(crate) fn roll_event(
    state: &GameState,
    source: &TokenDto,
    target: Option<&TokenDto>,
    label: &Label,
    roll: &dnd_dice::Roll,
    at_ms: i64,
    rng: &mut dyn GameRng,
) -> Event {
    let visible = crate::visibility::PlayersSee::of(state);
    let public = visible.tokens.contains(&source.id)
        && target.is_none_or(|t| visible.tokens.contains(&t.id));
    crate::chat::posted(ChatMessageDto {
        id: crate::chat::message_id(rng),
        kind: ChatKind::Roll,
        sender: None,
        sender_name: source.name.clone(),
        body: label.get("en").to_owned(),
        roll: Some(crate::chat::roll_dto(roll)),
        system: None,
        visibility: if public {
            ChatVisibility::All
        } else {
            ChatVisibility::Gm
        },
        related_characters: [source.character_id, target.and_then(|t| t.character_id)]
            .into_iter()
            .flatten()
            .collect(),
        at_ms: Some(at_ms),
    })
}

fn offer_valid(state: &GameState, pending: &PendingMovement, offer: &ReactionOffer) -> bool {
    let Some(rules) = state.rules() else {
        return false;
    };
    let Some(hook) = rules
        .mechanics
        .movement_rules
        .iter()
        .find(|h| h.id == offer.rule && h.action == offer.action)
    else {
        return false;
    };
    let Some(action) = rules
        .mechanics
        .actions
        .iter()
        .find(|a| a.id == offer.action)
    else {
        return false;
    };
    let Some(source) = state.tokens.get(&offer.source) else {
        return false;
    };
    let Some(target) = state.tokens.get(&pending.token) else {
        return false;
    };
    let (Ok(sc), Ok(tc)) = (character(state, source), character(state, target)) else {
        return false;
    };
    crate::modifiers::allowed(state, tc, RuleCapability::Provoke)
        && enabled(state, Some(&hook.feature))
        && eligible(state, source, action)
        && source.map_id == target.map_id
        && crate::spatial::permits(state, action, source, (target.x, target.y))
        && expr_holds(state, sc, &BTreeMap::new(), &hook.source_condition)
        && expr_holds(state, tc, &BTreeMap::new(), &hook.target_condition)
}
