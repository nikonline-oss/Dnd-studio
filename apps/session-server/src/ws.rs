//! `WS /ws/sessions/{id}` (CLAUDE.md §7): a snapshot on connect, then intents in and batches out,
//! each recipient getting what its role may see (ADR-0022).

use std::time::{Duration, Instant};

use axum::extract::ws::{CloseFrame, Message, Utf8Bytes, WebSocket, close_code};
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::Response;
use dnd_db::{ParticipantRow, ParticipantStatus};
use dnd_game_core::{ForPlayers, sees_everything};
use dnd_protocol::{
    Acceptance, ClientMessage, Envelope, IntentOutcome, MessageId, RejectReason, Rejection, Seq,
    ServerMessage, SessionId,
};
use tokio::sync::broadcast::error::RecvError;

use crate::app::App;
use crate::error::ApiError;
use crate::now_ms;
use crate::session::{SessionHandle, actor};

pub async fn upgrade(
    State(app): State<App>,
    Path(id): Path<SessionId>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let participant = app.participant(id, &headers).await?;
    let session = app.session(id).await?;
    let ordinary_max = app.config.limits.max_message_bytes;
    // Plugin descriptions have their own protocol bound; only GM roles can install them.
    let max = if sees_everything(participant.role) {
        ordinary_max.max(dnd_protocol::plugin::MAX_PLUGIN_BYTES + 4096)
    } else {
        ordinary_max
    };
    Ok(ws
        .max_message_size(max)
        .max_frame_size(max)
        .on_upgrade(move |socket| async move {
            let id = participant.id;
            app.db
                .set_participant_status(id, ParticipantStatus::Connected, now_ms())
                .await
                .ok();
            Connection {
                app: app.clone(),
                session,
                participant,
                socket,
                sent_seq: None,
            }
            .run()
            .await;
            app.db
                .set_participant_status(id, ParticipantStatus::Disconnected, now_ms())
                .await
                .ok();
        }))
}

struct Connection {
    app: App,
    session: SessionHandle,
    participant: ParticipantRow,
    socket: WebSocket,
    /// Sequence number of the last batch or snapshot this connection was sent (ADR-0053): the
    /// base a delta is replayed from after a lag or a resync request.
    sent_seq: Option<Seq>,
}

/// Why a connection ends (sent as the close frame).
struct Close(u16, &'static str);

impl Connection {
    async fn run(mut self) {
        let _guard = self.session.connected();
        let mut events = self.session.subscribe();
        let limits = self.app.config.limits.clone();
        let mut last_seen = Instant::now();
        let mut window = (Instant::now(), 0u32);
        // Checks for silence a few times per timeout.
        let mut tick = tokio::time::interval(
            (limits.idle_timeout / 3).clamp(Duration::from_millis(10), Duration::from_secs(5)),
        );
        let session = self.session.clone();
        let ended = session.ended();
        tokio::pin!(ended);
        tracing::info!(session = %self.session.id, participant = %self.participant.id, "connected");

        let close = 'conn: {
            if self.send_snapshot().await.is_err() {
                break 'conn None;
            }
            loop {
                tokio::select! {
                    incoming = self.socket.recv() => {
                        let Some(Ok(message)) = incoming else { break 'conn None };
                        last_seen = Instant::now();
                        match message {
                            Message::Text(text) => {
                                if window.0.elapsed() >= Duration::from_secs(60) {
                                    window = (Instant::now(), 0);
                                }
                                window.1 += 1;
                                if window.1 > limits.messages_per_minute {
                                    break 'conn Some(Close(close_code::POLICY, "rate_limited"));
                                }
                                match self.handle(text.as_str()).await {
                                    Ok(()) => {}
                                    Err(close) => break 'conn close,
                                }
                            }
                            Message::Binary(_) => {
                                break 'conn Some(Close(close_code::UNSUPPORTED, "binary_not_supported"));
                            }
                            Message::Close(_) => break 'conn None,
                            Message::Ping(_) | Message::Pong(_) => {}
                        }
                    }
                    batch = events.recv() => {
                        let sent = match batch {
                            Ok(batch) => self.send_batch(&batch).await,
                            // Missed batches: replay what fits the delta window, else a snapshot.
                            Err(RecvError::Lagged(_)) => self.resync().await,
                            Err(RecvError::Closed) => {
                                break 'conn Some(Close(close_code::AWAY, self.session.end_reason()));
                            }
                        };
                        if sent.is_err() {
                            break 'conn None;
                        }
                    }
                    reason = &mut ended => {
                        break 'conn Some(Close(close_code::AWAY, reason));
                    }
                    _ = tick.tick() => {
                        if last_seen.elapsed() > limits.idle_timeout {
                            break 'conn Some(Close(close_code::AWAY, "timeout"));
                        }
                    }
                }
            }
        };
        if let Some(Close(code, reason)) = close {
            tracing::info!(session = %self.session.id, participant = %self.participant.id, reason, "closing");
            let _ = self
                .socket
                .send(Message::Close(Some(CloseFrame {
                    code,
                    reason: Utf8Bytes::from_static(reason),
                })))
                .await;
        }
        tracing::info!(session = %self.session.id, participant = %self.participant.id, "disconnected");
    }

    /// One client message. `Err` closes the connection (with a frame when `Some`).
    async fn handle(&mut self, text: &str) -> Result<(), Option<Close>> {
        let Ok(envelope) = serde_json::from_str::<Envelope<ClientMessage>>(text) else {
            return Err(Some(Close(close_code::INVALID, "invalid_message")));
        };
        let plugin_install = sees_everything(self.participant.role)
            && matches!(&envelope.body, ClientMessage::Intent(intent)
                if matches!(&intent.intent, dnd_protocol::Intent::InstallPlugin { .. }));
        if text.len() > self.app.config.limits.max_message_bytes && !plugin_install {
            return Err(Some(Close(close_code::SIZE, "message_too_large")));
        }
        if let Err(reason) = envelope.validate() {
            // A wrong version is answered so the client can say "update", then closed.
            let mismatch = matches!(reason, RejectReason::ProtocolMismatch { .. });
            if let ClientMessage::Intent(intent) = &envelope.body {
                self.send(
                    ServerMessage::Rejected(Rejection {
                        intent_id: intent.intent_id,
                        reason,
                    }),
                    None,
                )
                .await
                .map_err(|()| None)?;
                if !mismatch {
                    return Ok(());
                }
            }
            return Err(Some(Close(
                close_code::PROTOCOL,
                if mismatch {
                    "protocol_mismatch"
                } else {
                    "invalid_message"
                },
            )));
        }
        let role = self.participant.role;
        let reason = self.session.end_reason();
        let gone = |_| Some(Close(close_code::AWAY, reason));
        match envelope.body {
            ClientMessage::Intent(intent) => {
                let intent_id = intent.intent_id;
                let outcome = self
                    .session
                    .submit(*intent, actor(self.participant.user_id, role))
                    .await
                    .map_err(gone)?;
                let reply = match outcome {
                    Ok(IntentOutcome::Accepted { seq }) => {
                        ServerMessage::Accepted(Acceptance { intent_id, seq })
                    }
                    Ok(IntentOutcome::Rejected { reason }) => {
                        ServerMessage::Rejected(Rejection { intent_id, reason })
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "intent failed");
                        return Err(Some(Close(close_code::ERROR, "internal")));
                    }
                };
                self.send(reply, None).await.map_err(|()| None)
            }
            ClientMessage::Ping => self
                .send(ServerMessage::Pong, None)
                .await
                .map_err(|()| None),
            // A resync replays what the recipient missed from the event log (ADR-0053); a
            // snapshot is sent only when history is gone or access changed in between.
            ClientMessage::Resync { last_seq } => {
                // Zero means "I have nothing": only a snapshot can help there.
                let from = (last_seq.0 > 0).then_some(last_seq);
                self.send_delta(from).await.map_err(|()| None)
            }
            ClientMessage::GetJournalEntry { id } => {
                let entry = self.session.journal_entry(id, role).await.map_err(gone)?;
                self.send(
                    ServerMessage::JournalEntry {
                        id,
                        entry: entry.map(Box::new),
                    },
                    None,
                )
                .await
                .map_err(|()| None)
            }
        }
    }

    /// Replays the events since `from` for this recipient, or sends a snapshot when the history
    /// is gone or access changed (ADR-0053).
    async fn send_delta(&mut self, from: Option<Seq>) -> Result<(), ()> {
        let Some(from) = from else {
            return self.send_snapshot().await;
        };
        match self
            .session
            .delta(from, self.participant.role, Some(self.participant.user_id))
            .await
            .map_err(|_| ())?
        {
            Some(batch) => {
                let seq = batch.last_seq();
                self.send(ServerMessage::Events(batch), Some(seq)).await
            }
            None => self.send_snapshot().await,
        }
    }

    /// A lagged broadcast receiver asks for everything it missed.
    async fn resync(&mut self) -> Result<(), ()> {
        let from = self.sent_seq;
        self.send_delta(from).await
    }

    async fn send_snapshot(&mut self) -> Result<(), ()> {
        let snapshot = self
            .session
            .snapshot_for_user(self.participant.role, self.participant.user_id)
            .await
            .map_err(|_| ())?;
        let seq = snapshot.seq;
        self.send(ServerMessage::Snapshot(Box::new(snapshot)), Some(seq))
            .await
    }

    async fn send_batch(&mut self, batch: &crate::session::Broadcast) -> Result<(), ()> {
        if sees_everything(self.participant.role) {
            let seq = batch.full.last_seq();
            return self
                .send(ServerMessage::Events(batch.full.clone()), Some(seq))
                .await;
        }
        match &batch.players {
            ForPlayers::Batch(players) => {
                let seq = players.last_seq();
                let personal = personal_events(
                    players,
                    batch,
                    self.participant.role,
                    self.participant.user_id,
                );
                self.send(ServerMessage::Events(personal), Some(seq)).await
            }
            ForPlayers::Resnapshot => self.send_snapshot().await,
        }
    }

    async fn send(&mut self, body: ServerMessage, seq: Option<Seq>) -> Result<(), ()> {
        if let Some(seq) = seq {
            self.sent_seq = Some(seq);
        }
        let mut envelope = Envelope::new(MessageId::new_v7(), now_ms(), body);
        envelope.seq = seq;
        envelope.session_id = Some(self.session.id);
        let json = serde_json::to_string(&envelope).map_err(|_| ())?;
        self.socket
            .send(Message::Text(json.into()))
            .await
            .map_err(|_| ())
    }
}

pub(crate) fn personal_events(
    public: &dnd_protocol::EventBatch,
    batch: &crate::session::Broadcast,
    role: dnd_protocol::Role,
    user: dnd_protocol::UserId,
) -> dnd_protocol::EventBatch {
    let mut personal = public.clone();
    for (event, full) in personal.events.iter_mut().zip(&batch.full.events) {
        match full {
            dnd_protocol::Event::RollRequested { request }
            | dnd_protocol::Event::RollRequestUpdated { request } => {
                *event = if role == dnd_protocol::Role::Player
                    && batch.request_owners.get(&request.id) == Some(&Some(user))
                {
                    full.clone()
                } else {
                    dnd_protocol::Event::Redacted
                };
            }
            // The shared batch carries public sheets (ADR-0049); the owner gets the full one.
            dnd_protocol::Event::CharacterCreated { character }
            | dnd_protocol::Event::CharacterUpdated { character }
                if character.owner == Some(user) =>
            {
                *event = full.clone();
            }
            _ => {}
        }
    }
    personal
}

#[cfg(test)]
mod personal_event_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use dnd_protocol::*;
    #[test]
    fn wire_batches_reveal_requests_only_to_the_owner_and_keep_sequence() {
        let owner = UserId::new_v7();
        let request: RollRequestDto = serde_json::from_value(serde_json::json!({
            "id": RollRequestId::new_v7(), "label":"Secret trap", "trigger":TriggerId::new_v7(),
            "character":CharacterId::new_v7(), "token":null, "dice":"1d20", "hit":null,
            "resource":"hp", "damage_dice":"1d6", "heal":false, "status":"pending", "total":null,"delta":null,"version":1
        })).unwrap();
        let full = EventBatch {
            seq: Seq(23),
            caused_by: None,
            events: vec![Event::RollRequested {
                request: request.clone(),
            }],
        };
        let public = EventBatch {
            events: vec![Event::Redacted],
            ..full.clone()
        };
        let broadcast = crate::session::Broadcast {
            full: full.clone(),
            players: ForPlayers::Batch(public.clone()),
            request_owners: std::collections::BTreeMap::from([(request.id, Some(owner))]),
        };
        assert_eq!(
            personal_events(&public, &broadcast, Role::Player, owner),
            full
        );
        for (role, user) in [(Role::Player, UserId::new_v7()), (Role::Spectator, owner)] {
            let seen = personal_events(&public, &broadcast, role, user);
            assert_eq!(seen.seq, Seq(23));
            assert_eq!(seen.events, vec![Event::Redacted]);
            assert!(
                !serde_json::to_string(&seen)
                    .unwrap()
                    .contains("Secret trap")
            );
        }
    }

    #[test]
    fn a_hidden_draft_reaches_its_owner_with_the_full_sheet() {
        let owner = UserId::new_v7();
        let character = CharacterDto {
            id: CharacterId::new_v7(),
            kind: CharacterKind::Pc,
            name: "Draft".into(),
            owner: Some(owner),
            status: CharacterStatus::Draft,
            sheet_json: r#"{"notes":"secret"}"#.into(),
            conditions: Vec::new(),
            version: Version::INITIAL,
        };
        let full = EventBatch {
            seq: Seq(24),
            caused_by: None,
            events: vec![Event::CharacterCreated {
                character: character.clone(),
            }],
        };
        let public = EventBatch {
            events: vec![Event::Redacted],
            ..full.clone()
        };
        let broadcast = crate::session::Broadcast {
            full: full.clone(),
            players: ForPlayers::Batch(public.clone()),
            request_owners: std::collections::BTreeMap::new(),
        };
        assert_eq!(
            personal_events(&public, &broadcast, Role::Player, owner),
            full
        );
        assert_eq!(
            personal_events(&public, &broadcast, Role::Player, UserId::new_v7()),
            public
        );
    }
}
