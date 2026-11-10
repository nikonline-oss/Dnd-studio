//! ADR-0008: every runtime query of `dnd-db` runs against a real SQLite file in some test.
//! This file covers the ones the other tests only reach by chance; the inventory test at the end
//! fails when a query is added without updating the coverage table in ADR-0008.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dnd_db::{BatchRecord, CampaignDb, LocalUser, RecentIntent, open_server_db};
use dnd_protocol::arbitrary::v7;
use dnd_protocol::*;
use sqlx::Row;

fn gm() -> LocalUser {
    LocalUser {
        id: UserId(v7(1)),
        name: "GM".into(),
        role: Role::Gm,
    }
}

fn map(n: u128, name: &str, version: i64) -> MapDto {
    MapDto {
        id: MapId(v7(0x100 + n)),
        name: name.into(),
        description: String::new(),
        background_asset_id: None,
        width: 1200,
        height: 800,
        grid: Grid {
            kind: GridKind::HexFlat,
            cell_size: 50.0,
            offset_x: 1.5,
            offset_y: -2.0,
        },
        visible_to_players: version % 2 == 0,
        version: Version(version),
    }
}

fn token(n: u128, map_n: u128, version: i64) -> TokenDto {
    TokenDto {
        id: TokenId(v7(0x200 + n)),
        map_id: MapId(v7(0x100 + map_n)),
        character_id: None,
        asset_id: None,
        name: format!("T{n}"),
        x: 164,
        y: 56,
        rotation: 90.0,
        scale: 1.5,
        layer: TokenLayer::Gm,
        disposition: TokenDisposition::Enemy,
        visible: false,
        inspector: None,
        version: Version(version),
    }
}

fn record(seq: i64, intent: Option<u128>, events: Vec<Event>) -> BatchRecord {
    BatchRecord {
        batch: EventBatch {
            seq: Seq(seq),
            caused_by: intent.map(|n| IntentId(v7(n))),
            events,
        },
        intent_hash: intent.map(|n| PayloadHash([u8::try_from(n % 256).unwrap(); 32])),
        actor: Some(UserId(v7(1))),
        at_ms: 1234,
    }
}

async fn new_campaign(dir: &tempfile::TempDir) -> CampaignDb {
    CampaignDb::create(&dir.path().join("c.sqlite"), "Queries", &gm(), 0)
        .await
        .unwrap()
}

/// Each projection query (insert and conflict-update paths, delete, move, FK cascade) and the
/// `event_log` insert, checked row by row.
#[tokio::test]
async fn every_event_kind_is_projected() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let campaign = CampaignMeta {
        game_clock: Default::default(),
        name: "Renamed".into(),
        description: "Desc".into(),
        token_inspector: dnd_protocol::TokenInspectorPolicy::base(),
        item_states: Vec::new(),
        visibility: Default::default(),
        version: Version(2),
    };
    db.project(&[
        record(
            1,
            Some(0xA),
            vec![Event::CampaignMetaUpdated {
                campaign: campaign.clone(),
            }],
        ),
        record(
            2,
            Some(0xB),
            vec![Event::MapCreated {
                map: map(1, "One", 1),
            }],
        ),
        record(
            3,
            Some(0xC),
            vec![Event::MapUpdated {
                map: map(1, "Uno", 2),
            }],
        ),
        record(
            4,
            Some(0xD),
            vec![Event::MapCreated {
                map: map(2, "Two", 1),
            }],
        ),
        record(
            5,
            Some(0xE),
            vec![
                Event::TokenCreated {
                    token: token(1, 1, 1),
                },
                Event::TokenCreated {
                    token: token(2, 2, 1),
                },
                Event::TokenCreated {
                    token: token(3, 2, 1),
                },
            ],
        ),
        record(
            8,
            None,
            vec![Event::TokenUpdated {
                token: token(1, 1, 2),
            }],
        ),
        record(
            9,
            Some(0xF),
            vec![
                Event::TokenMoved {
                    id: TokenId(v7(0x201)),
                    x: 1120,
                    y: 2240,
                },
                // A move of an unknown token is a no-op, not an error.
                Event::TokenMoved {
                    id: TokenId(v7(0x2FF)),
                    x: 16,
                    y: 16,
                },
            ],
        ),
        record(
            11,
            Some(0x10),
            vec![Event::TokenDeleted {
                id: TokenId(v7(0x202)),
            }],
        ),
        // Deleting map 2 cascades to its remaining token 3 through the foreign key.
        record(
            12,
            Some(0x11),
            vec![Event::MapDeleted {
                id: MapId(v7(0x102)),
            }],
        ),
    ])
    .await
    .unwrap();

    let (state, seq) = db.load_state().await.unwrap();
    assert_eq!(seq, Seq(12));
    assert_eq!(state.campaign, campaign);
    assert_eq!(
        state.maps.values().cloned().collect::<Vec<_>>(),
        [map(1, "Uno", 2)]
    );
    let mut moved = token(1, 1, 2);
    moved.x = 1120;
    moved.y = 2240;
    assert_eq!(state.tokens.values().cloned().collect::<Vec<_>>(), [moved]);

    let rows = sqlx::query(
        "SELECT seq, intent_id, actor_user_id, event, created_at FROM event_log ORDER BY seq",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    assert_eq!(rows.len(), 12);
    let seqs: Vec<i64> = rows.iter().map(|r| r.get("seq")).collect();
    assert_eq!(seqs, (1..=12).collect::<Vec<_>>());
    let first: String = rows[0].get("event");
    assert_eq!(
        serde_json::from_str::<Event>(&first).unwrap(),
        Event::CampaignMetaUpdated { campaign }
    );
    assert_eq!(
        rows[5].get::<Option<String>, _>("intent_id"),
        Some(IntentId(v7(0xE)).to_string())
    );
    assert_eq!(rows[7].get::<Option<String>, _>("intent_id"), None);
    assert_eq!(
        rows[0].get::<String, _>("actor_user_id"),
        UserId(v7(1)).to_string()
    );
    assert_eq!(rows[0].get::<i64, _>("created_at"), 1234);
}

/// A failing batch rolls back the whole transaction (no partial event_log / projection).
#[tokio::test]
async fn project_is_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let result = db
        .project(&[
            record(
                1,
                Some(1),
                vec![Event::MapCreated {
                    map: map(1, "One", 1),
                }],
            ),
            // A token on a map that does not exist violates the foreign key.
            record(
                2,
                Some(2),
                vec![Event::TokenCreated {
                    token: token(1, 9, 1),
                }],
            ),
        ])
        .await;
    assert!(result.is_err());
    let (state, seq) = db.load_state().await.unwrap();
    assert_eq!(seq, Seq(0));
    assert!(state.maps.is_empty());
}

#[tokio::test]
async fn recent_intents_are_ordered_limited_and_grouped() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    db.project(&[
        record(
            1,
            Some(0xA),
            vec![Event::MapCreated {
                map: map(1, "A", 1),
            }],
        ),
        record(
            2,
            None,
            vec![Event::MapUpdated {
                map: map(1, "B", 2),
            }],
        ),
        record(
            3,
            Some(0xB),
            vec![
                Event::MapCreated {
                    map: map(2, "C", 1),
                },
                Event::MapCreated {
                    map: map(3, "D", 1),
                },
            ],
        ),
        record(
            5,
            Some(0xC),
            vec![Event::MapUpdated {
                map: map(1, "E", 3),
            }],
        ),
    ])
    .await
    .unwrap();

    let ids = |v: Vec<RecentIntent>| -> Vec<(IntentId, Seq)> {
        v.into_iter().map(|r| (r.intent_id, r.last_seq)).collect()
    };
    assert_eq!(
        ids(db.recent_intents(10, 0).await.unwrap()),
        [
            (IntentId(v7(0xA)), Seq(1)),
            (IntentId(v7(0xB)), Seq(4)), // the batch's last event
            (IntentId(v7(0xC)), Seq(5)),
        ]
    );
    // The limit keeps the most recent ones, still oldest first.
    assert_eq!(
        ids(db.recent_intents(2, 0).await.unwrap()),
        [(IntentId(v7(0xB)), Seq(4)), (IntentId(v7(0xC)), Seq(5))]
    );
    assert!(db.recent_intents(0, 0).await.unwrap().is_empty());
    assert_eq!(db.recent_intents(10, 0).await.unwrap()[0].at_ms, 1234);
}

/// Only intents recorded at or after `since_ms` are restored (ADR-0015 window boundary).
#[tokio::test]
async fn recent_intents_respect_the_time_window() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let at = |mut r: BatchRecord, ms: i64| {
        r.at_ms = ms;
        r
    };
    db.project(&[
        at(
            record(
                1,
                Some(0xA),
                vec![Event::MapCreated {
                    map: map(1, "A", 1),
                }],
            ),
            999,
        ),
        at(
            record(
                2,
                Some(0xB),
                vec![Event::MapCreated {
                    map: map(2, "B", 1),
                }],
            ),
            1000,
        ),
        at(
            record(
                3,
                Some(0xC),
                vec![Event::MapCreated {
                    map: map(3, "C", 1),
                }],
            ),
            1001,
        ),
    ])
    .await
    .unwrap();
    let restored: Vec<IntentId> = db
        .recent_intents(10, 1000)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.intent_id)
        .collect();
    assert_eq!(restored, [IntentId(v7(0xB)), IntentId(v7(0xC))]);
}

#[tokio::test]
async fn offline_intents_are_ordered_and_conflicts_can_be_rebased_or_discarded() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    assert_eq!(db.remote_binding().await.unwrap(), None);
    db.set_remote_binding(Some("https://server/session"))
        .await
        .unwrap();
    assert_eq!(
        db.remote_binding().await.unwrap().as_deref(),
        Some("https://server/session")
    );
    db.set_remote_binding(None).await.unwrap();
    assert_eq!(db.remote_binding().await.unwrap(), None);
    let first = IntentEnvelope {
        intent_id: IntentId(v7(0xA10)),
        expected_version: Some(Version(3)),
        ts_ms: 0,
        intent: Intent::UpdateMap {
            id: MapId(v7(0xA11)),
            name: Some("Misty cave".into()),
            description: None,
            grid: None,
            visible_to_players: None,
        },
    };
    let second = IntentEnvelope {
        intent_id: IntentId(v7(0xA12)),
        expected_version: None,
        ts_ms: 0,
        intent: Intent::CreateMap {
            id: MapId(v7(0xA13)),
            name: "Hidden room".into(),
            description: String::new(),
            background_asset_id: None,
            width: 800,
            height: 600,
            grid: Grid {
                kind: GridKind::Square,
                cell_size: 50.0,
                offset_x: 0.0,
                offset_y: 0.0,
            },
            visible_to_players: false,
        },
    };
    let first_id = db.enqueue_pending_op(&first, 100).await.unwrap();
    assert_eq!(db.enqueue_pending_op(&first, 100).await.unwrap(), first_id);
    let mut reused = first.clone();
    reused.expected_version = Some(Version(99));
    assert!(db.enqueue_pending_op(&reused, 100).await.is_err());
    let second_id = db.enqueue_pending_op(&second, 101).await.unwrap();
    let queued = db.pending_ops().await.unwrap();
    assert_eq!(queued.len(), 2);
    assert_eq!(queued[0].envelope, first);
    assert_eq!(queued[1].envelope, second);
    assert_eq!(queued[0].status, "pending");

    let stale = RejectReason::StaleVersion {
        current: Version(4),
    };
    db.mark_pending_conflict(first_id, &stale).await.unwrap();
    let conflicts = db.pending_ops().await.unwrap();
    assert_eq!(conflicts[0].status, "conflict");
    assert_eq!(conflicts[0].conflict, Some(stale));

    let mut rebased = conflicts[0].envelope.clone();
    rebased.intent_id = IntentId(v7(0xA14));
    rebased.expected_version = Some(Version(4));
    db.rebase_pending_op(first_id, &rebased, 4).await.unwrap();
    assert_eq!(db.pending_ops().await.unwrap()[0].envelope, rebased);
    db.delete_pending_op(first_id).await.unwrap();
    db.delete_pending_op(second_id).await.unwrap();
    assert!(db.pending_ops().await.unwrap().is_empty());
    db.close().await.unwrap();
}

#[tokio::test]
async fn checkpoint_empties_the_wal() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    db.project(&[record(
        1,
        Some(1),
        vec![Event::MapCreated {
            map: map(1, "A", 1),
        }],
    )])
    .await
    .unwrap();
    let wal = dir.path().join("c.sqlite-wal");
    assert!(std::fs::metadata(&wal).unwrap().len() > 0);
    db.checkpoint().await.unwrap();
    assert_eq!(std::fs::metadata(&wal).unwrap().len(), 0);
}

#[tokio::test]
async fn server_db_opens_migrates_and_enforces_constraints() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.sqlite");
    let pool = open_server_db(&path).await.unwrap();
    sqlx::query(
        "INSERT INTO sessions (id, state, gm_key_hash, access_code_hash, created_at, \
         last_activity, storage_dir, campaign_path) VALUES ('s1', 'active', 'h', 'h', 0, 0, 'd', 'p')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO participants (id, session_id, role, name, status) \
         VALUES ('p1', 's1', 'player', 'Ann', 'connected')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO audit_log (session_id, actor, action, created_at) VALUES ('s1', 'gm', 'stop', 0)")
        .execute(&pool)
        .await
        .unwrap();

    // CHECK constraints and append-only audit.
    assert!(
        sqlx::query("UPDATE sessions SET state = 'paused' WHERE id = 's1'")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE audit_log SET action = 'x'")
            .execute(&pool)
            .await
            .is_err()
    );

    // Deleting a session cascades to participants and detaches audit entries.
    sqlx::query("DELETE FROM sessions WHERE id = 's1'")
        .execute(&pool)
        .await
        .unwrap();
    let participants: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM participants")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(participants, 0);
    let audit: Option<String> = sqlx::query_scalar("SELECT session_id FROM audit_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(audit, None);
    pool.close().await;

    // Reopening an existing server DB is a no-op migration.
    open_server_db(&path).await.unwrap().close().await;
}

/// Inventory of runtime queries (ADR-0008, "Покрытие запросов"). Adding or removing a query must
/// come with a test and an updated table — this count makes that impossible to forget.
#[test]
fn query_inventory_matches_adr_0008() {
    let sources = [
        include_str!("../src/backup.rs"),
        include_str!("../src/campaign.rs"),
        include_str!("../src/connect.rs"),
        include_str!("../src/server.rs"),
    ];
    let count: usize = sources
        .iter()
        .map(|s| s.matches("sqlx::query").count())
        .sum();
    assert_eq!(
        count, 104,
        "runtime queries changed: add a real-SQLite test and update the table in docs/adr/0008-db-conventions.md"
    );
}

/// Campaign migration 0002: deleting a user detaches their chat messages and dice rolls, while
/// any other update of those append-only rows is still rejected.
#[tokio::test]
async fn deleting_a_user_detaches_append_only_history() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let pool = db.pool();
    sqlx::query("INSERT INTO users (id, name, role, created_at) VALUES ('u2', 'Ann', 'player', 0)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO chat_messages (id, sender_user_id, kind, body, created_at) \
         VALUES ('m1', 'u2', 'text', 'hi', 0)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO dice_rolls (id, user_id, expression, result, total, chat_message_id, created_at) \
         VALUES ('r1', 'u2', '1d20', '[12]', 12, 'm1', 0)",
    )
    .execute(pool)
    .await
    .unwrap();

    for sql in [
        "UPDATE chat_messages SET body = 'edited'",
        "UPDATE chat_messages SET sender_user_id = NULL, body = 'sneaky'",
        "UPDATE dice_rolls SET total = 20",
        "DELETE FROM chat_messages",
    ] {
        let err = sqlx::query(sql).execute(pool).await.unwrap_err();
        assert!(err.to_string().contains("append-only"), "{sql}: {err}");
    }

    sqlx::query("DELETE FROM users WHERE id = 'u2'")
        .execute(pool)
        .await
        .unwrap();
    let sender: Option<String> = sqlx::query_scalar("SELECT sender_user_id FROM chat_messages")
        .fetch_one(pool)
        .await
        .unwrap();
    let roller: Option<String> = sqlx::query_scalar("SELECT user_id FROM dice_rolls")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!((sender, roller), (None, None));
    let body: String = sqlx::query_scalar("SELECT body FROM chat_messages")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(body, "hi");
}

/// Character projection (insert and update paths) and loading users and full characters.
#[tokio::test]
async fn characters_and_users_are_projected_and_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let mut character = CharacterDto {
        id: CharacterId(v7(0x300)),
        kind: CharacterKind::Pc,
        name: "Aria".into(),
        owner: Some(UserId(v7(1))),
        status: CharacterStatus::Draft,
        sheet_json: r#"{"hp": 12}"#.into(),
        conditions: vec!["poisoned".into(), "prone".into()],
        version: Version(1),
    };
    db.project(&[record(
        1,
        Some(0x31),
        vec![Event::CharacterCreated {
            character: character.clone(),
        }],
    )])
    .await
    .unwrap();
    character.status = CharacterStatus::PendingApproval;
    character.owner = None;
    character.sheet_json = r#"{"hp": 9}"#.into();
    character.version = Version(2);
    db.project(&[record(
        2,
        Some(0x32),
        vec![Event::CharacterUpdated {
            character: character.clone(),
        }],
    )])
    .await
    .unwrap();

    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(
        state.characters.values().cloned().collect::<Vec<_>>(),
        [character]
    );
    assert_eq!(
        state.users.values().cloned().collect::<Vec<_>>(),
        [UserDto {
            id: UserId(v7(1)),
            name: "GM".into(),
            role: Role::Gm
        }]
    );
    db.project(&[record(
        3,
        Some(0x33),
        vec![Event::CharacterDeleted {
            id: CharacterId(v7(0x300)),
        }],
    )])
    .await
    .unwrap();
    let (deleted, _) = db.load_state().await.unwrap();
    assert!(deleted.characters.is_empty());
}

/// Participants joining a session become users (ADR-0022); joining again updates name and role.
/// A redacted placeholder is never stored, and the whole batch fails with it.
#[tokio::test]
async fn joined_users_are_projected_and_redacted_events_refused() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let mut mira = UserDto {
        id: UserId(v7(0x700)),
        name: "Mira".into(),
        role: Role::Player,
    };
    db.project(&[record(
        1,
        None,
        vec![Event::UserJoined { user: mira.clone() }],
    )])
    .await
    .unwrap();
    mira.name = "Mira the Bold".into();
    db.project(&[record(
        2,
        None,
        vec![Event::UserJoined { user: mira.clone() }],
    )])
    .await
    .unwrap();
    let (state, last) = db.load_state().await.unwrap();
    assert_eq!(state.users.get(&mira.id), Some(&mira));
    assert_eq!(state.users.len(), 2);
    assert_eq!(last, Seq(2));

    let refused = db
        .project(&[record(
            3,
            None,
            vec![
                Event::UserJoined {
                    user: UserDto {
                        id: UserId(v7(0x701)),
                        name: "Never".into(),
                        role: Role::Player,
                    },
                },
                Event::Redacted,
            ],
        )])
        .await;
    assert!(refused.is_err());
    let (state, last) = db.load_state().await.unwrap();
    assert_eq!((state.users.len(), last), (2, Seq(2)));
}

/// Plugins (ADR-0023): installed with their rules, replaced on reinstall, chosen as the system,
/// uninstalled; everything survives a reload.
#[tokio::test]
async fn plugins_and_the_system_are_projected_and_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let mut plugin = PluginDto {
        feature_settings: Default::default(),
        theme: None,
        id: "srd5".into(),
        version: "0.1.0".into(),
        kind: PluginKind::System,
        name: Label::Text("SRD".into()),
        license: "CC-BY-4.0".into(),
        attribution: Some("SRD 5.2".into()),
        archive_sha256: "a".repeat(64),
        trigger_templates: Vec::new(),
        system: Some(SystemRulesDto {
            mechanics: Default::default(),
            features: Default::default(),
            sheet_schema: serde_json::json!({ "type": "object", "properties": {} }),
            sheet_layout: serde_json::json!({ "type": "section", "children": [] }),
            creature_sheet_layout: None,
            computed: std::collections::BTreeMap::new(),
            initiative: "1d20".into(),
            resources: Vec::new(),
            health_thresholds: vec![75, 25, 0],
            rolls: Vec::new(),
            conditions: Vec::new(),
            inventory: None,
        }),
    };
    let theme = PluginDto {
        feature_settings: Default::default(),
        id: "dark".into(),
        kind: PluginKind::Theme,
        system: None,
        ..plugin.clone()
    };
    db.project(&[record(
        1,
        None,
        vec![
            Event::PluginInstalled {
                plugin: Box::new(plugin.clone()),
            },
            Event::PluginInstalled {
                plugin: Box::new(theme),
            },
            Event::SystemPluginSet {
                id: Some("srd5".into()),
            },
        ],
    )])
    .await
    .unwrap();
    plugin.version = "0.2.0".into();
    db.project(&[record(
        4,
        None,
        vec![
            Event::PluginInstalled {
                plugin: Box::new(plugin.clone()),
            },
            Event::PluginUninstalled { id: "dark".into() },
        ],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(
        state.plugins.values().cloned().collect::<Vec<_>>(),
        [plugin]
    );
    assert_eq!(state.system_plugin.as_deref(), Some("srd5"));

    db.project(&[record(6, None, vec![Event::SystemPluginSet { id: None }])])
        .await
        .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(state.system_plugin, None);
}

/// Journal projection: insert keeps `created_at`, updates move `updated_at`, delete removes.
#[tokio::test]
async fn journal_entries_are_projected_and_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let mut entry = JournalEntryDto {
        id: JournalEntryId(v7(0x500)),
        folder_id: None,
        title: "Gods".into(),
        markdown: "# Pantheon".into(),
        sort_order: 0,
        visibility: JournalVisibility::Players,
        players_can_edit: true,
        table: None,
        author: Some(UserId(v7(1))),
        version: Version(1),
    };
    let at = |mut r: BatchRecord, ms: i64| {
        r.at_ms = ms;
        r
    };
    db.project(&[at(
        record(
            1,
            Some(0x51),
            vec![Event::JournalEntryCreated {
                entry: entry.clone(),
            }],
        ),
        100,
    )])
    .await
    .unwrap();
    entry.markdown = "# Pantheon\n\nUpdated".into();
    entry.sort_order = 3;
    entry.version = Version(2);
    db.project(&[at(
        record(
            2,
            Some(0x52),
            vec![Event::JournalEntryUpdated {
                entry: entry.clone(),
            }],
        ),
        200,
    )])
    .await
    .unwrap();

    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(
        state.journal.values().cloned().collect::<Vec<_>>(),
        [entry.clone()]
    );
    let times: (i64, i64) = sqlx::query_as("SELECT created_at, updated_at FROM journal_entries")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(times, (100, 200));

    // A table entry keeps its cells (ADR-0026).
    let sheet = JournalEntryDto {
        id: JournalEntryId(v7(0x501)),
        title: "Prices".into(),
        markdown: String::new(),
        table: Some(JournalTable {
            columns: vec![TableColumn {
                name: "Item".into(),
                width: 160,
            }],
            rows: vec![vec!["Rope".into()], vec!["=A1".into()]],
        }),
        ..entry.clone()
    };
    db.project(&[record(
        3,
        Some(0x54),
        vec![Event::JournalEntryCreated {
            entry: sheet.clone(),
        }],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(state.journal.get(&sheet.id), Some(&sheet));

    db.project(&[record(
        4,
        Some(0x53),
        vec![Event::JournalEntryDeleted { id: entry.id }],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(state.journal.keys().collect::<Vec<_>>(), [&sheet.id]);
}

#[tokio::test]
async fn roll_statistics_filter_hidden_rolls_and_count_only_kept_d20_faces() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    assert!(db.roll_statistics(Role::Player).await.unwrap().is_empty());
    let mut first = chat(10, ChatKind::Roll);
    first.roll = Some(DiceRollDto {
        expression: "2d20kh1+5".into(),
        total: 25,
        terms: vec![DiceTermDto {
            negative: false,
            notation: "2d20kh1".into(),
            subtotal: 20,
            dice: vec![
                DieDto {
                    value: 1,
                    kept: false,
                },
                DieDto {
                    value: 20,
                    kept: true,
                },
            ],
        }],
    });
    let mut hidden = first.clone();
    hidden.id = ChatMessageId(v7(0x999));
    hidden.visibility = ChatVisibility::Gm;
    hidden.sender_name = "Renamed GM".into();
    hidden.roll = Some(DiceRollDto {
        expression: "d200".into(),
        total: 1,
        terms: vec![DiceTermDto {
            negative: false,
            notation: "d200".into(),
            subtotal: 1,
            dice: vec![DieDto {
                value: 1,
                kept: true,
            }],
        }],
    });
    db.project(&[record(
        1,
        None,
        vec![
            Event::ChatMessagePosted { message: first },
            Event::ChatMessagePosted { message: hidden },
        ],
    )])
    .await
    .unwrap();
    for role in [Role::Player, Role::Spectator] {
        let stats = db.roll_statistics(role).await.unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].rolls, 1);
        assert_eq!(stats[0].average, 25.0);
        assert_eq!(stats[0].natural_ones, 0);
        assert_eq!(stats[0].natural_twenties, 1);
        assert_eq!(stats[0].name, "GM");
    }
    for role in [Role::Gm, Role::CoGm] {
        let stats = db.roll_statistics(role).await.unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].rolls, 2);
        assert_eq!(stats[0].average, 13.0);
        assert_eq!(stats[0].natural_ones, 0);
        assert_eq!(stats[0].natural_twenties, 1);
        assert_eq!(stats[0].name, "Renamed GM");
    }
}

#[tokio::test]
async fn roll_statistics_keep_distinct_users_with_equal_names_and_count_natural_ones() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let user = UserDto {
        id: UserId(v7(2)),
        name: "GM".into(),
        role: Role::Player,
    };
    let mut a = chat(30, ChatKind::Roll);
    a.roll = Some(DiceRollDto {
        expression: "d20".into(),
        total: 1,
        terms: vec![DiceTermDto {
            negative: false,
            notation: "d20".into(),
            subtotal: 1,
            dice: vec![DieDto {
                value: 1,
                kept: true,
            }],
        }],
    });
    let mut b = chat(31, ChatKind::Roll);
    b.sender = Some(user.id);
    db.project(&[record(
        1,
        None,
        vec![
            Event::UserJoined { user },
            Event::ChatMessagePosted { message: a },
            Event::ChatMessagePosted { message: b },
        ],
    )])
    .await
    .unwrap();
    let stats = db.roll_statistics(Role::Player).await.unwrap();
    assert_eq!(stats.len(), 2);
    let gm = stats
        .iter()
        .find(|s| s.user_id == Some(UserId(v7(1))))
        .unwrap();
    assert_eq!(gm.natural_ones, 1);
    assert_eq!(gm.minimum, 1);
    assert_eq!(gm.maximum, 1);
    let other = stats
        .iter()
        .find(|s| s.user_id == Some(UserId(v7(2))))
        .unwrap();
    assert_eq!(other.natural_ones, 0);
    assert_eq!(other.average, 8.0);
}

fn chat(n: u128, kind: ChatKind) -> ChatMessageDto {
    ChatMessageDto {
        id: ChatMessageId(v7(0x600 + n)),
        kind,
        sender: (kind != ChatKind::System).then(|| UserId(v7(1))),
        sender_name: if kind == ChatKind::System {
            String::new()
        } else {
            "GM".into()
        },
        body: match kind {
            ChatKind::Text => format!("message {n}"),
            ChatKind::Roll => "2d6 + 1".into(),
            ChatKind::System => String::new(),
        },
        roll: (kind == ChatKind::Roll).then(|| DiceRollDto {
            expression: "2d6+1".into(),
            terms: vec![
                DiceTermDto {
                    negative: false,
                    notation: "2d6".into(),
                    subtotal: 7,
                    dice: vec![
                        DieDto {
                            value: 3,
                            kept: true,
                        },
                        DieDto {
                            value: 4,
                            kept: true,
                        },
                    ],
                },
                DiceTermDto {
                    negative: false,
                    notation: "1".into(),
                    subtotal: 1,
                    dice: vec![],
                },
            ],
            total: 8,
        }),
        system: (kind == ChatKind::System).then(|| SystemMessage::CharacterApproved {
            name: "Aria".into(),
        }),
        visibility: if n == 2 {
            ChatVisibility::Gm
        } else {
            ChatVisibility::All
        },
        at_ms: None,
        related_characters: Vec::new(),
    }
}

/// Chat messages, rolls (with `dice_rolls` rows) and system messages round-trip through the
/// database; history pages backwards from a message.
#[tokio::test]
async fn chat_messages_and_rolls_are_stored_and_paged() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let mut messages = [
        chat(1, ChatKind::Text),
        chat(2, ChatKind::Roll),
        chat(3, ChatKind::System),
        chat(4, ChatKind::Text),
    ];
    messages[2].related_characters = vec![CharacterId(v7(0xabc))];
    for (i, m) in messages.iter().enumerate() {
        let mut r = record(
            i64::try_from(i).unwrap() + 1,
            Some(0x70 + u128::try_from(i).unwrap()),
            vec![Event::ChatMessagePosted { message: m.clone() }],
        );
        r.at_ms = 1000 + i64::try_from(i).unwrap();
        db.project(&[r]).await.unwrap();
    }

    let with_time = |m: &ChatMessageDto, at: i64| ChatMessageDto {
        at_ms: Some(at),
        ..m.clone()
    };
    let all = db.chat_history(None, 10).await.unwrap();
    assert_eq!(
        all,
        messages
            .iter()
            .zip(1000..)
            .map(|(m, at)| with_time(m, at))
            .collect::<Vec<_>>()
    );
    // The newest page, then the one before it.
    let newest = db.chat_history(None, 2).await.unwrap();
    assert_eq!(
        newest.iter().map(|m| m.id).collect::<Vec<_>>(),
        [messages[2].id, messages[3].id]
    );
    let older = db.chat_history(Some(messages[2].id), 10).await.unwrap();
    assert_eq!(
        older.iter().map(|m| m.id).collect::<Vec<_>>(),
        [messages[0].id, messages[1].id]
    );

    let (total, chat_id): (i64, String) =
        sqlx::query_as("SELECT total, chat_message_id FROM dice_rolls")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!((total, chat_id), (8, messages[1].id.to_string()));
    // Visibility is immutable like the rest of the row (migration 0006).
    assert!(
        sqlx::query("UPDATE chat_messages SET visibility = 'all'")
            .execute(db.pool())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn history_filters_before_limit_for_every_role() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let mut messages = Vec::new();
    for n in 1..=220 {
        let mut message = chat(n, ChatKind::Text);
        message.visibility = if n % 100 == 1 {
            ChatVisibility::All
        } else {
            ChatVisibility::Gm
        };
        db.project(&[record(
            i64::try_from(n).unwrap(),
            None,
            vec![Event::ChatMessagePosted {
                message: message.clone(),
            }],
        )])
        .await
        .unwrap();
        messages.push(message);
    }
    for role in [Role::Gm, Role::CoGm, Role::Player, Role::Spectator] {
        let expected: Vec<_> = messages
            .iter()
            .filter(|m| {
                dnd_game_core::sees_everything(role) || dnd_game_core::chat_visible_to_players(m)
            })
            .rev()
            .take(2)
            .map(|m| m.id)
            .collect();
        let page = db.chat_history_for(None, 2, role).await.unwrap();
        assert_eq!(
            page.iter().rev().map(|m| m.id).collect::<Vec<_>>(),
            expected,
            "{role:?}"
        );
        let older = db
            .chat_history_for(Some(page[0].id), 200, role)
            .await
            .unwrap();
        assert!(
            older.iter().all(|m| dnd_game_core::sees_everything(role)
                || dnd_game_core::chat_visible_to_players(m))
        );
        assert!(db.chat_history_for(None, 0, role).await.unwrap().is_empty());
        assert!(
            db.chat_history_for(Some(ChatMessageId::new_v7()), 10, role)
                .await
                .unwrap()
                .is_empty()
        );
    }
    db.close().await.unwrap();
}

/// Assets round-trip with their tiles; the original hash is unique (UNIQUE(type, sha256)).
#[tokio::test]
async fn assets_are_registered_and_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let hex = |n: u8| format!("{n:02x}").repeat(32);
    let asset = AssetDto {
        id: AssetId(v7(0x700)),
        usage: AssetUsage::Map,
        sha256: hex(1),
        mime: "image/png".into(),
        size_bytes: 4321,
        original_name: Some("crypt.png".into()),
        width: 3000,
        height: 2048,
        display_sha256: None,
        thumbnail_sha256: hex(2),
        tiles: vec![AssetTileDto {
            x: 0,
            y: 0,
            width: 2048,
            height: 2048,
            sha256: hex(3),
        }],
        folder_id: None,
        version: Version(1),
    };
    db.project(&[record(
        1,
        Some(0x81),
        vec![Event::AssetRegistered {
            asset: Box::new(asset.clone()),
        }],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(
        state.assets.values().cloned().collect::<Vec<_>>(),
        std::slice::from_ref(&asset)
    );

    let duplicate = AssetDto {
        id: AssetId(v7(0x701)),
        ..asset
    };
    assert!(
        db.project(&[record(
            2,
            Some(0x82),
            vec![Event::AssetRegistered {
                asset: Box::new(duplicate),
            }]
        )])
        .await
        .is_err()
    );
}

/// Library folders and audio assets (ADR-0026): a sound loads back as audio; moving and renaming
/// are stored; deleting a folder returns its assets to the root (`ON DELETE SET NULL`); sounds
/// are never stored.
#[tokio::test]
async fn library_folders_and_sounds_are_projected() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let hex = |n: u8| format!("{n:02x}").repeat(32);
    let music = LibraryFolderDto {
        id: LibraryFolderId(v7(0x7a0)),
        library: Library::Audio,
        parent: None,
        name: "Music".into(),
        version: Version(1),
    };
    let battle = LibraryFolderDto {
        id: LibraryFolderId(v7(0x7a1)),
        parent: Some(music.id),
        name: "Battle".into(),
        ..music.clone()
    };
    let mut sound = AssetDto {
        id: AssetId(v7(0x7a2)),
        usage: AssetUsage::Audio,
        sha256: hex(9),
        mime: "audio/ogg".into(),
        size_bytes: 99,
        original_name: Some("drums.ogg".into()),
        width: 0,
        height: 0,
        display_sha256: Some(hex(9)),
        thumbnail_sha256: hex(9),
        tiles: Vec::new(),
        folder_id: None,
        version: Version(1),
    };
    db.project(&[record(
        1,
        Some(0x91),
        vec![
            Event::AssetRegistered {
                asset: Box::new(sound.clone()),
            },
            Event::LibraryFolderCreated {
                folder: music.clone(),
            },
            Event::LibraryFolderCreated {
                folder: battle.clone(),
            },
        ],
    )])
    .await
    .unwrap();
    sound.folder_id = Some(battle.id);
    sound.original_name = Some("War drums".into());
    sound.version = Version(2);
    db.project(&[record(
        4,
        Some(0x92),
        vec![
            Event::AssetUpdated {
                asset: Box::new(sound.clone()),
            },
            Event::SoundPlayed {
                sound: SoundDto {
                    asset_id: sound.id,
                    sha256: hex(9),
                    mime: "audio/ogg".into(),
                    volume: 50,
                    looped: false,
                    position_ms: 1234,
                    paused: true,
                    sting: false,
                },
            },
        ],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(state.assets.get(&sound.id), Some(&sound));
    assert_eq!(state.library_folders.len(), 2);
    assert!(state.sounds.is_empty(), "a campaign opens in silence");

    db.project(&[record(
        6,
        Some(0x93),
        vec![Event::LibraryFolderDeleted { id: music.id }],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert!(state.library_folders.is_empty(), "subfolders go with it");
    assert_eq!(state.assets[&sound.id].folder_id, None);
}

/// Fog (ADR-0019): operations are appended to `maps.fog` in order, a reset replaces them, a map
/// that never had fog loads without any, and fog of an unknown map changes nothing.
#[tokio::test]
async fn fog_is_appended_reset_and_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let (m0, m1) = (map(0, "Crypt", 1), map(1, "Chapel", 1));
    let op = |n: u128, kind: FogOpKind, shape: FogShape| FogOp {
        id: FogOpId(v7(0x900 + n)),
        kind,
        shape,
    };
    let reveal = op(
        1,
        FogOpKind::Reveal,
        FogShape::Rect {
            x: 0,
            y: 16,
            width: 160,
            height: 320,
        },
    );
    let hide = op(
        2,
        FogOpKind::Hide,
        FogShape::Brush {
            radius: 48,
            points: vec![10, 20, 30, 40],
        },
    );
    db.project(&[
        record(1, Some(0x91), vec![Event::MapCreated { map: m0.clone() }]),
        record(2, Some(0x92), vec![Event::MapCreated { map: m1.clone() }]),
        // No reset first: the map starts clear.
        record(
            3,
            Some(0x93),
            vec![Event::FogApplied {
                map_id: m0.id,
                op: reveal.clone(),
            }],
        ),
        record(
            4,
            Some(0x94),
            vec![Event::FogApplied {
                map_id: m0.id,
                op: hide.clone(),
            }],
        ),
        record(
            5,
            Some(0x95),
            vec![Event::FogApplied {
                map_id: MapId(v7(0x7777)),
                op: op(
                    3,
                    FogOpKind::Hide,
                    FogShape::Rect {
                        x: 0,
                        y: 0,
                        width: 16,
                        height: 16,
                    },
                ),
            }],
        ),
    ])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(
        state.fog.values().cloned().collect::<Vec<_>>(),
        vec![FogDto {
            map_id: m0.id,
            covered: false,
            ops: vec![reveal, hide],
        }]
    );

    db.project(&[record(
        6,
        Some(0x96),
        vec![Event::FogReset {
            map_id: m0.id,
            covered: true,
        }],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(
        state.fog.get(&m0.id),
        Some(&FogDto {
            map_id: m0.id,
            covered: true,
            ops: vec![],
        })
    );
    // Renaming the map keeps its fog (upsert_map does not touch the column).
    db.project(&[record(
        7,
        Some(0x97),
        vec![Event::MapUpdated {
            map: map(0, "Flooded crypt", 2),
        }],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert!(state.fog[&m0.id].covered);
    assert!(!state.fog.contains_key(&m1.id));
}

/// Combat (ADR-0020): combats and combatants round-trip with updates; deleting a token clears the
/// reference; ending the combat or deleting its map removes everything (FK cascades).
#[tokio::test]
async fn combats_and_combatants_are_projected_and_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let (m0, t0) = (map(0, "Crypt", 1), token(0, 0, 1));
    let combat = CombatDto {
        id: CombatId(v7(0xC01)),
        map_id: m0.id,
        round: 1,
        turn: None,
        version: Version(1),
    };
    let knight = CombatantDto {
        id: CombatantId(v7(0xC11)),
        combat_id: combat.id,
        token_id: Some(t0.id),
        character_id: None,
        name: "Knight".into(),
        initiative: None,
        sort_order: 0,
        version: Version(1),
    };
    let goblin = CombatantDto {
        id: CombatantId(v7(0xC12)),
        token_id: None,
        name: "Goblin".into(),
        initiative: Some(-2),
        sort_order: 1,
        ..knight.clone()
    };
    let rolled = CombatantDto {
        initiative: Some(17),
        version: Version(2),
        ..knight.clone()
    };
    let turned = CombatDto {
        round: 3,
        turn: Some(knight.id),
        version: Version(2),
        ..combat.clone()
    };
    db.project(&[
        record(1, Some(0xA1), vec![Event::MapCreated { map: m0.clone() }]),
        record(
            2,
            Some(0xA2),
            vec![Event::TokenCreated { token: t0.clone() }],
        ),
        record(
            3,
            Some(0xA3),
            vec![
                Event::CombatStarted {
                    combat: combat.clone(),
                },
                Event::CombatantAdded {
                    combatant: knight.clone(),
                },
                Event::CombatantAdded {
                    combatant: goblin.clone(),
                },
            ],
        ),
        record(
            6,
            Some(0xA4),
            vec![
                Event::CombatantUpdated {
                    combatant: rolled.clone(),
                },
                Event::CombatUpdated {
                    combat: turned.clone(),
                },
            ],
        ),
    ])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(
        state.combats.values().cloned().collect::<Vec<_>>(),
        vec![turned]
    );
    assert_eq!(
        state.combatants.values().cloned().collect::<Vec<_>>(),
        vec![rolled.clone(), goblin.clone()]
    );

    // Deleting the token keeps its combatant without it; removing a combatant deletes the row.
    db.project(&[
        record(8, Some(0xA5), vec![Event::TokenDeleted { id: t0.id }]),
        record(
            9,
            Some(0xA6),
            vec![Event::CombatantRemoved { id: goblin.id }],
        ),
    ])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(
        state.combatants.values().cloned().collect::<Vec<_>>(),
        vec![CombatantDto {
            token_id: None,
            ..rolled.clone()
        }]
    );

    // A second combat on the same map is refused by the schema too.
    let again = CombatDto {
        id: CombatId(v7(0xC02)),
        ..combat.clone()
    };
    assert!(
        db.project(&[record(
            10,
            Some(0xA7),
            vec![Event::CombatStarted { combat: again }]
        )])
        .await
        .is_err()
    );

    // Ending the combat removes its combatants.
    db.project(&[record(
        10,
        Some(0xA7),
        vec![Event::CombatEnded { id: combat.id }],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert!(state.combats.is_empty() && state.combatants.is_empty());

    // Deleting a map removes its combat with everything in it.
    db.project(&[
        record(
            11,
            Some(0xA8),
            vec![
                Event::CombatStarted {
                    combat: combat.clone(),
                },
                Event::CombatantAdded {
                    combatant: goblin.clone(),
                },
            ],
        ),
        record(13, Some(0xA9), vec![Event::MapDeleted { id: m0.id }]),
    ])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert!(state.combats.is_empty() && state.combatants.is_empty());
}

/// Journal folders (ADR-0021): created, renamed and moved; entries reference them; deleting a
/// folder cascades to its subfolders and entries.
#[tokio::test]
async fn journal_folders_are_projected_loaded_and_cascade() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let folder = |n: u128, parent: Option<u128>, name: &str| JournalFolderDto {
        id: JournalFolderId(v7(0x600 + n)),
        parent: parent.map(|p| JournalFolderId(v7(0x600 + p))),
        name: name.into(),
        sort_order: 0,
        version: Version(1),
    };
    let (lore, places) = (folder(0, None, "Lore"), folder(1, Some(0), "Places"));
    let entry = JournalEntryDto {
        id: JournalEntryId(v7(0x700)),
        folder_id: Some(places.id),
        title: "Harbour".into(),
        markdown: String::new(),
        sort_order: 0,
        visibility: JournalVisibility::Gm,
        players_can_edit: false,
        table: None,
        author: None,
        version: Version(1),
    };
    let root = JournalEntryDto {
        id: JournalEntryId(v7(0x701)),
        folder_id: None,
        ..entry.clone()
    };
    db.project(&[record(
        1,
        Some(0x61),
        vec![
            Event::JournalFolderCreated {
                folder: lore.clone(),
            },
            Event::JournalFolderCreated {
                folder: places.clone(),
            },
            Event::JournalEntryCreated {
                entry: entry.clone(),
            },
            Event::JournalEntryCreated {
                entry: root.clone(),
            },
        ],
    )])
    .await
    .unwrap();
    let renamed = JournalFolderDto {
        name: "Regions".into(),
        parent: None,
        sort_order: 4,
        version: Version(2),
        ..places.clone()
    };
    db.project(&[record(
        5,
        Some(0x62),
        vec![Event::JournalFolderUpdated {
            folder: renamed.clone(),
        }],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(
        state.journal_folders.values().cloned().collect::<Vec<_>>(),
        vec![lore.clone(), renamed.clone()]
    );
    assert_eq!(state.journal[&entry.id].folder_id, Some(places.id));

    // An entry cannot point at a missing folder.
    let orphan = JournalEntryDto {
        id: JournalEntryId(v7(0x702)),
        folder_id: Some(JournalFolderId(v7(0x6FF))),
        ..entry.clone()
    };
    assert!(
        db.project(&[record(
            6,
            Some(0x63),
            vec![Event::JournalEntryCreated { entry: orphan }]
        )])
        .await
        .is_err()
    );

    // Deleting a folder takes its entries; the root entry stays.
    db.project(&[record(
        6,
        Some(0x64),
        vec![Event::JournalFolderDeleted { id: renamed.id }],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(
        state.journal_folders.keys().copied().collect::<Vec<_>>(),
        vec![lore.id]
    );
    assert_eq!(
        state.journal.keys().copied().collect::<Vec<_>>(),
        vec![root.id]
    );
}

/// Boards (ADR-0024): boards, nodes and edges are stored as their DTOs, moved in place, loaded
/// back, and deleting a node or a board cascades.
#[tokio::test]
async fn boards_are_projected_loaded_and_cascade() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let board = BoardDto {
        id: BoardId(v7(0x800)),
        name: "Plot".into(),
        sort_order: 0,
        visible_to_players: false,
        version: Version(1),
    };
    let node = |n: u128, text: &str| BoardNodeDto {
        id: BoardNodeId(v7(0x810 + n)),
        board_id: board.id,
        content: BoardNodeContent::Note {
            markdown: text.into(),
        },
        x: 0,
        y: 0,
        width: 3200,
        height: 1600,
        visibility: BoardVisibility::Gm,
        version: Version(1),
    };
    let (a, b, c) = (node(0, "Mayor"), node(1, "Cult"), node(2, "Crypt"));
    let edge = |n: u128, from: &BoardNodeDto, to: &BoardNodeDto| BoardEdgeDto {
        id: BoardEdgeId(v7(0x820 + n)),
        board_id: board.id,
        from: from.id,
        to: to.id,
        label: "knows".into(),
        directed: true,
        visibility: BoardVisibility::Players,
        version: Version(1),
    };
    let (ab, bc) = (edge(0, &a, &b), edge(1, &b, &c));
    let mut events = vec![Event::BoardCreated {
        board: board.clone(),
    }];
    events.extend([&a, &b, &c].map(|n| Event::BoardNodeCreated { node: n.clone() }));
    events.extend([&ab, &bc].map(|e| Event::BoardEdgeCreated { edge: e.clone() }));
    db.project(&[record(1, Some(0x81), events)]).await.unwrap();

    let shown = BoardDto {
        name: "Main plot".into(),
        visible_to_players: true,
        version: Version(2),
        ..board.clone()
    };
    let opened = BoardNodeDto {
        visibility: BoardVisibility::Players,
        version: Version(2),
        ..a.clone()
    };
    let relabelled = BoardEdgeDto {
        label: "fears".into(),
        version: Version(2),
        ..ab.clone()
    };
    db.project(&[record(
        10,
        Some(0x82),
        vec![
            Event::BoardUpdated {
                board: shown.clone(),
            },
            Event::BoardNodeUpdated {
                node: opened.clone(),
            },
            Event::BoardNodeMoved {
                id: opened.id,
                x: -480,
                y: 960,
            },
            Event::BoardEdgeUpdated {
                edge: relabelled.clone(),
            },
        ],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(state.boards[&board.id], shown);
    assert_eq!(
        state.board_nodes[&a.id],
        BoardNodeDto {
            x: -480,
            y: 960,
            ..opened
        }
    );
    assert_eq!(state.board_edges[&ab.id], relabelled);

    // Deleting a node takes its edges; deleting an edge, a board and its rest.
    db.project(&[record(
        20,
        Some(0x83),
        vec![
            Event::BoardNodeDeleted { id: c.id },
            Event::BoardEdgeDeleted { id: ab.id },
        ],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(state.board_nodes.len(), 2);
    assert!(state.board_edges.is_empty());
    db.project(&[record(
        30,
        Some(0x84),
        vec![Event::BoardDeleted { id: board.id }],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert!(state.boards.is_empty() && state.board_nodes.is_empty());

    // An edge cannot point at a missing node.
    db.project(&[record(
        40,
        Some(0x85),
        vec![Event::BoardCreated {
            board: board.clone(),
        }],
    )])
    .await
    .unwrap();
    assert!(
        db.project(&[record(
            41,
            Some(0x86),
            vec![Event::BoardEdgeCreated { edge: bc }]
        )])
        .await
        .is_err()
    );
}

/// Zones (ADR-0024): stored as their DTO, updated, loaded, and gone with their map.
#[tokio::test]
async fn zones_are_projected_loaded_and_go_with_their_map() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let map = map(0, "Crypt", 1);
    let zone = ZoneDto {
        occlusion: Default::default(),
        id: ZoneId(v7(0x900)),
        map_id: map.id,
        name: "Altar".into(),
        shape: ZoneShape::Polygon {
            points: vec![0, 0, 160, 0, 80, 160],
        },
        label: None,
        color: 0x9b7bff,
        version: Version(1),
    };
    db.project(&[record(
        1,
        Some(0x91),
        vec![
            Event::MapCreated { map: map.clone() },
            Event::ZoneCreated { zone: zone.clone() },
        ],
    )])
    .await
    .unwrap();
    let moved = ZoneDto {
        name: "Old altar".into(),
        occlusion: ZoneOcclusion {
            sight: true,
            effect: false,
        },
        color: 0x12abef,
        shape: ZoneShape::Rect {
            x: 16,
            y: 32,
            width: 160,
            height: 320,
        },
        label: Some(ZoneLabel { x: 48, y: 64 }),
        version: Version(2),
        ..zone.clone()
    };
    db.project(&[record(
        3,
        Some(0x92),
        vec![Event::ZoneUpdated {
            zone: moved.clone(),
        }],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(
        state.zones.values().cloned().collect::<Vec<_>>(),
        vec![moved]
    );

    db.project(&[record(
        4,
        Some(0x93),
        vec![Event::ZoneDeleted { id: zone.id }],
    )])
    .await
    .unwrap();
    assert!(db.load_state().await.unwrap().0.zones.is_empty());
    db.project(&[record(
        5,
        Some(0x94),
        vec![
            Event::ZoneCreated { zone: zone.clone() },
            Event::MapDeleted { id: map.id },
        ],
    )])
    .await
    .unwrap();
    assert!(db.load_state().await.unwrap().0.zones.is_empty());
}

/// Triggers (ADR-0024): stored as their DTO, updated (fired), loaded, deleted; a firing is only
/// logged.
#[tokio::test]
async fn triggers_are_projected_loaded_and_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let trigger = TriggerDto {
        fired_maps: Vec::new(),
        id: TriggerId(v7(0xA00)),
        definition: TriggerDef {
            map_ids: Vec::new(),
            name: "Secret passage".into(),
            enabled: true,
            once: true,
            event: TriggerEvent::Manual,
            tokens: Vec::new(),
            condition: Some("@total >= 15".into()),
            steps: vec![TriggerStep {
                when: None,
                action: TriggerAction::Chat {
                    text: "A draft from the wall.".into(),
                    gm_only: false,
                },
            }],
        },
        fired: false,
        version: Version(1),
    };
    db.project(&[record(
        1,
        Some(0xA1),
        vec![Event::TriggerCreated {
            trigger: trigger.clone(),
        }],
    )])
    .await
    .unwrap();
    let spent = TriggerDto {
        fired_maps: Vec::new(),
        fired: true,
        version: Version(2),
        ..trigger.clone()
    };
    db.project(&[record(
        2,
        Some(0xA2),
        vec![
            Event::TriggerFired {
                fired: TriggerFiredDto {
                    map_id: None,
                    trigger: trigger.id,
                    name: "Secret passage".into(),
                    character: None,
                    token: None,
                },
            },
            Event::TriggerUpdated {
                trigger: spent.clone(),
            },
        ],
    )])
    .await
    .unwrap();
    let (state, seq) = db.load_state().await.unwrap();
    assert_eq!(
        state.triggers.values().cloned().collect::<Vec<_>>(),
        vec![spent]
    );
    assert_eq!(seq, Seq(3));
    db.project(&[record(
        4,
        Some(0xA3),
        vec![Event::TriggerDeleted { id: trigger.id }],
    )])
    .await
    .unwrap();
    assert!(db.load_state().await.unwrap().0.triggers.is_empty());
}

/// Token effects (ADR-0024): stored as their DTO, updated (countdown), loaded, removed.
#[tokio::test]
async fn effects_are_projected_loaded_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let effect = TokenEffectDto {
        id: EffectId(v7(0xE10)),
        token: TokenId(v7(0xE11)),
        label: "Poison".into(),
        condition: Some("poisoned".into()),
        resource: Some("hp".into()),
        dice: Some("3".into()),
        heal: false,
        reveal: Vec::new(),
        duration: EffectDuration::Turns { remaining: 3 },
        version: Version(1),
    };
    db.project(&[record(
        1,
        Some(0xE1),
        vec![Event::TokenEffectApplied {
            effect: effect.clone(),
        }],
    )])
    .await
    .unwrap();
    let spent = TokenEffectDto {
        duration: EffectDuration::Turns { remaining: 2 },
        version: Version(2),
        ..effect.clone()
    };
    db.project(&[record(
        2,
        Some(0xE2),
        vec![Event::TokenEffectUpdated {
            effect: spent.clone(),
        }],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(
        state.effects.values().cloned().collect::<Vec<_>>(),
        vec![spent]
    );
    db.project(&[record(
        3,
        Some(0xE3),
        vec![Event::TokenEffectRemoved {
            id: effect.id,
            expired: true,
        }],
    )])
    .await
    .unwrap();
    assert!(db.load_state().await.unwrap().0.effects.is_empty());
}

/// Effects die with their token — directly (token deleted) and through the map cascade (map
/// deleted): `token_effects` has no foreign key, the projection removes them explicitly, like
/// the core's `evolve`.
#[tokio::test]
async fn effects_go_away_with_their_token_and_map() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let effect_on = |n: u128, token_n: u128| TokenEffectDto {
        id: EffectId(v7(n)),
        token: TokenId(v7(0x200 + token_n)),
        label: "Mark".into(),
        reveal: Vec::new(),
        condition: None,
        resource: None,
        dice: None,
        heal: false,
        duration: EffectDuration::Turns { remaining: 3 },
        version: Version(1),
    };
    db.project(&[
        record(
            1,
            Some(1),
            vec![Event::MapCreated {
                map: map(1, "One", 1),
            }],
        ),
        record(
            2,
            Some(2),
            vec![Event::MapCreated {
                map: map(2, "Two", 1),
            }],
        ),
        record(
            3,
            Some(3),
            vec![Event::TokenCreated {
                token: token(1, 1, 1),
            }],
        ),
        record(
            4,
            Some(4),
            vec![Event::TokenCreated {
                token: token(2, 2, 1),
            }],
        ),
        record(
            5,
            Some(5),
            vec![Event::TokenEffectApplied {
                effect: effect_on(0xE10, 1),
            }],
        ),
        record(
            6,
            Some(6),
            vec![Event::TokenEffectApplied {
                effect: effect_on(0xE11, 2),
            }],
        ),
    ])
    .await
    .unwrap();
    // The map's delete cascades its token, and the effect goes with it.
    db.project(&[record(
        7,
        Some(7),
        vec![Event::MapDeleted {
            id: MapId(v7(0x102)),
        }],
    )])
    .await
    .unwrap();
    assert_eq!(
        db.load_state()
            .await
            .unwrap()
            .0
            .effects
            .values()
            .cloned()
            .collect::<Vec<_>>(),
        vec![effect_on(0xE10, 1)]
    );
    db.project(&[record(
        8,
        Some(8),
        vec![Event::TokenDeleted {
            id: TokenId(v7(0x201)),
        }],
    )])
    .await
    .unwrap();
    assert!(db.load_state().await.unwrap().0.effects.is_empty());
}

#[tokio::test]
async fn image_presentations_are_projected_loaded_and_cleared() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    assert!(db.load_state().await.unwrap().0.presented_image.is_none());
    let presentation = dnd_protocol::model::PresentedImageDto {
        id: IntentId(v7(0xFA01)),
        asset_id: AssetId(v7(0xFA02)),
        version: Version(1),
    };
    db.project(&[record(
        1,
        None,
        vec![Event::ImagePresentationChanged {
            presentation: Some(presentation.clone()),
        }],
    )])
    .await
    .unwrap();
    assert_eq!(
        db.load_state().await.unwrap().0.presented_image,
        Some(presentation)
    );
    db.project(&[record(
        2,
        None,
        vec![Event::ImagePresentationChanged { presentation: None }],
    )])
    .await
    .unwrap();
    assert!(db.load_state().await.unwrap().0.presented_image.is_none());
}

/// Roll requests (trigger manual rolls) are projected, loaded and updated in place.
#[tokio::test]
async fn roll_requests_are_projected_loaded_and_updated() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let request = RollRequestDto {
        id: RollRequestId(v7(0xE20)),
        label: "Poisoned dart".into(),
        trigger: Some(TriggerId(v7(0xE21))),
        group: None,
        character: CharacterId(v7(0xE22)),
        token: Some(TokenId(v7(0xE23))),
        dice: "1d20 + 5".into(),
        hit: Some("@total >= @ac".into()),
        resource: "hp".into(),
        damage_dice: "2d6".into(),
        heal: false,
        status: RollRequestStatus::Pending,
        total: None,
        delta: None,
        version: Version(1),
    };
    let mut legacy = serde_json::to_value(&request).unwrap();
    legacy.as_object_mut().unwrap().remove("group");
    assert_eq!(
        serde_json::from_value::<RollRequestDto>(legacy).unwrap(),
        request
    );
    db.project(&[record(
        1,
        Some(0xE2),
        vec![Event::RollRequested {
            request: request.clone(),
        }],
    )])
    .await
    .unwrap();
    let rolled = RollRequestDto {
        status: RollRequestStatus::Rolled,
        total: Some(17),
        delta: Some(-7),
        version: Version(2),
        ..request.clone()
    };
    db.project(&[record(
        2,
        Some(0xE4),
        vec![Event::RollRequestUpdated {
            request: rolled.clone(),
        }],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(
        state.roll_requests.values().cloned().collect::<Vec<_>>(),
        vec![rolled]
    );
}

#[tokio::test]
async fn plugin_runtime_is_projected_and_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let runtime = PluginRuntime {
        saves: vec![PendingSaves {
            id: IntentId(v7(0x6650)),
            map: MapId(v7(0x6651)),
            source: None,
            owner: CharacterId(v7(0x6652)),
            damage: 28,
            difficulty: 15,
            action: serde_json::from_value(
                serde_json::from_str::<serde_json::Value>(include_str!(
                    "../../../plugins/system-srd5/system.json"
                ))
                .unwrap()["mechanics"]["actions"][2]
                    .clone(),
            )
            .unwrap(),
            targets: vec![SaveTarget {
                token: TokenId(v7(0x6653)),
                character: CharacterId(v7(0x6654)),
                status: SaveStatus::Pending,
                total: None,
            }],
        }],
        trigger_runs: vec![PendingTriggerRun {
            rolls: vec![RollRequestId(v7(0x7777))],
            action: IntentId(v7(0x9920)),
            trigger: TriggerId(v7(0x9930)),
            version: Version::INITIAL,
            map: Some(MapId(v7(0x9921))),
            character: Some(CharacterId(v7(0x9924))),
            token: Some(TokenId(v7(0x9923))),
            next_step: 1,
            variables: std::collections::BTreeMap::from([("total".into(), 17)]),
        }],
        action_stacks: vec![ActionStack {
            id: IntentId(v7(0x9920)),
            map: MapId(v7(0x9921)),
            frames: vec![PendingAction {
                definition: None,
                awaiting_reactions: true,
                id: IntentId(v7(0x9922)),
                source: TokenId(v7(0x9923)),
                owner: CharacterId(v7(0x9924)),
                controller: None,
                action: "casting".into(),
                label: Label::Text("Casting".into()),
                target: None,
                x: 1120,
                y: 2240,
                deferred: vec![DeferredCost {
                    resource: "slot".into(),
                    amount: 1,
                }],
                offers: Vec::new(),
                ready: true,
            }],
        }],
        maintained: vec![MaintainedEffect {
            id: EffectId(v7(0x9910)),
            owner: CharacterId(v7(0x9911)),
            source: TokenId(v7(0x9912)),
            rule: "maintain".into(),
            label: Label::Text("A maintained effect".into()),
            effects: vec![EffectId(v7(0x9913))],
        }],
        visuals: Vec::new(),
        movements: vec![PendingMovement {
            id: IntentId(v7(0x9900)),
            token: TokenId(v7(0x9901)),
            destination_x: 1120,
            destination_y: 2240,
            mode: MovementMode::Voluntary,
            handled: vec!["first:source".into()],
            offers: vec![ReactionOffer {
                ready: true,
                source: TokenId(v7(0x9902)),
                rule: "leave_reach".into(),
                action: "strike".into(),
                label: Label::Text("React".into()),
            }],
        }],
    };
    db.project(&[record(
        1,
        None,
        vec![Event::PluginRuntimeUpdated {
            runtime: Box::new(runtime.clone()),
        }],
    )])
    .await
    .unwrap();
    assert_eq!(db.load_state().await.unwrap().0.plugin_runtime, runtime);
    db.project(&[record(
        2,
        None,
        vec![Event::PluginRuntimeUpdated {
            runtime: Box::default(),
        }],
    )])
    .await
    .unwrap();
    assert_eq!(
        db.load_state().await.unwrap().0.plugin_runtime,
        PluginRuntime::default()
    );
}

#[tokio::test]
async fn item_instances_survive_reload_updates_and_deletion() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let mut item = ItemDto {
        id: ItemId(v7(0x2210)),
        kind: ItemKind::Item,
        name: "Wand".into(),
        type_id: Some("equipment".into()),
        is_template: false,
        template: Some(ItemId(v7(0x2211))),
        owner: None,
        visible_to_players: false,
        description: String::new(),
        state: None,
        effects: Vec::new(),
        grants: Vec::new(),
        values: Vec::new(),
        sheet_json: r#"{"charges":3,"state":"ready"}"#.into(),
        version: Version(1),
    };
    db.project(&[record(
        1,
        Some(0x2212),
        vec![Event::ItemCreated { item: item.clone() }],
    )])
    .await
    .unwrap();
    assert_eq!(db.load_state().await.unwrap().0.items[&item.id], item);
    item.sheet_json = r#"{"charges":2,"state":"used"}"#.into();
    item.version = Version(2);
    db.project(&[record(
        2,
        Some(0x2213),
        vec![Event::ItemUpdated { item: item.clone() }],
    )])
    .await
    .unwrap();
    assert_eq!(db.load_state().await.unwrap().0.items[&item.id], item);
    db.project(&[record(
        3,
        Some(0x2214),
        vec![Event::ItemDeleted { id: item.id }],
    )])
    .await
    .unwrap();
    assert!(db.load_state().await.unwrap().0.items.is_empty());
}

/// ADR-0061: the campaign policy lives in `campaign_meta` (absent at its base value) and a token
/// override lives in `tokens.inspector`; both survive a reload and both clear back to the default.
#[tokio::test]
async fn token_inspector_policy_is_projected_and_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let policy = TokenInspectorPolicy {
        reveal: vec![TokenField::Effects, TokenField::Name],
        conceal: vec![TokenField::Name],
    };
    let campaign = CampaignMeta {
        game_clock: Default::default(),
        name: "Queries".into(),
        description: String::new(),
        token_inspector: policy.clone(),
        item_states: Vec::new(),
        visibility: Default::default(),
        version: Version(2),
    };
    let mut tok = token(1, 0, 1);
    tok.visible = true;
    tok.inspector = Some(TokenInspectorPolicy {
        reveal: vec![TokenField::Position],
        conceal: vec![TokenField::Size],
    });
    let stored = map(0, "Crypt", 1);
    db.project(&[record(
        1,
        Some(0xA),
        vec![
            Event::MapCreated {
                map: stored.clone(),
            },
            Event::TokenCreated { token: tok.clone() },
            Event::CampaignMetaUpdated {
                campaign: campaign.clone(),
            },
        ],
    )])
    .await
    .unwrap();

    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(state.campaign.token_inspector, policy);
    assert_eq!(
        state.tokens[&tok.id].inspector,
        Some(TokenInspectorPolicy {
            reveal: vec![TokenField::Position],
            conceal: vec![TokenField::Size],
        })
    );

    // The base policy is stored as the absence of the key, and a token follows the campaign again.
    let mut base = campaign.clone();
    base.token_inspector = TokenInspectorPolicy::base();
    base.version = Version(3);
    let mut cleared = tok.clone();
    cleared.inspector = None;
    cleared.version = Version(2);
    db.project(&[record(
        4,
        Some(0xB),
        vec![
            Event::CampaignMetaUpdated { campaign: base },
            Event::TokenUpdated {
                token: cleared.clone(),
            },
        ],
    )])
    .await
    .unwrap();
    let key: Option<String> = sqlx::query_scalar("SELECT value FROM campaign_meta WHERE key = ?")
        .bind("campaign.token_inspector")
        .fetch_optional(db.pool())
        .await
        .unwrap();
    assert!(key.is_none(), "the base policy adds no key");
    let (state, _) = db.load_state().await.unwrap();
    assert!(state.campaign.token_inspector.is_base());
    assert!(state.tokens[&tok.id].inspector.is_none());
}

/// ADR-0062: item states, effects and grants live in the item DTO; campaign item states live in
/// `campaign_meta` (the key is absent while the built-in set is in use). Both survive a reload.
#[tokio::test]
async fn item_states_and_effects_are_projected_and_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let custom = vec![dnd_protocol::ItemStateDef {
        id: "sealed".into(),
        label: Label::Text("Sealed".into()),
        active: true,
    }];
    let campaign = CampaignMeta {
        game_clock: Default::default(),
        name: "Queries".into(),
        description: String::new(),
        token_inspector: dnd_protocol::TokenInspectorPolicy::base(),
        item_states: custom.clone(),
        visibility: Default::default(),
        version: Version(2),
    };
    let item = ItemDto {
        id: ItemId(v7(0x2210)),
        kind: ItemKind::Item,
        name: "Ring".into(),
        type_id: None,
        is_template: false,
        template: None,
        owner: None,
        visible_to_players: false,
        description: "A chipped ring.".into(),
        state: Some("sealed".into()),
        effects: vec![ItemEffect {
            path: "ac".into(),
            add: 2,
            minimum: Some(0),
            maximum: Some(50),
        }],
        grants: vec!["poisoned".into()],
        values: Vec::new(),
        sheet_json: "{}".into(),
        version: Version(1),
    };
    db.project(&[record(
        1,
        Some(0xC),
        vec![
            Event::CampaignMetaUpdated {
                campaign: campaign.clone(),
            },
            Event::ItemCreated { item: item.clone() },
        ],
    )])
    .await
    .unwrap();

    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(state.campaign.item_states, custom);
    assert_eq!(state.items[&item.id], item);

    // Back to the built-in states: the key goes away and old items keep their own fields.
    let mut base = campaign;
    base.item_states = Vec::new();
    base.version = Version(3);
    db.project(&[record(
        4,
        Some(0xD),
        vec![Event::CampaignMetaUpdated { campaign: base }],
    )])
    .await
    .unwrap();
    let key: Option<String> = sqlx::query_scalar("SELECT value FROM campaign_meta WHERE key = ?")
        .bind("campaign.item_states")
        .fetch_optional(db.pool())
        .await
        .unwrap();
    assert!(key.is_none(), "the built-in set adds no key");
    let (state, _) = db.load_state().await.unwrap();
    assert!(state.campaign.item_states.is_empty());
    assert_eq!(state.campaign.item_states(), ItemStateDef::builtin());
    assert_eq!(state.items[&item.id].state, Some("sealed".into()));
}

/// ADR-0063: the GM's visibility switches live in `campaign_meta` (the key is absent while they
/// match the system defaults) and survive a reload.
#[tokio::test]
async fn visibility_policy_is_projected_and_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let policy = VisibilityPolicy {
        sheet: vec![FieldVisibility {
            path: "hp".into(),
            visible: true,
        }],
        name: Some(false),
        icon: Some(true),
    };
    let campaign = CampaignMeta {
        game_clock: Default::default(),
        name: "Queries".into(),
        description: String::new(),
        token_inspector: dnd_protocol::TokenInspectorPolicy::base(),
        item_states: Vec::new(),
        visibility: policy.clone(),
        version: Version(2),
    };
    db.project(&[record(
        1,
        Some(0xE),
        vec![Event::CampaignMetaUpdated { campaign }],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(state.campaign.visibility, policy);

    // Back to the defaults: the key goes away.
    let mut base = CampaignMeta {
        game_clock: Default::default(),
        name: "Queries".into(),
        description: String::new(),
        token_inspector: dnd_protocol::TokenInspectorPolicy::base(),
        item_states: Vec::new(),
        visibility: VisibilityPolicy::default(),
        version: Version(3),
    };
    base.version = Version(4);
    db.project(&[record(
        4,
        Some(0xF),
        vec![Event::CampaignMetaUpdated { campaign: base }],
    )])
    .await
    .unwrap();
    let key: Option<String> = sqlx::query_scalar("SELECT value FROM campaign_meta WHERE key = ?")
        .bind("campaign.visibility")
        .fetch_optional(db.pool())
        .await
        .unwrap();
    assert!(key.is_none(), "defaults add no key");
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(state.campaign.visibility, VisibilityPolicy::default());
}
