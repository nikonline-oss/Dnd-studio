//! HTTP API (CLAUDE.md §7, ADR-0022).

use std::path::PathBuf;

use axum::Json;
use axum::body::Body;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use dnd_db::{
    ArchiveLimits, ParticipantRow, ParticipantStatus, SessionRow, SessionState, read_archive,
};
use dnd_protocol::{
    ChatMessageDto, ChatMessageId, PROTOCOL_VERSION, RejectReason, Role, SessionId, UserDto, UserId,
};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::app::{App, bearer};
use crate::error::ApiError;
use crate::now_ms;
use crate::secrets::{
    hash_secret, new_access_code, new_token, normalize_code, same_secret, token_hash, verify_secret,
};
use crate::session::SessionHandle;

#[derive(Serialize)]
pub struct Health {
    status: &'static str,
    version: &'static str,
    protocol: u32,
}

pub async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        protocol: PROTOCOL_VERSION,
    })
}

/// `GET /`: someone opened the server in a browser. Says it works and where to go instead.
pub async fn home() -> axum::response::Html<String> {
    axum::response::Html(HOME_PAGE.replace("{version}", env!("CARGO_PKG_VERSION")))
}

const HOME_PAGE: &str = r#"<!doctype html>
<html lang="ru">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>DndStudio — сервер сессий</title>
<style>
  :root { color-scheme: light dark; --bg: #f6f3ee; --fg: #23201c; --muted: #6b645b; --accent: #a8792a; }
  @media (prefers-color-scheme: dark) { :root { --bg: #16140f; --fg: #ece6dc; --muted: #9b9386; --accent: #e0b25a; } }
  body { margin: 0; min-height: 100vh; display: grid; place-items: center; padding: 16px;
         background: var(--bg); color: var(--fg); font: 16px/1.5 system-ui, sans-serif; }
  main { max-width: 480px; }
  h1 { margin: 0 0 8px; font-size: 24px; }
  .ok { color: var(--accent); font-weight: 600; }
  p { margin: 8px 0; }
  .muted { color: var(--muted); font-size: 14px; }
</style>
</head>
<body>
<main>
  <h1>DndStudio</h1>
  <p class="ok">Сервер сессий работает.</p>
  <p>Подключайтесь из приложения DndStudio: «Сессия → Подключиться к сессии…» — со ссылкой-приглашением и кодом доступа от мастера.</p>
  <p class="muted">The session server is running. Join from the DndStudio app: Session → Join a session…, with the invite link and access code from your GM.</p>
  <p class="muted">dnd-session-server {version}</p>
</main>
</body>
</html>
"#;

/// Answer to `POST /api/sessions`. The secrets are shown once; the server keeps only hashes.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreatedSession {
    pub session_id: SessionId,
    pub name: String,
    pub gm_key: String,
    pub access_code: String,
    pub invite_url: String,
}

/// `POST /api/sessions` with the host token and a multipart field `archive` (a campaign archive).
pub async fn create_session(
    State(app): State<App>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<CreatedSession>), ApiError> {
    let token = bearer(&headers).ok_or_else(ApiError::unauthorized)?;
    if !same_secret(token, &app.config.host_token) {
        tracing::warn!("session creation with a wrong host token");
        return Err(ApiError::unauthorized());
    }
    let active = app.db.count_sessions_in(SessionState::Active).await?;
    if usize::try_from(active).unwrap_or(usize::MAX) >= app.config.limits.max_sessions {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "too_many_sessions",
            "the server runs as many sessions as it may",
        ));
    }

    let id = SessionId::new_v7();
    let upload = app.tmp_dir().join(format!("upload-{id}.zip"));
    let received = receive_archive(&mut multipart, &upload).await;
    let created = match received {
        Ok(()) => unpack_and_start(&app, id, &upload).await,
        Err(e) => Err(e),
    };
    tokio::fs::remove_file(&upload).await.ok();
    let (handle, row) = match created {
        Ok(created) => created,
        Err(e) => {
            if app.session_dir(id).exists() {
                std::fs::remove_dir_all(app.session_dir(id)).ok();
            }
            return Err(e);
        }
    };

    let gm_key = new_token();
    let access_code = new_access_code();
    let (gm_hash, code_hash) = {
        let (gm_key, code) = (gm_key.clone(), normalize_code(&access_code));
        tokio::task::spawn_blocking(move || -> anyhow::Result<(String, String)> {
            Ok((hash_secret(&gm_key)?, hash_secret(&code)?))
        })
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?
    };
    let row = SessionRow {
        gm_key_hash: gm_hash,
        access_code_hash: code_hash,
        ..row
    };
    app.db.insert_session(&row).await?;
    app.db
        .audit(
            Some(id),
            "host",
            "session_created",
            &serde_json::json!({ "name": row.name }),
            now_ms(),
        )
        .await?;
    app.insert(handle).await;
    tracing::info!(session = %id, name = %row.name, "session created");

    let base = app
        .config
        .public_url
        .clone()
        .unwrap_or_else(|| request_base(&headers));
    Ok((
        StatusCode::CREATED,
        Json(CreatedSession {
            session_id: id,
            name: row.name,
            gm_key,
            access_code,
            invite_url: format!("{base}/api/sessions/{id}"),
        }),
    ))
}

async fn receive_archive(multipart: &mut Multipart, to: &std::path::Path) -> Result<(), ApiError> {
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request("invalid_upload", e.to_string()))?
    {
        if field.name() != Some("archive") {
            continue;
        }
        let mut file = tokio::fs::File::create(to)
            .await
            .map_err(ApiError::internal)?;
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| ApiError::bad_request("invalid_upload", e.to_string()))?
        {
            file.write_all(&chunk).await.map_err(ApiError::internal)?;
        }
        file.flush().await.map_err(ApiError::internal)?;
        return Ok(());
    }
    Err(ApiError::bad_request(
        "invalid_upload",
        "expected a multipart field `archive`",
    ))
}

/// Unpacks the upload into the session folder and starts the session (which opens, checks and
/// migrates the campaign file).
async fn unpack_and_start(
    app: &App,
    id: SessionId,
    upload: &std::path::Path,
) -> Result<(SessionHandle, SessionRow), ApiError> {
    let dir = app.session_dir(id);
    let limits = ArchiveLimits {
        max_total_bytes: u64::try_from(app.config.limits.max_upload_bytes)
            .unwrap_or(u64::MAX)
            .saturating_mul(4),
        ..ArchiveLimits::DEFAULT
    };
    let (archive, target) = (upload.to_owned(), dir.clone());
    tokio::task::spawn_blocking(move || read_archive(&archive, &target, limits))
        .await
        .map_err(ApiError::internal)??;
    let campaign = dir.join(dnd_db::ARCHIVE_DB);
    let handle = SessionHandle::start(id, &campaign).await.map_err(|e| {
        match e.downcast::<dnd_db::DbError>() {
            Ok(db) => ApiError::from(db),
            Err(other) => ApiError::bad_request("invalid_archive", other.to_string()),
        }
    })?;
    let now = now_ms();
    let row = SessionRow {
        id,
        name: handle.name.clone(),
        state: SessionState::Active,
        gm_key_hash: String::new(),
        access_code_hash: String::new(),
        created_at: now,
        last_activity: now,
        storage_dir: dir.display().to_string(),
        campaign_path: campaign.display().to_string(),
    };
    Ok((handle, row))
}

/// `scheme://host` of the request, for invite links when `DND_PUBLIC_URL` is not set.
fn request_base(headers: &HeaderMap) -> String {
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost");
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("http");
    format!("{scheme}://{host}")
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: SessionId,
    pub name: String,
    pub state: SessionState,
    pub protocol: u32,
}

/// `GET /api/sessions/{id}`: public facts for the connect screen.
pub async fn session_info(
    State(app): State<App>,
    Path(id): Path<SessionId>,
) -> Result<Json<SessionInfo>, ApiError> {
    let row = app.row(id).await?;
    Ok(Json(SessionInfo {
        session_id: id,
        name: row.name,
        state: row.state,
        protocol: PROTOCOL_VERSION,
    }))
}

/// `POST /api/sessions/{id}/join`: the GM key, or an access code and a name.
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum JoinRequest {
    Gm { gm_key: String },
    Player { access_code: String, name: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Joined {
    pub session_id: SessionId,
    pub participant_id: uuid::Uuid,
    /// Sent as `Authorization: Bearer` to the WS and the assets; shown once.
    pub token: String,
    pub user_id: UserId,
    pub role: Role,
    pub name: String,
    pub session_name: String,
}

pub async fn join_session(
    State(app): State<App>,
    Path(id): Path<SessionId>,
    Json(request): Json<JoinRequest>,
) -> Result<Json<Joined>, ApiError> {
    let row = app.row(id).await?;
    if row.state != SessionState::Active {
        return Err(ApiError::new(
            StatusCode::GONE,
            "session_closed",
            "the session is no longer running",
        ));
    }
    let (secret, phc, is_gm) = match &request {
        JoinRequest::Gm { gm_key } => (gm_key.clone(), row.gm_key_hash.clone(), true),
        JoinRequest::Player { access_code, .. } => (
            normalize_code(access_code),
            row.access_code_hash.clone(),
            false,
        ),
    };
    let valid = tokio::task::spawn_blocking(move || verify_secret(&secret, &phc))
        .await
        .map_err(ApiError::internal)?;
    if !valid {
        tracing::warn!(session = %id, gm = is_gm, "join with a wrong secret");
        return Err(ApiError::unauthorized());
    }
    let joined = app.db.count_participants(id).await?;
    if usize::try_from(joined).unwrap_or(usize::MAX) >= app.config.limits.max_participants {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "session_full",
            "the session has as many participants as it may",
        ));
    }

    let session = app.session(id).await?;
    let user = match request {
        JoinRequest::Gm { .. } => session.gm.clone(),
        JoinRequest::Player { name, .. } => UserDto {
            id: UserId::new_v7(),
            name,
            role: Role::Player,
        },
    };
    match session.join(user.clone()).await {
        Ok(Ok(Ok(()))) => {}
        Ok(Ok(Err(RejectReason::Invalid { .. }))) => {
            return Err(ApiError::bad_request("invalid_name", "enter a name"));
        }
        Ok(Ok(Err(reason))) => {
            return Err(ApiError::bad_request("invalid", reason.code()));
        }
        Ok(Err(e)) => return Err(ApiError::internal(e)),
        Err(e) => return Err(ApiError::internal(e)),
    }

    let token = new_token();
    let participant = ParticipantRow {
        id: uuid::Uuid::now_v7(),
        session_id: id,
        role: user.role,
        name: user.name.trim().to_owned(),
        user_id: user.id,
        status: ParticipantStatus::Disconnected,
    };
    let now = now_ms();
    app.db
        .insert_participant(&participant, &token_hash(&token), now)
        .await?;
    if is_gm {
        app.db
            .audit(Some(id), "gm", "gm_joined", &serde_json::json!({}), now)
            .await?;
    }
    tracing::info!(session = %id, role = ?participant.role, "participant joined");
    Ok(Json(Joined {
        session_id: id,
        participant_id: participant.id,
        token,
        user_id: participant.user_id,
        role: participant.role,
        name: participant.name,
        session_name: row.name,
    }))
}

#[derive(Deserialize)]
pub struct ChatPage {
    before: Option<ChatMessageId>,
    limit: Option<u32>,
}

/// Older messages, oldest first. Hidden rolls count towards GM pages only.
pub async fn chat_history(
    State(app): State<App>,
    Path(id): Path<SessionId>,
    headers: HeaderMap,
    Query(page): Query<ChatPage>,
) -> Result<Json<Vec<ChatMessageDto>>, ApiError> {
    let participant = app.participant(id, &headers).await?;
    let session = app.session(id).await?;
    let limit = usize::try_from(page.limit.unwrap_or(50).min(200)).unwrap_or(200);
    let messages = session
        .chat_history(page.before, limit, participant.role)
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;
    Ok(Json(messages))
}

/// `POST /api/sessions/{id}/stop` (GM): closes the session; connections end.
pub async fn stop_session(
    State(app): State<App>,
    Path(id): Path<SessionId>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let participant = app.participant(id, &headers).await?;
    if participant.role != Role::Gm {
        return Err(ApiError::forbidden());
    }
    let row = app.row(id).await?;
    if row.state == SessionState::Active {
        app.unload(id, crate::session::SESSION_CLOSED).await;
        let now = now_ms();
        app.db
            .set_session_state(id, SessionState::Stopped, now)
            .await?;
        app.db
            .audit(
                Some(id),
                "gm",
                "session_stopped",
                &serde_json::json!({}),
                now,
            )
            .await?;
        tracing::info!(session = %id, "session stopped");
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/sessions/{id}/export` (GM): the campaign archive as it is now (or as it was left).
pub async fn export_session(
    State(app): State<App>,
    Path(id): Path<SessionId>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let participant = app.participant(id, &headers).await?;
    if participant.role != Role::Gm {
        return Err(ApiError::forbidden());
    }
    let row = app.row(id).await?;
    let out = app
        .tmp_dir()
        .join(format!("export-{id}-{}.zip", uuid::Uuid::now_v7()));
    let result = if row.state == SessionState::Active {
        let session = app.session(id).await?;
        session
            .export(out.clone())
            .await
            .map_err(ApiError::internal)?
    } else {
        // Not running: start it just for the copy, then let it go again.
        let session = SessionHandle::start(id, &PathBuf::from(&row.campaign_path))
            .await
            .map_err(ApiError::internal)?;
        let result = session
            .export(out.clone())
            .await
            .map_err(ApiError::internal)?;
        session.stop(crate::session::SESSION_CLOSED).await;
        result
    };
    result.map_err(ApiError::internal)?;
    let bytes = tokio::fs::read(&out).await.map_err(ApiError::internal);
    tokio::fs::remove_file(&out).await.ok();
    app.db
        .audit(
            Some(id),
            "gm",
            "session_exported",
            &serde_json::json!({}),
            now_ms(),
        )
        .await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"session-{id}.zip\""),
            ),
        ],
        Body::from(bytes?),
    )
        .into_response())
}

/// `GET /api/sessions/{id}/assets/{sha256}`: a served (WebP) file the participant may see.
pub async fn asset(
    State(app): State<App>,
    Path((id, sha256)): Path<(SessionId, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let participant = app.participant(id, &headers).await?;
    let not_found = || ApiError::new(StatusCode::NOT_FOUND, "not_found", "no such asset");
    if sha256.len() != 64
        || !sha256
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(not_found());
    }
    let session = app.session(id).await?;
    if !session
        .asset_visible(sha256.clone(), participant.role)
        .await
        .map_err(ApiError::internal)?
    {
        return Err(not_found());
    }
    // A picture (`.webp`) or a sound (`.audio`, ADR-0026); the client knows the sound's type.
    let dir = app.session_dir(id).join("assets");
    let (bytes, content_type) = match tokio::fs::read(dir.join(format!("{sha256}.webp"))).await {
        Ok(bytes) => (bytes, "image/webp"),
        Err(_) => (
            tokio::fs::read(dir.join(format!("{sha256}.audio")))
                .await
                .map_err(|_| not_found())?,
            "application/octet-stream",
        ),
    };
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            // Content-addressed: a hash never changes its bytes.
            (
                header::CACHE_CONTROL,
                "private, max-age=31536000, immutable",
            ),
        ],
        bytes,
    )
        .into_response())
}

/// The package of an installed plugin; all participants may read shared rule compendia.
pub async fn plugin_package(
    State(app): State<App>,
    Path((id, sha256)): Path<(SessionId, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    app.participant(id, &headers).await?;
    let not_found = || {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "no such installed plugin",
        )
    };
    if !dnd_assets::is_sha256_hex(&sha256) {
        return Err(not_found());
    }
    let session = app.session(id).await?;
    if !session
        .plugin_visible(sha256.clone())
        .await
        .map_err(ApiError::internal)?
    {
        return Err(not_found());
    }
    let path = app
        .session_dir(id)
        .join("assets/plugins")
        .join(format!("{sha256}.dndplugin"));
    let max = dnd_plugin::PackageLimits::DEFAULT.max_package_bytes;
    if tokio::fs::metadata(&path)
        .await
        .map_err(|_| not_found())?
        .len()
        > max
    {
        return Err(ApiError::bad_request(
            "too_large",
            "plugin exceeds the package size limit",
        ));
    }
    let bytes = tokio::fs::read(path).await.map_err(|_| not_found())?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip"),
            (
                header::CACHE_CONTROL,
                "private, max-age=31536000, immutable",
            ),
        ],
        bytes,
    )
        .into_response())
}

/// A GM uploads a content-addressed derived file before registering it through an intent.
pub async fn upload_asset(
    State(app): State<App>,
    Path(id): Path<SessionId>,
    headers: HeaderMap,
    request: axum::extract::Request,
) -> Result<StatusCode, ApiError> {
    let participant = app.participant(id, &headers).await?;
    if participant.role != Role::Gm {
        return Err(ApiError::forbidden());
    }
    let _active = app.session(id).await?;
    let bytes = axum::body::to_bytes(
        request.into_body(),
        app.config.limits.max_upload_bytes.min(100 * 1024 * 1024),
    )
    .await
    .map_err(|_| {
        ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "too_large",
            "asset exceeds the upload limit",
        )
    })?;
    let sha = headers
        .get("x-asset-sha256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !dnd_assets::is_sha256_hex(sha) || dnd_assets::sha256_hex(&bytes) != sha {
        return Err(ApiError::bad_request(
            "invalid_asset",
            "asset hash does not match its bytes",
        ));
    }
    let kind = headers
        .get("x-asset-kind")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let destination = match kind.as_str() {
        "webp" => app
            .session_dir(id)
            .join("assets")
            .join(format!("{sha}.webp")),
        "audio" => app
            .session_dir(id)
            .join("assets")
            .join(format!("{sha}.audio")),
        "plugin" => app
            .session_dir(id)
            .join("assets/plugins")
            .join(format!("{sha}.dndplugin")),
        _ => {
            return Err(ApiError::bad_request(
                "invalid_asset",
                "unsupported asset kind",
            ));
        }
    };
    let audit_kind = kind.clone();
    let validated = tokio::task::spawn_blocking(move || {
        match kind.as_str() {
            "webp" => {
                if !bytes.starts_with(b"RIFF") || bytes.get(8..12) != Some(b"WEBP") {
                    return Err("expected WebP");
                }
                dnd_assets::import_image(&bytes, dnd_assets::AssetUsage::Token)
                    .map_err(|_| "invalid image")?;
            }
            "audio" => {
                dnd_assets::import_audio(&bytes).map_err(|_| "invalid audio")?;
            }
            "plugin" => {
                dnd_plugin::read_package(&bytes, dnd_plugin::PackageLimits::default())
                    .map_err(|_| "invalid plugin")?;
            }
            _ => return Err("unsupported asset kind"),
        }
        Ok(bytes)
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(|message| ApiError::bad_request("invalid_asset", message))?;
    if !destination.exists() {
        let parent = destination
            .parent()
            .ok_or_else(|| ApiError::bad_request("invalid_asset", "invalid destination"))?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(ApiError::internal)?;
        let partial = parent.join(format!(".upload-{}", uuid::Uuid::now_v7()));
        if let Err(error) = tokio::fs::write(&partial, &validated).await {
            tokio::fs::remove_file(&partial).await.ok();
            return Err(ApiError::internal(error));
        }
        if let Err(error) = tokio::fs::rename(&partial, &destination).await {
            tokio::fs::remove_file(&partial).await.ok();
            if !destination.exists() {
                return Err(ApiError::internal(error));
            }
        }
    }
    app.db
        .audit(
            Some(id),
            "gm",
            "asset_uploaded",
            &serde_json::json!({ "sha256": sha, "kind": audit_kind, "bytes": validated.len() }),
            now_ms(),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
