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
                }),
                step(TriggerAction::ChangeResource {
                    resource: "hp".into(),
                    dice: "4".into(),
                    heal: true,
                }),
                step(TriggerAction::ChangeResource {
                    resource: "hp".into(),
                    dice: "100".into(),
                    heal: true,
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
