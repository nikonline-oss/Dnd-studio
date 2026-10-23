//! The session server's index `server.sqlite` (CLAUDE.md §6): sessions, participants and the audit
//! of GM actions. Secrets are stored as hashes only (ADR-0022).

use std::path::Path;

use dnd_protocol::{Role, SessionId, UserId};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::codec::{enum_from_str, enum_to_str, uuid_from_str};
use crate::connect::open_server_db;
use crate::error::DbResult;

/// Lifecycle of a session (CLAUDE.md §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Creating,
    Active,
    Stopped,
    Expired,
}

/// A row of `sessions`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRow {
    pub id: SessionId,
    pub name: String,
    pub state: SessionState,
    /// argon2 PHC strings.
    pub gm_key_hash: String,
    pub access_code_hash: String,
    pub created_at: i64,
    pub last_activity: i64,
    /// The session folder (`/data/sessions/{id}`) and the campaign file inside it.
    pub storage_dir: String,
    pub campaign_path: String,
}

/// Whether a participant is online.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantStatus {
    Connected,
    Disconnected,
    Kicked,
}

/// A row of `participants`: someone who joined a session, as a user of its campaign.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParticipantRow {
    pub id: Uuid,
    pub session_id: SessionId,
    pub role: Role,
    pub name: String,
    pub user_id: UserId,
    pub status: ParticipantStatus,
}

/// `server.sqlite`.
#[derive(Clone, Debug)]
pub struct ServerDb {
    pool: SqlitePool,
}

impl ServerDb {
    /// Opens (creating if needed) and migrates the index.
    pub async fn open(path: &Path) -> DbResult<Self> {
        Ok(Self {
            pool: open_server_db(path).await?,
        })
    }

    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn insert_session(&self, s: &SessionRow) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO sessions (id, name, state, gm_key_hash, access_code_hash, created_at, \
                 last_activity, storage_dir, campaign_path) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(s.id.to_string())
        .bind(&s.name)
        .bind(enum_to_str(&s.state)?)
        .bind(&s.gm_key_hash)
        .bind(&s.access_code_hash)
        .bind(s.created_at)
        .bind(s.last_activity)
        .bind(&s.storage_dir)
        .bind(&s.campaign_path)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn session(&self, id: SessionId) -> DbResult<Option<SessionRow>> {
        let row = sqlx::query(
            "SELECT id, name, state, gm_key_hash, access_code_hash, created_at, last_activity, \
                 storage_dir, campaign_path \
             FROM sessions WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| session_from_row(&r)).transpose()
    }

    /// Sessions in `state`, oldest first.
    pub async fn sessions_in(&self, state: SessionState) -> DbResult<Vec<SessionRow>> {
        let rows = sqlx::query(
            "SELECT id, name, state, gm_key_hash, access_code_hash, created_at, last_activity, \
                 storage_dir, campaign_path \
             FROM sessions WHERE state = ? ORDER BY created_at, id",
        )
        .bind(enum_to_str(&state)?)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(session_from_row).collect()
    }

    pub async fn count_sessions_in(&self, state: SessionState) -> DbResult<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE state = ?")
                .bind(enum_to_str(&state)?)
                .fetch_one(&self.pool)
                .await?,
        )
    }

    /// Changes the state and records the time as the last activity.
    pub async fn set_session_state(
        &self,
        id: SessionId,
        state: SessionState,
        now_ms: i64,
    ) -> DbResult<()> {
        sqlx::query("UPDATE sessions SET state = ?, last_activity = ? WHERE id = ?")
            .bind(enum_to_str(&state)?)
            .bind(now_ms)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn touch_session(&self, id: SessionId, now_ms: i64) -> DbResult<()> {
        sqlx::query("UPDATE sessions SET last_activity = MAX(last_activity, ?) WHERE id = ?")
            .bind(now_ms)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Adds a participant with the SHA-256 (hex) of its token.
    pub async fn insert_participant(
        &self,
        p: &ParticipantRow,
        token_hash: &str,
        now_ms: i64,
    ) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO participants (id, session_id, role, name, user_id, token_hash, \
                 connected_at, status) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(p.id.to_string())
        .bind(p.session_id.to_string())
        .bind(enum_to_str(&p.role)?)
        .bind(&p.name)
        .bind(p.user_id.to_string())
        .bind(token_hash)
        .bind(now_ms)
        .bind(enum_to_str(&p.status)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The participant of `session_id` holding the token with this hash.
    pub async fn participant_by_token(
        &self,
        session_id: SessionId,
        token_hash: &str,
    ) -> DbResult<Option<ParticipantRow>> {
        let row = sqlx::query(
            "SELECT id, session_id, role, name, user_id, status FROM participants \
             WHERE session_id = ? AND token_hash = ?",
        )
        .bind(session_id.to_string())
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| {
            Ok(ParticipantRow {
                id: uuid_from_str("participants.id", &r.try_get::<String, _>("id")?)?,
                session_id: SessionId(uuid_from_str(
                    "participants.session_id",
                    &r.try_get::<String, _>("session_id")?,
                )?),
                role: enum_from_str("participants.role", &r.try_get::<String, _>("role")?)?,
                name: r.try_get("name")?,
                user_id: UserId(uuid_from_str(
                    "participants.user_id",
                    &r.try_get::<Option<String>, _>("user_id")?
                        .unwrap_or_default(),
                )?),
                status: enum_from_str("participants.status", &r.try_get::<String, _>("status")?)?,
            })
        })
        .transpose()
    }

    pub async fn set_participant_status(
        &self,
        id: Uuid,
        status: ParticipantStatus,
        now_ms: i64,
    ) -> DbResult<()> {
        sqlx::query(
            "UPDATE participants SET status = ?, \
                 connected_at = CASE WHEN ? = 'connected' THEN ? ELSE connected_at END \
             WHERE id = ?",
        )
        .bind(enum_to_str(&status)?)
        .bind(enum_to_str(&status)?)
        .bind(now_ms)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Participants who ever joined `session_id` (the limit of CLAUDE.md §7 counts them).
    pub async fn count_participants(&self, session_id: SessionId) -> DbResult<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM participants WHERE session_id = ?")
                .bind(session_id.to_string())
                .fetch_one(&self.pool)
                .await?,
        )
    }

    /// Appends to the audit of GM actions (CLAUDE.md §9). `details` is a JSON object.
    pub async fn audit(
        &self,
        session_id: Option<SessionId>,
        actor: &str,
        action: &str,
        details: &serde_json::Value,
        now_ms: i64,
    ) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO audit_log (session_id, actor, action, details, created_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(session_id.map(|s| s.to_string()))
        .bind(actor)
        .bind(action)
        .bind(details.to_string())
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }
}

fn session_from_row(r: &sqlx::sqlite::SqliteRow) -> DbResult<SessionRow> {
    Ok(SessionRow {
        id: SessionId(uuid_from_str(
            "sessions.id",
            &r.try_get::<String, _>("id")?,
        )?),
        name: r.try_get("name")?,
        state: enum_from_str("sessions.state", &r.try_get::<String, _>("state")?)?,
        gm_key_hash: r.try_get("gm_key_hash")?,
        access_code_hash: r.try_get("access_code_hash")?,
        created_at: r.try_get("created_at")?,
        last_activity: r.try_get("last_activity")?,
        storage_dir: r.try_get("storage_dir")?,
        campaign_path: r.try_get("campaign_path")?,
    })
}
