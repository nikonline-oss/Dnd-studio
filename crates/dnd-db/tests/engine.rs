//! `CampaignEngine`: the shared intent pipeline with several actors on one campaign (ADR-0022).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dnd_db::{CampaignDb, CampaignEngine, LocalUser};
use dnd_game_core::{Actor, IntentWindow};
use dnd_protocol::*;

async fn engine(dir: &std::path::Path) -> (CampaignEngine, Actor) {
    let gm = LocalUser {
        id: UserId::new_v7(),
        name: "GM".into(),
        role: Role::Gm,
    };
    let db = CampaignDb::create(&dir.join("campaign.sqlite"), "Shared", &gm, 0)
        .await
        .unwrap();
    let engine = CampaignEngine::load(db, IntentWindow::DEFAULT, 7, 0)
        .await
        .unwrap();
    let actor = Actor {
        user_id: gm.id,
        role: gm.role,
    };
    (engine, actor)
}

fn create_map() -> IntentEnvelope {
    IntentEnvelope {
        intent_id: IntentId::new_v7(),
        expected_version: None,
        intent: Intent::CreateMap {
            id: MapId::new_v7(),
            name: "Cave".into(),
            background_asset_id: None,
            width: 4000,
            height: 3000,
            grid: Grid::default(),
            visible_to_players: true,
        },
    }
}

#[tokio::test]
async fn each_submit_is_judged_for_its_own_actor() {
    let dir = tempfile::tempdir().unwrap();
    let (mut engine, gm) = engine(dir.path()).await;
    let player = Actor {
        user_id: UserId::new_v7(),
        role: Role::Player,
    };

    let (outcome, batch) = engine.submit(&create_map(), &player, 1).await.unwrap();
    assert_eq!(
        outcome,
        IntentOutcome::Rejected {
            reason: RejectReason::Forbidden
        }
    );
    assert!(batch.is_none());
    assert_eq!(engine.last_seq(), Seq(0));

    let (outcome, batch) = engine.submit(&create_map(), &gm, 2).await.unwrap();
    assert_eq!(outcome, IntentOutcome::Accepted { seq: Seq(1) });
    assert!(batch.unwrap().caused_by.is_some());
    assert_eq!(engine.db().last_seq().await.unwrap(), Seq(1));
}

#[tokio::test]
async fn events_without_an_intent_are_committed_and_numbered() {
    let dir = tempfile::tempdir().unwrap();
    let (mut engine, gm) = engine(dir.path()).await;
    engine.submit(&create_map(), &gm, 1).await.unwrap();

    let renamed = CampaignMeta {
        name: "Renamed".into(),
        description: String::new(),
        version: Version(2),
    };
    let batch = engine
        .commit_events(
            vec![Event::CampaignMetaUpdated {
                campaign: renamed.clone(),
            }],
            None,
            2,
        )
        .await
        .unwrap();
    assert_eq!(batch.seq, Seq(2));
    assert_eq!(batch.caused_by, None);
    assert_eq!(engine.snapshot().campaign, renamed);
    assert_eq!(engine.snapshot().seq, Seq(2));

    // Reloading from the file gives the same state and continues the numbering.
    let folder = dir.path().to_owned();
    engine.close().await.unwrap();
    let (db, _) = CampaignDb::open(&folder.join("campaign.sqlite"), 3)
        .await
        .unwrap();
    let mut reopened = CampaignEngine::load(db, IntentWindow::DEFAULT, 7, 3)
        .await
        .unwrap();
    assert_eq!(reopened.snapshot().campaign, renamed);
    let (outcome, _) = reopened.submit(&create_map(), &gm, 4).await.unwrap();
    assert_eq!(outcome, IntentOutcome::Accepted { seq: Seq(3) });
}
