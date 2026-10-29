//! Self-hosted session server (CLAUDE.md §1, §7; ADR-0022): the GM uploads a campaign, the server
//! runs `dnd-game-core` over it authoritatively, participants connect over WS.
//!
//! Each session is a task owning its campaign file (`/data/sessions/{id}/campaign.sqlite`, the same
//! schema as on the desktop); `/data/server.sqlite` indexes sessions, participants and the audit.

mod app;
mod config;
mod error;
mod http;
mod secrets;
mod session;
mod ws;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use tokio::net::TcpListener;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_http::trace::TraceLayer;

pub use app::{App, AppState};
pub use config::{Config, Limits, MIN_HOST_TOKEN_LEN};
pub use http::{CreatedSession, JoinRequest, Joined, SessionInfo};

#[must_use]
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// The HTTP and WS routes.
pub fn router(app: App) -> Router {
    let limits = &app.config.limits;
    // New connections and joins per IP (CLAUDE.md §7): `connections_per_minute` with a burst of as
    // many, refilled evenly over the minute.
    let per_minute = limits.connections_per_minute.max(1);
    let governor = GovernorConfigBuilder::default()
        .per_millisecond(u64::from(60_000 / per_minute).max(1))
        .burst_size(per_minute)
        .finish()
        .map(Arc::new);
    let limited = Router::new()
        .route("/api/sessions/{id}/join", post(http::join_session))
        .route("/ws/sessions/{id}", get(ws::upgrade));
    let limited = match governor {
        Some(config) => limited.layer(GovernorLayer::new(config)),
        None => limited,
    };
    Router::new()
        .route("/", get(http::home))
        .route("/api/health", get(http::health))
        .route(
            "/api/sessions",
            post(http::create_session).layer(DefaultBodyLimit::max(limits.max_upload_bytes)),
        )
        .route("/api/sessions/{id}", get(http::session_info))
        .route("/api/sessions/{id}/chat", get(http::chat_history))
        .route("/api/sessions/{id}/stop", post(http::stop_session))
        .route("/api/sessions/{id}/export", get(http::export_session))
        .route("/api/sessions/{id}/assets/{sha256}", get(http::asset))
        .route(
            "/api/sessions/{id}/plugins/{sha256}",
            get(http::plugin_package),
        )
        .route(
            "/api/sessions/{id}/assets",
            post(http::upload_asset).layer(DefaultBodyLimit::max(
                limits.max_upload_bytes.min(100 * 1024 * 1024),
            )),
        )
        .merge(limited)
        .layer(TraceLayer::new_for_http())
        .with_state(app)
}

/// A running server (tests and `main`).
#[derive(Debug)]
pub struct Server {
    pub addr: SocketAddr,
    pub app: App,
    shutdown: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl Server {
    /// Binds `config.bind` and serves until [`Server::stop`].
    pub async fn start(config: Config) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(config.bind).await?;
        let addr = listener.local_addr()?;
        let app = AppState::open(config).await?;
        let (shutdown, stopped) = tokio::sync::oneshot::channel::<()>();

        let sweeper_app = app.clone();
        let sweep_every = app.config.limits.sweep_interval;
        let sweeper = tokio::spawn(async move {
            let mut tick = tokio::time::interval(sweep_every);
            loop {
                tick.tick().await;
                if let Err(e) = sweeper_app.sweep().await {
                    tracing::error!(error = %e, "sweep failed");
                }
            }
        });

        let service = router(app.clone()).into_make_service_with_connect_info::<SocketAddr>();
        let served = app.clone();
        let task = tokio::spawn(async move {
            axum::serve(listener, service)
                .with_graceful_shutdown(async {
                    stopped.await.ok();
                })
                .await?;
            sweeper.abort();
            served.shutdown().await;
            Ok(())
        });
        tracing::info!(%addr, "session server listening");
        Ok(Self {
            addr,
            app,
            shutdown,
            task,
        })
    }

    /// Stops accepting, closes every session file and waits for the server to end.
    pub async fn stop(self) -> anyhow::Result<()> {
        let _ = self.shutdown.send(());
        self.task.await?
    }
}
