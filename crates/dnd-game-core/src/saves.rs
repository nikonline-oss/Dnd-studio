//! Shared damage is rolled once, then target controllers resolve their own saves.
use crate::mechanics::{emit, runtime_event};
use crate::{GameRng, GameState};
use dnd_protocol::*;
use std::collections::BTreeMap;
fn invalid() -> RejectReason {
    RejectReason::invalid("saving_throw")
}
fn notice(group: &PendingSaves, outcome: ActionResolution, rng: &mut dyn GameRng) -> Event {
    crate::chat::system(
        SystemMessage::PluginActionResolved {
            action: Some(group.id),
            label: group.action.label.clone(),
            outcome,
        },
        rng,
    )
}
fn damage(
    work: &mut GameState,
    events: &mut Vec<Event>,
    group: &PendingSaves,
    target: &SaveTarget,
    success: bool,
    at: i64,
    rng: &mut dyn GameRng,
) -> Result<(), RejectReason> {
    let c = work.characters.get(&target.character).ok_or_else(invalid)?;
    let amount = if success {
        group.damage / i64::from(group.action.save_divisor.max(1))
    } else {
        group.damage
    };
    let mut change = crate::mechanics::apply_damage(work, c, &group.action, amount, at, rng)?;
    let see = crate::visibility::PlayersSee::of(work);
    if !see.tokens.contains(&target.token)
        || group.source.is_none_or(|id| !see.tokens.contains(&id))
    {
        for event in &mut change {
            if let Event::ChatMessagePosted { message } = event {
                message.visibility = ChatVisibility::Gm;
            }
        }
    }
    emit(work, events, change);
    Ok(())
}
#[allow(clippy::too_many_arguments)] // Frozen cast context shared with the action executor.
pub(crate) fn start(
    state: &GameState,
    source: &TokenDto,
    action: &PluginAction,
    targets: &[TokenDto],
    amount: i64,
    root: Option<IntentId>,
    at: i64,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    if state.plugin_runtime.saves.len() >= 16 {
        return Err(invalid());
    }
    let id = if let Some(id) = root {
        id
    } else {
        crate::interruptions::fresh_frame_id(state, rng)?
    };
    let c = crate::mechanics::character(state, source)?;
    let mut seen = std::collections::BTreeSet::new();
    let mut group = PendingSaves {
        id,
        map: source.map_id,
        source: Some(source.id),
        owner: c.id,
        action: action.clone(),
        damage: amount.max(0),
        difficulty: crate::mechanics::number(
            state,
            c,
            action.save_dc.as_deref().ok_or_else(invalid)?,
        )?,
        targets: Vec::new(),
    };
    if group.difficulty < 0 || group.difficulty > 9_007_199_254_740_991 {
        return Err(invalid());
    }
    for t in targets {
        let Some(c) = t.character_id.and_then(|id| state.characters.get(&id)) else {
            continue;
        };
        if !seen.insert(c.id) {
            continue;
        }
        let automatic = !action.save_failure.is_empty()
            && crate::triggers::expr_holds(state, c, &BTreeMap::new(), &action.save_failure);
        group.targets.push(SaveTarget {
            token: t.id,
            character: c.id,
            status: if automatic {
                SaveStatus::AutomaticFailure
            } else {
                SaveStatus::Pending
            },
            total: None,
        });
    }
    let mut work = state.clone();
    let mut out = Vec::new();
    for t in &group.targets {
        if t.status == SaveStatus::AutomaticFailure {
            damage(&mut work, &mut out, &group, t, false, at, rng)?;
        }
    }
    if group
        .targets
        .iter()
        .any(|t| t.status == SaveStatus::Pending)
    {
        work.plugin_runtime.saves.push(group);
        out.push(runtime_event(&work));
    } else if root.is_none() {
        out.push(notice(&group, ActionResolution::Completed, rng));
    }
    Ok(out)
}
pub(crate) fn resolve(
    state: &GameState,
    id: IntentId,
    token: TokenId,
    at: i64,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    let mut group = state
        .plugin_runtime
        .saves
        .iter()
        .find(|g| g.id == id)
        .cloned()
        .ok_or(RejectReason::NotFound)?;
    let pos = group
        .targets
        .iter()
        .position(|t| t.token == token && t.status == SaveStatus::Pending)
        .ok_or_else(invalid)?;
    let target = group.targets[pos].clone();
    let c = state
        .characters
        .get(&target.character)
        .ok_or_else(invalid)?;
    let mut work = state.clone();
    let mut out = Vec::new();
    let automatic = !group.action.save_failure.is_empty()
        && crate::triggers::expr_holds(state, c, &BTreeMap::new(), &group.action.save_failure);
    let success = if automatic {
        group.targets[pos].status = SaveStatus::AutomaticFailure;
        false
    } else {
        let source = group.source.and_then(|id| state.tokens.get(&id));
        let source_c = source
            .and_then(|t| t.character_id)
            .and_then(|id| state.characters.get(&id));
        let roll = crate::modifiers::roll(
            state,
            group.action.save_dice.as_deref().ok_or_else(invalid)?,
            crate::modifiers::RollContext {
                actor: c,
                target: source_c,
                kind: RollKind::Save,
                tags: &group.action.tags,
                distance: None,
            },
            rng,
        )?;
        let vars = BTreeMap::from([
            ("total".into(), roll.total),
            ("difficulty".into(), group.difficulty),
        ]);
        let success = group
            .action
            .hit
            .as_ref()
            .is_some_and(|e| crate::triggers::expr_holds(state, c, &vars, e));
        group.targets[pos].status = if success {
            SaveStatus::Succeeded
        } else {
            SaveStatus::Failed
        };
        group.targets[pos].total = Some(roll.total);
        if let Some(t) = state.tokens.get(&target.token) {
            out.push(crate::mechanics::roll_event(
                state,
                t,
                source,
                &group.action.label,
                &roll,
                at,
                rng,
            ));
        }
        success
    };
    damage(&mut work, &mut out, &group, &target, success, at, rng)?;
    work.plugin_runtime.saves.retain(|g| g.id != id);
    if group
        .targets
        .iter()
        .any(|t| t.status == SaveStatus::Pending)
    {
        work.plugin_runtime.saves.push(group);
    } else {
        out.push(notice(&group, ActionResolution::Completed, rng));
    }
    out.push(runtime_event(&work));
    Ok(out)
}
pub(crate) fn cancel(
    state: &GameState,
    id: IntentId,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    let g = state
        .plugin_runtime
        .saves
        .iter()
        .find(|g| g.id == id)
        .ok_or(RejectReason::NotFound)?;
    let mut work = state.clone();
    work.plugin_runtime.saves.retain(|g| g.id != id);
    Ok(vec![
        notice(g, ActionResolution::Cancelled, rng),
        runtime_event(&work),
    ])
}
pub(crate) fn cleanup(
    state: &GameState,
    events: &[Event],
    at: i64,
    rng: &mut dyn GameRng,
) -> Vec<Event> {
    let mut work = state.clone();
    for e in events {
        crate::evolve(&mut work, e);
    }
    let mut out = Vec::new();
    let configuration = state.rules() != work.rules()
        || state.system_plugin != work.system_plugin
        || state
            .system_plugin
            .as_ref()
            .and_then(|id| state.plugins.get(id))
            != work
                .system_plugin
                .as_ref()
                .and_then(|id| work.plugins.get(id));
    for g in work.plugin_runtime.saves.clone() {
        if configuration
            || !work.maps.contains_key(&g.map)
            || g.targets.iter().any(|t| {
                !work.characters.contains_key(&t.character) || !work.tokens.contains_key(&t.token)
            })
        {
            if let Ok(events) = cancel(&work, g.id, rng) {
                emit(&mut work, &mut out, events);
            }
        } else {
            for target in &g.targets {
                if target.status == SaveStatus::Pending
                    && work.characters.get(&target.character).is_some_and(|c| {
                        !g.action.save_failure.is_empty()
                            && crate::triggers::expr_holds(
                                &work,
                                c,
                                &BTreeMap::new(),
                                &g.action.save_failure,
                            )
                    })
                    && let Ok(events) = resolve(&work, g.id, target.token, at, rng)
                {
                    emit(&mut work, &mut out, events);
                }
            }
        }
    }
    out
}
