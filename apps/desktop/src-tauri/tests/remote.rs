//! Playing through a session server from the desktop side (ADR-0022), against a real server in the
//! same process. Secrets go to memory instead of the OS keychain.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::time::Duration;

use dnd_desktop_lib::LocalServer;
use dnd_desktop_lib::remote::{
    self, MemorySecrets, RemoteBook, RemoteEvent, RemoteSession, RemoteStatus, Secrets,
};
use dnd_net::ConnectOptions;
use dnd_protocol::*;
use dnd_session_server::{Config, Limits, Server};
use tokio::sync::mpsc;

const HOST_TOKEN: &str = "host-token-for-tests-0123";

async fn server(dir: &std::path::Path) -> Server {
    Server::start(Config {
        data_dir: dir.join("server"),
        bind: "127.0.0.1:0".parse().unwrap(),
        host_token: HOST_TOKEN.into(),
        public_url: None,
        limits: Limits {
            connections_per_minute: 10_000,
            ..Limits::default()
        },
    })
    .await
    .unwrap()
}

fn connect(
    entry: remote::RemoteEntry,
    token: String,
) -> (RemoteSession, mpsc::UnboundedReceiver<RemoteEvent>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let session = RemoteSession::connect(entry, token, ConnectOptions::default(), move |e| {
        let _ = tx.send(e);
    })
    .unwrap();
    (session, rx)
}

async fn next_snapshot(rx: &mut mpsc::UnboundedReceiver<RemoteEvent>) -> Snapshot {
    loop {
        match tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap()
        {
            RemoteEvent::Snapshot(s) => return *s,
            _ => continue,
        }
    }
}

async fn next_events(rx: &mut mpsc::UnboundedReceiver<RemoteEvent>) -> EventBatch {
    loop {
        match tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap()
        {
            RemoteEvent::Events(b) => return b,
            _ => continue,
        }
    }
}

#[tokio::test]
async fn host_join_and_play_through_the_server() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path()).await;
    let base = format!("http://{}", server.addr);

    let mut local = LocalServer::create(dir.path(), "Lost Mine").await.unwrap();
    let map = MapId::new_v7();
    local
        .submit(&IntentEnvelope {
            intent_id: IntentId::new_v7(),
            expected_version: None,
            intent: Intent::CreateMap {
                id: map,
                name: "Cave".into(),
                background_asset_id: None,
                width: 100,
                height: 100,
                grid: Grid::default(),
                visible_to_players: true,
            },
        })
        .await
        .unwrap();

    let gm_secrets = MemorySecrets::default();
    let gm_book = RemoteBook::new(dir.path().join("gm/remote_sessions.json"));
    // No saved host token yet: an empty one is refused.
    let missing = remote::host(&local, &base, "", &gm_secrets, &gm_book).await;
    assert_eq!(missing.unwrap_err().code, "no_host_token");
    let hosted = remote::host(&local, &base, HOST_TOKEN, &gm_secrets, &gm_book)
        .await
        .unwrap();
    assert_eq!(hosted.name, "Lost Mine");
    // The token is remembered for the next upload.
    let again = remote::host(&local, &base, "", &gm_secrets, &gm_book)
        .await
        .unwrap();
    assert_ne!(again.session_id, hosted.session_id);

    let saved = remote::sessions(&gm_secrets, &gm_book);
    assert_eq!(saved.len(), 2);
    let entry = saved
        .iter()
        .find(|e| e.session_id == hosted.session_id)
        .unwrap();
    assert!(entry.hosted);
    assert_eq!(
        entry.access_code.as_deref(),
        Some(hosted.access_code.as_str())
    );
    // The list on disk holds no secrets.
    let on_disk = std::fs::read_to_string(dir.path().join("gm/remote_sessions.json")).unwrap();
    assert!(!on_disk.contains(&hosted.access_code));

    // The GM connects with the saved GM key, the player with the invite and the code.
    let (gm_entry, gm_token) = remote::token_for(&base, hosted.session_id, &gm_secrets, &gm_book)
        .await
        .unwrap();
    assert_eq!(gm_entry.role, Some(Role::Gm));
    assert_eq!(gm_entry.user_id, Some(local.info().user_id));
    let (gm, mut gm_rx) = connect(gm_entry, gm_token);
    assert_eq!(gm.campaign_info().role, Role::Gm);
    let gm_view = next_snapshot(&mut gm_rx).await;
    assert_eq!(gm_view.maps.len(), 1);

    let player_secrets = MemorySecrets::default();
    let player_book = RemoteBook::new(dir.path().join("player/remote_sessions.json"));
    let (player_entry, player_token) = remote::join(
        &hosted.invite_url,
        &hosted.access_code,
        "Mira",
        &player_secrets,
        &player_book,
    )
    .await
    .unwrap();
    assert_eq!(player_entry.role, Some(Role::Player));
    let (player, mut player_rx) = connect(player_entry.clone(), player_token);
    next_snapshot(&mut player_rx).await;
    assert_eq!(player.snapshot().unwrap().maps.len(), 1);
    assert!(matches!(player.status(), RemoteStatus::Online));

    // A chat message from the player: accepted, and both views follow.
    let outcome = player
        .submit(IntentEnvelope {
            intent_id: IntentId::new_v7(),
            expected_version: None,
            intent: Intent::SendMessage {
                id: ChatMessageId::new_v7(),
                body: "hello".into(),
            },
        })
        .await
        .unwrap();
    assert!(matches!(outcome, IntentOutcome::Accepted { .. }));
    // (The GM first sees Mira join.)
    let chat = |b: &EventBatch| {
        b.events
            .iter()
            .any(|e| matches!(e, Event::ChatMessagePosted { .. }))
    };
    let mut batch = next_events(&mut gm_rx).await;
    while !chat(&batch) {
        batch = next_events(&mut gm_rx).await;
    }
    while !chat(&next_events(&mut player_rx).await) {}
    let gm_now = gm.snapshot().unwrap();
    assert_eq!(gm_now.seq, batch.last_seq());
    assert!(gm_now.chat.iter().any(|m| m.body == "hello"));
    assert!(gm_now.users.iter().any(|u| u.name == "Mira"));
    assert_eq!(player.snapshot().unwrap().seq, batch.last_seq());

    // Reconnecting later uses the saved participant token.
    let (entry_again, token_again) =
        remote::token_for(&base, hosted.session_id, &player_secrets, &player_book)
            .await
            .unwrap();
    assert_eq!(entry_again.user_id, player_entry.user_id);
    assert!(!token_again.is_empty());

    // Only the GM stops the session.
    assert_eq!(player.stop().await.unwrap_err().code, "forbidden");
    gm.stop().await.unwrap();

    remote::forget(&base, hosted.session_id, &gm_secrets, &gm_book).unwrap();
    assert_eq!(remote::sessions(&gm_secrets, &gm_book).len(), 1);
    assert!(
        gm_secrets
            .get(&format!("gm:{base}/{}", hosted.session_id))
            .is_none()
    );

    local.close().await.unwrap();
    server.stop().await.unwrap();
}
