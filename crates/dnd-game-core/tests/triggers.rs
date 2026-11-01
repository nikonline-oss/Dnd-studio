//! Triggers (ADR-0024) on the bundled SRD system: rights, checks, each event, actions with
//! conditions, `once`, and that players never see them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::path::Path;

use common::*;
use dnd_game_core::{ForPlayers, GameState, filter_for_players, players_state};
use dnd_protocol::arbitrary::{pooled_character_id, pooled_map_id, pooled_token_id, v7};
use dnd_protocol::*;

fn srd() -> PluginDto {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/system-srd5");
    let bytes = dnd_plugin::pack_directory(&path).unwrap();
    dnd_plugin::read_package(&bytes, dnd_plugin::PackageLimits::default())
        .unwrap()
        .to_dto()
}

const ZONE: u128 = 0x7400;
const BOARD: u128 = 0x7410;
const NODE: u128 = 0x7411;

fn trigger(n: u128) -> TriggerId {
    TriggerId(v7(0x7500 + n))
}

/// SRD 5.2; map 0 has the zone "Altar" around (0, 0), where tokens 0 (player's hero), 1 (other
/// player's hero) and 3 (NPC) stand; a board with a hidden "Secret passage" node.
fn world() -> GameState {
    let mut state = table_state();
    // Token 1 starts outside the zone, far right.
    state.tokens.get_mut(&pooled_token_id(1)).unwrap().x = 30_000;
    for (intent, version) in [
        (
            Intent::InstallPlugin {
                plugin: Box::new(srd()),
            },
            None,
        ),
        (
            Intent::SetSystemPlugin {
                id: Some("srd5".into()),
            },
            None,
        ),
        (
            Intent::CreateZone {
                id: ZoneId(v7(ZONE)),
                map_id: pooled_map_id(0),
                name: "Altar".into(),
                shape: ZoneShape::Rect {
                    x: 0,
                    y: 0,
                    width: 3200,
                    height: 3200,
                },
            },
            None,
        ),
        (
            Intent::CreateBoard {
                id: BoardId(v7(BOARD)),
                name: "Plot".into(),
            },
            None,
        ),
        (
            Intent::CreateBoardNode {
                id: BoardNodeId(v7(NODE)),
                board_id: BoardId(v7(BOARD)),
                content: BoardNodeContent::Note {
                    markdown: "Secret passage".into(),
                },
                x: 0,
                y: 0,
                width: 3200,
                height: 1600,
                visibility: BoardVisibility::Gm,
            },
            None,
        ),
    ] {
        run(&mut state, Role::Gm, intent, version).unwrap();
    }
    // Characters get the system's default sheet (hp 10/10, AC 10).
    for n in [0, 1] {
        let id = pooled_character_id(n);
        let version = state.characters[&id].version;
        run(
            &mut state,
            Role::Gm,
            Intent::UpdateCharacter {
                id,
                name: None,
                sheet_json: Some("{}".into()),
            },
            Some(version),
        )
        .unwrap();
    }
    state
}

fn step(action: TriggerAction) -> TriggerStep {
    TriggerStep { when: None, action }
}

fn def(event: TriggerEvent, condition: Option<&str>, steps: Vec<TriggerStep>) -> TriggerDef {
    TriggerDef {
        map_ids: Vec::new(),
        name: "Secret passage".into(),
        enabled: true,
        once: false,
        event,
        tokens: Vec::new(),
        condition: condition.map(str::to_owned),
        steps,
    }
}

fn create(state: &mut GameState, n: u128, definition: TriggerDef) {
    run(
        state,
        Role::Gm,
        Intent::CreateTrigger {
            id: trigger(n),
            definition,
        },
        None,
    )
    .unwrap();
}

fn perception(state: &mut GameState, role: Role, character: u128) -> Vec<Event> {
    run(
        state,
        role,
        Intent::RollDice {
            id: ChatMessageId(v7(0x7600 + character)),
            expression: "1d20 + @perception".into(),
            hidden: false,
            character_id: Some(pooled_character_id(character)),
            roll: Some("perception".into()),
        },
        None,
    )
    .unwrap()
}

fn reveal_secret() -> TriggerStep {
    step(TriggerAction::Reveal {
        target: RevealTarget::Node {
            id: BoardNodeId(v7(NODE)),
        },
    })
}

fn node_visibility(state: &GameState) -> BoardVisibility {
    state.board_nodes[&BoardNodeId(v7(NODE))].visibility
}

fn fired(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, Event::TriggerFired { .. }))
        .count()
}

fn scoped(mut definition: TriggerDef, maps: &[MapId]) -> TriggerDef {
    // JSON also exercises compatibility with definitions written before map scoping.
    let mut json = serde_json::to_value(&definition).unwrap();
    json["map_ids"] = serde_json::to_value(maps).unwrap();
    definition = serde_json::from_value(json).unwrap();
    definition
}

fn second_altar(state: &mut GameState) {
    let mut zone = state.zones[&ZoneId(v7(ZONE))].clone();
    zone.id = ZoneId(v7(ZONE + 1));
    zone.map_id = pooled_map_id(1);
    state.zones.insert(zone.id, zone);
}

#[test]
fn map_scope_matches_zone_names_and_rearms_each_map_after_edit() {
    let mut state = world();
    second_altar(&mut state);
    let mut definition = scoped(
        def(
            TriggerEvent::RollInZone {
                zone: ZoneId(v7(ZONE)),
                rolls: Vec::new(),
            },
            None,
            vec![reveal_secret()],
        ),
        &[pooled_map_id(0), pooled_map_id(1)],
    );
    definition.once = true;
    definition.tokens = vec![pooled_token_id(0)];
    create(&mut state, 0, definition.clone());
    assert_eq!(fired(&perception(&mut state, Role::Gm, 0)), 1);
    assert_eq!(fired(&perception(&mut state, Role::Gm, 0)), 0);
    state.tokens.get_mut(&pooled_token_id(0)).unwrap().map_id = pooled_map_id(1);
    let events = perception(&mut state, Role::Gm, 0);
    assert_eq!(fired(&events), 1, "same zone name on the second bound map");
    let event = events
        .iter()
        .find(|e| matches!(e, Event::TriggerFired { .. }))
        .unwrap();
    assert_eq!(
        serde_json::to_value(event).unwrap()["fired"]["map_id"],
        serde_json::to_value(pooled_map_id(1)).unwrap()
    );
    assert_eq!(fired(&perception(&mut state, Role::Gm, 0)), 0);
    let version = state.triggers[&trigger(0)].version;
    run(
        &mut state,
        Role::CoGm,
        Intent::UpdateTrigger {
            id: trigger(0),
            definition,
        },
        Some(version),
    )
    .unwrap();
    assert_eq!(fired(&perception(&mut state, Role::Gm, 0)), 1);
    state.tokens.get_mut(&pooled_token_id(0)).unwrap().map_id = pooled_map_id(0);
    assert_eq!(fired(&perception(&mut state, Role::Gm, 0)), 1);
}

#[test]
fn map_scope_excludes_unbound_map_and_rejects_missing_map() {
    let mut state = world();
    second_altar(&mut state);
    let mut definition = scoped(
        def(
            TriggerEvent::RollInZone {
                zone: ZoneId(v7(ZONE)),
                rolls: Vec::new(),
            },
            None,
            vec![reveal_secret()],
        ),
        &[pooled_map_id(1)],
    );
    definition.tokens = vec![pooled_token_id(0)];
    create(&mut state, 0, definition);
    assert_eq!(fired(&perception(&mut state, Role::Gm, 0)), 0);
    state.tokens.get_mut(&pooled_token_id(0)).unwrap().map_id = pooled_map_id(1);
    state.zones.get_mut(&ZoneId(v7(ZONE + 1))).unwrap().name = "Different".into();
    assert_eq!(fired(&perception(&mut state, Role::Gm, 0)), 0);
    let bad = scoped(
        def(TriggerEvent::Manual, None, vec![reveal_secret()]),
        &[MapId(v7(999))],
    );
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::CreateTrigger {
                id: trigger(9),
                definition: bad
            },
            None
        ),
        Err(RejectReason::invalid("map_ids"))
    );
}

#[test]
fn map_scope_manual_runs_each_bound_map_once_and_rejects_outside_token() {
    let mut state = world();
    let mut definition = scoped(
        def(TriggerEvent::Manual, None, vec![reveal_secret()]),
        &[pooled_map_id(0), pooled_map_id(1)],
    );
    definition.once = true;
    create(&mut state, 0, definition);
    let events = run(
        &mut state,
        Role::CoGm,
        Intent::FireTrigger {
            id: trigger(0),
            token: None,
        },
        None,
    )
    .unwrap();
    assert_eq!(fired(&events), 2);
    assert_eq!(
        serde_json::to_value(&state.triggers[&trigger(0)]).unwrap()["fired_maps"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        run(
            &mut state,
            Role::Gm,
            Intent::FireTrigger {
                id: trigger(0),
                token: None
            },
            None
        )
        .is_err()
    );
    let definition = scoped(
        def(TriggerEvent::Manual, None, vec![reveal_secret()]),
        &[pooled_map_id(1)],
    );
    create(&mut state, 1, definition);
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::FireTrigger {
                id: trigger(1),
                token: Some(pooled_token_id(0))
            },
            None
        ),
        Err(RejectReason::invalid("token"))
    );
}

#[test]
fn map_scope_board_links_are_atomic_reused_and_removed_with_binding() {
    let mut state = world();
    create(
        &mut state,
        0,
        scoped(
            def(TriggerEvent::Manual, None, vec![reveal_secret()]),
            &[pooled_map_id(0)],
        ),
    );
    let card = BoardNodeId(v7(NODE + 10));
    run(
        &mut state,
        Role::Gm,
        Intent::CreateBoardNode {
            id: card,
            board_id: BoardId(v7(BOARD)),
            content: BoardNodeContent::Trigger { id: trigger(0) },
            x: 0,
            y: 0,
            width: 3200,
            height: 1600,
            visibility: BoardVisibility::Gm,
        },
        None,
    )
    .unwrap();
    assert_eq!(state.board_edges.len(), 1);
    let map_cards = |state: &GameState| {
        state
            .board_nodes
            .values()
            .filter(|n| {
                matches!(
                    n.content,
                    BoardNodeContent::Link {
                        target: BoardLink::Map { .. }
                    }
                )
            })
            .count()
    };
    assert_eq!(map_cards(&state), 1);
    let mut definition = state.triggers[&trigger(0)].definition.clone();
    definition = scoped(definition, &[pooled_map_id(0), pooled_map_id(1)]);
    run(
        &mut state,
        Role::CoGm,
        Intent::UpdateTrigger {
            id: trigger(0),
            definition: definition.clone(),
        },
        Some(Version::INITIAL),
    )
    .unwrap();
    assert_eq!(state.board_edges.len(), 2);
    assert_eq!(map_cards(&state), 2);
    definition = scoped(definition, &[pooled_map_id(1)]);
    let version = state.triggers[&trigger(0)].version;
    run(
        &mut state,
        Role::Gm,
        Intent::UpdateTrigger {
            id: trigger(0),
            definition,
        },
        Some(version),
    )
    .unwrap();
    assert_eq!(state.board_edges.len(), 1);
    assert_eq!(
        map_cards(&state),
        2,
        "unbinding never deletes maps or their cards"
    );
    let version = state.triggers[&trigger(0)].version;
    run(
        &mut state,
        Role::Gm,
        Intent::DeleteTrigger { id: trigger(0) },
        Some(version),
    )
    .unwrap();
    assert!(!state.board_nodes.contains_key(&card));
    assert!(state.board_edges.is_empty());
}

#[test]
fn only_gm_roles_manage_and_fire_triggers() {
    let manual = def(TriggerEvent::Manual, None, vec![reveal_secret()]);
    for (role, allowed) in [
        (Role::Gm, true),
        (Role::CoGm, true),
        (Role::Player, false),
        (Role::Spectator, false),
    ] {
        let mut state = world();
        create(&mut state, 0, manual.clone());
        let intents = [
            (
                Intent::CreateTrigger {
                    id: trigger(9),
                    definition: manual.clone(),
                },
                None,
            ),
            (
                Intent::UpdateTrigger {
                    id: trigger(0),
                    definition: manual.clone(),
                },
                Some(Version::INITIAL),
            ),
            (
                Intent::FireTrigger {
                    id: trigger(0),
                    token: None,
                },
                None,
            ),
            (
                Intent::DeleteTrigger { id: trigger(0) },
                Some(Version::INITIAL),
            ),
        ];
        for (intent, version) in intents {
            let result = run(&mut state.clone(), role, intent.clone(), version);
            if allowed {
                assert!(result.is_ok(), "{role:?} {intent:?}: {result:?}");
            } else {
                assert_eq!(result, Err(RejectReason::Forbidden), "{role:?} {intent:?}");
            }
        }
    }
}

#[test]
fn definitions_are_checked() {
    let state = world();
    let attempt = |definition: TriggerDef| {
        run(
            &mut state.clone(),
            Role::Gm,
            Intent::CreateTrigger {
                id: trigger(9),
                definition,
            },
            None,
        )
    };
    let zone = |zone| TriggerEvent::TokenEnteredZone { zone };
    assert_eq!(
        attempt(def(zone(ZoneId(v7(1))), None, vec![reveal_secret()])),
        Err(RejectReason::invalid("event"))
    );
    assert_eq!(
        attempt(def(
            TriggerEvent::RollInZone {
                zone: ZoneId(v7(ZONE)),
                rolls: vec!["flying".into()]
            },
            None,
            vec![reveal_secret()]
        )),
        Err(RejectReason::invalid("event"))
    );
    assert_eq!(
        attempt(def(
            TriggerEvent::Manual,
            Some("@total >="),
            vec![reveal_secret()]
        )),
        Err(RejectReason::invalid("condition"))
    );
    assert_eq!(
        attempt(def(TriggerEvent::Manual, None, Vec::new())),
        Err(RejectReason::invalid("steps"))
    );
    let bad_steps = [
        TriggerAction::Reveal {
            target: RevealTarget::Node {
                id: BoardNodeId(v7(1)),
            },
        },
        TriggerAction::Roll {
            dice: "1d".into(),
            label: String::new(),
        },
        TriggerAction::AddCondition {
            condition: "on_fire".into(),
        },
        // A computed resource could not be written back.
        TriggerAction::ChangeResource {
            resource: "nope".into(),
            dice: "1d6".into(),
            heal: false,
            target: ResourceTarget::Activator,
        },
    ];
    for action in bad_steps {
        assert_eq!(
            attempt(def(TriggerEvent::Manual, None, vec![step(action.clone())])),
            Err(RejectReason::invalid("steps")),
            "{action:?}"
        );
    }
}

/// The scenario from ADR-0024: a perception check in the altar zone opens the secret passage.
#[test]
fn a_successful_roll_in_the_zone_reveals_the_node_to_everyone() {
    let event = TriggerEvent::RollInZone {
        zone: ZoneId(v7(ZONE)),
        rolls: vec!["perception".into()],
    };
    // Too hard: nothing happens.
    let mut state = world();
    create(
        &mut state,
        0,
        def(event.clone(), Some("@total >= 100"), vec![reveal_secret()]),
    );
    let events = perception(&mut state, Role::Player, 0);
    assert_eq!(fired(&events), 0);
    assert_eq!(node_visibility(&state), BoardVisibility::Gm);

    // Passed: the node opens in the same batch, and players get a fresh snapshot.
    let mut state = world();
    create(
        &mut state,
        0,
        def(event, Some("@total >= 1"), vec![reveal_secret()]),
    );
    let before = state.clone();
    let events = perception(&mut state, Role::Player, 0);
    assert_eq!(fired(&events), 1);
    assert_eq!(node_visibility(&state), BoardVisibility::Players);
    let batch = EventBatch {
        seq: Seq(1),
        caused_by: None,
        events: events.clone(),
    };
    // The board itself is still hidden from players: they see nothing yet.
    assert!(matches!(
        filter_for_players(&before, &state, &batch),
        ForPlayers::Batch(_)
    ));

    // Outside the zone the same roll does nothing.
    let mut state = world();
    create(
        &mut state,
        0,
        def(
            TriggerEvent::RollInZone {
                zone: ZoneId(v7(ZONE)),
                rolls: Vec::new(),
            },
            None,
            vec![reveal_secret()],
        ),
    );
    assert_eq!(fired(&perception(&mut state, Role::Gm, 1)), 0);
}

#[test]
fn entering_the_zone_fires_once_per_entry_and_respects_the_token_filter() {
    let mut state = world();
    let mut definition = def(
        TriggerEvent::TokenEnteredZone {
            zone: ZoneId(v7(ZONE)),
        },
        None,
        vec![step(TriggerAction::Chat {
            text: "The floor creaks.".into(),
            gm_only: false,
        })],
    );
    definition.tokens = vec![pooled_token_id(1)];
    create(&mut state, 0, definition);
    let step_to = |state: &mut GameState, token: u128, x: i64| {
        run(
            state,
            Role::Gm,
            Intent::MoveToken {
                id: pooled_token_id(token),
                x,
                y: 160,
            },
            None,
        )
        .unwrap()
    };
    // Token 0 is not in the filter.
    assert_eq!(fired(&step_to(&mut state, 0, 320)), 0);
    // Token 1 enters, then moves inside: one firing.
    assert_eq!(fired(&step_to(&mut state, 1, 160)), 1);
    assert_eq!(fired(&step_to(&mut state, 1, 640)), 0);
    // Out and back in: again.
    assert_eq!(fired(&step_to(&mut state, 1, 30_000)), 0);
    assert_eq!(fired(&step_to(&mut state, 1, 160)), 1);
}

/// An attack: roll to hit, then damage and a condition only on a hit.
#[test]
fn steps_roll_damage_and_put_conditions_when_their_condition_holds() {
    let attack = |to_hit: &str| {
        let mut state = world();
        create(
            &mut state,
            0,
            def(
                TriggerEvent::Manual,
                None,
                vec![
                    step(TriggerAction::Roll {
                        dice: to_hit.into(),
                        label: "Poisoned dart".into(),
                    }),
                    TriggerStep {
                        when: Some("@roll >= @ac".into()),
                        action: TriggerAction::ChangeResource {
                            resource: "hp".into(),
                            dice: "3".into(),
                            heal: false,
                            target: ResourceTarget::Activator,
                        },
                    },
                    TriggerStep {
                        when: Some("@roll >= @ac".into()),
                        action: TriggerAction::AddCondition {
                            condition: "poisoned".into(),
                        },
                    },
                ],
            ),
        );
        run(
            &mut state,
            Role::Gm,
            Intent::FireTrigger {
                id: trigger(0),
                token: Some(pooled_token_id(0)),
            },
            None,
        )
        .unwrap();
        state.characters[&pooled_character_id(0)].clone()
    };
    let hit = attack("1d20 + 100");
    let sheet: serde_json::Value = serde_json::from_str(&hit.sheet_json).unwrap();
    assert_eq!(sheet["hp"]["current"], 7);
    assert_eq!(hit.conditions, ["poisoned"]);

    let miss = attack("1d20 - 100");
    let sheet: serde_json::Value = serde_json::from_str(&miss.sheet_json).unwrap();
    assert_eq!(sheet["hp"]["current"], 10);
    assert!(miss.conditions.is_empty());
}

#[test]
fn damage_and_healing_stay_within_the_resource() {
    let mut state = world();
    create(
        &mut state,
        0,
        def(
            TriggerEvent::Manual,
            None,
            vec![
                step(TriggerAction::ChangeResource {
                    resource: "hp".into(),
                    dice: "50".into(),
                    heal: false,
                    target: ResourceTarget::Activator,
                }),
                step(TriggerAction::ChangeResource {
                    resource: "hp".into(),
                    dice: "4".into(),
                    heal: true,
                    target: ResourceTarget::Activator,
                }),
                step(TriggerAction::ChangeResource {
                    resource: "hp".into(),
                    dice: "100".into(),
                    heal: true,
                    target: ResourceTarget::Activator,
                }),
            ],
        ),
    );
    let events = run(
        &mut state,
        Role::Gm,
        Intent::FireTrigger {
            id: trigger(0),
            token: Some(pooled_token_id(0)),
        },
        None,
    )
    .unwrap();
    let deltas: Vec<i64> = events
        .iter()
        .filter_map(|e| match e {
            Event::ChatMessagePosted { message } => match &message.system {
                Some(SystemMessage::ResourceChanged { delta, .. }) => Some(*delta),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(deltas, [-10, 4, 6]);
}

/// (character name, delta) announcements of resource changes.
fn resource_deltas(events: &[Event]) -> Vec<(String, i64)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::ChatMessagePosted { message } => match &message.system {
                Some(SystemMessage::ResourceChanged { name, delta, .. }) => {
                    Some((name.clone(), *delta))
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn hp_of(state: &GameState, character: u128) -> i64 {
    let sheet: serde_json::Value =
        serde_json::from_str(&state.characters[&pooled_character_id(character)].sheet_json)
            .unwrap();
    sheet["hp"]["current"].as_i64().unwrap()
}

#[test]
fn change_resource_zone_target_hits_each_character_in_the_zone_once() {
    let mut state = world();
    create(
        &mut state,
        0,
        manual_scoped(vec![step(TriggerAction::ChangeResource {
            resource: "hp".into(),
            dice: "1d6".into(),
            heal: false,
            target: ResourceTarget::Zone {
                id: ZoneId(v7(ZONE)),
            },
        })]),
    );
    // Token 0 (inside the zone) fires; the zone also holds token 2 of the same character and
    // token 3 without a character. Token 1 (another character) stands outside.
    let events = run(
        &mut state,
        Role::Gm,
        Intent::FireTrigger {
            id: trigger(0),
            token: Some(pooled_token_id(0)),
        },
        None,
    )
    .unwrap();
    let deltas = resource_deltas(&events);
    assert_eq!(deltas.len(), 1);
    assert_eq!(deltas[0].0, "Hero 0");
    assert_eq!(hp_of(&state, 0), 10 + deltas[0].1);
    assert_eq!(hp_of(&state, 1), 10);
}

#[test]
fn change_resource_map_target_shares_one_roll_with_every_character_on_the_map() {
    let mut state = world();
    create(
        &mut state,
        0,
        manual_scoped(vec![step(TriggerAction::ChangeResource {
            resource: "hp".into(),
            dice: "1d6".into(),
            heal: false,
            target: ResourceTarget::Map,
        })]),
    );
    // Map 0 holds tokens of characters 0 (twice) and 1 and a token without a character; the
    // same character also has a token on map 1, which the firing map does not cover.
    let events = run(
        &mut state,
        Role::Gm,
        Intent::FireTrigger {
            id: trigger(0),
            token: Some(pooled_token_id(0)),
        },
        None,
    )
    .unwrap();
    let mut deltas: Vec<i64> = resource_deltas(&events)
        .into_iter()
        .map(|(_, d)| d)
        .collect();
    deltas.sort();
    assert_eq!(deltas.len(), 2);
    assert_eq!(deltas[0], deltas[1], "one roll is shared by all targets");
    assert!(hp_of(&state, 0) < 10);
    assert_eq!(hp_of(&state, 0), hp_of(&state, 1));
}

#[test]
fn change_resource_rejects_an_unknown_zone_target() {
    let mut state = world();
    let result = run(
        &mut state,
        Role::Gm,
        Intent::CreateTrigger {
            id: trigger(0),
            definition: def(
                TriggerEvent::Manual,
                None,
                vec![step(TriggerAction::ChangeResource {
                    resource: "hp".into(),
                    dice: "3".into(),
                    heal: false,
                    target: ResourceTarget::Zone {
                        id: ZoneId(v7(0xDEAD_BEEF)),
                    },
                })],
            ),
        },
        None,
    );
    assert_eq!(result, Err(RejectReason::invalid("steps")));
}

#[test]
fn legacy_change_resource_without_target_still_hits_the_activator() {
    let json = serde_json::json!({
        "name": "Poisoned dart",
        "enabled": true,
        "once": false,
        "map_ids": [],
        "event": { "type": "manual" },
        "tokens": [],
        "condition": null,
        "steps": [{
            "when": null,
            "action": { "type": "change_resource", "resource": "hp", "dice": "3", "heal": false }
        }]
    });
    let definition: TriggerDef = serde_json::from_value(json).unwrap();
    let TriggerAction::ChangeResource { target, .. } = &definition.steps[0].action else {
        panic!("change_resource step expected");
    };
    assert_eq!(*target, ResourceTarget::Activator);
}

#[test]
fn once_triggers_fire_once_until_edited() {
    let mut state = world();
    let mut definition = def(TriggerEvent::Manual, None, vec![reveal_secret()]);
    definition.once = true;
    create(&mut state, 0, definition.clone());
    let fire = |state: &mut GameState| {
        run(
            state,
            Role::Gm,
            Intent::FireTrigger {
                id: trigger(0),
                token: None,
            },
            None,
        )
    };
    assert_eq!(fired(&fire(&mut state).unwrap()), 1);
    assert!(state.triggers[&trigger(0)].fired);
    assert_eq!(fire(&mut state), Err(RejectReason::invalid("id")));
    let version = state.triggers[&trigger(0)].version;
    run(
        &mut state,
        Role::Gm,
        Intent::UpdateTrigger {
            id: trigger(0),
            definition,
        },
        Some(version),
    )
    .unwrap();
    assert!(!state.triggers[&trigger(0)].fired);
    assert!(fire(&mut state).is_ok());
}

#[test]
fn receiving_an_item_fires_ignoring_case() {
    let mut state = world();
    create(
        &mut state,
        0,
        def(
            TriggerEvent::ItemReceived {
                name: "старый ключ".into(),
            },
            None,
            vec![step(TriggerAction::Chat {
                text: "The key fits the crypt door.".into(),
                gm_only: true,
            })],
        ),
    );
    let give = |state: &mut GameState, name: &str| {
        let version = state.characters[&pooled_character_id(0)].version;
        run(
            state,
            Role::Gm,
            Intent::GiveItem {
                id: pooled_character_id(0),
                name: name.into(),
            },
            Some(version),
        )
        .unwrap()
    };
    assert_eq!(fired(&give(&mut state, "Rope")), 0);
    let events = give(&mut state, "Старый ключ");
    assert_eq!(fired(&events), 1);
    // A GM-only narration.
    assert!(events.iter().any(|e| matches!(
        e,
        Event::ChatMessagePosted { message } if message.visibility == ChatVisibility::Gm
    )));
}

#[test]
fn players_never_see_triggers() {
    let mut state = world();
    let before = state.clone();
    let events = run(
        &mut state,
        Role::Gm,
        Intent::CreateTrigger {
            id: trigger(0),
            definition: def(TriggerEvent::Manual, None, vec![reveal_secret()]),
        },
        None,
    )
    .unwrap();
    assert!(players_state(&state).triggers.is_empty());
    let batch = EventBatch {
        seq: Seq(1),
        caused_by: None,
        events,
    };
    assert_eq!(
        filter_for_players(&before, &state, &batch),
        ForPlayers::Batch(EventBatch {
            events: vec![Event::Redacted],
            ..batch
        })
    );
}

/// Fills `{{param}}` blanks the way the editor does: a whole-string blank of a number parameter
/// becomes a number, any other blank is replaced in place as text.
fn fill(value: &serde_json::Value, params: &[(&str, &str)]) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => {
            for (id, v) in params {
                if s == &format!("{{{{{id}}}}}")
                    && let Ok(n) = v.parse::<i64>()
                {
                    return serde_json::Value::from(n);
                }
            }
            let mut out = s.clone();
            for (id, v) in params {
                out = out.replace(&format!("{{{{{id}}}}}"), v);
            }
            serde_json::Value::String(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(|i| fill(i, params)).collect())
        }
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), fill(v, params)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[test]
fn bundled_templates_make_triggers_the_core_accepts() {
    let state = world();
    let zone = ZoneId(v7(ZONE)).to_string();
    let node = BoardNodeId(v7(NODE)).to_string();
    let params = [
        ("zone", zone.as_str()),
        ("node", node.as_str()),
        ("dc", "13"),
        ("message", "A draft from the wall."),
        ("damage", "1d6"),
        ("attack", "4"),
        ("target", "8"),
        ("turns", "3"),
    ];
    let srd = srd();
    assert_eq!(srd.trigger_templates.len(), 4);
    for (n, template) in srd.trigger_templates.iter().enumerate() {
        let definition: TriggerDef =
            serde_json::from_value(fill(&template.definition, &params)).unwrap();
        let result = run(
            &mut state.clone(),
            Role::Gm,
            Intent::CreateTrigger {
                id: trigger(20 + n as u128),
                definition,
            },
            None,
        );
        assert!(result.is_ok(), "{}: {result:?}", template.id);
    }
    // The poison template end to end: the effect lands on the token with its duration.
    let poison = srd
        .trigger_templates
        .iter()
        .find(|t| t.id == "poison")
        .unwrap();
    let definition: TriggerDef = serde_json::from_value(fill(&poison.definition, &params)).unwrap();
    let mut state = world();
    run(
        &mut state,
        Role::Gm,
        Intent::CreateTrigger {
            id: trigger(30),
            definition,
        },
        None,
    )
    .unwrap();
    let events = run(
        &mut state,
        Role::Gm,
        Intent::FireTrigger {
            id: trigger(30),
            token: Some(pooled_token_id(0)),
        },
        None,
    )
    .unwrap();
    let Some(Event::TokenEffectApplied { effect }) = events
        .iter()
        .find(|e| matches!(e, Event::TokenEffectApplied { .. }))
    else {
        panic!("the poison template applies an effect: {events:?}");
    };
    assert_eq!(effect.condition.as_deref(), Some("poisoned"));
    assert_eq!(effect.duration, EffectDuration::Turns { remaining: 3 });
}

fn sound_asset(n: u128) -> AssetDto {
    let sha = format!("{n:02x}").repeat(32);
    AssetDto {
        id: AssetId(v7(0x9200 + n)),
        usage: AssetUsage::Audio,
        sha256: sha.clone(),
        mime: "audio/ogg".into(),
        size_bytes: 1000,
        original_name: Some(format!("sound {n}.ogg")),
        width: 0,
        height: 0,
        display_sha256: Some(sha.clone()),
        thumbnail_sha256: sha,
        tiles: Vec::new(),
        folder_id: None,
        version: Version::INITIAL,
    }
}

fn manual_scoped(steps: Vec<TriggerStep>) -> TriggerDef {
    scoped(def(TriggerEvent::Manual, None, steps), &[pooled_map_id(0)])
}

fn fire(state: &mut GameState, n: u128) -> Vec<Event> {
    run(
        state,
        Role::Gm,
        Intent::FireTrigger {
            id: trigger(n),
            token: None,
        },
        None,
    )
    .unwrap()
}

#[test]
fn spawn_tokens_places_copies_of_a_character_on_the_firing_map() {
    let mut state = world();
    create(
        &mut state,
        0,
        manual_scoped(vec![step(TriggerAction::SpawnTokens {
            character: pooled_character_id(0),
            count: 3,
            placement: SpawnPlacement::Point { x: 8000, y: 8000 },
        })]),
    );
    let before = state.tokens.len();
    let events = fire(&mut state, 0);
    let spawned: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::TokenCreated { token } => Some(token),
            _ => None,
        })
        .collect();
    assert_eq!(spawned.len(), 3);
    assert_eq!(state.tokens.len(), before + 3);
    for token in &spawned {
        assert_eq!(token.map_id, pooled_map_id(0));
        assert_eq!(token.character_id, Some(pooled_character_id(0)));
        assert_eq!(token.layer, TokenLayer::Tokens);
        assert!(token.visible);
    }
    // Fanned out: no two tokens share a cell.
    let mut spots: Vec<_> = spawned.iter().map(|t| (t.x, t.y)).collect();
    spots.sort();
    spots.dedup();
    assert_eq!(spots.len(), 3);
}

#[test]
fn spawn_tokens_in_a_zone_lands_inside_it() {
    let mut state = world();
    create(
        &mut state,
        0,
        manual_scoped(vec![step(TriggerAction::SpawnTokens {
            character: pooled_character_id(1),
            count: 5,
            placement: SpawnPlacement::Zone {
                id: ZoneId(v7(ZONE)),
            },
        })]),
    );
    let events = fire(&mut state, 0);
    let zone = &state.zones[&ZoneId(v7(ZONE))];
    let spawned: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::TokenCreated { token } => Some(token),
            _ => None,
        })
        .collect();
    assert_eq!(spawned.len(), 5);
    for token in &spawned {
        assert!(
            zone.shape.contains(token.x, token.y),
            "spawned at {:?}",
            (token.x, token.y)
        );
    }
}

#[test]
fn spawn_tokens_checks_the_character_count_and_zone() {
    let mut state = world();
    for placement in [
        SpawnPlacement::Point { x: 0, y: 0 },
        SpawnPlacement::Zone {
            id: ZoneId(v7(ZONE)),
        },
    ] {
        let result = run(
            &mut state,
            Role::Gm,
            Intent::CreateTrigger {
                id: trigger(9),
                definition: manual_scoped(vec![step(TriggerAction::SpawnTokens {
                    character: pooled_character_id(99),
                    count: 1,
                    placement,
                })]),
            },
            None,
        );
        assert!(result.is_err());
    }
    for count in [0, 100] {
        let result = run(
            &mut state,
            Role::Gm,
            Intent::CreateTrigger {
                id: trigger(9),
                definition: manual_scoped(vec![step(TriggerAction::SpawnTokens {
                    character: pooled_character_id(0),
                    count,
                    placement: SpawnPlacement::Point { x: 0, y: 0 },
                })]),
            },
            None,
        );
        assert!(result.is_err(), "count {count} must be rejected");
    }
    let result = run(
        &mut state,
        Role::Gm,
        Intent::CreateTrigger {
            id: trigger(9),
            definition: manual_scoped(vec![step(TriggerAction::SpawnTokens {
                character: pooled_character_id(0),
                count: 1,
                placement: SpawnPlacement::Zone {
                    id: ZoneId(v7(0xDEAD)),
                },
            })]),
        },
        None,
    );
    assert!(result.is_err());
}

#[test]
fn spawn_tokens_without_a_map_context_spawns_nothing() {
    let mut state = world();
    // A global manual trigger fires with no map.
    create(
        &mut state,
        0,
        def(
            TriggerEvent::Manual,
            None,
            vec![step(TriggerAction::SpawnTokens {
                character: pooled_character_id(0),
                count: 2,
                placement: SpawnPlacement::Point { x: 0, y: 0 },
            })],
        ),
    );
    let before = state.tokens.len();
    let events = fire(&mut state, 0);
    assert_eq!(fired(&events), 1);
    assert_eq!(state.tokens.len(), before);
}

#[test]
fn start_combat_adds_visible_character_tokens_and_rolls_initiative() {
    let mut state = world();
    create(
        &mut state,
        0,
        manual_scoped(vec![step(TriggerAction::StartCombat)]),
    );
    let events = fire(&mut state, 0);
    let combat = events.iter().find_map(|e| match e {
        Event::CombatStarted { combat } => Some(combat.clone()),
        _ => None,
    });
    let combat = combat.expect("a combat starts");
    assert_eq!(combat.map_id, pooled_map_id(0));
    let joined: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::CombatantAdded { combatant } => Some(combatant.clone()),
            _ => None,
        })
        .collect();
    // Tokens 0 and 1 (visible, with characters); token 2 is GM-layer, token 3 has no character.
    let mut tokens: Vec<_> = joined.iter().filter_map(|c| c.token_id).collect();
    tokens.sort();
    assert_eq!(tokens, vec![pooled_token_id(0), pooled_token_id(1)]);
    for combatant in &joined {
        assert!(combatant.initiative.is_some());
    }
    // Every join rolled its initiative in the chat.
    let rolls = events
        .iter()
        .filter(
            |e| matches!(e, Event::ChatMessagePosted { message } if message.kind == ChatKind::Roll),
        )
        .count();
    assert_eq!(rolls, 2);
    // Firing again while the combat runs starts no second combat.
    let again = fire(&mut state, 0);
    assert!(
        !again
            .iter()
            .any(|e| matches!(e, Event::CombatStarted { .. }))
    );
    assert_eq!(state.combats.len(), 1);
}

#[test]
fn play_sting_marks_a_one_shot_sound() {
    let mut state = world();
    run(
        &mut state,
        Role::Gm,
        Intent::RegisterAsset {
            asset: Box::new(sound_asset(1)),
        },
        None,
    )
    .unwrap();
    create(
        &mut state,
        0,
        manual_scoped(vec![step(TriggerAction::PlaySting {
            asset: AssetId(v7(0x9201)),
            volume: 70,
        })]),
    );
    let events = fire(&mut state, 0);
    let sound = events.iter().find_map(|e| match e {
        Event::SoundPlayed { sound } => Some(sound.clone()),
        _ => None,
    });
    let sound = sound.expect("the sting plays");
    assert!(sound.sting);
    assert!(!sound.looped);
    assert_eq!(sound.volume, 70);
    // Stings are not retained in the session's sound list.
    assert!(state.sounds.is_empty());
}

#[test]
fn play_sting_requires_an_audio_asset_and_valid_volume() {
    let mut state = world();
    let result = run(
        &mut state,
        Role::Gm,
        Intent::CreateTrigger {
            id: trigger(9),
            definition: manual_scoped(vec![step(TriggerAction::PlaySting {
                asset: AssetId(v7(0x9201)),
                volume: 50,
            })]),
        },
        None,
    );
    assert!(result.is_err(), "unknown asset must be rejected");
    run(
        &mut state,
        Role::Gm,
        Intent::RegisterAsset {
            asset: Box::new(sound_asset(1)),
        },
        None,
    )
    .unwrap();
    let result = run(
        &mut state,
        Role::Gm,
        Intent::CreateTrigger {
            id: trigger(9),
            definition: manual_scoped(vec![step(TriggerAction::PlaySting {
                asset: AssetId(v7(0x9201)),
                volume: 200,
            })]),
        },
        None,
    );
    assert!(result.is_err(), "volume above the max must be rejected");
}
