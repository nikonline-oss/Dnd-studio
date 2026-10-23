//! Shared server state: configuration, the index database and the sessions loaded in memory.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::http::HeaderMap;
use axum::http::StatusCode;
use dnd_db::{ParticipantRow, ServerDb, SessionRow, SessionState};
use dnd_protocol::SessionId;
use tokio::sync::Mutex;

use crate::config::Config;
use crate::error::ApiError;
use crate::now_ms;
use crate::secrets::token_hash;
use crate::session::SessionHandle;

pub type App = Arc<AppState>;

#[derive(Debug)]
pub struct AppState {
    pub config: Config,
    pub db: ServerDb,
    sessions: Mutex<HashMap<SessionId, SessionHandle>>,
}

impl AppState {
    pub async fn open(config: Config) -> anyhow::Result<App> {
        std::fs::create_dir_all(config.data_dir.join("sessions"))?;
        std::fs::create_dir_all(config.data_dir.join("tmp"))?;
        let db = ServerDb::open(&config.data_dir.join("server.sqlite")).await?;
        Ok(Arc::new(Self {
            config,
            db,
            sessions: Mutex::new(HashMap::new()),
        }))
    }

    /// Folder for files being uploaded or exported.
    #[must_use]
    pub fn tmp_dir(&self) -> PathBuf {
        self.config.data_dir.join("tmp")
    }

    #[must_use]
    pub fn session_dir(&self, id: SessionId) -> PathBuf {
        self.config.data_dir.join("sessions").join(id.to_string())
    }

    /// The index row of a session, or 404.
    pub async fn row(&self, id: SessionId) -> Result<SessionRow, ApiError> {
        self.db.session(id).await?.ok_or_else(ApiError::not_found)
    }

    /// The running session, loading it from its campaign file if needed. Only active sessions.
    pub async fn session(&self, id: SessionId) -> Result<SessionHandle, ApiError> {
        let mut sessions = self.sessions.lock().await;
        if let Some(handle) = sessions.get(&id) {
            return Ok(handle.clone());
        }
        let row = self.row(id).await?;
        if row.state != SessionState::Active {
            return Err(ApiError::new(
                StatusCode::GONE,
                "session_closed",
                "the session is no longer running",
            ));
        }
        let handle = SessionHandle::start(id, &PathBuf::from(&row.campaign_path))
            .await
            .map_err(ApiError::internal)?;
        sessions.insert(id, handle.clone());
        Ok(handle)
    }

    /// Registers a session started by its creation.
    pub async fn insert(&self, handle: SessionHandle) {
        self.sessions.lock().await.insert(handle.id, handle);
    }

    /// Stops and forgets a loaded session (no-op when it is not loaded).
    pub async fn unload(&self, id: SessionId) {
        let handle = self.sessions.lock().await.remove(&id);
        if let Some(handle) = handle {
            handle.stop().await;
            tracing::info!(session = %id, "session unloaded");
        }
    }

    /// The participant whose token is in `Authorization: Bearer`, for this session.
    pub async fn participant(
        &self,
        id: SessionId,
        headers: &HeaderMap,
    ) -> Result<ParticipantRow, ApiError> {
        let token = bearer(headers).ok_or_else(ApiError::unauthorized)?;
        self.db
            .participant_by_token(id, &token_hash(token))
            .await?
            .ok_or_else(ApiError::unauthorized)
    }

    /// Expires loaded sessions nobody is connected to after the TTL, and active sessions left from
    /// an earlier run that are just as old.
    pub async fn sweep(&self) -> anyhow::Result<()> {
        let now = now_ms();
        let ttl = i64::try_from(self.config.limits.session_ttl.as_millis()).unwrap_or(i64::MAX);
        let loaded: Vec<SessionHandle> = self.sessions.lock().await.values().cloned().collect();
        for handle in &loaded {
            self.db
                .touch_session(handle.id, handle.last_activity())
                .await?;
        }
        for row in self.db.sessions_in(SessionState::Active).await? {
            let handle = loaded.iter().find(|h| h.id == row.id);
            let idle_since = handle.map_or(row.last_activity, SessionHandle::last_activity);
            let busy = handle.is_some_and(|h| h.connection_count() > 0);
            if !busy && now.saturating_sub(idle_since) > ttl {
                self.unload(row.id).await;
                self.db
                    .set_session_state(row.id, SessionState::Expired, now)
                    .await?;
                self.db
                    .audit(
                        Some(row.id),
                        "server",
                        "session_expired",
                        &serde_json::json!({}),
                        now,
                    )
                    .await?;
                tracing::info!(session = %row.id, "session expired");
            }
        }
        Ok(())
    }

    /// Stops every loaded session (graceful shutdown): campaign files are checkpointed and closed.
    pub async fn shutdown(&self) {
        let ids: Vec<SessionId> = self.sessions.lock().await.keys().copied().collect();
        for id in ids {
            if let Some(handle) = self.sessions.lock().await.get(&id).cloned() {
                self.db.touch_session(id, handle.last_activity()).await.ok();
            }
            self.unload(id).await;
        }
        self.db.close().await;
    }
}

/// The token of `Authorization: Bearer <token>`.
#[must_use]
pub fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|t| !t.is_empty())
}
