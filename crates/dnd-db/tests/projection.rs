//! Projection: events written to the database and read back equal `evolve` applied in memory.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dnd_db::{BatchRecord, CampaignDb, LocalUser};
use dnd_game_core::{Actor, GameState, SplitMix64, apply, evolve_all};
use dnd_protocol::arbitrary::{self, pooled_character_id};
use dnd_protocol::*;
use proptest::prelude::*;
use uuid::Uuid;

const GM: UserId = UserId(Uuid::from_u128(0xA1));

fn gm() -> LocalUser {
    LocalUser {
        id: GM,
        name: "Master".into(),
        role: Role::Gm,
    }
}

/// Runs random GM intents (with the current expected version half of the time) and returns the
/// accepted batches and the resulting in-memory state.
fn run_intents(steps: &[(IntentEnvelope, bool)]) -> (Vec<BatchRecord>, GameState) {
    let mut state = GameState::new("Projected");
    let mut rng = SplitMix64::new(7);
    let mut seq = Seq(1);
    let mut records = Vec::new();
    let actor = Actor {
        user_id: GM,
        role: Role::Gm,
    };
    for (envelope, use_current) in steps {
        let mut envelope = envelope.clone();
        if *use_current {
            envelope.expected_version = match &envelope.intent {
                Intent::UpdateCampaignMeta { .. } => Some(state.campaign.version),
                Intent::UpdateMap { id, .. } | Intent::DeleteMap { id } => {
                    state.maps.get(id).map(|m| m.version)
                }
                Intent::UpdateToken { id, .. } | Intent::DeleteToken { id } => {
                    state.tokens.get(id).map(|t| t.version)
                }
                _ => None,
            };
        }
        // Tokens referencing characters would need character rows; drop the link here.
        if let Intent::CreateToken { character_id, .. } = &mut envelope.intent {
            *character_id = None;
        }
        if let Ok(events) = apply(&state, &envelope, &actor, &mut rng) {
            evolve_all(&mut state, &events);
            let batch = EventBatch {
                seq,
                caused_by: Some(envelope.intent_id),
                events,
            };
            seq = batch.last_seq().next();
            records.push(BatchRecord {
                batch,
                intent_hash: Some(envelope.payload_hash()),
                actor: Some(GM),
                at_ms: 1,
            });
        }
    }
    (records, state)
}

async fn project_and_load(records: &[BatchRecord], chunk: usize) -> (GameState, Seq) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("campaign.sqlite");
    let db = CampaignDb::create(&path, "Projected", &gm(), 0)
        .await
        .unwrap();
    for part in records.chunks(chunk.max(1)) {
        db.project(part).await.unwrap();
    }
    db.close().await.unwrap();

    // Reopen from disk to prove the data is persisted, not just cached.
    let (db, _) = CampaignDb::open(&path, 0).await.unwrap();
    let loaded = db.load_state().await.unwrap();
    db.close().await.unwrap();
    loaded
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn projection_matches_evolve(
        steps in proptest::collection::vec((arbitrary::intent_envelope(), any::<bool>()), 1..40),
        chunk in 1usize..5,
    ) {
        let (records, expected) = run_intents(&steps);
        let expected_seq = records.last().map_or(Seq(0), |r| r.batch.last_seq());
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (loaded, seq) = runtime.block_on(project_and_load(&records, chunk));
        prop_assert_eq!(loaded, expected);
        prop_assert_eq!(seq, expected_seq);
    }
}

#[tokio::test]
async fn create_and_reopen_keeps_meta_and_local_user() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.sqlite");
    let db = CampaignDb::create(&path, "Curse of Tests", &gm(), 5)
        .await
        .unwrap();
    assert!(matches!(
        CampaignDb::create(&path, "again", &gm(), 5).await,
        Err(dnd_db::DbError::AlreadyExists(_))
    ));
    db.close().await.unwrap();

    let (db, report) = CampaignDb::open(&path, 6).await.unwrap();
    assert_eq!(report.pre_migration_backup, None);
    assert_eq!(db.local_user().await.unwrap(), Some(gm()));
    let (state, seq) = db.load_state().await.unwrap();
    assert_eq!(state, GameState::new("Curse of Tests"));
    assert_eq!(seq, Seq(0));
}

#[tokio::test]
async fn character_owners_are_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.sqlite");
    let db = CampaignDb::create(&path, "C", &gm(), 0).await.unwrap();
    let id = pooled_character_id(0);
    sqlx::query(
        "INSERT INTO characters (id, kind, name, owner_user_id) VALUES (?, 'pc', 'Aria', ?)",
    )
    .bind(id.to_string())
    .bind(GM.to_string())
    .execute(db.pool())
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(
        state.characters[&id],
        CharacterRef {
            id,
            name: "Aria".into(),
            owner: Some(GM)
        }
    );
}
