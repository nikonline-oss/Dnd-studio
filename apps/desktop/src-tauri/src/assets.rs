//! Importing images into the open campaign and serving their WebP files to the webview
//! (ADR-0017). The webview never supplies paths: files are picked in a native dialog here and
//! served only by SHA-256 from the open campaign's `assets/` folder.

use std::path::{Path, PathBuf};

use dnd_assets::{AssetUsage, Imported, MAX_FILE_BYTES, import_image, is_sha256_hex, served_path};
use dnd_protocol::{
    AssetDto, AssetId, AssetTileDto, EventBatch, Grid, Intent, IntentEnvelope, IntentId,
    IntentOutcome, MapId, Version,
};
use serde::Serialize;
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{AppHandle, Manager, Runtime, State, UriSchemeContext, UriSchemeResponder};
use tauri_plugin_dialog::DialogExt;
use tauri_specta::Event;

use crate::commands::{AppState, GameEventsEmitted};
use crate::error::{IpcError, IpcErrorKind};
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

    let assets_dir = {
        let guard = state.server.lock().await;
        let server = guard.as_ref().ok_or_else(IpcError::no_campaign)?;
        server.folder().join(ASSETS_DIR)
    };
    // The campaign lock is not held while decoding: other intents keep flowing.
    let imported = read_and_import(path, assets_dir, AssetUsage::Map).await?;

    let mut guard = state.server.lock().await;
    let server = guard.as_mut().ok_or_else(IpcError::no_campaign)?;
    register_map(server, &imported, name, original_name, |batch| {
        if let Err(e) = GameEventsEmitted(batch).emit(&app) {
            tracing::error!(error = %e, "failed to emit game events");
        }
    })
    .await
    .map(Some)
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
    let assets_dir = {
        let guard = state.server.lock().await;
        let server = guard.as_ref().ok_or_else(IpcError::no_campaign)?;
        server.folder().join(ASSETS_DIR)
    };
    let mut ids = Vec::with_capacity(paths.len());
    for path in paths {
        let original_name = path.file_name().map(|s| s.to_string_lossy().into_owned());
        let imported = read_and_import(path, assets_dir.clone(), AssetUsage::Token).await?;
        let mut guard = state.server.lock().await;
        let server = guard.as_mut().ok_or_else(IpcError::no_campaign)?;
        let id = register_image(server, &imported, original_name, |batch| {
            if let Err(e) = GameEventsEmitted(batch).emit(&app) {
                tracing::error!(error = %e, "failed to emit game events");
            }
        })
        .await?;
        ids.push(id);
    }
    Ok(ids)
}

/// Registers an imported picture unless the same original exists; returns its asset.
pub async fn register_image(
    server: &mut LocalServer,
    imported: &Imported,
    original_name: Option<String>,
    mut on_batch: impl FnMut(EventBatch),
) -> Result<AssetId, IpcError> {
    if let Some(existing) = server.asset_by_sha(&imported.original.sha256) {
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
    let asset_id = match server.asset_by_sha(&imported.original.sha256) {
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

fn asset_dto(imported: &Imported, original_name: Option<String>) -> AssetDto {
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
            // On a session server: fetched once (hash checked), then from the cache.
            let cache = state.dirs.get().map(|d| d.asset_cache.clone());
            let bytes = match cache {
                Some(cache) if is_sha256_hex(&sha) => remote.asset(&sha, &cache).await.ok(),
                _ => None,
            };
            responder.respond(webp_response(bytes));
            return;
        }
        let folder = {
            let guard = state.server.lock().await;
            guard.as_ref().map(|s| s.folder().to_owned())
        };
        responder.respond(response_for(folder.as_deref(), &sha));
    });
}

/// Builds the response for a request path. Pure enough to test without a webview.
pub fn response_for(campaign_folder: Option<&Path>, sha: &str) -> Response<Vec<u8>> {
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
    webp_response(std::fs::read(served_path(&folder.join(ASSETS_DIR), sha)).ok())
}

/// A WebP file, or 404.
fn webp_response(bytes: Option<Vec<u8>>) -> Response<Vec<u8>> {
    let not_found = || {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap_or_default()
    };
    match bytes {
        Some(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/webp")
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
