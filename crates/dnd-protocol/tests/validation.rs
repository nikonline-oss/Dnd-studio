//! ADR-0015: integers sent to TS stay within ±(2^53−1); client-generated ids are UUIDv7.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dnd_protocol::arbitrary::{self, v7};
use dnd_protocol::*;
use proptest::prelude::*;

fn with_version(mut env: IntentEnvelope, version: i64) -> IntentEnvelope {
    env.expected_version = Some(Version(version));
    env
}

proptest! {
    /// Valid exactly when the integer is JS-safe.
    #[test]
    fn expected_version_must_be_js_safe(env in arbitrary::intent_envelope(), v in any::<i64>()) {
        let result = with_version(env, v).validate();
        if v.unsigned_abs() <= MAX_SAFE_INTEGER.unsigned_abs() {
            prop_assert_eq!(result, Ok(()));
        } else {
            prop_assert_eq!(result, Err(RejectReason::invalid("expected_version")));
        }
    }

    /// Edges are exact: ±(2^53−1) passes, one beyond fails.
    #[test]
    fn js_safe_edges(env in arbitrary::intent_envelope()) {
        for v in [MAX_SAFE_INTEGER, -MAX_SAFE_INTEGER, 0] {
            prop_assert!(with_version(env.clone(), v).validate().is_ok());
        }
        for v in [MAX_SAFE_INTEGER + 1, -MAX_SAFE_INTEGER - 1, i64::MAX, i64::MIN] {
            prop_assert!(with_version(env.clone(), v).validate().is_err());
        }
    }

    /// Any UUID that is not v7 (random v4, nil, max, arbitrary bits) is rejected as intent_id.
    #[test]
    fn intent_id_must_be_v7(env in arbitrary::intent_envelope(), bits in any::<u128>()) {
        let id = Uuid::from_u128(bits);
        let mut env = env;
        env.expected_version = None;
        env.intent_id = IntentId(id);
        let expected = if is_uuid_v7(&id) { Ok(()) } else { Err(RejectReason::invalid("intent_id")) };
        prop_assert_eq!(env.validate(), expected);
    }

    #[test]
    fn resync_seq_must_be_js_safe(seq in any::<i64>()) {
        let msg = ClientMessage::Resync { last_seq: Seq(seq) };
        prop_assert_eq!(msg.validate().is_ok(), is_js_safe(seq));
    }

    /// Strategies used across the workspace only produce valid envelopes.
    #[test]
    fn generated_envelopes_are_valid(env in arbitrary::intent_envelope()) {
        prop_assert_eq!(env.validate(), Ok(()));
    }
}

#[test]
fn created_entity_ids_must_be_v7() {
    let create = |id: Uuid| IntentEnvelope {
        intent_id: IntentId(v7(1)),
        expected_version: None,
        intent: Intent::CreateMap {
            id: MapId(id),
            name: "M".into(),
            background_asset_id: None,
            grid: Grid::default(),
            visible_to_players: false,
        },
    };
    assert_eq!(create(v7(2)).validate(), Ok(()));
    assert_eq!(
        create(Uuid::nil()).validate(),
        Err(RejectReason::invalid("id"))
    );
    // A v4 UUID (as produced by `crypto.randomUUID`).
    let v4 = Uuid::from_u128(0x6e4c_3f2a_9b1d_4c7e_8f00_1234_5678_9abc);
    assert_eq!(v4.get_version_num(), 4);
    assert_eq!(create(v4).validate(), Err(RejectReason::invalid("id")));
}

#[test]
fn envelope_header_is_validated() {
    let mut env = Envelope::new(MessageId(v7(1)), 0, ClientMessage::Ping);
    assert_eq!(env.validate(), Ok(()));
    env.ts = MAX_SAFE_INTEGER + 1;
    assert_eq!(env.validate(), Err(RejectReason::invalid("ts")));
    env.ts = 0;
    env.id = MessageId(Uuid::nil());
    assert_eq!(env.validate(), Err(RejectReason::invalid("id")));
}

#[test]
fn checked_next_stops_at_max_safe_integer() {
    assert_eq!(Version(1).checked_next(), Some(Version(2)));
    assert_eq!(
        Version(MAX_SAFE_INTEGER - 1).checked_next(),
        Some(Version(MAX_SAFE_INTEGER))
    );
    assert_eq!(Version(MAX_SAFE_INTEGER).checked_next(), None);
    assert_eq!(Seq(MAX_SAFE_INTEGER).checked_next(), None);
}
