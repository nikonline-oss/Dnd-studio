//! Wire-format stability: any change to the JSON shape or the TypeScript types shows up as a
//! snapshot diff in review.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dnd_protocol::*;
use uuid::Uuid;

fn envelope<B>(body: B) -> Envelope<B> {
    let mut env = Envelope::new(MessageId(Uuid::from_u128(1)), 1_760_000_000_000, body);
    env.session_id = Some(SessionId(Uuid::from_u128(2)));
    env.sender = Some(UserId(Uuid::from_u128(3)));
    env
}

fn sample_token() -> TokenDto {
    TokenDto {
        id: TokenId(Uuid::from_u128(20)),
        map_id: MapId(Uuid::from_u128(10)),
        character_id: None,
        asset_id: None,
        name: "Goblin".into(),
        x: 2240,
        y: 3360,
        rotation: 0.0,
        scale: 1.0,
        layer: TokenLayer::Tokens,
        visible: true,
        version: Version::INITIAL,
    }
}

#[test]
fn client_messages_json() {
    let messages = vec![
        envelope(ClientMessage::Intent(IntentEnvelope {
            intent_id: IntentId(Uuid::from_u128(4)),
            expected_version: Some(Version(3)),
            intent: Intent::UpdateMap {
                id: MapId(Uuid::from_u128(10)),
                name: Some("Crypt".into()),
                grid: None,
                visible_to_players: Some(true),
            },
        })),
        envelope(ClientMessage::Intent(IntentEnvelope {
            intent_id: IntentId(Uuid::from_u128(5)),
            expected_version: None,
            intent: Intent::MoveToken {
                id: TokenId(Uuid::from_u128(20)),
                x: 1120,
                y: 568,
            },
        })),
        envelope(ClientMessage::Ping),
        envelope(ClientMessage::Resync { last_seq: Seq(41) }),
        envelope(ClientMessage::GetJournalEntry {
            id: JournalEntryId(Uuid::from_u128(30)),
        }),
    ];
    insta::assert_json_snapshot!(messages);
}

#[test]
fn server_messages_json() {
    let mut events = envelope(ServerMessage::Events(EventBatch {
        seq: Seq(42),
        caused_by: Some(IntentId(Uuid::from_u128(5))),
        events: vec![
            Event::TokenCreated {
                token: sample_token(),
            },
            Event::TokenMoved {
                id: TokenId(Uuid::from_u128(20)),
                x: 1120,
                y: 568,
            },
            Event::Redacted,
            Event::UserJoined {
                user: UserDto {
                    id: UserId(Uuid::from_u128(2)),
                    name: "Mira".into(),
                    role: Role::Player,
                },
            },
        ],
    }));
    events.seq = Some(Seq(43));
    let messages = vec![
        events,
        envelope(ServerMessage::Rejected(Rejection {
            intent_id: IntentId(Uuid::from_u128(4)),
            reason: RejectReason::StaleVersion {
                current: Version(4),
            },
        })),
        envelope(ServerMessage::Rejected(Rejection {
            intent_id: IntentId(Uuid::from_u128(6)),
            reason: RejectReason::ProtocolMismatch {
                server: 1,
                client: 2,
            },
        })),
        envelope(ServerMessage::Pong),
        envelope(ServerMessage::Accepted(Acceptance {
            intent_id: IntentId(Uuid::from_u128(5)),
            seq: Seq(45),
        })),
        envelope(ServerMessage::JournalEntry {
            id: JournalEntryId(Uuid::from_u128(30)),
            entry: None,
        }),
    ];
    insta::assert_json_snapshot!(messages);
}

#[test]
fn protocol_mismatch_is_detected() {
    let mut env = envelope(ClientMessage::Ping);
    assert!(env.check_version().is_ok());
    env.v = PROTOCOL_VERSION + 1;
    assert_eq!(
        env.check_version(),
        Err(RejectReason::ProtocolMismatch {
            server: PROTOCOL_VERSION,
            client: PROTOCOL_VERSION + 1
        })
    );
}

#[test]
fn shared_playback_wire_contract_and_legacy_defaults() {
    let intent = Intent::SetSoundPlayback {
        asset_id: AssetId(Uuid::from_u128(20)),
        position_ms: 1234,
        paused: true,
        volume: 80,
        looped: false,
    };
    assert_eq!(intent.kind(), "set_sound_playback");
    assert_eq!(intent.version_policy(), VersionPolicy::LastWriteWins);
    assert_eq!(intent.created_id(), None);
    let json = serde_json::to_value(&intent).unwrap();
    assert_eq!(json["kind"], "set_sound_playback");
    let decoded: Intent = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, intent);
    let legacy: SoundDto = serde_json::from_value(serde_json::json!({
        "asset_id": Uuid::from_u128(20), "sha256": "00".repeat(32),
        "mime": "audio/ogg", "volume": 80, "looped": true
    }))
    .unwrap();
    assert_eq!((legacy.position_ms, legacy.paused), (0, false));
    assert_eq!(PROTOCOL_VERSION, 8);
    let mut old = envelope(ClientMessage::Ping);
    old.v = 7;
    assert!(old.check_version().is_err());
}

#[test]
fn reject_codes_match_json_tag() {
    let reasons = [
        RejectReason::StaleVersion {
            current: Version(1),
        },
        RejectReason::MissingExpectedVersion,
        RejectReason::Forbidden,
        RejectReason::NotFound,
        RejectReason::AlreadyExists,
        RejectReason::invalid("name"),
        RejectReason::ProtocolMismatch {
            server: 1,
            client: 2,
        },
    ];
    for reason in reasons {
        let json = serde_json::to_value(&reason).unwrap();
        assert_eq!(json["code"], reason.code());
    }
}

#[cfg(feature = "specta")]
#[test]
fn typescript_bindings() {
    let types = register_types(specta::Types::default());
    let ts = specta_typescript::Typescript::default()
        .export(&types, specta_serde::Format)
        .unwrap();
    insta::assert_snapshot!(ts);
}
