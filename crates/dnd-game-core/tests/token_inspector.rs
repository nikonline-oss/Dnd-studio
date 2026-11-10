//! Which token fields a recipient sees in the inspector (ADR-0061): the campaign policy, the
//! token override, and the snapshot/batch contract that carries the resolved rows.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_game_core::{
    ForPlayers, GameState, evolve_all, filter_for_players, players_state, snapshot_for,
    token_fields,
};
use dnd_protocol::arbitrary::{pooled_map_id, pooled_token_id, v7};
use dnd_protocol::*;

fn policy(reveal: &[TokenField], conceal: &[TokenField]) -> TokenInspectorPolicy {
    TokenInspectorPolicy {
        reveal: reveal.to_vec(),
        conceal: conceal.to_vec(),
    }
}

fn fields_of(snapshot: &Snapshot, id: TokenId) -> Vec<TokenField> {
    snapshot
        .token_inspector
        .iter()
        .find(|view| view.token == id)
        .map(|view| view.fields.clone())
        .expect("the token has inspector rows")
}

fn rows(role: Role, state: &GameState) -> Vec<TokenField> {
    fields_of(&snapshot_for(role, state, Seq(1), []), pooled_token_id(0))
}

fn set_campaign(state: &mut GameState, policy: TokenInspectorPolicy) {
    run(
        state,
        Role::Gm,
        Intent::UpdateCampaignMeta {
            name: None,
            description: None,
            token_inspector: Some(policy),
            item_states: None,
            visibility: None,
        },
        Some(state.campaign.version),
    )
    .unwrap();
}

fn set_override(state: &mut GameState, id: TokenId, policy: TokenInspectorPolicy) {
    let version = state.tokens[&id].version;
    run(
        state,
        Role::Gm,
        Intent::UpdateToken {
            id,
            name: None,
            rotation: None,
            scale: None,
            layer: None,
            visible: None,
            disposition: None,
            inspector: Some(policy),
        },
        Some(version),
    )
    .unwrap();
}

#[test]
fn players_get_the_base_fields_and_gm_roles_get_everything() {
    let state = table_state();
    assert_eq!(rows(Role::Player, &state), TokenField::BASE.to_vec());
    assert_eq!(rows(Role::Spectator, &state), TokenField::BASE.to_vec());
    assert_eq!(rows(Role::Gm, &state), TokenField::ALL.to_vec());
    assert_eq!(rows(Role::CoGm, &state), TokenField::ALL.to_vec());
}

#[test]
fn the_campaign_policy_replaces_the_default_set() {
    let mut state = table_state();
    set_campaign(
        &mut state,
        policy(
            &[TokenField::Name, TokenField::Size, TokenField::Disposition],
            &[],
        ),
    );
    assert_eq!(
        rows(Role::Player, &state),
        vec![TokenField::Name, TokenField::Size, TokenField::Disposition]
    );
    // Rows come back in inspector order, whatever order the policy lists them in.
    assert_eq!(rows(Role::Player, &state)[0], TokenField::Name);
    // The GM's own panel is unaffected.
    assert_eq!(rows(Role::Gm, &state), TokenField::ALL.to_vec());
}

#[test]
fn a_conceal_beats_a_reveal_of_the_same_field() {
    let mut state = table_state();
    set_campaign(
        &mut state,
        policy(
            &[TokenField::Name, TokenField::Size, TokenField::Effects],
            &[TokenField::Size, TokenField::Effects],
        ),
    );
    assert_eq!(rows(Role::Player, &state), vec![TokenField::Name]);
}

#[test]
fn a_token_override_adds_and_closes_fields() {
    let mut state = table_state();
    set_override(
        &mut state,
        pooled_token_id(0),
        policy(
            &[TokenField::Effects, TokenField::Position],
            &[TokenField::Name],
        ),
    );
    assert_eq!(
        rows(Role::Player, &state),
        vec![
            TokenField::Size,
            TokenField::Character,
            TokenField::Conditions,
            TokenField::Effects,
            TokenField::Position,
        ]
    );
}

#[test]
fn an_empty_override_returns_the_token_to_the_campaign_policy() {
    let mut state = table_state();
    set_override(
        &mut state,
        pooled_token_id(0),
        policy(&[TokenField::Position], &[]),
    );
    assert!(state.tokens[&pooled_token_id(0)].inspector.is_some());
    set_override(&mut state, pooled_token_id(0), policy(&[], &[]));
    assert!(state.tokens[&pooled_token_id(0)].inspector.is_none());
    assert_eq!(rows(Role::Player, &state), TokenField::BASE.to_vec());
}

#[test]
fn duplicate_fields_are_cleaned_and_bounded() {
    let mut state = table_state();
    let duplicated = policy(
        &[TokenField::Name, TokenField::Name, TokenField::Effects],
        &[TokenField::Effects, TokenField::Effects],
    );
    let version = state.campaign.version;
    run(
        &mut state,
        Role::Gm,
        Intent::UpdateCampaignMeta {
            name: None,
            description: None,
            token_inspector: Some(duplicated),
            item_states: None,
            visibility: None,
        },
        Some(version),
    )
    .unwrap();
    // Sorted and deduplicated, so the campaign holds the same rows a fresh policy would.
    assert_eq!(
        state.campaign.token_inspector,
        policy(
            &[TokenField::Name, TokenField::Effects],
            &[TokenField::Effects]
        )
    );
}

#[test]
fn players_never_receive_the_override_itself() {
    let mut state = table_state();
    set_override(
        &mut state,
        pooled_token_id(0),
        policy(&[TokenField::Position], &[]),
    );
    let player = snapshot_for(Role::Player, &state, Seq(1), []);
    assert!(player.tokens.iter().all(|t| t.inspector.is_none()));
    assert!(
        player
            .token_inspector
            .iter()
            .any(|view| view.token == pooled_token_id(0))
    );
    let gm = snapshot_for(Role::Gm, &state, Seq(1), []);
    assert!(
        gm.tokens
            .iter()
            .find(|t| t.id == pooled_token_id(0))
            .and_then(|t| t.inspector.as_ref())
            .is_some()
    );
}

#[test]
fn hidden_tokens_have_no_rows() {
    let state = table_state();
    let player = snapshot_for(Role::Player, &state, Seq(1), []);
    let hidden = pooled_token_id(2); // the GM-layer token
    assert!(!player.tokens.iter().any(|t| t.id == hidden));
    assert!(!player.token_inspector.iter().any(|v| v.token == hidden));
}

#[test]
fn a_policy_change_needs_a_fresh_snapshot() {
    let before = table_state();
    let mut after = before.clone();
    set_campaign(&mut after, policy(&[TokenField::Effects], &[]));
    let batch = EventBatch {
        seq: Seq(2),
        caused_by: None,
        events: vec![Event::CampaignMetaUpdated {
            campaign: after.campaign.clone(),
        }],
    };
    assert_eq!(
        filter_for_players(&before, &after, &batch),
        ForPlayers::Resnapshot
    );
}

#[test]
fn an_override_change_needs_a_fresh_snapshot() {
    let before = table_state();
    let mut after = before.clone();
    set_override(
        &mut after,
        pooled_token_id(0),
        policy(&[TokenField::Position], &[]),
    );
    let batch = EventBatch {
        seq: Seq(2),
        caused_by: None,
        events: vec![Event::TokenUpdated {
            token: after.tokens[&pooled_token_id(0)].clone(),
        }],
    };
    assert_eq!(
        filter_for_players(&before, &after, &batch),
        ForPlayers::Resnapshot
    );
}

#[test]
fn a_hidden_token_override_changes_nothing_for_players() {
    let before = table_state();
    let mut after = before.clone();
    let hidden = pooled_token_id(2); // GM layer
    set_override(&mut after, hidden, policy(&[TokenField::Position], &[]));
    let batch = EventBatch {
        seq: Seq(2),
        caused_by: None,
        events: vec![Event::TokenUpdated {
            token: after.tokens[&hidden].clone(),
        }],
    };
    assert!(matches!(
        filter_for_players(&before, &after, &batch),
        ForPlayers::Batch(_)
    ));
}

#[test]
fn a_new_visible_token_needs_a_fresh_snapshot() {
    let mut before = table_state();
    before.tokens.remove(&pooled_token_id(3));
    let mut after = before.clone();
    let mut token = common::token(3, 0, None, TokenLayer::Tokens);
    token.id = pooled_token_id(3);
    after.tokens.insert(token.id, token.clone());
    let batch = EventBatch {
        seq: Seq(2),
        caused_by: None,
        events: vec![Event::TokenCreated { token }],
    };
    assert_eq!(
        filter_for_players(&before, &after, &batch),
        ForPlayers::Resnapshot
    );
}

#[test]
fn a_shared_token_event_hides_the_override() {
    let before = table_state();
    let mut after = before.clone();
    set_override(
        &mut after,
        pooled_token_id(0),
        policy(&[TokenField::Position], &[]),
    );
    // A batch that keeps the fields stable still strips the override when it travels.
    let mut same_policy = before.clone();
    same_policy
        .tokens
        .get_mut(&pooled_token_id(0))
        .unwrap()
        .name = "Renamed".into();
    let batch = EventBatch {
        seq: Seq(2),
        caused_by: None,
        events: vec![Event::TokenUpdated {
            token: same_policy.tokens[&pooled_token_id(0)].clone(),
        }],
    };
    let ForPlayers::Batch(filtered) = filter_for_players(&before, &same_policy, &batch) else {
        panic!("expected a batch");
    };
    let Event::TokenUpdated { token } = &filtered.events[0] else {
        panic!("expected the token event");
    };
    assert!(token.inspector.is_none());
    assert_eq!(token.name, "Renamed");
}

#[test]
fn the_view_is_built_from_the_full_token_not_the_filtered_one() {
    let mut state = table_state();
    set_override(
        &mut state,
        pooled_token_id(0),
        policy(&[TokenField::Effects], &[]),
    );
    let view = players_state(&state);
    assert!(view.tokens[&pooled_token_id(0)].inspector.is_none());
    assert!(
        token_fields(
            &state,
            &state.tokens[&pooled_token_id(0)],
            dnd_game_core::Viewer::player(PLAYER),
        )
        .contains(&TokenField::Effects)
    );
    // And the server-side token is untouched by the projection.
    assert!(state.tokens[&pooled_token_id(0)].inspector.is_some());
    assert_eq!(state.maps.len(), 2);
    assert!(
        !state.maps[&pooled_map_id(1)].visible_to_players,
        "the projection must not leak into the authority state"
    );
}

fn effect(id: u128, token: TokenId, reveal: &[TokenField]) -> TokenEffectDto {
    TokenEffectDto {
        id: EffectId(v7(0x9000 + id)),
        token,
        label: "Detect magic".into(),
        condition: None,
        resource: None,
        dice: None,
        heal: false,
        reveal: reveal.to_vec(),
        duration: EffectDuration::Rounds { remaining: 3 },
        version: Version::INITIAL,
    }
}

#[test]
fn an_effect_opens_a_row_that_the_policy_closes() {
    let mut state = table_state();
    set_campaign(
        &mut state,
        policy(&[TokenField::Size], &[TokenField::Effects]),
    );
    assert_eq!(rows(Role::Player, &state), vec![TokenField::Size]);

    let applied = effect(
        1,
        pooled_token_id(0),
        &[TokenField::Effects, TokenField::Position],
    );
    let events = vec![Event::TokenEffectApplied {
        effect: applied.clone(),
    }];
    let mut after = state.clone();
    evolve_all(&mut after, &events);
    // A temporary reveal beats a conceal: that is the whole point of the effect.
    assert_eq!(
        rows(Role::Player, &after),
        vec![TokenField::Size, TokenField::Effects, TokenField::Position]
    );
}

#[test]
fn an_effect_does_not_open_rows_for_hidden_tokens() {
    let mut state = table_state();
    let applied = effect(2, pooled_token_id(2), &[TokenField::Position]); // GM-layer token
    let events = vec![Event::TokenEffectApplied {
        effect: applied.clone(),
    }];
    let mut after = state.clone();
    evolve_all(&mut after, &events);
    let snapshot = snapshot_for(Role::Player, &after, Seq(2), []);
    assert!(!snapshot.tokens.iter().any(|t| t.id == pooled_token_id(2)));
    assert!(
        !snapshot
            .token_inspector
            .iter()
            .any(|v| v.token == pooled_token_id(2))
    );
    state = after;
    assert_eq!(state.effects.len(), 1);
}

#[test]
fn applying_and_expiring_an_effect_needs_a_fresh_snapshot() {
    let before = table_state();
    let applied = effect(3, pooled_token_id(0), &[TokenField::Position]);
    let mut after = before.clone();
    evolve_all(
        &mut after,
        &[Event::TokenEffectApplied {
            effect: applied.clone(),
        }],
    );
    let batch = EventBatch {
        seq: Seq(2),
        caused_by: None,
        events: vec![Event::TokenEffectApplied {
            effect: applied.clone(),
        }],
    };
    assert_eq!(
        filter_for_players(&before, &after, &batch),
        ForPlayers::Resnapshot
    );

    // Expiry is a removal event; the rows go with it.
    let expiry = EventBatch {
        seq: Seq(3),
        caused_by: None,
        events: vec![Event::TokenEffectRemoved {
            id: applied.id,
            expired: true,
        }],
    };
    assert_eq!(
        filter_for_players(&after, &before, &expiry),
        ForPlayers::Resnapshot
    );
}

#[test]
fn an_effect_without_reveal_changes_no_rows() {
    let before = table_state();
    let plain = effect(4, pooled_token_id(0), &[]);
    let mut after = before.clone();
    evolve_all(
        &mut after,
        &[Event::TokenEffectApplied {
            effect: plain.clone(),
        }],
    );
    let batch = EventBatch {
        seq: Seq(2),
        caused_by: None,
        events: vec![Event::TokenEffectApplied { effect: plain }],
    };
    assert!(matches!(
        filter_for_players(&before, &after, &batch),
        ForPlayers::Batch(_)
    ));
}

/// A trigger applies an effect whose reveal opens rows for players, and expiry closes them again.
#[test]
fn a_trigger_effect_reveals_rows_and_expiry_takes_them_back() {
    let mut state = table_state();
    set_campaign(
        &mut state,
        policy(&[TokenField::Size], &[TokenField::Position]),
    );
    let token = pooled_token_id(0);
    run(
        &mut state,
        Role::Gm,
        Intent::CreateTrigger {
            id: dnd_protocol::TriggerId(v7(0x9100)),
            definition: TriggerDef {
                map_ids: Vec::new(),
                name: "Detect magic".into(),
                enabled: true,
                once: false,
                event: TriggerEvent::Manual,
                tokens: Vec::new(),
                condition: None,
                steps: vec![TriggerStep {
                    when: None,
                    action: TriggerAction::ApplyEffect {
                        label: "Detect magic".into(),
                        condition: None,
                        resource: None,
                        dice: None,
                        heal: false,
                        reveal: vec![TokenField::Position, TokenField::Position],
                        duration: EffectDurationDef::Rounds { rounds: 2 },
                    },
                }],
            },
        },
        None,
    )
    .unwrap();
    run(
        &mut state,
        Role::Gm,
        Intent::FireTrigger {
            id: dnd_protocol::TriggerId(v7(0x9100)),
            token: Some(token),
        },
        None,
    )
    .unwrap();

    let effect = state
        .effects
        .values()
        .next()
        .expect("the effect is applied")
        .clone();
    // Duplicates are cleaned by validation, so the stored effect names the row once.
    assert_eq!(effect.reveal, vec![TokenField::Position]);
    assert_eq!(
        rows(Role::Player, &state),
        vec![TokenField::Size, TokenField::Position],
        "the effect opens a row the campaign closes"
    );

    // Removing the effect (expiry) closes the row again.
    run(
        &mut state,
        Role::Gm,
        Intent::RemoveTokenEffect { id: effect.id },
        Some(effect.version),
    )
    .unwrap();
    assert_eq!(rows(Role::Player, &state), vec![TokenField::Size]);
}
