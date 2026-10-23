//! `dnd-session-server`: configuration from `DND_*` variables (see `Config::from_env`), logs to
//! stdout (`RUST_LOG`, default `info`), graceful shutdown on Ctrl+C / SIGTERM.

use dnd_session_server::{Config, Server};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let config = Config::from_env()?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        data = %config.data_dir.display(),
        "starting dnd-session-server"
    );
    let server = Server::start(config).await?;
    shutdown_signal().await;
    tracing::info!("shutting down");
    server.stop().await
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}
