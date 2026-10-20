//! The local server end to end: intent → events → SQLite → reopen → same state.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dnd_desktop_lib::{IntentOutcome, LocalServer, folder_name};
use dnd_protocol::*;

fn envelope(intent: Intent, expected_version: Option<Version>) -> IntentEnvelope {
    IntentEnvelope {
        intent_id: IntentId::new_v7(),
        expected_version,
        intent,
    }
}

#[tokio::test]
async fn intents_persist_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = LocalServer::create(dir.path(), "Lost Mine").await.unwrap();
    let info = server.info();
    assert_eq!(info.name, "Lost Mine");
    assert_eq!(info.role, Role::Gm);

    let map_id = MapId::new_v7();
    let (outcome, batch) = server
        .submit(envelope(
            Intent::CreateMap {
                id: map_id,
                name: "Cave".into(),
                background_asset_id: None,
                grid: Grid::default(),
                visible_to_players: false,
            },
            None,
        ))
        .await
        .unwrap();
    assert_eq!(outcome, IntentOutcome::Accepted { seq: Seq(1) });
    assert_eq!(batch.unwrap().seq, Seq(1));

    let token_id = TokenId::new_v7();
    server
        .submit(envelope(
            Intent::CreateToken {
                id: token_id,
                map_id,
                character_id: None,
                asset_id: None,
                name: "Goblin".into(),
                x: 0.0,
                y: 0.0,
                rotation: 0.0,
                scale: 1.0,
                layer: TokenLayer::Tokens,
                visible: true,
            },
            None,
        ))
        .await
        .unwrap();
    server
        .submit(envelope(
            Intent::MoveToken {
                id: token_id,
                x: 70.0,
                y: 140.0,
            },
            None,
        ))
        .await
        .unwrap();

    // A stale edit is rejected without touching state or disk.
    let (outcome, batch) = server
        .submit(envelope(
            Intent::UpdateMap {
                id: map_id,
                name: Some("Stale".into()),
                grid: None,
                visible_to_players: None,
            },
            Some(Version(7)),
        ))
        .await
        .unwrap();
    assert_eq!(
        outcome,
        IntentOutcome::Rejected {
            reason: RejectReason::StaleVersion {
                current: Version(1)
            }
        }
    );
    assert!(batch.is_none());

    let before = server.snapshot();
    assert_eq!(before.seq, Seq(3));
    let folder = server.folder().to_owned();
    server.close().await.unwrap();

    let reopened = LocalServer::open(&folder).await.unwrap();
    assert_eq!(reopened.snapshot(), before);
    assert_eq!(reopened.info().user_id, info.user_id);
}

#[tokio::test]
async fn creating_twice_in_same_place_fails() {
    let dir = tempfile::tempdir().unwrap();
    let first = LocalServer::create(dir.path(), "Twin").await.unwrap();
    first.close().await.unwrap();
    let err = LocalServer::create(dir.path(), "Twin").await.unwrap_err();
    assert_eq!(err.code, "already_exists");
}

#[tokio::test]
async fn opening_a_non_campaign_folder_is_a_user_error() {
    let dir = tempfile::tempdir().unwrap();
    let err = LocalServer::open(dir.path()).await.unwrap_err();
    assert_eq!(err.code, "not_a_campaign");
}

#[test]
fn folder_names_are_safe() {
    assert_eq!(folder_name("Curse of Strahd"), "Curse of Strahd");
    assert_eq!(folder_name("a/b\\c:d*e?"), "a_b_c_d_e_");
    assert_eq!(folder_name("  trailing dots... "), "trailing dots");
    assert_eq!(folder_name("..."), "campaign");
    assert_eq!(folder_name(""), "campaign");
    assert_eq!(folder_name(&"x".repeat(100)).len(), 64);
}

/// Regenerates `src/shared/ipc/bindings.ts`. CI fails if the committed file differs.
#[test]
fn export_bindings() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(dnd_desktop_lib::BINDINGS_PATH);
    dnd_desktop_lib::export_bindings(&path).unwrap();
}
