//! Derived data is evaluated by the authority, never by the presentation client.
use crate::{GameState, evolve};
use dnd_protocol::*;
use std::collections::BTreeMap;
pub(crate) fn life(state: &GameState, c: &CharacterDto) -> LifeStatus {
    let Some(r) = state.rules() else {
        return LifeStatus::Alive;
    };
    let l = &r.mechanics.life;
    let dead = if c.kind == CharacterKind::Npc && !l.npc_dead.is_empty() {
        &l.npc_dead
    } else {
        &l.dead
    };
    let holds =
        |s: &str| !s.is_empty() && crate::triggers::expr_holds(state, c, &BTreeMap::new(), s);
    if holds(dead) {
        LifeStatus::Dead
    } else if holds(&l.unconscious) {
        LifeStatus::Unconscious
    } else {
        LifeStatus::Alive
    }
}
pub(crate) fn map_busy(state: &GameState, map: MapId) -> bool {
    state.plugin_runtime.saves.iter().any(|g| g.map == map)
}
pub fn view(state: &GameState) -> RulesView {
    let characters = state
        .characters
        .values()
        .filter(|_| state.rules().is_some())
        .map(|c| CharacterRuleView {
            character: c.id,
            life: life(state, c),
        })
        .collect();
    let tokens = state
        .tokens
        .values()
        .filter(|t| state.rules().is_some() && t.character_id.is_some())
        .map(|t| {
            let actions = state
                .rules()
                .map(|r| {
                    r.mechanics
                        .actions
                        .iter()
                        .cloned()
                        .map(|a| (a, None))
                        .chain(
                            crate::sheet_actions::rows(state, t)
                                .into_iter()
                                .map(|row| (row.action.clone(), Some(row))),
                        )
                        .map(|(a, sheet_action)| {
                            let range = crate::mechanics::character(state, t)
                                .ok()
                                .and_then(|c| {
                                    crate::mechanics::number(state, c, &a.range_cells).ok()
                                })
                                .unwrap_or(0);
                            let reason = if a.reaction_only {
                                Some("reaction")
                            } else if map_busy(state, t.map_id)
                                || crate::interruptions::busy(state, t.id)
                            {
                                Some("pending")
                            } else if !crate::modifiers::feature_enabled(
                                state,
                                a.feature.as_deref(),
                            ) {
                                Some("disabled")
                            } else if !crate::mechanics::eligible_base(state, t, &a) {
                                Some("condition_or_turn")
                            } else if !crate::mechanics::eligible(state, t, &a) {
                                Some("resources")
                            } else {
                                None
                            };
                            ActionAvailability {
                                sheet_action,
                                id: a.id.clone(),
                                enabled: reason.is_none(),
                                reason: reason.map(str::to_owned),
                                range_cells: u16::try_from(range).unwrap_or(0),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            TokenActionView {
                token: t.id,
                actions,
            }
        })
        .collect();
    RulesView {
        characters,
        tokens,
        calendar: Some(crate::calendar::view(state)),
    }
}
pub(crate) fn after(before: &GameState, events: &[Event]) -> Vec<Event> {
    if !events.iter().any(|e| {
        matches!(
            e,
            Event::GameTimeAdvanced { .. }
                | Event::PluginInstalled { .. }
                | Event::PluginUninstalled { .. }
                | Event::SystemPluginSet { .. }
                | Event::CharacterCreated { .. }
                | Event::CharacterUpdated { .. }
                | Event::CharacterDeleted { .. }
                | Event::TokenCreated { .. }
                | Event::TokenUpdated { .. }
                | Event::TokenDeleted { .. }
                | Event::CombatStarted { .. }
                | Event::CombatUpdated { .. }
                | Event::CombatEnded { .. }
                | Event::CombatantAdded { .. }
                | Event::CombatantUpdated { .. }
                | Event::CombatantRemoved { .. }
                | Event::TokenEffectApplied { .. }
                | Event::TokenEffectUpdated { .. }
                | Event::TokenEffectRemoved { .. }
                | Event::PluginRuntimeUpdated { .. }
        )
    }) {
        return Vec::new();
    }

    let mut work = before.clone();
    for e in events {
        evolve(&mut work, e);
    }
    let next = view(&work);
    if next == view(before) {
        Vec::new()
    } else {
        vec![Event::RulesViewUpdated { view: next }]
    }
}
