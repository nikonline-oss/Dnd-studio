//! Declarative timing, budgets and reset scopes; no game-system identifiers.
use crate::mechanics::{emit, number, resource};
use crate::{GameRng, GameState};
use dnd_protocol::*;
use std::collections::BTreeSet;

pub(crate) fn combat_for<'a>(state: &'a GameState, c: &CharacterDto) -> Option<&'a CombatDto> {
    // A character represented in multiple scenes cannot avoid its active combat's budgets.
    state
        .combats
        .values()
        .find(|combat| {
            state
                .combatants
                .values()
                .any(|p| p.combat_id == combat.id && p.character_id == Some(c.id))
        })
        .or_else(|| {
            state.combats.values().find(|combat| {
                state
                    .tokens
                    .values()
                    .any(|t| t.map_id == combat.map_id && t.character_id == Some(c.id))
            })
        })
}
pub(crate) fn cost_applies(state: &GameState, c: &CharacterDto, cost: &ActionCost) -> bool {
    crate::modifiers::feature_enabled(state, cost.feature.as_deref())
        && (cost.scope == CostScope::Always || combat_for(state, c).is_some())
}
pub(crate) fn timing_allows(
    state: &GameState,
    source: &TokenDto,
    c: &CharacterDto,
    a: &PluginAction,
) -> bool {
    let Some(timing) = &a.timing else {
        return true;
    };
    if !crate::modifiers::feature_enabled(state, timing.feature.as_deref()) {
        return true;
    }
    let Some(combat) = state
        .combats
        .values()
        .find(|b| b.map_id == source.map_id)
        .or_else(|| combat_for(state, c))
    else {
        return true;
    };
    let turn = combat.turn.and_then(|id| state.combatants.get(&id));
    match timing.turn {
        RequiredTurn::Own => turn.is_some_and(|p| p.character_id == Some(c.id)),
        RequiredTurn::Other => turn.is_some_and(|p| p.character_id != Some(c.id)),
    }
}
pub(crate) fn writable_budget(rules: &SystemRulesDto, id: &str) -> bool {
    let Some(path) = rules
        .resources
        .iter()
        .find(|r| r.id == id)
        .and_then(|r| r.value.trim().strip_prefix('@'))
    else {
        return false;
    };
    let mut schema = &rules.sheet_schema;
    for part in path.split('.') {
        let Some(next) = schema.get("properties").and_then(|p| p.get(part)) else {
            return false;
        };
        schema = next;
    }
    schema.get("type").and_then(serde_json::Value::as_str) == Some("integer")
}
fn target_amount(state: &GameState, c: &CharacterDto, b: &TurnBudget) -> Result<i64, RejectReason> {
    let (_, max) =
        resource(state, c, &b.resource).ok_or_else(|| RejectReason::invalid("turn_budget"))?;
    let amount = b.amount.as_ref().map_or(Ok(max), |e| number(state, c, e))?;
    if !(0..=max).contains(&amount) {
        return Err(RejectReason::invalid("turn_budget"));
    }
    Ok(amount)
}
/// Called after the combat/turn event is evolved and before periodic turn effects.
pub(crate) fn refresh(
    state: &GameState,
    combat: &CombatDto,
    initial: bool,
    rng: &mut dyn GameRng,
) -> Vec<Event> {
    let Some(rules) = state.rules() else {
        return Vec::new();
    };
    let own = combat
        .turn
        .and_then(|id| state.combatants.get(&id))
        .and_then(|p| p.character_id);
    let members: BTreeSet<_> = state
        .tokens
        .values()
        .filter(|t| t.map_id == combat.map_id)
        .filter_map(|t| t.character_id)
        .chain(
            state
                .combatants
                .values()
                .filter(|p| p.combat_id == combat.id)
                .filter_map(|p| p.character_id),
        )
        .collect();
    let mut work = state.clone();
    let mut out = Vec::new();
    for id in members {
        // A character has one set of budgets; don't reset it through a duplicate on a second map.
        if !work
            .characters
            .get(&id)
            .is_some_and(|c| combat_for(&work, c).is_some_and(|b| b.id == combat.id))
        {
            continue;
        }
        let policies: Vec<_> = rules
            .mechanics
            .turn_budgets
            .iter()
            .filter(|b| crate::modifiers::feature_enabled(state, b.feature.as_deref()))
            .collect();
        let legacy = rules
            .mechanics
            .turn_resources
            .iter()
            .filter(|resource| !policies.iter().any(|b| &b.resource == *resource))
            .map(|id| (id.as_str(), None, BudgetRefresh::OwnTurn));
        let declared = policies
            .iter()
            .map(|b| (b.resource.as_str(), Some(*b), b.refresh));
        for (resource_id, policy, scope) in legacy.chain(declared) {
            if !initial && scope == BudgetRefresh::OwnTurn && own != Some(id) {
                continue;
            }
            let Some(c) = work.characters.get(&id) else {
                continue;
            };
            let Some((current, max)) = resource(&work, c, resource_id) else {
                continue;
            };
            let Ok(target) = policy.map_or(Ok(max), |b| target_amount(&work, c, b)) else {
                continue;
            };
            if target == current {
                continue;
            }
            let (mut events, delta) = crate::triggers::apply_resource_change_delta(
                &work,
                c,
                resource_id,
                (target - current).abs(),
                target > current,
                rng,
            );
            if delta != Some(target - current) {
                continue;
            }
            // Hidden actors' budget resets are state changes, not public chat announcements.
            events.retain(|e| !matches!(e, Event::ChatMessagePosted { .. }));
            emit(&mut work, &mut out, events);
        }
    }
    out
}
pub fn view(state: &GameState, character: &CharacterDto) -> Option<TurnEconomyView> {
    let rules = state.rules()?;
    let combat = combat_for(state, character);
    let current = combat
        .and_then(|b| b.turn)
        .and_then(|id| state.combatants.get(&id));
    let budgets = rules
        .mechanics
        .turn_budgets
        .iter()
        .filter(|b| crate::modifiers::feature_enabled(state, b.feature.as_deref()))
        .filter_map(|b| {
            let (current, _) = resource(state, character, &b.resource)?;
            let reserved = crate::interruptions::reserved(state, character.id, &b.resource);
            Some(TurnBudgetValue {
                resource: b.resource.clone(),
                label: b.label.clone(),
                refresh: b.refresh,
                remaining: (current - reserved).max(0),
                reserved,
                limit: target_amount(state, character, b).ok()?,
            })
        })
        .collect();
    Some(TurnEconomyView {
        in_combat: combat.is_some(),
        own_turn: current.is_some_and(|p| p.character_id == Some(character.id)),
        turn_started: combat.and_then(|b| b.turn).is_some(),
        budgets,
    })
}
