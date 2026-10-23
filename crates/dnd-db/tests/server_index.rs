//! `ServerDb`: the session server's index (ADR-0022). Every query on a real SQLite file.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dnd_db::{ParticipantRow, ParticipantStatus, ServerDb, SessionRow, SessionState};
use dnd_protocol::arbitrary::v7;
use dnd_protocol::{Role, SessionId, UserId};

fn session(n: u128, created_at: i64) -> SessionRow {
    SessionRow {
        id: SessionId(v7(n)),
        name: format!("Campaign {n}"),
        state: SessionState::Active,
        gm_key_hash: "$argon2id$gm".into(),
        access_code_hash: "$argon2id$code".into(),
        created_at,
        last_activity: created_at,
        storage_dir: format!("/data/sessions/{n}"),
        campaign_path: format!("/data/sessions/{n}/campaign.sqlite"),
    }
}

#[tokio::test]
async fn sessions_are_stored_listed_and_change_state() {
    let dir = tempfile::tempdir().unwrap();
    let db = ServerDb::open(&dir.path().join("server.sqlite"))
        .await
        .unwrap();
    let (a, b) = (session(1, 10), session(2, 20));
    db.insert_session(&b).await.unwrap();
    db.insert_session(&a).await.unwrap();
    assert!(db.insert_session(&a).await.is_err(), "ids are unique");

    assert_eq!(db.session(a.id).await.unwrap(), Some(a.clone()));
    assert_eq!(db.session(SessionId(v7(9))).await.unwrap(), None);
    assert_eq!(
        db.sessions_in(SessionState::Active).await.unwrap(),
        [a.clone(), b.clone()]
    );
    assert_eq!(db.count_sessions_in(SessionState::Active).await.unwrap(), 2);

    // Activity only moves forward.
    db.touch_session(a.id, 50).await.unwrap();
    db.touch_session(a.id, 40).await.unwrap();
    assert_eq!(db.session(a.id).await.unwrap().unwrap().last_activity, 50);

    db.set_session_state(a.id, SessionState::Stopped, 60)
        .await
        .unwrap();
    let stopped = db.session(a.id).await.unwrap().unwrap();
    assert_eq!(
        (stopped.state, stopped.last_activity),
        (SessionState::Stopped, 60)
    );
    assert_eq!(db.count_sessions_in(SessionState::Active).await.unwrap(), 1);
    assert_eq!(
        db.sessions_in(SessionState::Stopped).await.unwrap(),
        [stopped]
    );
    db.close().await;
}

#[tokio::test]
async fn participants_are_found_by_token_within_their_session() {
    let dir = tempfile::tempdir().unwrap();
    let db = ServerDb::open(&dir.path().join("server.sqlite"))
        .await
        .unwrap();
    let (s1, s2) = (session(1, 0), session(2, 0));
    db.insert_session(&s1).await.unwrap();
    db.insert_session(&s2).await.unwrap();
    let mira = ParticipantRow {
        id: v7(0x100),
        session_id: s1.id,
        role: Role::Player,
        name: "Mira".into(),
        user_id: UserId(v7(0x200)),
        status: ParticipantStatus::Connected,
    };
    db.insert_participant(&mira, "hash-mira", 5).await.unwrap();
    let other = ParticipantRow {
        id: v7(0x101),
        ..mira.clone()
    };
    assert!(
        db.insert_participant(&other, "hash-mira", 5).await.is_err(),
        "token hashes are unique"
    );

    assert_eq!(
        db.participant_by_token(s1.id, "hash-mira").await.unwrap(),
        Some(mira.clone())
    );
    // A token of one session opens nothing in another.
    assert_eq!(
        db.participant_by_token(s2.id, "hash-mira").await.unwrap(),
        None
    );
    assert_eq!(db.participant_by_token(s1.id, "nope").await.unwrap(), None);

    db.set_participant_status(mira.id, ParticipantStatus::Disconnected, 9)
        .await
        .unwrap();
    let found = db
        .participant_by_token(s1.id, "hash-mira")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.status, ParticipantStatus::Disconnected);
    assert_eq!(db.count_participants(s1.id).await.unwrap(), 1);
    assert_eq!(db.count_participants(s2.id).await.unwrap(), 0);
    db.close().await;
}

#[tokio::test]
async fn audit_is_appended() {
    let dir = tempfile::tempdir().unwrap();
    let db = ServerDb::open(&dir.path().join("server.sqlite"))
        .await
        .unwrap();
    let s = session(1, 0);
    db.insert_session(&s).await.unwrap();
    db.audit(
        Some(s.id),
        "gm",
        "session_stopped",
        &serde_json::json!({ "by": "Game Master" }),
        7,
    )
    .await
    .unwrap();
    db.audit(None, "host", "session_rejected", &serde_json::json!({}), 8)
        .await
        .unwrap();
    let rows: Vec<(Option<String>, String, String)> =
        sqlx::query_as("SELECT session_id, action, details FROM audit_log ORDER BY id")
            .fetch_all(db.pool())
            .await
            .unwrap();
    assert_eq!(
        rows,
        [
            (
                Some(s.id.to_string()),
                "session_stopped".to_owned(),
                r#"{"by":"Game Master"}"#.to_owned()
            ),
            (None, "session_rejected".to_owned(), "{}".to_owned()),
        ]
    );
    db.close().await;
}
