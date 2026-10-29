//! The session server end to end: real HTTP and WS on a local port (ADR-0022).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::Path;
use std::time::Duration;

use dnd_db::{
    ARCHIVE_FORMAT, ArchiveLimits, ArchiveManifest, CampaignDb, CampaignEngine, LocalUser,
    read_archive, write_archive,
};
use dnd_game_core::{Actor, IntentWindow};
use dnd_protocol::arbitrary::v7;
use dnd_protocol::*;
use dnd_session_server::{Config, CreatedSession, Joined, Limits, Server};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::{self, Message, client::IntoClientRequest};

const HOST_TOKEN: &str = "host-token-for-tests-0123";

// The test world: a shared map with the player's token and a GM-layer token, a hidden map with a
// token, a shared and a GM-only journal entry, and one map image per map.
const SHARED_MAP: u128 = 0x10;
const HIDDEN_MAP: u128 = 0x11;
const PLAYER_TOKEN: u128 = 0x20;
const GM_TOKEN: u128 = 0x21;
const HIDDEN_TOKEN: u128 = 0x22;
const SHARED_ENTRY: u128 = 0x30;
const GM_ENTRY: u128 = 0x31;
const SHARED_IMAGE: &[u8] = b"shared map image";
const HIDDEN_IMAGE: &[u8] = b"hidden map image";

fn sha(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn gm_intent(intent: Intent) -> IntentEnvelope {
    IntentEnvelope {
        intent_id: IntentId::new_v7(),
        expected_version: None,
        intent,
    }
}

fn asset(n: u128, image: &[u8]) -> AssetDto {
    AssetDto {
        id: AssetId(v7(n)),
        usage: AssetUsage::Map,
        sha256: sha(&[image, b"original"].concat()),
        mime: "image/png".into(),
        size_bytes: 1,
        original_name: None,
        width: 100,
        height: 100,
        display_sha256: None,
        thumbnail_sha256: sha(image),
        tiles: Vec::new(),
        folder_id: None,
        version: Version::INITIAL,
    }
}

/// Builds the test campaign and returns its archive.
async fn campaign_archive(dir: &Path) -> Vec<u8> {
    let folder = dir.join("campaign");
    std::fs::create_dir_all(folder.join("assets")).unwrap();
    let gm = LocalUser {
        id: UserId(v7(1)),
        name: "Game Master".into(),
        role: Role::Gm,
    };
    let db = CampaignDb::create(&folder.join("campaign.sqlite"), "Lost Mine", &gm, 0)
        .await
        .unwrap();
    let mut engine = CampaignEngine::load(db, IntentWindow::DEFAULT, 1, 0)
        .await
        .unwrap();
    let actor = Actor {
        user_id: gm.id,
        role: Role::Gm,
    };
    let (shared_asset, hidden_asset) = (asset(0x40, SHARED_IMAGE), asset(0x41, HIDDEN_IMAGE));
    for image in [SHARED_IMAGE, HIDDEN_IMAGE] {
        std::fs::write(folder.join(format!("assets/{}.webp", sha(image))), image).unwrap();
    }
    engine
        .commit_events(
            vec![
                Event::AssetRegistered {
                    asset: Box::new(shared_asset.clone()),
                },
                Event::AssetRegistered {
                    asset: Box::new(hidden_asset.clone()),
                },
            ],
            Some(gm.id),
            0,
        )
        .await
        .unwrap();
    let map = |n, asset: &AssetDto, visible| Intent::CreateMap {
        id: MapId(v7(n)),
        name: format!("Map {n}"),
        background_asset_id: Some(asset.id),
        width: 1000,
        height: 1000,
        grid: Grid::default(),
        visible_to_players: visible,
    };
    let token = |n, map, layer| Intent::CreateToken {
        id: TokenId(v7(n)),
        map_id: MapId(v7(map)),
        character_id: None,
        asset_id: None,
        name: format!("Token {n}"),
        x: 0,
        y: 0,
        rotation: 0.0,
        scale: 1.0,
        layer,
        visible: true,
    };
    let entry = |n, visibility| Intent::CreateJournalEntry {
        id: JournalEntryId(v7(n)),
        folder_id: None,
        title: format!("Entry {n}"),
        markdown: format!("text {n}"),
        visibility,
        players_can_edit: false,
        table: None,
    };
    for intent in [
        map(SHARED_MAP, &shared_asset, true),
        map(HIDDEN_MAP, &hidden_asset, false),
        token(PLAYER_TOKEN, SHARED_MAP, TokenLayer::Tokens),
        token(GM_TOKEN, SHARED_MAP, TokenLayer::Gm),
        token(HIDDEN_TOKEN, HIDDEN_MAP, TokenLayer::Tokens),
        entry(SHARED_ENTRY, JournalVisibility::Players),
        entry(GM_ENTRY, JournalVisibility::Gm),
    ] {
        let (outcome, _) = engine.submit(&gm_intent(intent), &actor, 0).await.unwrap();
        assert!(
            matches!(outcome, IntentOutcome::Accepted { .. }),
            "{outcome:?}"
        );
    }
    let copy = dir.join("copy.sqlite");
    engine.db().copy_to(&copy).await.unwrap();
    engine.close().await.unwrap();
    let archive = dir.join("campaign.zip");
    write_archive(
        &archive,
        &copy,
        &folder.join("assets"),
        &ArchiveManifest {
            format: ARCHIVE_FORMAT,
            name: "Lost Mine".into(),
            exported_at_ms: 0,
            generator: "test".into(),
        },
        false,
    )
    .unwrap();
    std::fs::read(archive).unwrap()
}

struct Harness {
    server: Server,
    http: reqwest::Client,
    _dir: tempfile::TempDir,
    archive: Vec<u8>,
}

impl Harness {
    async fn start(limits: Limits) -> Self {
        // In a workspace build reqwest may have no default TLS provider (see Cargo.toml).
        let _ = rustls::crypto::ring::default_provider().install_default();
        let dir = tempfile::tempdir().unwrap();
        let archive = campaign_archive(dir.path()).await;
        let server = Server::start(Config {
            data_dir: dir.path().join("data"),
            bind: "127.0.0.1:0".parse().unwrap(),
            host_token: HOST_TOKEN.into(),
            public_url: None,
            limits,
        })
        .await
        .unwrap();
        Self {
            server,
            http: reqwest::Client::new(),
            _dir: dir,
            archive,
        }
    }

    async fn default() -> Self {
        Self::start(relaxed()).await
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.server.addr)
    }

    async fn upload(&self, token: &str, archive: Vec<u8>) -> reqwest::Response {
        let form = reqwest::multipart::Form::new().part(
            "archive",
            reqwest::multipart::Part::bytes(archive).file_name("c.zip"),
        );
        self.http
            .post(self.url("/api/sessions"))
            .bearer_auth(token)
            .multipart(form)
            .send()
            .await
            .unwrap()
    }

    async fn create(&self) -> CreatedSession {
        let response = self.upload(HOST_TOKEN, self.archive.clone()).await;
        assert_eq!(response.status(), 201);
        response.json().await.unwrap()
    }

    async fn join_raw(&self, session: SessionId, body: serde_json::Value) -> reqwest::Response {
        self.http
            .post(self.url(&format!("/api/sessions/{session}/join")))
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    async fn join_gm(&self, created: &CreatedSession) -> Joined {
        let response = self
            .join_raw(
                created.session_id,
                serde_json::json!({ "gm_key": created.gm_key }),
            )
            .await;
        assert_eq!(response.status(), 200);
        response.json().await.unwrap()
    }

    async fn join_player(&self, created: &CreatedSession, name: &str) -> Joined {
        // Codes may be typed in lower case and without the dash.
        let code = created.access_code.replace('-', "").to_lowercase();
        let response = self
            .join_raw(
                created.session_id,
                serde_json::json!({ "access_code": code, "name": name }),
            )
            .await;
        assert_eq!(response.status(), 200);
        response.json().await.unwrap()
    }

    async fn connect(&self, session: SessionId, token: &str) -> Ws {
        self.try_connect(session, token).await.unwrap()
    }

    async fn try_connect(&self, session: SessionId, token: &str) -> Result<Ws, tungstenite::Error> {
        let mut request = format!("ws://{}/ws/sessions/{session}", self.server.addr)
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        let (stream, _) = tokio_tungstenite::connect_async(request).await?;
        Ok(Ws { stream })
    }
}

/// Limits that let tests connect often from one IP.
fn relaxed() -> Limits {
    Limits {
        connections_per_minute: 10_000,
        sweep_interval: Duration::from_secs(3600),
        ..Limits::default()
    }
}

struct Ws {
    stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
}

impl Ws {
    async fn send(&mut self, body: ClientMessage) {
        let envelope = Envelope::new(MessageId::new_v7(), 0, body);
        self.send_raw(serde_json::to_string(&envelope).unwrap())
            .await;
    }

    async fn send_raw(&mut self, text: String) {
        self.stream.send(Message::Text(text.into())).await.unwrap();
    }

    async fn intent(&mut self, intent: Intent) -> IntentId {
        let envelope = gm_intent(intent);
        let id = envelope.intent_id;
        self.send(ClientMessage::Intent(envelope)).await;
        id
    }

    /// The next server message (text frames only).
    async fn next(&mut self) -> ServerMessage {
        self.next_envelope().await.body
    }

    async fn next_envelope(&mut self) -> Envelope<ServerMessage> {
        loop {
            let frame = tokio::time::timeout(Duration::from_secs(10), self.stream.next())
                .await
                .expect("timed out waiting for a message")
                .expect("connection ended")
                .unwrap();
            if let Message::Text(text) = frame {
                return serde_json::from_str(&text).unwrap();
            }
        }
    }

    async fn snapshot(&mut self) -> Snapshot {
        match self.next().await {
            ServerMessage::Snapshot(snapshot) => *snapshot,
            other => panic!("expected a snapshot, got {other:?}"),
        }
    }

    async fn events(&mut self) -> EventBatch {
        match self.next().await {
            ServerMessage::Events(batch) => batch,
            other => panic!("expected events, got {other:?}"),
        }
    }

    /// The close frame's reason, once the server closes.
    async fn closed(&mut self) -> String {
        loop {
            let frame = tokio::time::timeout(Duration::from_secs(10), self.stream.next())
                .await
                .expect("timed out waiting for the close");
            match frame {
                Some(Ok(Message::Close(frame))) => {
                    return frame.map(|f| f.reason.to_string()).unwrap_or_default();
                }
                Some(Ok(_)) => {}
                None | Some(Err(_)) => return String::new(),
            }
        }
    }
}

fn ids<T>(items: &[T], id: impl Fn(&T) -> uuid::Uuid) -> Vec<uuid::Uuid> {
    let mut ids: Vec<_> = items.iter().map(id).collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn gm_and_player_play_through_the_server() {
    let h = Harness::default().await;
    assert_eq!(
        h.upload("wrong-token-wrong-token", h.archive.clone())
            .await
            .status(),
        401
    );
    let created = h.create().await;
    assert_eq!(created.name, "Lost Mine");
    assert!(
        created
            .invite_url
            .ends_with(&format!("/api/sessions/{}", created.session_id))
    );
    let info: serde_json::Value = h
        .http
        .get(&created.invite_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(info["name"], "Lost Mine");
    assert_eq!(info["state"], "active");

    let wrong = h
        .join_raw(
            created.session_id,
            serde_json::json!({ "access_code": "AAAAA-AAAAA", "name": "Eve" }),
        )
        .await;
    assert_eq!(wrong.status(), 401);
    let gm = h.join_gm(&created).await;
    assert_eq!((gm.role, gm.user_id), (Role::Gm, UserId(v7(1))));
    let mira = h.join_player(&created, "Mira").await;
    assert_eq!(mira.role, Role::Player);

    let mut gm_ws = h.connect(created.session_id, &gm.token).await;
    let mut mira_ws = h.connect(created.session_id, &mira.token).await;
    let full = gm_ws.snapshot().await;
    let seen = mira_ws.snapshot().await;
    assert_eq!(full.tokens.len(), 3);
    assert_eq!(
        ids(&seen.tokens, |t| t.id.0),
        [v7(PLAYER_TOKEN)],
        "no GM-layer token, nothing of the hidden map"
    );
    assert_eq!(ids(&seen.maps, |m| m.id.0), [v7(SHARED_MAP)]);
    assert_eq!(ids(&seen.journal, |e| e.id.0), [v7(SHARED_ENTRY)]);
    assert_eq!(seen.assets.len(), 1);
    assert!(
        seen.users.iter().any(|u| u.name == "Mira"),
        "the joined player is a user"
    );

    // A player's chat message reaches both; the sender also gets `accepted`.
    let sent = mira_ws
        .intent(Intent::SendMessage {
            id: ChatMessageId::new_v7(),
            body: "hello".into(),
        })
        .await;
    let mut accepted = None;
    let mut mira_batch = None;
    for _ in 0..2 {
        match mira_ws.next().await {
            ServerMessage::Accepted(a) => accepted = Some(a),
            ServerMessage::Events(b) => mira_batch = Some(b),
            other => panic!("unexpected {other:?}"),
        }
    }
    let accepted = accepted.unwrap();
    assert_eq!(accepted.intent_id, sent);
    let gm_batch = gm_ws.events().await;
    assert_eq!(gm_batch.caused_by, Some(sent));
    assert_eq!(mira_batch.unwrap(), gm_batch);
    assert_eq!(gm_batch.last_seq(), accepted.seq);

    // The GM drags the GM-layer token: the player gets a placeholder with the same number.
    gm_ws
        .intent(Intent::MoveToken {
            id: TokenId(v7(GM_TOKEN)),
            x: 160,
            y: 160,
        })
        .await;
    let mut gm_moved = None;
    while gm_moved.is_none() {
        if let ServerMessage::Events(b) = gm_ws.next().await {
            gm_moved = Some(b);
        }
    }
    let gm_moved = gm_moved.unwrap();
    let hidden = mira_ws.events().await;
    assert_eq!(hidden.seq, gm_moved.seq);
    assert_eq!(hidden.events, [Event::Redacted]);

    // Revealing the hidden map: the player is sent a new snapshot with it.
    gm_ws
        .send(ClientMessage::Intent(IntentEnvelope {
            intent_id: IntentId::new_v7(),
            expected_version: Some(Version(1)),
            intent: Intent::UpdateMap {
                id: MapId(v7(HIDDEN_MAP)),
                name: None,
                grid: None,
                visible_to_players: Some(true),
            },
        }))
        .await;
    let revealed = mira_ws.snapshot().await;
    assert_eq!(
        ids(&revealed.maps, |m| m.id.0),
        [v7(SHARED_MAP), v7(HIDDEN_MAP)]
    );
    assert!(revealed.tokens.iter().any(|t| t.id.0 == v7(HIDDEN_TOKEN)));

    // Journal texts: only what the reader may see.
    for (id, expected) in [(GM_ENTRY, false), (SHARED_ENTRY, true)] {
        mira_ws
            .send(ClientMessage::GetJournalEntry {
                id: JournalEntryId(v7(id)),
            })
            .await;
        loop {
            if let ServerMessage::JournalEntry { entry, .. } = mira_ws.next().await {
                assert_eq!(entry.is_some(), expected);
                break;
            }
        }
    }
    gm_ws
        .send(ClientMessage::GetJournalEntry {
            id: JournalEntryId(v7(GM_ENTRY)),
        })
        .await;
    loop {
        if let ServerMessage::JournalEntry { entry, .. } = gm_ws.next().await {
            assert_eq!(entry.unwrap().markdown, format!("text {GM_ENTRY}"));
            break;
        }
    }

    // A forbidden intent is rejected for the sender only.
    let denied = mira_ws
        .intent(Intent::DeleteMap {
            id: MapId(v7(SHARED_MAP)),
        })
        .await;
    loop {
        if let ServerMessage::Rejected(r) = mira_ws.next().await {
            assert_eq!(r.intent_id, denied);
            break;
        }
    }
    h.server.stop().await.unwrap();
}

#[tokio::test]
async fn assets_are_served_to_those_who_may_see_them() {
    let h = Harness::default().await;
    let created = h.create().await;
    let gm = h.join_gm(&created).await;
    let mira = h.join_player(&created, "Mira").await;
    let get = |token: Option<String>, image: &[u8]| {
        let mut request = h.http.get(h.url(&format!(
            "/api/sessions/{}/assets/{}",
            created.session_id,
            sha(image)
        )));
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        request.send()
    };
    let shared = get(Some(mira.token.clone()), SHARED_IMAGE).await.unwrap();
    assert_eq!(shared.status(), 200);
    assert_eq!(shared.headers()["content-type"], "image/webp");
    assert_eq!(shared.bytes().await.unwrap(), SHARED_IMAGE);
    assert_eq!(
        get(Some(mira.token.clone()), HIDDEN_IMAGE)
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        get(Some(gm.token.clone()), HIDDEN_IMAGE)
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(get(None, SHARED_IMAGE).await.unwrap().status(), 401);
    let bad = h
        .http
        .get(h.url(&format!(
            "/api/sessions/{}/assets/..%2F..%2Fserver.sqlite",
            created.session_id
        )))
        .bearer_auth(&gm.token)
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 404);
    h.server.stop().await.unwrap();
}

#[tokio::test]
async fn asset_upload_checks_gm_hash_format_and_registration_visibility() {
    let h = Harness::default().await;
    let created = h.create().await;
    let gm = h.join_gm(&created).await;
    let player = h.join_player(&created, "Mira").await;
    let bytes = b"OggS offline ambience";
    let hash = sha(bytes);
    let upload = |token: String, hash: String, kind: &str| {
        h.http
            .post(h.url(&format!("/api/sessions/{}/assets", created.session_id)))
            .bearer_auth(token)
            .header("x-asset-sha256", hash)
            .header("x-asset-kind", kind)
            .body(bytes.to_vec())
            .send()
    };
    assert_eq!(
        upload(player.token.clone(), hash.clone(), "audio")
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        upload(gm.token.clone(), "0".repeat(64), "audio")
            .await
            .unwrap()
            .status(),
        400
    );
    assert_eq!(
        upload(gm.token.clone(), hash.clone(), "../escape")
            .await
            .unwrap()
            .status(),
        400
    );
    assert_eq!(
        upload(gm.token.clone(), hash.clone(), "webp")
            .await
            .unwrap()
            .status(),
        400
    );
    for _ in 0..2 {
        assert_eq!(
            upload(gm.token.clone(), hash.clone(), "audio")
                .await
                .unwrap()
                .status(),
            204
        );
    }
    let url = h.url(&format!(
        "/api/sessions/{}/assets/{hash}",
        created.session_id
    ));
    assert_eq!(
        h.http
            .get(&url)
            .bearer_auth(&player.token)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let mut connection = h.connect(created.session_id, &gm.token).await;
    connection.snapshot().await;
    let intent_id = connection
        .intent(Intent::RegisterAsset {
            asset: Box::new(AssetDto {
                id: AssetId::new_v7(),
                usage: AssetUsage::Audio,
                sha256: hash.clone(),
                mime: "audio/ogg".into(),
                size_bytes: i64::try_from(bytes.len()).unwrap(),
                original_name: Some("Ambience.ogg".into()),
                width: 0,
                height: 0,
                display_sha256: Some(hash.clone()),
                thumbnail_sha256: hash.clone(),
                tiles: vec![],
                folder_id: None,
                version: Version(1),
            }),
        })
        .await;
    loop {
        if let ServerMessage::Accepted(accepted) = connection.next().await
            && accepted.intent_id == intent_id
        {
            break;
        }
    }
    let fetched = h
        .http
        .get(&url)
        .bearer_auth(&gm.token)
        .send()
        .await
        .unwrap();
    assert_eq!(fetched.status(), 200);
    assert_eq!(fetched.bytes().await.unwrap().as_ref(), bytes);
    h.server.stop().await.unwrap();
}

#[tokio::test]
async fn tokens_reconnect_as_the_same_user_and_only_to_their_session() {
    let h = Harness::default().await;
    let (a, b) = (h.create().await, h.create().await);
    let mira = h.join_player(&a, "Mira").await;
    let first = h.connect(a.session_id, &mira.token).await;
    drop(first);
    let mut again = h.connect(a.session_id, &mira.token).await;
    let snapshot = again.snapshot().await;
    assert_eq!(
        snapshot.users.iter().filter(|u| u.name == "Mira").count(),
        1
    );
    assert!(snapshot.users.iter().any(|u| u.id == mira.user_id));

    match h.try_connect(b.session_id, &mira.token).await {
        Err(tungstenite::Error::Http(response)) => assert_eq!(response.status(), 401),
        other => panic!("expected 401, got {:?}", other.map(|_| ())),
    }
    match h.try_connect(a.session_id, "not-a-token").await {
        Err(tungstenite::Error::Http(response)) => assert_eq!(response.status(), 401),
        other => panic!("expected 401, got {:?}", other.map(|_| ())),
    }
    h.server.stop().await.unwrap();
}

#[tokio::test]
async fn the_gm_stops_and_exports_the_session() {
    let h = Harness::default().await;
    let created = h.create().await;
    let gm = h.join_gm(&created).await;
    let mira = h.join_player(&created, "Mira").await;
    let mut mira_ws = h.connect(created.session_id, &mira.token).await;
    mira_ws.snapshot().await;
    mira_ws
        .intent(Intent::SendMessage {
            id: ChatMessageId::new_v7(),
            body: "remember me".into(),
        })
        .await;
    mira_ws.next().await;
    mira_ws.next().await;

    let stop = |token: String| {
        h.http
            .post(h.url(&format!("/api/sessions/{}/stop", created.session_id)))
            .bearer_auth(token)
            .send()
    };
    assert_eq!(stop(mira.token.clone()).await.unwrap().status(), 403);
    assert_eq!(stop(gm.token.clone()).await.unwrap().status(), 204);
    assert_eq!(mira_ws.closed().await, "session_closed");

    let late = h
        .join_raw(
            created.session_id,
            serde_json::json!({ "access_code": created.access_code, "name": "Late" }),
        )
        .await;
    assert_eq!(late.status(), 410);

    let export = h
        .http
        .get(h.url(&format!("/api/sessions/{}/export", created.session_id)))
        .bearer_auth(&gm.token)
        .send()
        .await
        .unwrap();
    assert_eq!(export.status(), 200);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("e.zip"), export.bytes().await.unwrap()).unwrap();
    let (manifest, _) = read_archive(
        &dir.path().join("e.zip"),
        &dir.path().join("out"),
        ArchiveLimits::DEFAULT,
    )
    .unwrap();
    assert_eq!(manifest.name, "Lost Mine");
    let (db, _) = CampaignDb::open(&dir.path().join("out/campaign.sqlite"), 0)
        .await
        .unwrap();
    let chat = db.chat_history(None, 10).await.unwrap();
    assert!(chat.iter().any(|m| m.body == "remember me"));
    let (state, _) = db.load_state().await.unwrap();
    assert!(state.users.values().any(|u| u.name == "Mira"));
    db.close().await.unwrap();
    assert!(
        dir.path()
            .join(format!("out/assets/{}.webp", sha(SHARED_IMAGE)))
            .exists()
    );

    let by_player = h
        .http
        .get(h.url(&format!("/api/sessions/{}/export", created.session_id)))
        .bearer_auth(&mira.token)
        .send()
        .await
        .unwrap();
    assert_eq!(by_player.status(), 403);
    h.server.stop().await.unwrap();
}

#[tokio::test]
async fn limits_and_bad_input_are_refused() {
    let h = Harness::start(Limits {
        max_sessions: 1,
        max_participants: 2,
        messages_per_minute: 5,
        ..relaxed()
    })
    .await;

    let garbage = h.upload(HOST_TOKEN, b"not a zip".to_vec()).await;
    assert_eq!(garbage.status(), 400);
    let error: serde_json::Value = garbage.json().await.unwrap();
    assert_eq!(error["code"], "invalid_archive");

    let created = h.create().await;
    let second = h.upload(HOST_TOKEN, h.archive.clone()).await;
    assert_eq!(second.status(), 503);

    let mira = h.join_player(&created, "Mira").await;
    h.join_player(&created, "Bran").await;
    let third = h
        .join_raw(
            created.session_id,
            serde_json::json!({ "access_code": created.access_code, "name": "Cora" }),
        )
        .await;
    assert_eq!(third.status(), 409);

    // Too many messages in a minute.
    let mut ws = h.connect(created.session_id, &mira.token).await;
    ws.snapshot().await;
    for _ in 0..6 {
        ws.send(ClientMessage::Ping).await;
    }
    assert_eq!(ws.closed().await, "rate_limited");

    // Not JSON.
    let mut ws = h.connect(created.session_id, &mira.token).await;
    ws.snapshot().await;
    ws.send_raw("hello".into()).await;
    assert_eq!(ws.closed().await, "invalid_message");

    // An older client: told why, then closed.
    let mut ws = h.connect(created.session_id, &mira.token).await;
    ws.snapshot().await;
    let mut old = Envelope::new(
        MessageId::new_v7(),
        0,
        ClientMessage::Intent(gm_intent(Intent::SendMessage {
            id: ChatMessageId::new_v7(),
            body: "hi".into(),
        })),
    );
    old.v = PROTOCOL_VERSION - 1;
    ws.send_raw(serde_json::to_string(&old).unwrap()).await;
    match ws.next().await {
        ServerMessage::Rejected(r) => {
            assert!(matches!(r.reason, RejectReason::ProtocolMismatch { .. }));
        }
        other => panic!("expected a rejection, got {other:?}"),
    }
    assert_eq!(ws.closed().await, "protocol_mismatch");
    h.server.stop().await.unwrap();
}

#[tokio::test]
async fn idle_sessions_expire() {
    let h = Harness::start(Limits {
        session_ttl: Duration::from_millis(1),
        ..relaxed()
    })
    .await;
    let created = h.create().await;
    let mira = h.join_player(&created, "Mira").await;
    let mut ws = h.connect(created.session_id, &mira.token).await;
    ws.snapshot().await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    // Someone is connected: the session stays.
    h.server.app.sweep().await.unwrap();
    ws.send(ClientMessage::Ping).await;
    assert!(matches!(ws.next().await, ServerMessage::Pong));
    drop(ws);
    tokio::time::sleep(Duration::from_millis(100)).await;
    h.server.app.sweep().await.unwrap();
    let info: serde_json::Value = h
        .http
        .get(&created.invite_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(info["state"], "expired");
    assert_eq!(
        h.try_connect(created.session_id, &mira.token)
            .await
            .err()
            .map(|e| match e {
                tungstenite::Error::Http(r) => r.status().as_u16(),
                _ => 0,
            }),
        Some(410)
    );
    h.server.stop().await.unwrap();
}

#[tokio::test]
async fn silent_connections_are_closed() {
    let h = Harness::start(Limits {
        idle_timeout: Duration::from_millis(300),
        ..relaxed()
    })
    .await;
    let created = h.create().await;
    let mira = h.join_player(&created, "Mira").await;
    let mut ws = h.connect(created.session_id, &mira.token).await;
    ws.snapshot().await;
    // Pings keep it open past the timeout…
    for _ in 0..4 {
        tokio::time::sleep(Duration::from_millis(150)).await;
        ws.send(ClientMessage::Ping).await;
        assert!(matches!(ws.next().await, ServerMessage::Pong));
    }
    // …silence does not.
    assert_eq!(ws.closed().await, "timeout");
    h.server.stop().await.unwrap();
}

#[tokio::test]
async fn the_root_says_the_server_works() {
    let h = Harness::default().await;
    let page = h.http.get(h.url("/")).send().await.unwrap();
    assert_eq!(page.status(), 200);
    assert!(
        page.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    let text = page.text().await.unwrap();
    assert!(text.contains("Сервер сессий работает"));
    assert!(text.contains(env!("CARGO_PKG_VERSION")));
    assert!(!text.contains("{version}"));
    h.server.stop().await.unwrap();
}

/// The next message `pick` accepts; others are skipped.
async fn until<T>(ws: &mut Ws, mut pick: impl FnMut(ServerMessage) -> Option<T>) -> T {
    loop {
        if let Some(found) = pick(ws.next().await) {
            return found;
        }
    }
}

fn secret_trigger(zone: ZoneId, node: BoardNodeId, condition: &str) -> TriggerDef {
    TriggerDef {
        name: "Secret passage".into(),
        enabled: true,
        once: true,
        event: TriggerEvent::RollInZone {
            zone,
            rolls: vec!["perception".into()],
        },
        tokens: Vec::new(),
        condition: Some(condition.into()),
        steps: vec![TriggerStep {
            when: None,
            action: TriggerAction::Reveal {
                target: RevealTarget::Node { id: node },
            },
        }],
    }
}

/// The scenario of ADR-0024 through the server: a perception check in a zone opens the secret
/// passage card for everyone, only on a success; players never see the zone or the trigger.
#[tokio::test]
async fn a_trigger_reveals_a_secret_through_the_server() {
    let h = Harness::default().await;
    let created = h.create().await;
    let gm = h.join_gm(&created).await;
    let mira = h.join_player(&created, "Mira").await;
    let mut gm_ws = h.connect(created.session_id, &gm.token).await;
    let mut mira_ws = h.connect(created.session_id, &mira.token).await;
    gm_ws.snapshot().await;
    mira_ws.snapshot().await;

    let srd = {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/system-srd5");
        let bytes = dnd_plugin::pack_directory(&path).unwrap();
        dnd_plugin::read_package(&bytes, dnd_plugin::PackageLimits::default())
            .unwrap()
            .to_dto()
    };
    let (hero, token, zone, board, node, trigger) = (
        CharacterId(v7(0x60)),
        TokenId(v7(0x61)),
        ZoneId(v7(0x62)),
        BoardId(v7(0x63)),
        BoardNodeId(v7(0x64)),
        TriggerId(v7(0x65)),
    );
    let setup = [
        (
            Intent::InstallPlugin {
                plugin: Box::new(srd),
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
            Intent::CreateCharacter {
                id: hero,
                character_kind: CharacterKind::Pc,
                name: "Mira".into(),
                owner: Some(mira.user_id),
                sheet_json: "{}".into(),
            },
            None,
        ),
        (
            Intent::CreateToken {
                id: token,
                map_id: MapId(v7(SHARED_MAP)),
                character_id: Some(hero),
                asset_id: None,
                name: "Mira".into(),
                x: 320,
                y: 320,
                rotation: 0.0,
                scale: 1.0,
                layer: TokenLayer::Tokens,
                visible: true,
            },
            None,
        ),
        (
            Intent::CreateZone {
                id: zone,
                map_id: MapId(v7(SHARED_MAP)),
                name: "Altar".into(),
                shape: ZoneShape::Rect {
                    x: 0,
                    y: 0,
                    width: 1600,
                    height: 1600,
                },
            },
            None,
        ),
        (
            Intent::CreateBoard {
                id: board,
                name: "Plot".into(),
            },
            None,
        ),
        (
            Intent::UpdateBoard {
                id: board,
                name: None,
                visible_to_players: Some(true),
            },
            Some(Version(1)),
        ),
        (
            Intent::CreateBoardNode {
                id: node,
                board_id: board,
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
        (
            Intent::CreateTrigger {
                id: trigger,
                // Impossible first; made easy below.
                definition: secret_trigger(zone, node, "@total >= 100"),
            },
            None,
        ),
    ];
    for (intent, expected_version) in setup {
        let intent_id = IntentId::new_v7();
        gm_ws
            .send(ClientMessage::Intent(IntentEnvelope {
                intent_id,
                expected_version,
                intent: intent.clone(),
            }))
            .await;
        until(&mut gm_ws, |m| match m {
            ServerMessage::Accepted(a) if a.intent_id == intent_id => Some(()),
            ServerMessage::Rejected(r) if r.intent_id == intent_id => {
                panic!("{intent:?} rejected: {:?}", r.reason)
            }
            _ => None,
        })
        .await;
    }

    let roll = |n: u128| {
        ClientMessage::Intent(IntentEnvelope {
            intent_id: IntentId(v7(0x70 + n)),
            expected_version: None,
            intent: Intent::RollDice {
                id: ChatMessageId(v7(0x80 + n)),
                expression: "1d20 + @perception".into(),
                hidden: false,
                character_id: Some(hero),
                roll: Some("perception".into()),
            },
        })
    };

    // A failed check: the roll, and nothing else.
    mira_ws.send(roll(0)).await;
    let batch = until(&mut gm_ws, |m| match m {
        ServerMessage::Events(b) if b.caused_by == Some(IntentId(v7(0x70))) => Some(b),
        _ => None,
    })
    .await;
    assert_eq!(batch.events.len(), 1, "{batch:?}");

    // Easier now: the next check opens the card.
    let update = IntentId::new_v7();
    gm_ws
        .send(ClientMessage::Intent(IntentEnvelope {
            intent_id: update,
            expected_version: Some(Version(1)),
            intent: Intent::UpdateTrigger {
                id: trigger,
                definition: secret_trigger(zone, node, "@total >= 1"),
            },
        }))
        .await;
    until(&mut gm_ws, |m| match m {
        ServerMessage::Accepted(a) if a.intent_id == update => Some(()),
        _ => None,
    })
    .await;
    mira_ws.send(roll(1)).await;
    let batch = until(&mut gm_ws, |m| match m {
        ServerMessage::Events(b) if b.caused_by == Some(IntentId(v7(0x71))) => Some(b),
        _ => None,
    })
    .await;
    assert!(
        batch
            .events
            .iter()
            .any(|e| matches!(e, Event::TriggerFired { .. })),
        "{batch:?}"
    );

    // The player is sent a fresh view: the card is open, no zone or trigger in sight.
    let view = until(&mut mira_ws, |m| match m {
        ServerMessage::Snapshot(s) if s.board_nodes.iter().any(|n| n.id == node) => Some(*s),
        _ => None,
    })
    .await;
    assert_eq!(
        view.board_nodes
            .iter()
            .find(|n| n.id == node)
            .map(|n| n.visibility),
        Some(BoardVisibility::Players)
    );
    assert!(view.zones.is_empty() && view.triggers.is_empty());
    h.server.stop().await.unwrap();
}
