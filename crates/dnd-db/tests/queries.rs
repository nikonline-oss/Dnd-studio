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
        visible: false,
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
        name: "Renamed".into(),
        description: "Desc".into(),
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
    ];
    let count: usize = sources
        .iter()
        .map(|s| s.matches("sqlx::query").count())
        .sum();
    assert_eq!(
        count, 30,
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
}

/// Journal projection: insert keeps `created_at`, updates move `updated_at`, delete removes.
#[tokio::test]
async fn journal_entries_are_projected_and_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let mut entry = JournalEntryDto {
        id: JournalEntryId(v7(0x500)),
        folder: "Lore".into(),
        title: "Gods".into(),
        markdown: "# Pantheon".into(),
        sort_order: 0,
        visibility: JournalVisibility::Players,
        players_can_edit: true,
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
    entry.folder = "Lore/Old".into();
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

    db.project(&[record(
        3,
        Some(0x53),
        vec![Event::JournalEntryDeleted { id: entry.id }],
    )])
    .await
    .unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert!(state.journal.is_empty());
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
    }
}

/// Chat messages, rolls (with `dice_rolls` rows) and system messages round-trip through the
/// database; history pages backwards from a message.
#[tokio::test]
async fn chat_messages_and_rolls_are_stored_and_paged() {
    let dir = tempfile::tempdir().unwrap();
    let db = new_campaign(&dir).await;
    let messages = [
        chat(1, ChatKind::Text),
        chat(2, ChatKind::Roll),
        chat(3, ChatKind::System),
        chat(4, ChatKind::Text),
    ];
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
