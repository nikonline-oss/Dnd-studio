//! `dnd-session-server`: configuration from `DND_*` variables (see `Config::from_env`), logs to
//! stdout (`RUST_LOG`, default `info`), graceful shutdown on Ctrl+C / SIGTERM.
//!
//! `dnd-session-server healthcheck` asks the running server for `/api/health` on `DND_BIND`'s
//! port and exits 0 when it answers 200 (the Docker image has no curl).

use dnd_session_server::{Config, Server};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        return healthcheck();
    }
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

fn healthcheck() -> anyhow::Result<()> {
    use std::io::{Read, Write};
    use std::time::Duration;

    let bind = std::env::var("DND_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let port = bind.rsplit(':').next().unwrap_or("8080");
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port.parse::<u16>()?))?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream
        .write_all(b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    let mut answer = String::new();
    stream.read_to_string(&mut answer)?;
    if answer.starts_with("HTTP/1.1 200") {
        Ok(())
    } else {
        anyhow::bail!("unhealthy: {}", answer.lines().next().unwrap_or_default())
    }
}
