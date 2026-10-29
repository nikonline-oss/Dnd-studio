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
    let (outcome, _) = dnd_desktop_lib::plugins::install(
        &mut local,
        dnd_desktop_lib::bundled::package("srd5").unwrap(),
    )
    .await
    .unwrap();
    assert!(matches!(outcome, IntentOutcome::Accepted { .. }));
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
    let other_local = LocalServer::create(dir.path(), "Other campaign")
        .await
        .unwrap();
    let again = remote::host(&other_local, &base, "", &gm_secrets, &gm_book)
        .await
        .unwrap();
    other_local.close().await.unwrap();
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
    let binding = format!("{base}/{}", hosted.session_id);
    local
        .db()
        .set_remote_binding(Some("https://different/session"))
        .await
        .unwrap();
    assert_eq!(
        gm.sync_pending(local.db()).await.unwrap_err().code,
        "replica_mismatch"
    );
    let before = local.snapshot();
    let mut mismatched = Some(local);
    assert_eq!(
        gm.refresh_replica(&mut mismatched).await.unwrap_err().code,
        "replica_mismatch"
    );
    let mut local = mismatched.take().unwrap();
    assert_eq!(local.snapshot(), before);
    local.db().set_remote_binding(Some(&binding)).await.unwrap();

    let wrong_owner = LocalServer::create(dir.path(), "Wrong replica")
        .await
        .unwrap();
    wrong_owner
        .db()
        .set_remote_binding(Some(&binding))
        .await
        .unwrap();
    assert_eq!(
        gm.sync_pending(wrong_owner.db()).await.unwrap_err().code,
        "replica_mismatch"
    );
    wrong_owner.close().await.unwrap();

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
    let installed =
        dnd_desktop_lib::compendia::remote_installed(&player, &dir.path().join("cache"))
            .await
            .unwrap();
    let spells =
        dnd_desktop_lib::compendia::page(&installed, "srd5", "srd5-spells", "FIRE", 0, 20).unwrap();
    assert_eq!(spells.total, 1);
    assert!(!spells.entries.is_empty());
    assert_eq!(
        player
            .install_plugin(
                dnd_desktop_lib::bundled::package("generic-d6")
                    .unwrap()
                    .to_vec(),
                &dir.path().join("install-cache")
            )
            .await
            .unwrap_err()
            .code,
        "forbidden"
    );
    let installed_live = gm
        .install_plugin(
            dnd_desktop_lib::bundled::package("generic-d6")
                .unwrap()
                .to_vec(),
            &dir.path().join("install-cache"),
        )
        .await
        .unwrap();
    assert!(matches!(installed_live, IntentOutcome::Accepted { .. }));
    let active = dnd_net::HttpClient::new(&base)
        .unwrap()
        .session_info(hosted.session_id)
        .await
        .unwrap();
    assert_eq!(active.state, "active");
    let plugin_sha = player
        .snapshot()
        .unwrap()
        .plugins
        .iter()
        .find(|p| p.id == "srd5")
        .unwrap()
        .archive_sha256
        .clone();
    let package_cache = dir.path().join("cache");
    let cache_file = package_cache.join(format!("{plugin_sha}.dndplugin"));
    let package_bytes = player
        .plugin_package(&plugin_sha, &package_cache)
        .await
        .unwrap();
    std::fs::write(&cache_file, b"broken cache").unwrap();
    assert_eq!(
        player
            .plugin_package(&plugin_sha, &package_cache)
            .await
            .unwrap(),
        package_bytes
    );
    assert_eq!(std::fs::read(&cache_file).unwrap(), package_bytes);
    assert_eq!(
        player
            .plugin_package("../escape", &package_cache)
            .await
            .unwrap_err()
            .code,
        "invalid_asset"
    );
    let server_file = server
        .app
        .session_dir(hosted.session_id)
        .join(format!("assets/plugins/{plugin_sha}.dndplugin"));
    std::fs::write(&server_file, b"bad server response").unwrap();
    std::fs::remove_file(&cache_file).unwrap();
    assert_eq!(
        player
            .plugin_package(&plugin_sha, &package_cache)
            .await
            .unwrap_err()
            .code,
        "protocol_error"
    );
    std::fs::write(server_file, &package_bytes).unwrap();
    assert_eq!(
        player
            .plugin_package(&plugin_sha, &package_cache)
            .await
            .unwrap(),
        package_bytes
    );

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
    let history = player.chat_history(None, 50).await.unwrap();
    assert!(
        history
            .iter()
            .any(|m| m.body == "hello" && m.at_ms.is_some())
    );

    // Reconnecting later uses the saved participant token.
    let (entry_again, token_again) =
        remote::token_for(&base, hosted.session_id, &player_secrets, &player_book)
            .await
            .unwrap();
    assert_eq!(entry_again.user_id, player_entry.user_id);
    assert!(!token_again.is_empty());

    // An offline imported map uploads its derivatives before its registration intent.
    let imported = dnd_desktop_lib::assets_api::read_and_import(
        std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/icons/32x32.png")),
        local.folder().join("assets"),
        dnd_assets::AssetUsage::Map,
    )
    .await
    .unwrap();
    dnd_desktop_lib::assets_api::register_map(
        &mut local,
        &imported,
        "Offline map".into(),
        None,
        |_| {},
    )
    .await
    .unwrap();
    gm.sync_pending(local.db()).await.unwrap();
    let downloaded = gm
        .asset(
            &imported.thumbnail.sha256,
            &dir.path().join("cache"),
            "image/webp",
        )
        .await
        .unwrap();
    assert_eq!(
        dnd_assets::sha256_hex(&downloaded),
        imported.thumbnail.sha256
    );

    // Durable offline edits replay in order and pause at a conflict.
    let (queue, _) = dnd_db::CampaignDb::open(&local.folder().join("campaign.sqlite"), 1234)
        .await
        .unwrap();
    for name in ["Prepared offline", "Conflicting edit", "Dependent edit"] {
        queue
            .enqueue_pending_op(
                &IntentEnvelope {
                    intent_id: IntentId::new_v7(),
                    expected_version: Some(Version(1)),
                    intent: Intent::UpdateMap {
                        id: map,
                        name: Some(name.into()),
                        grid: None,
                        visible_to_players: None,
                    },
                },
                1234,
            )
            .await
            .unwrap();
    }
    assert_eq!(
        player.sync_pending(&queue).await.unwrap_err().code,
        "forbidden"
    );
    gm.sync_pending(&queue).await.unwrap();
    let pending = queue.pending_ops().await.unwrap();
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0].status, "conflict");
    assert_eq!(pending[1].status, "pending");
    gm.sync_pending(&queue).await.unwrap();
    assert_eq!(queue.pending_ops().await.unwrap().len(), 2);
    let offline_note = JournalEntryId::new_v7();
    local
        .submit_offline(&IntentEnvelope {
            intent_id: IntentId::new_v7(),
            expected_version: None,
            intent: Intent::CreateJournalEntry {
                id: offline_note,
                folder_id: None,
                title: "Offline note".into(),
                markdown: "Continue editing me".into(),
                visibility: JournalVisibility::Gm,
                players_can_edit: false,
                table: None,
            },
        })
        .await
        .unwrap();
    queue.close().await.unwrap();

    let mut replica = Some(local);
    gm.refresh_replica(&mut replica).await.unwrap();
    let mut local = replica.take().unwrap();
    assert_eq!(local.snapshot().maps[0].name, "Dependent edit");
    assert_eq!(local.db().pending_ops().await.unwrap().len(), 3);
    assert!(
        local.journal_entry(&offline_note).is_some(),
        "refresh must preserve pending creations in the offline view"
    );
    local
        .submit_offline(&IntentEnvelope {
            intent_id: IntentId::new_v7(),
            expected_version: Some(Version(1)),
            intent: Intent::UpdateJournalEntry {
                id: offline_note,
                title: None,
                markdown: Some("Edited after refresh".into()),
                visibility: None,
                players_can_edit: None,
                table: None,
            },
        })
        .await
        .unwrap();
    let pending = local.db().pending_ops().await.unwrap();
    let mut rebased = pending[0].envelope.clone();
    rebased.intent_id = IntentId::new_v7();
    rebased.expected_version = Some(Version(2));
    local
        .db()
        .rebase_pending_op(pending[0].id, &rebased, 2)
        .await
        .unwrap();
    gm.sync_pending(local.db()).await.unwrap();
    let remaining = local.db().pending_ops().await.unwrap();
    assert_eq!(remaining.len(), 3);
    assert!(remaining[0].conflict.is_some());
    local.db().delete_pending_op(remaining[0].id).await.unwrap();
    gm.sync_pending(local.db()).await.unwrap();
    assert!(local.db().pending_ops().await.unwrap().is_empty());

    // Only the GM stops the session.
    assert_eq!(player.stop().await.unwrap_err().code, "forbidden");
    gm.stop().await.unwrap();
    // A verified cached package remains readable after the server session has stopped.
    assert_eq!(
        player
            .plugin_package(&plugin_sha, &package_cache)
            .await
            .unwrap(),
        package_bytes
    );
    let mut final_copy = Some(local);
    gm.refresh_replica(&mut final_copy).await.unwrap();
    let local = final_copy.take().unwrap();
    assert_eq!(local.snapshot().maps[0].name, "Conflicting edit");
    assert_eq!(
        local.journal_entry(&offline_note).unwrap().markdown,
        "Edited after refresh"
    );

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
