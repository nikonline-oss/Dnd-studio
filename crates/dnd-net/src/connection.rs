//! The WS connection to a session (CLAUDE.md §7): reconnects with exponential backoff, pings, and
//! matches answers to intents. While offline only chat (younger than 5 min) and token moves wait in
//! a queue of at most 100; anything else fails at once. After a reconnect the server sends a
//! snapshot, then the queue is sent. Intents that were in flight when the connection broke are
//! resent with the same `intent_id` (idempotent, ADR-0015) if they may wait, and fail otherwise —
//! they are never replayed blindly.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use dnd_protocol::{
    ClientMessage, Envelope, EventBatch, Intent, IntentEnvelope, IntentId, IntentOutcome,
    JournalEntryDto, JournalEntryId, MessageId, RejectReason, ServerMessage, SessionId, Snapshot,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::{self, Message, client::IntoClientRequest};

use crate::error::NetError;
use crate::http::{normalize_base, tls_ready};

/// Timing and queue limits (CLAUDE.md §7 by default).
#[derive(Clone, Debug)]
pub struct ConnectOptions {
    pub backoff_initial: Duration,
    pub backoff_max: Duration,
    pub ping_every: Duration,
    /// No message from the server for this long: the connection is dead.
    pub dead_after: Duration,
    /// An intent without an answer for this long fails with [`NetError::Timeout`].
    pub request_timeout: Duration,
    /// Chat older than this is not sent after a reconnect.
    pub chat_max_age: Duration,
    pub queue_len: usize,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            backoff_initial: Duration::from_secs(1),
            backoff_max: Duration::from_secs(30),
            ping_every: Duration::from_secs(15),
            dead_after: Duration::from_secs(45),
            request_timeout: Duration::from_secs(15),
            chat_max_age: Duration::from_secs(5 * 60),
            queue_len: 100,
        }
    }
}

/// Connection state, for the status bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Connecting,
    Online,
    /// Lost; the next attempt is in `retry_in`.
    Reconnecting {
        attempt: u32,
        retry_in: Duration,
    },
    /// Over for good; `reason` is a code (`session_closed`, `unauthorized`, `protocol_mismatch`…).
    Closed {
        reason: String,
    },
}

/// What the connection delivers to the application.
#[derive(Debug)]
pub enum Incoming {
    /// The whole visible state: on connect, after a reconnect, or when visibility changed.
    Snapshot(Box<Snapshot>),
    Events(EventBatch),
    Status(Status),
}

type Reply<T> = oneshot::Sender<Result<T, NetError>>;

enum Request {
    Submit(IntentEnvelope, Reply<IntentOutcome>),
    Journal(JournalEntryId, Reply<Option<JournalEntryDto>>),
    Resync,
    Close,
}

/// A live connection to one session. Dropping it closes the connection.
#[derive(Debug)]
pub struct Connection {
    tx: mpsc::UnboundedSender<Request>,
    status: watch::Receiver<Status>,
}

impl Connection {
    /// Starts connecting in the background; everything the server sends goes to `incoming`.
    pub fn open(
        base: &str,
        session: SessionId,
        token: &str,
        options: ConnectOptions,
        incoming: mpsc::UnboundedSender<Incoming>,
    ) -> Result<Self, NetError> {
        tls_ready();
        let base = normalize_base(base)?;
        let url = format!("{}/ws/sessions/{session}", base.replacen("http", "ws", 1));
        let (tx, rx) = mpsc::unbounded_channel();
        let (status_tx, status) = watch::channel(Status::Connecting);
        let worker = Worker {
            url,
            session,
            token: token.to_owned(),
            options,
            incoming,
            status: status_tx,
            rx,
            queue: VecDeque::new(),
        };
        tokio::spawn(worker.run());
        Ok(Self { tx, status })
    }

    /// Sends an intent and waits for the server's answer (see the module docs for offline).
    pub async fn submit(&self, envelope: IntentEnvelope) -> Result<IntentOutcome, NetError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Request::Submit(envelope, reply))
            .map_err(|_| self.closed())?;
        rx.await.map_err(|_| self.closed())?
    }

    /// The text of a journal entry, if the server lets this participant read it.
    pub async fn journal_entry(
        &self,
        id: JournalEntryId,
    ) -> Result<Option<JournalEntryDto>, NetError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Request::Journal(id, reply))
            .map_err(|_| self.closed())?;
        rx.await.map_err(|_| self.closed())?
    }

    /// Asks for a fresh snapshot (it arrives as [`Incoming::Snapshot`]).
    pub fn resync(&self) {
        let _ = self.tx.send(Request::Resync);
    }

    #[must_use]
    pub fn status(&self) -> Status {
        self.status.borrow().clone()
    }

    pub fn close(&self) {
        let _ = self.tx.send(Request::Close);
    }

    fn closed(&self) -> NetError {
        match self.status() {
            Status::Closed { reason } => NetError::Closed(reason),
            _ => NetError::Closed("closed".to_owned()),
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.close();
    }
}

/// May this intent wait for a reconnect (CLAUDE.md §7)?
fn may_wait(intent: &Intent) -> bool {
    matches!(
        intent,
        Intent::SendMessage { .. } | Intent::MoveToken { .. }
    )
}

struct Queued {
    envelope: IntentEnvelope,
    reply: Reply<IntentOutcome>,
    since: Instant,
}

struct InFlight {
    queued: Queued,
    sent: Instant,
}

/// How a connected period ended.
enum Ended {
    /// Reconnect.
    Lost,
    /// Do not reconnect.
    Closed(String),
    /// The application closed it.
    ByClient,
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

struct Worker {
    url: String,
    session: SessionId,
    token: String,
    options: ConnectOptions,
    incoming: mpsc::UnboundedSender<Incoming>,
    status: watch::Sender<Status>,
    rx: mpsc::UnboundedReceiver<Request>,
    /// Intents waiting for the connection.
    queue: VecDeque<Queued>,
}

impl Worker {
    fn set_status(&self, status: Status) {
        if *self.status.borrow() != status {
            self.status.send_replace(status.clone());
            let _ = self.incoming.send(Incoming::Status(status));
        }
    }

    async fn run(mut self) {
        let mut attempt: u32 = 0;
        let reason = loop {
            match self.connect().await {
                Ok(socket) => {
                    attempt = 0;
                    self.set_status(Status::Online);
                    match self.connected(socket).await {
                        Ended::Lost => {}
                        Ended::Closed(reason) => break reason,
                        Ended::ByClient => break "closed".to_owned(),
                    }
                }
                Err(Some(reason)) => break reason,
                Err(None) => {}
            }
            let retry_in = self
                .options
                .backoff_initial
                .saturating_mul(1 << attempt.min(16))
                .min(self.options.backoff_max);
            attempt = attempt.saturating_add(1);
            self.set_status(Status::Reconnecting { attempt, retry_in });
            if !self.wait_offline(retry_in).await {
                break "closed".to_owned();
            }
        };
        self.set_status(Status::Closed {
            reason: reason.clone(),
        });
        for queued in self.queue.drain(..) {
            let _ = queued.reply.send(Err(NetError::Closed(reason.clone())));
        }
        // Requests that arrive from now on fail when their reply sender is dropped.
        self.rx.close();
        while let Some(request) = self.rx.recv().await {
            fail(request, &NetError::Closed(reason.clone()));
        }
    }

    /// `Err(Some(reason))` when trying again is pointless (no access, session over).
    async fn connect(&self) -> Result<Socket, Option<String>> {
        let mut request = self.url.as_str().into_client_request().map_err(|e| {
            tracing::error!(error = %e, "bad session URL");
            Some("invalid_address".to_owned())
        })?;
        let auth = format!("Bearer {}", self.token)
            .parse()
            .map_err(|_| Some("unauthorized".to_owned()))?;
        request.headers_mut().insert("authorization", auth);
        match tokio_tungstenite::connect_async(request).await {
            Ok((socket, _)) => Ok(socket),
            Err(tungstenite::Error::Http(response)) => {
                let status = response.status().as_u16();
                tracing::warn!(status, session = %self.session, "connection refused");
                match status {
                    401 => Err(Some("unauthorized".to_owned())),
                    403 => Err(Some("forbidden".to_owned())),
                    404 => Err(Some("not_found".to_owned())),
                    410 => Err(Some("session_closed".to_owned())),
                    _ => Err(None),
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, "connection failed");
                Err(None)
            }
        }
    }

    /// Waits `delay` while answering requests offline. `false` when the client closed.
    async fn wait_offline(&mut self, delay: Duration) -> bool {
        let sleep = tokio::time::sleep(delay);
        tokio::pin!(sleep);
        loop {
            tokio::select! {
                () = &mut sleep => return true,
                request = self.rx.recv() => match request {
                    None | Some(Request::Close) => return false,
                    // A snapshot comes with the reconnect anyway.
                    Some(Request::Resync) => {}
                    Some(Request::Journal(_, reply)) => {
                        let _ = reply.send(Err(NetError::Offline));
                    }
                    Some(Request::Submit(envelope, reply)) => self.enqueue(Queued {
                        envelope,
                        reply,
                        since: Instant::now(),
                    }),
                },
            }
        }
    }

    fn enqueue(&mut self, queued: Queued) {
        self.expire_chat();
        if may_wait(&queued.envelope.intent) && self.queue.len() < self.options.queue_len {
            self.queue.push_back(queued);
        } else {
            let _ = queued.reply.send(Err(NetError::Offline));
        }
    }

    fn expire_chat(&mut self) {
        let max_age = self.options.chat_max_age;
        let (keep, old): (VecDeque<_>, VecDeque<_>) = self.queue.drain(..).partition(|q| {
            !matches!(q.envelope.intent, Intent::SendMessage { .. }) || q.since.elapsed() < max_age
        });
        self.queue = keep;
        for q in old {
            let _ = q.reply.send(Err(NetError::Offline));
        }
    }

    async fn connected(&mut self, mut socket: Socket) -> Ended {
        let mut in_flight: HashMap<IntentId, InFlight> = HashMap::new();
        let mut journal: HashMap<JournalEntryId, Vec<Reply<Option<JournalEntryDto>>>> =
            HashMap::new();
        let mut last_heard = Instant::now();
        let mut ping = tokio::time::interval(self.options.ping_every);
        ping.reset();
        let mut flushed = false;

        let ended = loop {
            tokio::select! {
                frame = socket.next() => {
                    let text = match frame {
                        Some(Ok(Message::Text(text))) => text,
                        Some(Ok(Message::Close(frame))) => {
                            let reason = frame.map(|f| f.reason.to_string()).unwrap_or_default();
                            break match reason.as_str() {
                                "session_closed" | "protocol_mismatch" => Ended::Closed(reason),
                                _ => Ended::Lost,
                            };
                        }
                        Some(Ok(_)) => continue,
                        None | Some(Err(_)) => break Ended::Lost,
                    };
                    last_heard = Instant::now();
                    let Ok(envelope) = serde_json::from_str::<Envelope<ServerMessage>>(&text) else {
                        tracing::warn!("unreadable server message");
                        continue;
                    };
                    match envelope.body {
                        ServerMessage::Snapshot(snapshot) => {
                            let _ = self.incoming.send(Incoming::Snapshot(snapshot));
                            // The first snapshot after (re)connecting: now the queue may go.
                            if !flushed {
                                flushed = true;
                                self.expire_chat();
                                for queued in std::mem::take(&mut self.queue) {
                                    if self.send_intent(&mut socket, queued, &mut in_flight).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        ServerMessage::Events(batch) => {
                            let _ = self.incoming.send(Incoming::Events(batch));
                        }
                        ServerMessage::Accepted(a) => {
                            if let Some(f) = in_flight.remove(&a.intent_id) {
                                let _ = f.queued.reply.send(Ok(IntentOutcome::Accepted { seq: a.seq }));
                            }
                        }
                        ServerMessage::Rejected(r) => {
                            let mismatch = matches!(r.reason, RejectReason::ProtocolMismatch { .. });
                            if let Some(f) = in_flight.remove(&r.intent_id) {
                                let _ = f.queued.reply.send(Ok(IntentOutcome::Rejected { reason: r.reason }));
                            }
                            if mismatch {
                                break Ended::Closed("protocol_mismatch".to_owned());
                            }
                        }
                        ServerMessage::JournalEntry { id, entry } => {
                            for reply in journal.remove(&id).unwrap_or_default() {
                                let _ = reply.send(Ok(entry.clone().map(|e| *e)));
                            }
                        }
                        ServerMessage::Pong => {}
                    }
                }
                request = self.rx.recv() => match request {
                    None | Some(Request::Close) => {
                        let _ = socket.close(None).await;
                        break Ended::ByClient;
                    }
                    Some(Request::Resync) => {
                        let last_seq = dnd_protocol::Seq(0);
                        if self.send(&mut socket, ClientMessage::Resync { last_seq }).await.is_err() {
                            break Ended::Lost;
                        }
                    }
                    Some(Request::Journal(id, reply)) => {
                        let first = !journal.contains_key(&id);
                        journal.entry(id).or_default().push(reply);
                        if first && self.send(&mut socket, ClientMessage::GetJournalEntry { id }).await.is_err() {
                            break Ended::Lost;
                        }
                    }
                    Some(Request::Submit(envelope, reply)) => {
                        let queued = Queued { envelope, reply, since: Instant::now() };
                        if !flushed {
                            // Still waiting for the first snapshot: keep the order.
                            self.enqueue(queued);
                        } else if self.send_intent(&mut socket, queued, &mut in_flight).await.is_err() {
                            break Ended::Lost;
                        }
                    }
                },
                _ = ping.tick() => {
                    if last_heard.elapsed() > self.options.dead_after {
                        break Ended::Lost;
                    }
                    let timeout = self.options.request_timeout;
                    let late: Vec<IntentId> = in_flight
                        .iter()
                        .filter(|(_, f)| f.sent.elapsed() > timeout)
                        .map(|(id, _)| *id)
                        .collect();
                    for id in late {
                        if let Some(f) = in_flight.remove(&id) {
                            let _ = f.queued.reply.send(Err(NetError::Timeout));
                        }
                    }
                    if self.send(&mut socket, ClientMessage::Ping).await.is_err() {
                        break Ended::Lost;
                    }
                }
            }
        };

        // Unanswered intents: those that may wait go back to the queue (same intent_id, so a
        // repeat is harmless); the rest may have been applied or not — the caller must look.
        let mut retry: Vec<InFlight> = in_flight.into_values().collect();
        retry.sort_by_key(|f| f.sent);
        for f in retry {
            if matches!(ended, Ended::Lost) && may_wait(&f.queued.envelope.intent) {
                self.queue.push_back(f.queued);
            } else {
                let error = match &ended {
                    Ended::Closed(reason) => NetError::Closed(reason.clone()),
                    _ => NetError::ConnectionLost,
                };
                let _ = f.queued.reply.send(Err(error));
            }
        }
        for reply in journal.into_values().flatten() {
            let _ = reply.send(Err(NetError::ConnectionLost));
        }
        ended
    }

    async fn send_intent(
        &self,
        socket: &mut Socket,
        queued: Queued,
        in_flight: &mut HashMap<IntentId, InFlight>,
    ) -> Result<(), ()> {
        let intent_id = queued.envelope.intent_id;
        let body = ClientMessage::Intent(queued.envelope.clone());
        in_flight.insert(
            intent_id,
            InFlight {
                queued,
                sent: Instant::now(),
            },
        );
        self.send(socket, body).await
    }

    async fn send(&self, socket: &mut Socket, body: ClientMessage) -> Result<(), ()> {
        let mut envelope = Envelope::new(MessageId::new_v7(), now_ms(), body);
        envelope.session_id = Some(self.session);
        let json = serde_json::to_string(&envelope).map_err(|_| ())?;
        socket
            .send(Message::Text(json.into()))
            .await
            .map_err(|_| ())
    }
}

fn fail(request: Request, error: &NetError) {
    match request {
        Request::Submit(_, reply) => {
            let _ = reply.send(Err(error.clone()));
        }
        Request::Journal(_, reply) => {
            let _ = reply.send(Err(error.clone()));
        }
        Request::Resync | Request::Close => {}
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}
