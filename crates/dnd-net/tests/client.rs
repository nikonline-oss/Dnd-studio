//! `dnd-net` against a real session server in the same process.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use dnd_db::{
    ARCHIVE_FORMAT, ArchiveManifest, CampaignDb, CampaignEngine, LocalUser, write_archive,
};
use dnd_game_core::{Actor, IntentWindow};
use dnd_net::{
    ConnectOptions, Connection, HttpClient, Incoming, NetError, Status, normalize_base,
    parse_invite,
};
use dnd_protocol::arbitrary::v7;
use dnd_protocol::*;
use dnd_session_server::{Config, Limits, Server};
use tokio::sync::mpsc;

const HOST_TOKEN: &str = "host-token-for-tests-0123";
const IMAGE: &[u8] = b"map image";

fn sha(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn envelope(intent: Intent, expected_version: Option<Version>) -> IntentEnvelope {
    IntentEnvelope {
        intent_id: IntentId::new_v7(),
        expected_version,
        intent,
    }
}

fn chat(body: &str) -> IntentEnvelope {
    envelope(
        Intent::SendMessage {
            id: ChatMessageId::new_v7(),
            body: body.into(),
        },
        None,
    )
}

/// A campaign with a shared map (its image as an asset) and a shared journal entry.
async fn archive(dir: &Path) -> PathBuf {
    let folder = dir.join("campaign");
    std::fs::create_dir_all(folder.join("assets")).unwrap();
    std::fs::write(folder.join(format!("assets/{}.webp", sha(IMAGE))), IMAGE).unwrap();
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
    let asset = AssetDto {
        id: AssetId(v7(0x40)),
        usage: AssetUsage::Map,
        sha256: sha(b"original"),
        mime: "image/png".into(),
        size_bytes: 1,
        original_name: None,
        width: 10,
        height: 10,
        display_sha256: None,
        thumbnail_sha256: sha(IMAGE),
        tiles: Vec::new(),
        version: Version::INITIAL,
    };
    engine
        .commit_events(
            vec![Event::AssetRegistered {
                asset: Box::new(asset.clone()),
            }],
            Some(gm.id),
            0,
        )
        .await
        .unwrap();
    let actor = Actor {
        user_id: gm.id,
        role: Role::Gm,
    };
    for intent in [
        Intent::CreateMap {
            id: MapId(v7(0x10)),
            name: "Cave".into(),
            background_asset_id: Some(asset.id),
            width: 100,
            height: 100,
            grid: Grid::default(),
            visible_to_players: true,
        },
        Intent::CreateJournalEntry {
            id: JournalEntryId(v7(0x30)),
            folder_id: None,
            title: "Gods".into(),
            markdown: "# Pantheon".into(),
            visibility: JournalVisibility::Players,
            players_can_edit: false,
            table: None,
        },
    ] {
        engine
            .submit(&envelope(intent, None), &actor, 0)
            .await
            .unwrap();
    }
    let copy = dir.join("copy.sqlite");
    engine.db().copy_to(&copy).await.unwrap();
    engine.close().await.unwrap();
    let out = dir.join("campaign.zip");
    write_archive(
        &out,
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
    out
}

async fn server(data: &Path, bind: SocketAddr) -> Server {
    Server::start(Config {
        data_dir: data.to_owned(),
        bind,
        host_token: HOST_TOKEN.into(),
        public_url: None,
        limits: Limits {
            connections_per_minute: 10_000,
            sweep_interval: Duration::from_secs(3600),
            ..Limits::default()
        },
    })
    .await
    .unwrap()
}

fn fast() -> ConnectOptions {
    ConnectOptions {
        backoff_initial: Duration::from_millis(50),
        backoff_max: Duration::from_millis(200),
        ..ConnectOptions::default()
    }
}

/// The next incoming item that `pick` accepts (others are skipped).
async fn next<T>(
    rx: &mut mpsc::UnboundedReceiver<Incoming>,
    pick: impl Fn(Incoming) -> Option<T>,
) -> T {
    loop {
        let item = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("timed out")
            .expect("connection gone");
        if let Some(found) = pick(item) {
            return found;
        }
    }
}

fn snapshot(item: Incoming) -> Option<Snapshot> {
    match item {
        Incoming::Snapshot(s) => Some(*s),
        _ => None,
    }
}

fn events(item: Incoming) -> Option<EventBatch> {
    match item {
        Incoming::Events(b) => Some(b),
        _ => None,
    }
}

fn status(wanted: fn(&Status) -> bool) -> impl Fn(Incoming) -> Option<Status> {
    move |item| match item {
        Incoming::Status(s) if wanted(&s) => Some(s),
        _ => None,
    }
}

#[test]
fn addresses_and_invites_are_understood() {
    assert_eq!(
        normalize_base("dnd.example.org:8080/").unwrap(),
        "http://dnd.example.org:8080"
    );
    assert_eq!(
        normalize_base("https://dnd.example.org").unwrap(),
        "https://dnd.example.org"
    );
    assert!(normalize_base("  ").is_err());
    let id = SessionId(v7(5));
    assert_eq!(
        parse_invite(&format!(" https://dnd.example.org/api/sessions/{id}/ ")).unwrap(),
        ("https://dnd.example.org".to_owned(), id)
    );
    assert!(parse_invite("https://dnd.example.org/api/sessions/nope").is_err());
    assert!(parse_invite("hello").is_err());
}

#[tokio::test]
async fn http_api_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let archive = archive(dir.path()).await;
    let server = server(&dir.path().join("data"), "127.0.0.1:0".parse().unwrap()).await;
    let http = HttpClient::new(&server.addr.to_string()).unwrap();

    let wrong = http.create_session("wrong-host-token-xxxx", &archive).await;
    assert!(matches!(wrong, Err(NetError::Http { status: 401, .. })));
    let created = http.create_session(HOST_TOKEN, &archive).await.unwrap();
    let (base, id) = parse_invite(&created.invite_url).unwrap();
    assert_eq!((base.as_str(), id), (http.base(), created.session_id));
    let info = http.session_info(id).await.unwrap();
    assert_eq!(
        (info.name.as_str(), info.state.as_str()),
        ("Lost Mine", "active")
    );

    let denied = http
        .join_player(id, "WRONG-CODE0", "Eve")
        .await
        .unwrap_err();
    assert_eq!(denied.code(), "unauthorized");
    let gm = http.join_gm(id, &created.gm_key).await.unwrap();
    let mira = http
        .join_player(id, &created.access_code, "Mira")
        .await
        .unwrap();
    assert_eq!((gm.role, mira.role), (Role::Gm, Role::Player));

    assert_eq!(
        http.asset(id, &mira.token, &sha(IMAGE)).await.unwrap(),
        IMAGE
    );
    assert!(http.asset(id, &mira.token, &sha(b"other")).await.is_err());

    assert_eq!(
        http.stop(id, &mira.token).await.unwrap_err().code(),
        "forbidden"
    );
    http.stop(id, &gm.token).await.unwrap();
    let out = dir.path().join("export.zip");
    http.export(id, &gm.token, &out).await.unwrap();
    assert!(std::fs::metadata(&out).unwrap().len() > 0);
    server.stop().await.unwrap();
}

#[tokio::test]
async fn a_connection_syncs_and_answers_intents() {
    let dir = tempfile::tempdir().unwrap();
    let archive = archive(dir.path()).await;
    let server = server(&dir.path().join("data"), "127.0.0.1:0".parse().unwrap()).await;
    let http = HttpClient::new(&server.addr.to_string()).unwrap();
    let created = http.create_session(HOST_TOKEN, &archive).await.unwrap();
    let gm = http
        .join_gm(created.session_id, &created.gm_key)
        .await
        .unwrap();
    let mira = http
        .join_player(created.session_id, &created.access_code, "Mira")
        .await
        .unwrap();

    let (gm_tx, mut gm_rx) = mpsc::unbounded_channel();
    let (mira_tx, mut mira_rx) = mpsc::unbounded_channel();
    let gm_conn =
        Connection::open(http.base(), created.session_id, &gm.token, fast(), gm_tx).unwrap();
    let mira_conn = Connection::open(
        http.base(),
        created.session_id,
        &mira.token,
        fast(),
        mira_tx,
    )
    .unwrap();
    let gm_view = next(&mut gm_rx, snapshot).await;
    let mira_view = next(&mut mira_rx, snapshot).await;
    assert_eq!(gm_view.maps.len(), 1);
    assert_eq!(mira_view.journal.len(), 1);
    assert_eq!(mira_conn.status(), Status::Online);

    let outcome = mira_conn.submit(chat("hello")).await.unwrap();
    let IntentOutcome::Accepted { seq } = outcome else {
        panic!("expected accepted, got {outcome:?}");
    };
    let batch = next(&mut gm_rx, events).await;
    assert_eq!(batch.last_seq(), seq);

    let rejected = mira_conn
        .submit(envelope(
            Intent::DeleteMap {
                id: MapId(v7(0x10)),
            },
            Some(Version(1)),
        ))
        .await
        .unwrap();
    assert!(matches!(rejected, IntentOutcome::Rejected { .. }));

    let text = mira_conn
        .journal_entry(JournalEntryId(v7(0x30)))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(text.markdown, "# Pantheon");
    assert_eq!(
        mira_conn
            .journal_entry(JournalEntryId(v7(0x99)))
            .await
            .unwrap(),
        None
    );

    mira_conn.resync();
    next(&mut mira_rx, snapshot).await;
    drop(gm_conn);
    server.stop().await.unwrap();
}

#[tokio::test]
async fn a_lost_connection_comes_back_and_sends_the_waiting_chat() {
    let dir = tempfile::tempdir().unwrap();
    let archive = archive(dir.path()).await;
    let data = dir.path().join("data");
    let first = server(&data, "127.0.0.1:0".parse().unwrap()).await;
    let addr = first.addr;
    let http = HttpClient::new(&addr.to_string()).unwrap();
    let created = http.create_session(HOST_TOKEN, &archive).await.unwrap();
    let mira = http
        .join_player(created.session_id, &created.access_code, "Mira")
        .await
        .unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let conn = Connection::open(http.base(), created.session_id, &mira.token, fast(), tx).unwrap();
    next(&mut rx, snapshot).await;

    first.stop().await.unwrap();
    next(
        &mut rx,
        status(|s| matches!(s, Status::Reconnecting { .. })),
    )
    .await;

    // Offline: a change of the map may not wait, a chat message may.
    let map_change = conn
        .submit(envelope(
            Intent::UpdateMap {
                id: MapId(v7(0x10)),
                name: Some("Renamed".into()),
                grid: None,
                visible_to_players: None,
            },
            Some(Version(1)),
        ))
        .await;
    assert_eq!(map_change, Err(NetError::Offline));
    let waiting = tokio::spawn({
        let conn = std::sync::Arc::new(conn);
        let sender = conn.clone();
        async move { (sender.submit(chat("still here")).await, conn) }
    });

    // The same server comes back on the same address with the same data.
    let second = server(&data, addr).await;
    let back = next(&mut rx, snapshot).await;
    assert!(back.users.iter().any(|u| u.id == mira.user_id));
    let (sent, conn) = waiting.await.unwrap();
    assert!(
        matches!(sent, Ok(IntentOutcome::Accepted { .. })),
        "{sent:?}"
    );
    assert_eq!(conn.status(), Status::Online);
    let batch = next(&mut rx, events).await;
    assert!(matches!(
        &batch.events[..],
        [Event::ChatMessagePosted { message }] if message.body == "still here"
    ));
    second.stop().await.unwrap();
}

#[tokio::test]
async fn a_stopped_session_or_a_bad_token_ends_the_connection() {
    let dir = tempfile::tempdir().unwrap();
    let archive = archive(dir.path()).await;
    let server = server(&dir.path().join("data"), "127.0.0.1:0".parse().unwrap()).await;
    let http = HttpClient::new(&server.addr.to_string()).unwrap();
    let created = http.create_session(HOST_TOKEN, &archive).await.unwrap();
    let gm = http
        .join_gm(created.session_id, &created.gm_key)
        .await
        .unwrap();
    let mira = http
        .join_player(created.session_id, &created.access_code, "Mira")
        .await
        .unwrap();

    let (tx, mut rx) = mpsc::unbounded_channel();
    let stranger =
        Connection::open(http.base(), created.session_id, "bad-token", fast(), tx).unwrap();
    let closed = next(&mut rx, status(|s| matches!(s, Status::Closed { .. }))).await;
    assert_eq!(
        closed,
        Status::Closed {
            reason: "unauthorized".into()
        }
    );
    assert_eq!(
        stranger.submit(chat("hi")).await,
        Err(NetError::Closed("unauthorized".into()))
    );

    let (tx, mut rx) = mpsc::unbounded_channel();
    let conn = Connection::open(http.base(), created.session_id, &mira.token, fast(), tx).unwrap();
    next(&mut rx, snapshot).await;
    http.stop(created.session_id, &gm.token).await.unwrap();
    let closed = next(&mut rx, status(|s| matches!(s, Status::Closed { .. }))).await;
    assert_eq!(
        closed,
        Status::Closed {
            reason: "session_closed".into()
        }
    );
    assert_eq!(
        conn.submit(chat("hi")).await,
        Err(NetError::Closed("session_closed".into()))
    );
    server.stop().await.unwrap();
}
