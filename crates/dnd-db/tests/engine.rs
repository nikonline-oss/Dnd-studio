//! `CampaignEngine`: the shared intent pipeline with several actors on one campaign (ADR-0022).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dnd_db::{CampaignDb, CampaignEngine, LocalUser};
use dnd_game_core::{Actor, IntentWindow, SplitMix64};
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
    let engine = CampaignEngine::load(db, IntentWindow::DEFAULT, Box::new(SplitMix64::new(7)), 0)
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
        ts_ms: 0,
        intent: Intent::CreateMap {
            id: MapId::new_v7(),
            name: "Cave".into(),
            description: String::new(),
            background_asset_id: None,
            width: 4000,
            height: 3000,
            grid: Grid::default(),
            visible_to_players: true,
        },
    }
}

#[tokio::test]
async fn a_durable_retry_after_the_cache_expires_never_rewinds_a_token() {
    let dir = tempfile::tempdir().unwrap();
    let (mut engine, gm) = engine(dir.path()).await;
    let create = create_map();
    let Intent::CreateMap { id: map, .. } = create.intent else {
        unreachable!()
    };
    engine.submit(&create, &gm, 0).await.unwrap();
    let token = TokenId::new_v7();
    engine
        .commit_events(
            vec![Event::TokenCreated {
                token: TokenDto {
                    id: token,
                    map_id: map,
                    character_id: None,
                    asset_id: None,
                    name: "Pawn".into(),
                    x: 0,
                    y: 0,
                    rotation: 0.0,
                    scale: 1.0,
                    layer: TokenLayer::Tokens,
                    disposition: TokenDisposition::Ally,
                    visible: true,
                    inspector: None,
                    version: Version(1),
                },
            }],
            Some(gm.user_id),
            1,
        )
        .await
        .unwrap();
    let first = IntentEnvelope {
        intent_id: IntentId::new_v7(),
        expected_version: None,
        ts_ms: 0,
        intent: Intent::MoveToken {
            id: token,
            x: 16,
            y: 16,
        },
    };
    let accepted = engine.submit(&first, &gm, 2).await.unwrap().0;
    let later = IntentEnvelope {
        intent_id: IntentId::new_v7(),
        expected_version: None,
        ts_ms: 0,
        intent: Intent::MoveToken {
            id: token,
            x: 32,
            y: 32,
        },
    };
    engine.submit(&later, &gm, 3).await.unwrap();
    engine.close().await.unwrap();
    let now = 3 * 24 * 60 * 60 * 1000;
    let (db, _) = CampaignDb::open(&dir.path().join("campaign.sqlite"), now)
        .await
        .unwrap();
    let mut engine =
        CampaignEngine::load(db, IntentWindow::DEFAULT, Box::new(SplitMix64::new(7)), now)
            .await
            .unwrap();
    let (outcome, batch) = engine.submit(&first, &gm, now).await.unwrap();
    assert_eq!(outcome, accepted);
    assert!(batch.is_none());
    assert_eq!(engine.state().tokens[&token].x, 32);
    let reused = IntentEnvelope {
        intent: Intent::MoveToken {
            id: token,
            x: 99,
            y: 99,
        },
        ..first
    };
    assert_eq!(
        engine.submit(&reused, &gm, now + 1).await.unwrap().0,
        IntentOutcome::Rejected {
            reason: RejectReason::IntentIdReused
        }
    );
    let (retry, batch) = engine.submit(&first, &gm, now + 2).await.unwrap();
    assert_eq!(retry, accepted);
    assert!(batch.is_none());
    assert_eq!(engine.state().tokens[&token].x, 32);
    engine.close().await.unwrap();
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
async fn adapter_checks_cannot_bypass_core_rules_or_change_a_retry() {
    let dir = tempfile::tempdir().unwrap();
    let (mut engine, gm) = engine(dir.path()).await;
    let player = Actor {
        user_id: UserId::new_v7(),
        role: Role::Player,
    };
    assert!(matches!(
        engine
            .submit_with_precondition(&create_map(), &player, 0, Ok(()))
            .await
            .unwrap()
            .0,
        IntentOutcome::Rejected {
            reason: RejectReason::Forbidden
        }
    ));
    let rejected = create_map();
    let denied = engine
        .submit_with_precondition(&rejected, &gm, 1, Err(RejectReason::invalid("upload")))
        .await
        .unwrap();
    assert!(denied.1.is_none());
    assert_eq!(engine.last_seq(), Seq(0));
    assert_eq!(
        engine
            .submit_with_precondition(&rejected, &gm, 2, Ok(()))
            .await
            .unwrap()
            .0,
        denied.0
    );
    let allowed = create_map();
    let accepted = engine
        .submit_with_precondition(&allowed, &gm, 3, Ok(()))
        .await
        .unwrap()
        .0;
    assert!(matches!(accepted, IntentOutcome::Accepted { .. }));
    let (retry, batch) = engine
        .submit_with_precondition(&allowed, &gm, 4, Err(RejectReason::invalid("upload")))
        .await
        .unwrap();
    assert_eq!(retry, accepted);
    assert!(batch.is_none());
    engine.close().await.unwrap();
}

#[tokio::test]
async fn events_without_an_intent_are_committed_and_numbered() {
    let dir = tempfile::tempdir().unwrap();
    let (mut engine, gm) = engine(dir.path()).await;
    engine.submit(&create_map(), &gm, 1).await.unwrap();

    let renamed = CampaignMeta {
        game_clock: Default::default(),
        name: "Renamed".into(),
        description: String::new(),
        token_inspector: dnd_protocol::TokenInspectorPolicy::base(),
        item_states: Vec::new(),
        visibility: Default::default(),
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
    let mut reopened =
        CampaignEngine::load(db, IntentWindow::DEFAULT, Box::new(SplitMix64::new(7)), 3)
            .await
            .unwrap();
    assert_eq!(reopened.snapshot().campaign, renamed);
    let (outcome, _) = reopened.submit(&create_map(), &gm, 4).await.unwrap();
    assert_eq!(outcome, IntentOutcome::Accepted { seq: Seq(3) });
}

#[tokio::test]
async fn fictional_clock_persists_and_retry_after_reopen_does_not_advance_twice() {
    let dir = tempfile::tempdir().unwrap();
    let (mut engine, gm) = engine(dir.path()).await;
    let intent = IntentEnvelope {
        intent_id: IntentId::new_v7(),
        expected_version: Some(Version::INITIAL),
        ts_ms: 0,
        intent: Intent::AdvanceGameTime { seconds: 600 },
    };
    let first = engine.submit(&intent, &gm, 1).await.unwrap().0;
    assert_eq!(engine.snapshot().campaign.game_clock.seconds, 600);
    engine.close().await.unwrap();
    let (db, _) = CampaignDb::open(&dir.path().join("campaign.sqlite"), 2)
        .await
        .unwrap();
    let mut engine =
        CampaignEngine::load(db, IntentWindow::DEFAULT, Box::new(SplitMix64::new(7)), 2)
            .await
            .unwrap();
    assert_eq!(engine.snapshot().campaign.game_clock.seconds, 600);
    assert_eq!(engine.snapshot().campaign.game_clock.version, Version(2));
    let retry = engine.submit(&intent, &gm, 3).await.unwrap().0;
    assert_eq!(retry, first);
    assert_eq!(engine.snapshot().campaign.game_clock.seconds, 600);
}

fn roll() -> IntentEnvelope {
    IntentEnvelope {
        intent_id: IntentId::new_v7(),
        expected_version: None,
        ts_ms: 0,
        intent: Intent::RollDice {
            id: ChatMessageId::new_v7(),
            expression: "1d20".into(),
            hidden: false,
            character_id: None,
            roll: None,
        },
    }
}

fn roll_total(batch: &EventBatch) -> i64 {
    match batch.events.as_slice() {
        [Event::ChatMessagePosted { message }] => {
            message
                .roll
                .as_ref()
                .expect("a roll message carries its result")
                .total
        }
        other => panic!("expected one roll event: {other:?}"),
    }
}

/// A repeat returns the stored outcome and draws no new randomness (ADR-0050): the next roll of
/// the same engine matches a control engine that never saw the repeat.
#[tokio::test]
async fn a_repeated_roll_does_not_consume_randomness() {
    let dir = tempfile::tempdir().unwrap();
    let (mut played, gm) = engine(dir.path()).await;
    let first = roll();
    let (accepted, batch) = played.submit(&first, &gm, 0).await.unwrap();
    let batch = batch.expect("the first roll is committed");
    let (repeat, none) = played.submit(&first, &gm, 1).await.unwrap();
    assert_eq!(repeat, accepted);
    assert!(none.is_none());
    let second = roll();
    let (_, next) = played.submit(&second, &gm, 2).await.unwrap();
    let next = next.expect("the second roll is committed");

    let control_dir = tempfile::tempdir().unwrap();
    let (mut control, control_gm) = engine(control_dir.path()).await;
    let (_, control_first) = control.submit(&first, &control_gm, 0).await.unwrap();
    let (_, control_next) = control.submit(&second, &control_gm, 2).await.unwrap();
    // Only the GM id differs between the two campaigns; the drawn dice must not.
    assert_eq!(
        roll_total(&batch),
        roll_total(&control_first.expect("committed"))
    );
    assert_eq!(
        roll_total(&next),
        roll_total(&control_next.expect("committed"))
    );

    played.close().await.unwrap();
    control.close().await.unwrap();
}

/// A failed write commits nothing: memory, sequence counter and recorded outcomes stay as they
/// were, so the next successful batch takes the same number (ADR-0014, ADR-0050).
#[tokio::test]
async fn a_failed_commit_leaves_no_trace_and_the_sequence_intact() {
    let dir = tempfile::tempdir().unwrap();
    let (mut engine, gm) = engine(dir.path()).await;
    let before = engine.snapshot();
    assert!(
        engine
            .commit_events(vec![Event::Redacted], Some(gm.user_id), 1)
            .await
            .is_err()
    );
    assert_eq!(engine.snapshot(), before);
    let (outcome, batch) = engine.submit(&create_map(), &gm, 2).await.unwrap();
    assert_eq!(outcome, IntentOutcome::Accepted { seq: Seq(1) });
    assert_eq!(batch.expect("committed").seq, Seq(1));
    engine.close().await.unwrap();
}

// --- A8: real-time expiry without a user command (ADR-0056) ---

fn effect(until_ts: i64) -> TokenEffectDto {
    TokenEffectDto {
        id: EffectId::new_v7(),
        token: TokenId::new_v7(),
        label: "Bless".into(),
        condition: None,
        resource: None,
        dice: None,
        heal: false,
        reveal: Vec::new(),
        duration: EffectDuration::RealTime { until_ts },
        version: Version::INITIAL,
    }
}

#[tokio::test]
async fn an_overdue_effect_expires_after_a_restart_without_a_command() {
    let dir = tempfile::tempdir().unwrap();
    let (mut engine, gm) = engine(dir.path()).await;
    engine
        .commit_events(
            vec![Event::TokenEffectApplied {
                effect: effect(1_000),
            }],
            Some(gm.user_id),
            0,
        )
        .await
        .unwrap();
    assert_eq!(engine.next_deadline(), Some(1_000));
    // A signal before the deadline removes nothing and writes nothing.
    assert!(engine.expire_due(999).await.unwrap().is_none());
    assert_eq!(engine.state().effects.len(), 1);
    engine.close().await.unwrap();

    // Reopen past the deadline: the internal command removes the effect with no user at the table.
    let (db, _) = CampaignDb::open(&dir.path().join("campaign.sqlite"), 2_000)
        .await
        .unwrap();
    let mut engine = CampaignEngine::load(
        db,
        IntentWindow::DEFAULT,
        Box::new(SplitMix64::new(7)),
        2_000,
    )
    .await
    .unwrap();
    assert_eq!(
        engine.next_deadline(),
        Some(1_000),
        "an overdue deadline is visible after the restart"
    );
    let batch = engine
        .expire_due(2_000)
        .await
        .unwrap()
        .expect("the overdue effect is removed");
    assert!(
        batch
            .events
            .iter()
            .any(|event| matches!(event, Event::TokenEffectRemoved { expired: true, .. }))
    );
    assert!(engine.state().effects.is_empty());
    // A second signal — another adapter, a retry, a racing command — does nothing.
    assert!(engine.expire_due(3_000).await.unwrap().is_none());
    engine.close().await.unwrap();

    let (db, _) = CampaignDb::open(&dir.path().join("campaign.sqlite"), 4_000)
        .await
        .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert!(
        state.effects.is_empty(),
        "the expiry is written through, not only in memory"
    );
    db.close().await.unwrap();
}
