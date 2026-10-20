//! Every protocol message survives a JSON round trip unchanged.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dnd_protocol::arbitrary;
use dnd_protocol::{ClientMessage, Envelope, MessageId, ServerMessage};
use proptest::prelude::*;
use uuid::Uuid;

fn roundtrip<T>(value: &T) -> T
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let json = serde_json::to_string(value).expect("serialize");
    serde_json::from_str(&json).unwrap_or_else(|e| panic!("deserialize {json}: {e}"))
}

proptest! {
    #[test]
    fn client_intent_roundtrip(intent in arbitrary::intent_envelope()) {
        let msg = Envelope::new(MessageId(Uuid::from_u128(1)), 0, ClientMessage::Intent(intent));
        prop_assert_eq!(roundtrip(&msg), msg);
    }

    #[test]
    fn server_events_roundtrip(batch in arbitrary::event_batch()) {
        let msg = Envelope::new(MessageId(Uuid::from_u128(2)), 0, ServerMessage::Events(batch));
        prop_assert_eq!(roundtrip(&msg), msg);
    }
}
