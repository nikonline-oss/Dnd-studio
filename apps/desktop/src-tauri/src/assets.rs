//! Importing images into the open campaign and serving their WebP files to the webview
//! (ADR-0017). The webview never supplies paths: files are picked in a native dialog here and
//! served only by SHA-256 from the open campaign's `assets/` folder.

use std::path::{Path, PathBuf};

use dnd_assets::{
    AssetUsage, Imported, MAX_AUDIO_BYTES, MAX_FILE_BYTES, audio_path, import_audio, import_image,
    is_sha256_hex, served_path,
};
use dnd_protocol::{
    AssetDto, AssetId, AssetTileDto, EventBatch, Grid, Intent, IntentEnvelope, IntentId,
    IntentOutcome, MapId, Version,
};
use serde::Serialize;
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{AppHandle, Manager, Runtime, State, UriSchemeContext, UriSchemeResponder};
use tauri_plugin_dialog::DialogExt;
use tauri_specta::Event;

use crate::commands::{AppState, GameSnapshotReplaced};
use crate::error::{IpcError, IpcErrorKind};
use crate::import_target::ImportTarget;
use crate::local_server::{ASSETS_DIR, LocalServer};

/// URI scheme serving derived asset files: `dnd-asset://localhost/<sha256>`
/// (`http://dnd-asset.localhost/<sha256>` on Windows).
pub const SCHEME: &str = "dnd-asset";

/// Result of importing a map image.
#[derive(Clone, Debug, Serialize, specta::Type)]
pub struct MapImported {
    pub map_id: MapId,
    /// The image was larger than allowed and was scaled down (ADR-0017).
    pub downscaled: bool,
    pub source_width: u32,
    pub source_height: u32,
}

fn asset_error(e: &dnd_assets::AssetError) -> IpcError {
    IpcError::new(IpcErrorKind::User, e.code(), e.to_string())
}

/// Asks for an image, imports it as a map background and creates a map of the same logical size
/// (ADR-0016). `None` if the dialog was cancelled.
#[tauri::command]
#[specta::specta]
pub async fn asset_import_map<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<Option<MapImported>, IpcError> {
    let Some(path) = pick_image(&app).await else {
        return Ok(None);
    };
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Map".to_owned());
    let original_name = path.file_name().map(|s| s.to_string_lossy().into_owned());

    let target = ImportTarget::capture(&state).await?;
    let assets_dir = target.directory();
    // The campaign lock is not held while decoding: other intents keep flowing.
    let imported = read_and_import(path, assets_dir.clone(), AssetUsage::Map).await?;
    if let ImportTarget::Remote { session, .. } = &target {
        return session
            .import_map(&imported, &assets_dir, name, original_name)
            .await
            .map(Some);
    }
    let mut guard = target.local_server(&state).await?;
    let server = guard.as_mut().ok_or_else(IpcError::no_campaign)?;
    let result = register_map(server, &imported, name, original_name, |_| {}).await;
    // A replica can have an older seq than the UI's server snapshot. Replace the view rather
    // than emitting a local batch that would be discarded as already seen. Also expose a
    // committed asset if creating the map itself was rejected.
    emit_import_snapshot(&app, server);
    result.map(Some)
}

/// Asks for pictures (several at once) and imports them into the campaign's image library
/// (ADR-0024): one WebP each, at most 2048 px per side, like portraits. The same picture imported
/// again is reused. Returns the assets in the order picked (empty if cancelled).
#[tauri::command]
#[specta::specta]
pub async fn asset_import_images<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<Vec<AssetId>, IpcError> {
    let paths = pick_images(&app).await;
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let target = ImportTarget::capture(&state).await?;
    let assets_dir = target.directory();
    let mut ids = Vec::with_capacity(paths.len());
    for path in paths {
        let original_name = path.file_name().map(|s| s.to_string_lossy().into_owned());
        let imported = read_and_import(path, assets_dir.clone(), AssetUsage::Token).await?;
        if let ImportTarget::Remote { session, .. } = &target {
            ids.push(
                session
                    .register_image(&imported, &assets_dir, original_name)
                    .await?,
            );
            continue;
        }
        let mut guard = target.local_server(&state).await?;
        let server = guard.as_mut().ok_or_else(IpcError::no_campaign)?;
        let id = register_image(server, &imported, original_name, |_| {}).await?;
        emit_import_snapshot(&app, server);
        ids.push(id);
    }
    Ok(ids)
}

/// Asks for sounds and music (several at once) and imports them into the campaign's audio
/// library (ADR-0026): kept as they are, at most 50 MiB each. The same file imported again is
/// reused. Returns the assets in the order picked (empty if cancelled).
#[tauri::command]
#[specta::specta]
pub async fn asset_import_audio<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<Vec<AssetId>, IpcError> {
    let paths = pick_audio(&app).await;
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let target = crate::import_target::ImportTarget::capture(&state).await?;
    let assets_dir = target.directory();
    let mut ids = Vec::with_capacity(paths.len());
    for path in paths {
        let original_name = path.file_name().map(|s| s.to_string_lossy().into_owned());
        let audio = read_audio(path, assets_dir.clone()).await?;
        if let crate::import_target::ImportTarget::Remote { session, .. } = &target {
            ids.push(
                session
                    .register_audio(&audio, &assets_dir, original_name)
                    .await?,
            );
            continue;
        }
        let mut guard = target.local_server(&state).await?;
        let server = guard.as_mut().ok_or_else(IpcError::no_campaign)?;
        if let Some(existing) =
            server.asset_by_sha(dnd_protocol::AssetUsage::Audio, &audio.file.sha256)
        {
            emit_import_snapshot(&app, server);
            ids.push(existing);
            continue;
        }
        let asset = audio_dto(&audio, original_name);
        let id = asset.id;
        submit(
            server,
            Intent::RegisterAsset {
                asset: Box::new(asset),
            },
        )
        .await?;
        emit_import_snapshot(&app, server);
        ids.push(id);
    }
    Ok(ids)
}

fn emit_import_snapshot<R: Runtime>(app: &AppHandle<R>, server: &LocalServer) {
    if let Err(error) = GameSnapshotReplaced(server.snapshot()).emit(app) {
        tracing::error!(%error, "failed to emit imported assets");
    }
}

pub(crate) async fn read_audio(
    path: PathBuf,
    directory: PathBuf,
) -> Result<dnd_assets::ImportedAudio, IpcError> {
    tauri::async_runtime::spawn_blocking(move || {
        let size = std::fs::metadata(&path)?.len();
        if size > MAX_AUDIO_BYTES as u64 {
            return Err(asset_error(&dnd_assets::AssetError::TooLarge {
                bytes: usize::try_from(size).unwrap_or(usize::MAX),
            }));
        }
        let audio = import_audio(&std::fs::read(path)?).map_err(|e| asset_error(&e))?;
        dnd_assets::store_audio(&directory, &audio)?;
        Ok(audio)
    })
    .await
    .map_err(|e| IpcError::internal("import_failed", e.to_string()))?
}

pub(crate) fn audio_dto(
    audio: &dnd_assets::ImportedAudio,
    original_name: Option<String>,
) -> AssetDto {
    let sha = audio.file.sha256.clone();
    AssetDto {
        id: AssetId::new_v7(),
        usage: dnd_protocol::AssetUsage::Audio,
        sha256: sha.clone(),
        mime: audio.mime.to_owned(),
        size_bytes: i64::try_from(audio.file.bytes.len()).unwrap_or(i64::MAX),
        original_name,
        width: 0,
        height: 0,
        display_sha256: Some(sha.clone()),
        thumbnail_sha256: sha,
        tiles: Vec::new(),
        folder_id: None,
        version: Version::INITIAL,
    }
}

async fn pick_audio<R: Runtime>(app: &AppHandle<R>) -> Vec<PathBuf> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter(
            "Audio",
            &["mp3", "ogg", "oga", "opus", "wav", "flac", "m4a", "webm"],
        )
        .pick_files(move |files| {
            let paths = files
                .unwrap_or_default()
                .into_iter()
                .filter_map(|f| f.into_path().ok())
                .collect();
            let _ = tx.send(paths);
        });
    rx.await.unwrap_or_default()
}

/// Registers an imported picture unless the same original exists; returns its asset.
pub async fn register_image(
    server: &mut LocalServer,
    imported: &Imported,
    original_name: Option<String>,
    mut on_batch: impl FnMut(EventBatch),
) -> Result<AssetId, IpcError> {
    if let Some(existing) = server.asset_by_sha(imported.usage, &imported.original.sha256) {
        return Ok(existing);
    }
    let asset = asset_dto(imported, original_name);
    let id = asset.id;
    on_batch(
        submit(
            server,
            Intent::RegisterAsset {
                asset: Box::new(asset),
            },
        )
        .await?,
    );
    Ok(id)
}

/// Registers an imported image (unless the same original exists) and creates a map of its size.
pub async fn register_map(
    server: &mut LocalServer,
    imported: &Imported,
    name: String,
    original_name: Option<String>,
    mut on_batch: impl FnMut(EventBatch),
) -> Result<MapImported, IpcError> {
    let asset_id = match server.asset_by_sha(imported.usage, &imported.original.sha256) {
        // The same image was imported before: reuse it.
        Some(existing) => existing,
        None => {
            let asset = asset_dto(imported, original_name);
            let id = asset.id;
            let intent = Intent::RegisterAsset {
                asset: Box::new(asset),
            };
            on_batch(submit(server, intent).await?);
            id
        }
    };
    let map_id = MapId::new_v7();
    on_batch(
        submit(
            server,
            Intent::CreateMap {
                id: map_id,
                name,
                background_asset_id: Some(asset_id),
                width: i64::from(imported.width),
                height: i64::from(imported.height),
                grid: Grid::default(),
                visible_to_players: false,
            },
        )
        .await?,
    );
    Ok(MapImported {
        map_id,
        downscaled: imported.downscaled,
        source_width: imported.source_width,
        source_height: imported.source_height,
    })
}

pub async fn read_and_import(
    path: PathBuf,
    assets_dir: PathBuf,
    usage: AssetUsage,
) -> Result<Imported, IpcError> {
    // Decoding and encoding are CPU-heavy; keep them off the async runtime's workers.
    tauri::async_runtime::spawn_blocking(move || {
        let size = std::fs::metadata(&path)?.len();
        if size > MAX_FILE_BYTES as u64 {
            return Err(asset_error(&dnd_assets::AssetError::TooLarge {
                bytes: usize::try_from(size).unwrap_or(usize::MAX),
            }));
        }
        let bytes = std::fs::read(&path)?;
        let imported = import_image(&bytes, usage).map_err(|e| asset_error(&e))?;
        dnd_assets::store(&assets_dir, &imported)?;
        tracing::info!(
            sha256 = %imported.original.sha256,
            width = imported.width,
            height = imported.height,
            tiles = imported.tiles.len(),
            downscaled = imported.downscaled,
            "image imported"
        );
        Ok(imported)
    })
    .await
    .map_err(|e| IpcError::internal("import_failed", e.to_string()))?
}

pub(crate) fn asset_dto(imported: &Imported, original_name: Option<String>) -> AssetDto {
    AssetDto {
        id: AssetId::new_v7(),
        usage: imported.usage,
        sha256: imported.original.sha256.clone(),
        mime: imported.mime.to_owned(),
        size_bytes: i64::try_from(imported.original.bytes.len()).unwrap_or(i64::MAX),
        original_name,
        width: imported.width,
        height: imported.height,
        display_sha256: imported.display.as_ref().map(|d| d.sha256.clone()),
        thumbnail_sha256: imported.thumbnail.sha256.clone(),
        tiles: imported
            .tiles
            .iter()
            .map(|t| AssetTileDto {
                x: t.x,
                y: t.y,
                width: t.width,
                height: t.height,
                sha256: t.webp.sha256.clone(),
            })
            .collect(),
        folder_id: None,
        version: Version::INITIAL,
    }
}

/// Submits an intent composed on the Rust side; the caller broadcasts the batch.
async fn submit(server: &mut LocalServer, intent: Intent) -> Result<EventBatch, IpcError> {
    let envelope = IntentEnvelope {
        intent_id: IntentId::new_v7(),
        expected_version: None,
        intent,
    };
    match server.submit(&envelope).await? {
        (IntentOutcome::Accepted { .. }, Some(batch)) => Ok(batch),
        (IntentOutcome::Rejected { reason }, _) => {
            Err(IpcError::user(reason.code(), reason.to_string()))
        }
        (IntentOutcome::Accepted { .. }, None) => Err(IpcError::internal(
            "import_failed",
            "intent was already applied",
        )),
    }
}

async fn pick_image<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
        .pick_file(move |file| {
            let _ = tx.send(file.and_then(|f| f.into_path().ok()));
        });
    rx.await.ok().flatten()
}

async fn pick_images<R: Runtime>(app: &AppHandle<R>) -> Vec<PathBuf> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
        .pick_files(move |files| {
            let paths = files
                .unwrap_or_default()
                .into_iter()
                .filter_map(|f| f.into_path().ok())
                .collect();
            let _ = tx.send(paths);
        });
    rx.await.unwrap_or_default()
}

/// `dnd-asset` handler: serves `assets/<sha256>.webp` of the open campaign, nothing else.
pub fn serve<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    let sha = request.uri().path().trim_start_matches('/').to_owned();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let remote = state.remote.lock().await.clone();
        if let Some(remote) = remote {
            if remote.info().hosted
                && matches!(
                    remote.status(),
                    crate::remote::RemoteStatus::Reconnecting { .. }
                )
            {
                let local = state.server.lock().await;
                if let Some(local) = local.as_ref() {
                    let mime = local
                        .state()
                        .assets
                        .values()
                        .find(|asset| asset.sha256 == sha)
                        .map_or("image/webp", |asset| asset.mime.as_str());
                    responder.respond(response_for_mime(Some(local.folder()), &sha, mime));
                    return;
                }
            }
            // On a session server: fetched once (hash checked), then from the cache.
            let cache = state.dirs.get().map(|d| d.asset_cache.clone());
            let mime = remote.asset_mime(&sha);
            let bytes = match cache {
                Some(cache) if is_sha256_hex(&sha) => remote.asset(&sha, &cache, mime).await.ok(),
                _ => None,
            };
            responder.respond(file_response(bytes, mime));
            return;
        }
        let (folder, mime) = {
            let guard = state.server.lock().await;
            guard.as_ref().map_or((None, "image/webp".to_owned()), |s| {
                let mime = s
                    .state()
                    .assets
                    .values()
                    .find(|asset| asset.sha256 == sha)
                    .map_or_else(|| "image/webp".to_owned(), |asset| asset.mime.clone());
                (Some(s.folder().to_owned()), mime)
            })
        };
        responder.respond(response_for_mime(folder.as_deref(), &sha, &mime));
    });
}

/// Builds the response for a request path. Pure enough to test without a webview.
pub fn response_for(campaign_folder: Option<&Path>, sha: &str) -> Response<Vec<u8>> {
    response_for_mime(campaign_folder, sha, "image/webp")
}

/// Builds the response for an asset when the caller knows its validated MIME type.
pub fn response_for_mime(
    campaign_folder: Option<&Path>,
    sha: &str,
    mime: &str,
) -> Response<Vec<u8>> {
    let not_found = || {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap_or_default()
    };
    let Some(folder) = campaign_folder else {
        return not_found();
    };
    if !is_sha256_hex(sha) {
        return not_found();
    }
    let dir = folder.join(ASSETS_DIR);
    let (path, content_type) = if mime.starts_with("audio/") {
        (audio_path(&dir, sha), mime)
    } else {
        (served_path(&dir, sha), "image/webp")
    };
    file_response(std::fs::read(path).ok(), content_type)
}

/// A served file of `content_type`, or 404.
fn file_response(bytes: Option<Vec<u8>>, content_type: &str) -> Response<Vec<u8>> {
    let not_found = || {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap_or_default()
    };
    match bytes {
        Some(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            // The page is another origin (`tauri.localhost`, or the dev server), and tiles are
            // read with `fetch`, which needs CORS. Only this webview can reach the scheme, and
            // the files are public within the campaign, so any origin is fine.
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            // Content-addressed: a hash never changes its bytes.
            .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
            .body(bytes)
            .unwrap_or_default(),
        None => not_found(),
    }
}
