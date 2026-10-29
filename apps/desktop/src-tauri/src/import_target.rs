//! Capture an import destination before decoding, then verify it before mutating a local game.

use std::path::PathBuf;

use dnd_protocol::{Role, UserId};
use tokio::sync::MutexGuard;

use crate::commands::AppState;
use crate::error::IpcError;
use crate::local_server::{ASSETS_DIR, LocalServer};
use crate::remote::{RemoteSession, RemoteStatus, check_replica};

pub(crate) enum ImportTarget {
    Local {
        folder: PathBuf,
        owner: UserId,
        remote: Option<RemoteSession>,
    },
    Remote {
        session: RemoteSession,
        directory: PathBuf,
    },
}

impl ImportTarget {
    pub async fn capture(state: &AppState) -> Result<Self, IpcError> {
        let remote = state.remote.lock().await.clone();
        if let Some(session) = &remote {
            let info = session.info();
            if !matches!(info.role, Role::Gm | Role::CoGm) {
                return Err(IpcError::user(
                    "forbidden",
                    "only GM roles import campaign files",
                ));
            }
            if session.status() == RemoteStatus::Online {
                let replica = if info.hosted {
                    let guard = state.server.lock().await;
                    if let Some(local) = guard.as_ref() {
                        check_replica(
                            local.db(),
                            session.campaign_info().user_id,
                            &format!("{}/{}", info.server, info.session_id),
                        )
                        .await?;
                        Some(local.folder().join(ASSETS_DIR))
                    } else {
                        None
                    }
                } else {
                    None
                };
                let directory = match replica {
                    Some(directory) => directory,
                    None => state
                        .dirs
                        .get()
                        .ok_or_else(|| {
                            IpcError::internal("not_ready", "app directories are not initialized")
                        })?
                        .asset_cache
                        .join("imports"),
                };
                return Ok(Self::Remote {
                    session: session.clone(),
                    directory,
                });
            }
            if !info.hosted || !matches!(session.status(), RemoteStatus::Reconnecting { .. }) {
                return Err(IpcError::user("offline", "the session is not connected"));
            }
        }
        let guard = state.server.lock().await;
        let local = guard.as_ref().ok_or_else(IpcError::no_campaign)?;
        let info = local.info();
        if !matches!(info.role, Role::Gm | Role::CoGm) {
            return Err(IpcError::user(
                "forbidden",
                "only GM roles import campaign files",
            ));
        }
        if let Some(session) = &remote {
            let info = session.info();
            check_replica(
                local.db(),
                session.campaign_info().user_id,
                &format!("{}/{}", info.server, info.session_id),
            )
            .await?;
        }
        Ok(Self::Local {
            folder: local.folder().to_owned(),
            owner: info.user_id,
            remote,
        })
    }

    pub fn directory(&self) -> PathBuf {
        match self {
            Self::Local { folder, .. } => folder.join(ASSETS_DIR),
            Self::Remote { directory, .. } => directory.clone(),
        }
    }

    pub async fn local_server<'a>(
        &self,
        state: &'a AppState,
    ) -> Result<MutexGuard<'a, Option<LocalServer>>, IpcError> {
        let Self::Local {
            folder,
            owner,
            remote,
        } = self
        else {
            return Err(IpcError::internal(
                "invalid_import_target",
                "expected a local import target",
            ));
        };
        let changed = || {
            IpcError::user(
                "import_context_changed",
                "campaign or connection changed during import",
            )
        };
        let actual = state.remote.lock().await.clone();
        let same = match (remote, &actual) {
            (None, None) => true,
            (Some(expected), Some(actual)) => {
                let expected = expected.info();
                let info = actual.info();
                expected.server == info.server
                    && expected.session_id == info.session_id
                    && actual.campaign_info().user_id == *owner
                    && matches!(info.role, Role::Gm | Role::CoGm)
                    && matches!(actual.status(), RemoteStatus::Reconnecting { .. })
            }
            _ => false,
        };
        if !same {
            return Err(changed());
        }
        let guard = state.server.lock().await;
        let local = guard.as_ref().ok_or_else(changed)?;
        if local.folder() != folder || local.info().user_id != *owner {
            return Err(changed());
        }
        if let Some(session) = remote {
            let info = session.info();
            check_replica(
                local.db(),
                *owner,
                &format!("{}/{}", info.server, info.session_id),
            )
            .await?;
        }
        Ok(guard)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[tokio::test]
    async fn switching_campaign_during_decode_does_not_register_in_the_new_campaign() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::new(crate::logging::LogBuffer::default());
        let first = LocalServer::create(dir.path(), "First").await.unwrap();
        *state.server.lock().await = Some(first);
        let target = ImportTarget::capture(&state).await.unwrap();
        let original = state.server.lock().await.take().unwrap();
        original.close().await.unwrap();
        *state.server.lock().await = Some(LocalServer::create(dir.path(), "Second").await.unwrap());
        let error = target.local_server(&state).await.unwrap_err();
        assert_eq!(error.code, "import_context_changed");
        let second = state.server.lock().await.take().unwrap();
        assert!(second.snapshot().assets.is_empty());
        second.close().await.unwrap();
    }
}
