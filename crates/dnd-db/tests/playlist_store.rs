#![allow(clippy::unwrap_used)]
use dnd_db::{BatchRecord, CampaignDb, LocalUser};
use dnd_protocol::arbitrary::v7;
use dnd_protocol::*;

#[tokio::test]
async fn independent_playlist_projects_updates_and_survives_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.sqlite");
    let user = LocalUser {
        id: UserId(v7(1)),
        name: "GM".into(),
        role: Role::Gm,
    };
    let db = CampaignDb::create(&path, "Queue", &user, 0).await.unwrap();
    let playlist = PlaylistDto {
        id: PlaylistId(v7(2)),
        name: "Rain".into(),
        folder_id: None,
        tracks: vec![PlaylistTrackDto {
            asset_id: AssetId(v7(3)),
            sha256: "a".repeat(64),
            mime: "audio/ogg".into(),
            title: "Retained audio".into(),
        }],
        version: Version::INITIAL,
    };
    let batch = |seq, events| BatchRecord {
        batch: EventBatch {
            seq: Seq(seq),
            caused_by: None,
            events,
        },
        at_ms: 0,
        intent_hash: None,
        actor: Some(user.id),
    };
    db.project(&[batch(
        1,
        vec![Event::PlaylistCreated {
            playlist: playlist.clone(),
        }],
    )])
    .await
    .unwrap();
    let mut updated = playlist;
    updated.name = "Storm".into();
    updated.version = Version(2);
    db.project(&[batch(
        2,
        vec![Event::PlaylistUpdated {
            playlist: updated.clone(),
        }],
    )])
    .await
    .unwrap();
    assert_eq!(
        db.load_state().await.unwrap().0.playlists[&updated.id],
        updated
    );
    db.close().await.unwrap();
    let (db, _) = CampaignDb::open(&path, 0).await.unwrap();
    assert_eq!(
        db.load_state().await.unwrap().0.playlists[&updated.id],
        updated
    );
    db.project(&[batch(3, vec![Event::PlaylistDeleted { id: updated.id }])])
        .await
        .unwrap();
    assert!(db.load_state().await.unwrap().0.playlists.is_empty());
}
