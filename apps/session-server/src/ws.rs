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
    let max = app.config.limits.max_message_bytes;
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
                            // Missed batches: start over from a snapshot.
                            Err(RecvError::Lagged(_)) => self.send_snapshot().await,
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
                    .submit(intent, actor(self.participant.user_id, role))
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
            // Stage 2b answers every resync with a snapshot; deltas come with stage 4.
            ClientMessage::Resync { .. } => self.send_snapshot().await.map_err(|()| None),
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

    async fn send_snapshot(&mut self) -> Result<(), ()> {
        let snapshot = self
            .session
            .snapshot(self.participant.role)
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
                self.send(ServerMessage::Events(players.clone()), Some(seq))
                    .await
            }
            ForPlayers::Resnapshot => self.send_snapshot().await,
        }
    }

    async fn send(&mut self, body: ServerMessage, seq: Option<Seq>) -> Result<(), ()> {
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
