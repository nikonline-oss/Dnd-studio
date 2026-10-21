//! A campaign file: open/create, load state, project events.

use std::path::{Path, PathBuf};

use dnd_game_core::GameState;
use dnd_protocol::{
    AssetDto, AssetId, AssetTileDto, CampaignMeta, CharacterDto, CharacterId, ChatKind,
    ChatMessageDto, ChatMessageId, DiceRollDto, Event, EventBatch, Grid, GridKind, IntentId,
    JournalEntryDto, JournalEntryId, MapDto, MapId, PayloadHash, Role, Seq, SystemMessage,
    TokenDto, TokenId, TokenLayer, UserDto, UserId, Version,
};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::backup::create_backup;
use crate::codec::{enum_from_str, enum_to_str, opt_uuid_from_str, uuid_from_str};
use crate::connect::{CAMPAIGN_MIGRATOR, has_pending_migrations, migrate, pool, quick_check};
use crate::error::{DbError, DbResult};

const META_NAME: &str = "campaign.name";
const META_DESCRIPTION: &str = "campaign.description";
const META_VERSION: &str = "campaign.version";
const META_LOCAL_USER: &str = "local.user_id";

/// The user that owns the campaign on this machine (the GM in local play).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalUser {
    pub id: UserId,
    pub name: String,
    pub role: Role,
}

/// One accepted batch of events, as written to `event_log`.
#[derive(Clone, Debug, PartialEq)]
pub struct BatchRecord {
    pub batch: EventBatch,
    /// Hash of the intent payload (`IntentEnvelope::payload_hash`), if the batch has an intent.
    pub intent_hash: Option<PayloadHash>,
    pub actor: Option<UserId>,
    pub at_ms: i64,
}

/// An accepted intent as recorded in `event_log`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentIntent {
    pub intent_id: IntentId,
    /// `None` for rows written before payload hashes were stored (migration 0004).
    pub intent_hash: Option<PayloadHash>,
    /// Sequence number of the last event of its batch.
    pub last_seq: Seq,
    /// When it was recorded (unix ms).
    pub at_ms: i64,
}

/// What happened while opening a campaign.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenReport {
    /// Backup taken because migrations were pending.
    pub pre_migration_backup: Option<PathBuf>,
}

/// An open campaign file.
#[derive(Debug, Clone)]
pub struct CampaignDb {
    pool: SqlitePool,
    path: PathBuf,
}

impl CampaignDb {
    /// Creates a new campaign file. Fails if `path` already exists.
    pub async fn create(path: &Path, name: &str, gm: &LocalUser, now_ms: i64) -> DbResult<Self> {
        if path.exists() {
            return Err(DbError::AlreadyExists(path.to_owned()));
        }
        let pool = pool(path, true).await?;
        migrate(&pool, &CAMPAIGN_MIGRATOR).await?;

        let mut tx = pool.begin().await?;
        let state = GameState::new(name);
        write_campaign_meta(&mut tx, &state.campaign).await?;
        set_meta(&mut tx, META_LOCAL_USER, &gm.id.to_string()).await?;
        sqlx::query("INSERT INTO users (id, name, role, created_at) VALUES (?, ?, ?, ?)")
            .bind(gm.id.to_string())
            .bind(&gm.name)
            .bind(enum_to_str(&gm.role)?)
            .bind(now_ms)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        Ok(Self {
            pool,
            path: path.to_owned(),
        })
    }

    /// Opens an existing campaign: integrity check, backup before pending migrations, migrate.
    pub async fn open(path: &Path, now_ms: i64) -> DbResult<(Self, OpenReport)> {
        quick_check(path).await?;
        let pool = pool(path, false).await?;
        let mut report = OpenReport::default();
        let prepared = async {
            if has_pending_migrations(&pool, &CAMPAIGN_MIGRATOR).await? {
                report.pre_migration_backup =
                    Some(create_backup(&pool, path, "pre-migration", now_ms).await?);
                migrate(&pool, &CAMPAIGN_MIGRATOR).await?;
            }
            Ok::<(), DbError>(())
        }
        .await;
        if let Err(e) = prepared {
            // Release the file before returning (Windows keeps it locked otherwise).
            pool.close().await;
            return Err(e);
        }
        Ok((
            Self {
                pool,
                path: path.to_owned(),
            },
            report,
        ))
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Takes a backup now.
    pub async fn backup(&self, reason: &str, now_ms: i64) -> DbResult<PathBuf> {
        create_backup(&self.pool, &self.path, reason, now_ms).await
    }

    /// Moves WAL contents into the main file (periodic flush / before closing).
    pub async fn checkpoint(&self) -> DbResult<()> {
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Checkpoints and closes all connections.
    pub async fn close(self) -> DbResult<()> {
        self.checkpoint().await?;
        self.pool.close().await;
        Ok(())
    }

    /// The campaign owner on this machine.
    pub async fn local_user(&self) -> DbResult<Option<LocalUser>> {
        let Some(id) = get_meta(&self.pool, META_LOCAL_USER).await? else {
            return Ok(None);
        };
        let row = sqlx::query("SELECT id, name, role FROM users WHERE id = ?")
            .bind(&id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|r| {
            Ok(LocalUser {
                id: UserId(uuid_from_str("users.id", &r.try_get::<String, _>("id")?)?),
                name: r.try_get("name")?,
                role: enum_from_str("users.role", &r.try_get::<String, _>("role")?)?,
            })
        })
        .transpose()
    }

    /// Sequence number of the last stored event (0 if none).
    pub async fn last_seq(&self) -> DbResult<Seq> {
        let seq: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM event_log")
            .fetch_one(&self.pool)
            .await?;
        Ok(Seq(seq))
    }

    /// Accepted intents for the idempotency window after a restart (ADR-0015): at most `limit`,
    /// recorded at or after `since_ms`, most recent ones, returned oldest first.
    pub async fn recent_intents(&self, limit: usize, since_ms: i64) -> DbResult<Vec<RecentIntent>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows = sqlx::query(
            "SELECT intent_id, MAX(seq) AS last_seq, MAX(created_at) AS at_ms, \
             MAX(intent_hash) AS intent_hash FROM event_log \
             WHERE intent_id IS NOT NULL AND created_at >= ? GROUP BY intent_id \
             ORDER BY last_seq DESC LIMIT ?",
        )
        .bind(since_ms)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut intents = rows
            .iter()
            .map(|r| {
                Ok(RecentIntent {
                    intent_id: IntentId(uuid_from_str(
                        "event_log.intent_id",
                        &r.try_get::<String, _>("intent_id")?,
                    )?),
                    intent_hash: r
                        .try_get::<Option<String>, _>("intent_hash")?
                        .map(|h| {
                            PayloadHash::from_hex(&h).ok_or(DbError::InvalidData {
                                column: "event_log.intent_hash",
                                details: h,
                            })
                        })
                        .transpose()?,
                    last_seq: Seq(r.try_get("last_seq")?),
                    at_ms: r.try_get("at_ms")?,
                })
            })
            .collect::<DbResult<Vec<_>>>()?;
        intents.reverse();
        Ok(intents)
    }

    /// Up to `limit` chat messages written before message `before` (or the newest ones), oldest
    /// first, with their rolls and stored times.
    pub async fn chat_history(
        &self,
        before: Option<ChatMessageId>,
        limit: usize,
    ) -> DbResult<Vec<ChatMessageDto>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows = sqlx::query(
            "SELECT c.id, c.sender_user_id, c.kind, c.body, c.created_at, c.sender_name, \
                 c.visibility, d.result \
             FROM chat_messages AS c LEFT JOIN dice_rolls AS d ON d.chat_message_id = c.id \
             WHERE ?1 IS NULL OR c.rowid < (SELECT rowid FROM chat_messages WHERE id = ?1) \
             ORDER BY c.rowid DESC LIMIT ?2",
        )
        .bind(before.map(|b| b.to_string()))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut messages = rows
            .iter()
            .map(|r| {
                let kind: ChatKind =
                    enum_from_str("chat_messages.kind", &r.try_get::<String, _>("kind")?)?;
                let body: String = r.try_get("body")?;
                let (body, system) = if kind == ChatKind::System {
                    (
                        String::new(),
                        Some(serde_json::from_str::<SystemMessage>(&body)?),
                    )
                } else {
                    (body, None)
                };
                Ok(ChatMessageDto {
                    id: ChatMessageId(uuid_from_str(
                        "chat_messages.id",
                        &r.try_get::<String, _>("id")?,
                    )?),
                    kind,
                    sender: opt_uuid_from_str(
                        "chat_messages.sender_user_id",
                        r.try_get("sender_user_id")?,
                    )?
                    .map(UserId),
                    sender_name: r.try_get("sender_name")?,
                    body,
                    roll: r
                        .try_get::<Option<String>, _>("result")?
                        .map(|json| serde_json::from_str::<DiceRollDto>(&json))
                        .transpose()?,
                    system,
                    visibility: enum_from_str(
                        "chat_messages.visibility",
                        &r.try_get::<String, _>("visibility")?,
                    )?,
                    at_ms: Some(r.try_get("created_at")?),
                })
            })
            .collect::<DbResult<Vec<_>>>()?;
        messages.reverse();
        Ok(messages)
    }

    /// Loads the full game state and the last event sequence number.
    pub async fn load_state(&self) -> DbResult<(GameState, Seq)> {
        let mut state = GameState::new("");
        state.campaign = CampaignMeta {
            name: get_meta(&self.pool, META_NAME).await?.unwrap_or_default(),
            description: get_meta(&self.pool, META_DESCRIPTION)
                .await?
                .unwrap_or_default(),
            version: Version(
                get_meta(&self.pool, META_VERSION)
                    .await?
                    .map(|v| {
                        v.parse::<i64>().map_err(|e| DbError::InvalidData {
                            column: "campaign_meta.value",
                            details: e.to_string(),
                        })
                    })
                    .transpose()?
                    .unwrap_or(1),
            ),
        };

        for row in sqlx::query("SELECT * FROM maps ORDER BY id")
            .fetch_all(&self.pool)
            .await?
        {
            let map = map_from_row(&row)?;
            state.maps.insert(map.id, map);
        }
        for row in sqlx::query("SELECT * FROM tokens ORDER BY id")
            .fetch_all(&self.pool)
            .await?
        {
            let token = token_from_row(&row)?;
            state.tokens.insert(token.id, token);
        }
        for row in sqlx::query(
            "SELECT id, kind, name, owner_user_id, status, sheet_data, version \
             FROM characters ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?
        {
            let character = CharacterDto {
                id: CharacterId(uuid_from_str(
                    "characters.id",
                    &row.try_get::<String, _>("id")?,
                )?),
                kind: enum_from_str("characters.kind", &row.try_get::<String, _>("kind")?)?,
                name: row.try_get("name")?,
                owner: opt_uuid_from_str(
                    "characters.owner_user_id",
                    row.try_get("owner_user_id")?,
                )?
                .map(UserId),
                status: enum_from_str("characters.status", &row.try_get::<String, _>("status")?)?,
                sheet_json: row.try_get("sheet_data")?,
                version: Version(row.try_get("version")?),
            };
            state.characters.insert(character.id, character);
        }
        for row in sqlx::query(
            "SELECT id, folder, title, markdown, sort_order, visibility, players_can_edit, \n             author_user_id, version FROM journal_entries ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?
        {
            let entry = JournalEntryDto {
                id: JournalEntryId(uuid_from_str(
                    "journal_entries.id",
                    &row.try_get::<String, _>("id")?,
                )?),
                folder: row.try_get("folder")?,
                title: row.try_get("title")?,
                markdown: row.try_get("markdown")?,
                sort_order: row.try_get("sort_order")?,
                visibility: enum_from_str(
                    "journal_entries.visibility",
                    &row.try_get::<String, _>("visibility")?,
                )?,
                players_can_edit: row.try_get("players_can_edit")?,
                author: opt_uuid_from_str(
                    "journal_entries.author_user_id",
                    row.try_get("author_user_id")?,
                )?
                .map(UserId),
                version: Version(row.try_get("version")?),
            };
            state.journal.insert(entry.id, entry);
        }
        for row in sqlx::query(
            "SELECT id, usage, sha256, mime, size_bytes, original_name, width, height, \n             display_sha256, thumbnail_sha256, tiles, version FROM assets ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?
        {
            let size = |column: &'static str| -> DbResult<u32> {
                let v: i64 = row.try_get(column)?;
                u32::try_from(v).map_err(|e| DbError::InvalidData {
                    column,
                    details: e.to_string(),
                })
            };
            let asset = AssetDto {
                id: AssetId(uuid_from_str("assets.id", &row.try_get::<String, _>("id")?)?),
                usage: enum_from_str("assets.usage", &row.try_get::<String, _>("usage")?)?,
                sha256: row.try_get("sha256")?,
                mime: row.try_get("mime")?,
                size_bytes: row.try_get("size_bytes")?,
                original_name: row.try_get("original_name")?,
                width: size("width")?,
                height: size("height")?,
                display_sha256: row.try_get("display_sha256")?,
                thumbnail_sha256: row.try_get::<Option<String>, _>("thumbnail_sha256")?.unwrap_or_default(),
                tiles: serde_json::from_str::<Vec<AssetTileDto>>(&row.try_get::<String, _>("tiles")?)?,
                version: Version(row.try_get("version")?),
            };
            state.assets.insert(asset.id, asset);
        }
        for row in sqlx::query("SELECT id, name, role FROM users ORDER BY id")
            .fetch_all(&self.pool)
            .await?
        {
            let user = UserDto {
                id: UserId(uuid_from_str("users.id", &row.try_get::<String, _>("id")?)?),
                name: row.try_get("name")?,
                role: enum_from_str("users.role", &row.try_get::<String, _>("role")?)?,
            };
            state.users.insert(user.id, user);
        }

        Ok((state, self.last_seq().await?))
    }

    /// Appends batches to `event_log` and applies them to the tables, all in one transaction.
    pub async fn project(&self, records: &[BatchRecord]) -> DbResult<()> {
        if records.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        for record in records {
            let mut seq = record.batch.seq;
            for event in &record.batch.events {
                sqlx::query(
                    "INSERT INTO event_log \
                     (seq, intent_id, intent_hash, actor_user_id, event, created_at) \
                     VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(seq.0)
                .bind(record.batch.caused_by.map(|i| i.to_string()))
                .bind(record.intent_hash.map(|h| h.to_hex()))
                .bind(record.actor.map(|u| u.to_string()))
                .bind(serde_json::to_string(event)?)
                .bind(record.at_ms)
                .execute(&mut *tx)
                .await?;
                project_event(&mut tx, event, record.at_ms).await?;
                seq = seq.next();
            }
        }
        tx.commit().await?;
        Ok(())
    }
}

async fn project_event(conn: &mut SqliteConnection, event: &Event, at_ms: i64) -> DbResult<()> {
    match event {
        Event::CampaignMetaUpdated { campaign } => write_campaign_meta(conn, campaign).await,
        Event::MapCreated { map } | Event::MapUpdated { map } => upsert_map(conn, map).await,
        Event::MapDeleted { id } => {
            sqlx::query("DELETE FROM maps WHERE id = ?")
                .bind(id.to_string())
                .execute(conn)
                .await?;
            Ok(())
        }
        Event::TokenCreated { token } | Event::TokenUpdated { token } => {
            upsert_token(conn, token).await
        }
        Event::TokenMoved { id, x, y } => {
            sqlx::query("UPDATE tokens SET x = ?, y = ? WHERE id = ?")
                .bind(x)
                .bind(y)
                .bind(id.to_string())
                .execute(conn)
                .await?;
            Ok(())
        }
        Event::TokenDeleted { id } => {
            sqlx::query("DELETE FROM tokens WHERE id = ?")
                .bind(id.to_string())
                .execute(conn)
                .await?;
            Ok(())
        }
        Event::CharacterCreated { character } | Event::CharacterUpdated { character } => {
            upsert_character(conn, character).await
        }
        Event::JournalEntryCreated { entry } | Event::JournalEntryUpdated { entry } => {
            upsert_journal_entry(conn, entry, at_ms).await
        }
        Event::JournalEntryDeleted { id } => {
            sqlx::query("DELETE FROM journal_entries WHERE id = ?")
                .bind(id.to_string())
                .execute(conn)
                .await?;
            Ok(())
        }
        Event::ChatMessagePosted { message } => insert_chat_message(conn, message, at_ms).await,
        Event::AssetRegistered { asset } => insert_asset(conn, asset, at_ms).await,
    }
}

/// Chat messages are append-only. A system message stores its [`SystemMessage`] as JSON in
/// `body`; a roll also gets a `dice_rolls` row with the full result.
async fn insert_chat_message(
    conn: &mut SqliteConnection,
    m: &ChatMessageDto,
    at_ms: i64,
) -> DbResult<()> {
    let body = match &m.system {
        Some(system) => serde_json::to_string(system)?,
        None => m.body.clone(),
    };
    sqlx::query(
        "INSERT INTO chat_messages (id, sender_user_id, kind, body, created_at, sender_name, \
             visibility) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(m.id.to_string())
    .bind(m.sender.map(|u| u.to_string()))
    .bind(enum_to_str(&m.kind)?)
    .bind(body)
    .bind(at_ms)
    .bind(&m.sender_name)
    .bind(enum_to_str(&m.visibility)?)
    .execute(&mut *conn)
    .await?;
    if let Some(roll) = &m.roll {
        sqlx::query(
            "INSERT INTO dice_rolls (id, user_id, expression, result, total, chat_message_id, \
                 created_at, user_name) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(m.id.to_string())
        .bind(m.sender.map(|u| u.to_string()))
        .bind(&roll.expression)
        .bind(serde_json::to_string(roll)?)
        .bind(roll.total)
        .bind(m.id.to_string())
        .bind(at_ms)
        .bind(&m.sender_name)
        .execute(conn)
        .await?;
    }
    Ok(())
}

async fn upsert_character(conn: &mut SqliteConnection, c: &CharacterDto) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO characters (id, kind, name, owner_user_id, status, sheet_data, version) \
         VALUES (?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT (id) DO UPDATE SET \
             kind = excluded.kind, name = excluded.name, owner_user_id = excluded.owner_user_id, \
             status = excluded.status, sheet_data = excluded.sheet_data, version = excluded.version",
    )
    .bind(c.id.to_string())
    .bind(enum_to_str(&c.kind)?)
    .bind(&c.name)
    .bind(c.owner.map(|u| u.to_string()))
    .bind(enum_to_str(&c.status)?)
    .bind(&c.sheet_json)
    .bind(c.version.0)
    .execute(conn)
    .await?;
    Ok(())
}

async fn write_campaign_meta(conn: &mut SqliteConnection, campaign: &CampaignMeta) -> DbResult<()> {
    set_meta(conn, META_NAME, &campaign.name).await?;
    set_meta(conn, META_DESCRIPTION, &campaign.description).await?;
    set_meta(conn, META_VERSION, &campaign.version.0.to_string()).await
}

async fn set_meta(conn: &mut SqliteConnection, key: &str, value: &str) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO campaign_meta (key, value) VALUES (?, ?) \
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(conn)
    .await?;
    Ok(())
}

async fn get_meta(pool: &SqlitePool, key: &str) -> DbResult<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT value FROM campaign_meta WHERE key = ?")
            .bind(key)
            .fetch_optional(pool)
            .await?,
    )
}

async fn upsert_map(conn: &mut SqliteConnection, map: &MapDto) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO maps (id, name, background_asset_id, width, height, grid_kind, \
             grid_cell_size, grid_offset_x, grid_offset_y, visible_to_players, version) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT (id) DO UPDATE SET \
             name = excluded.name, background_asset_id = excluded.background_asset_id, \
             grid_kind = excluded.grid_kind, grid_cell_size = excluded.grid_cell_size, \
             grid_offset_x = excluded.grid_offset_x, grid_offset_y = excluded.grid_offset_y, \
             visible_to_players = excluded.visible_to_players, version = excluded.version",
    )
    .bind(map.id.to_string())
    .bind(&map.name)
    .bind(map.background_asset_id.map(|a| a.to_string()))
    .bind(map.width)
    .bind(map.height)
    .bind(enum_to_str(&map.grid.kind)?)
    .bind(map.grid.cell_size)
    .bind(map.grid.offset_x)
    .bind(map.grid.offset_y)
    .bind(map.visible_to_players)
    .bind(map.version.0)
    .execute(conn)
    .await?;
    Ok(())
}

async fn upsert_token(conn: &mut SqliteConnection, token: &TokenDto) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO tokens (id, map_id, character_id, asset_id, name, x, y, rotation, scale, \
             layer, visible, version) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT (id) DO UPDATE SET \
             map_id = excluded.map_id, character_id = excluded.character_id, \
             asset_id = excluded.asset_id, name = excluded.name, x = excluded.x, y = excluded.y, \
             rotation = excluded.rotation, scale = excluded.scale, layer = excluded.layer, \
             visible = excluded.visible, version = excluded.version",
    )
    .bind(token.id.to_string())
    .bind(token.map_id.to_string())
    .bind(token.character_id.map(|c| c.to_string()))
    .bind(token.asset_id.map(|a| a.to_string()))
    .bind(&token.name)
    .bind(token.x)
    .bind(token.y)
    .bind(token.rotation)
    .bind(token.scale)
    .bind(enum_to_str(&token.layer)?)
    .bind(token.visible)
    .bind(token.version.0)
    .execute(conn)
    .await?;
    Ok(())
}

fn map_from_row(row: &SqliteRow) -> DbResult<MapDto> {
    Ok(MapDto {
        id: MapId(uuid_from_str("maps.id", &row.try_get::<String, _>("id")?)?),
        name: row.try_get("name")?,
        background_asset_id: opt_uuid_from_str(
            "maps.background_asset_id",
            row.try_get("background_asset_id")?,
        )?
        .map(dnd_protocol::AssetId),
        width: row.try_get("width")?,
        height: row.try_get("height")?,
        grid: Grid {
            kind: enum_from_str::<GridKind>(
                "maps.grid_kind",
                &row.try_get::<String, _>("grid_kind")?,
            )?,
            cell_size: row.try_get("grid_cell_size")?,
            offset_x: row.try_get("grid_offset_x")?,
            offset_y: row.try_get("grid_offset_y")?,
        },
        visible_to_players: row.try_get("visible_to_players")?,
        version: Version(row.try_get("version")?),
    })
}

fn token_from_row(row: &SqliteRow) -> DbResult<TokenDto> {
    Ok(TokenDto {
        id: TokenId(uuid_from_str(
            "tokens.id",
            &row.try_get::<String, _>("id")?,
        )?),
        map_id: MapId(uuid_from_str(
            "tokens.map_id",
            &row.try_get::<String, _>("map_id")?,
        )?),
        character_id: opt_uuid_from_str("tokens.character_id", row.try_get("character_id")?)?
            .map(CharacterId),
        asset_id: opt_uuid_from_str("tokens.asset_id", row.try_get("asset_id")?)?
            .map(dnd_protocol::AssetId),
        name: row.try_get("name")?,
        x: row.try_get("x")?,
        y: row.try_get("y")?,
        rotation: row.try_get("rotation")?,
        scale: row.try_get("scale")?,
        layer: enum_from_str::<TokenLayer>("tokens.layer", &row.try_get::<String, _>("layer")?)?,
        visible: row.try_get("visible")?,
        version: Version(row.try_get("version")?),
    })
}

/// `created_at` is set on insert only; `updated_at` follows every change.
async fn upsert_journal_entry(
    conn: &mut SqliteConnection,
    e: &JournalEntryDto,
    at_ms: i64,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO journal_entries (id, folder, title, markdown, sort_order, visibility, \
             players_can_edit, author_user_id, created_at, updated_at, version) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT (id) DO UPDATE SET \
             folder = excluded.folder, title = excluded.title, markdown = excluded.markdown, \
             sort_order = excluded.sort_order, visibility = excluded.visibility, \
             players_can_edit = excluded.players_can_edit, \
             author_user_id = excluded.author_user_id, updated_at = excluded.updated_at, \
             version = excluded.version",
    )
    .bind(e.id.to_string())
    .bind(&e.folder)
    .bind(&e.title)
    .bind(&e.markdown)
    .bind(e.sort_order)
    .bind(enum_to_str(&e.visibility)?)
    .bind(e.players_can_edit)
    .bind(e.author.map(|u| u.to_string()))
    .bind(at_ms)
    .bind(at_ms)
    .bind(e.version.0)
    .execute(conn)
    .await?;
    Ok(())
}

/// Assets are immutable records (files are content-addressed); registration inserts once.
async fn insert_asset(conn: &mut SqliteConnection, a: &AssetDto, at_ms: i64) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO assets (id, type, usage, sha256, mime, size_bytes, original_name, width, \
             height, display_sha256, thumbnail_sha256, tiles, created_at, version) \
         VALUES (?, 'image', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(a.id.to_string())
    .bind(enum_to_str(&a.usage)?)
    .bind(&a.sha256)
    .bind(&a.mime)
    .bind(a.size_bytes)
    .bind(&a.original_name)
    .bind(i64::from(a.width))
    .bind(i64::from(a.height))
    .bind(&a.display_sha256)
    .bind(&a.thumbnail_sha256)
    .bind(serde_json::to_string(&a.tiles)?)
    .bind(at_ms)
    .bind(a.version.0)
    .execute(conn)
    .await?;
    Ok(())
}
