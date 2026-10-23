//! Configuration from environment variables (Docker-friendly; CLAUDE.md §7 limits by default).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, bail};

/// Limits of CLAUDE.md §7; all configurable.
#[derive(Clone, Debug)]
pub struct Limits {
    /// Sessions in state `active` at once.
    pub max_sessions: usize,
    /// Participants who may ever join one session.
    pub max_participants: usize,
    /// Largest incoming WS message.
    pub max_message_bytes: usize,
    /// A connection that sent nothing (not even a ping) for this long is closed.
    pub idle_timeout: Duration,
    /// A session nobody is connected to expires after this long without activity.
    pub session_ttl: Duration,
    /// New connections (WS and joins) per minute per IP.
    pub connections_per_minute: u32,
    /// Messages per minute per connection.
    pub messages_per_minute: u32,
    /// Largest campaign upload.
    pub max_upload_bytes: usize,
    /// How often expired sessions are looked for.
    pub sweep_interval: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_sessions: 100,
            max_participants: 20,
            max_message_bytes: 64 * 1024,
            idle_timeout: Duration::from_secs(45),
            session_ttl: Duration::from_secs(60 * 60),
            connections_per_minute: 10,
            messages_per_minute: 100,
            max_upload_bytes: 1024 * 1024 * 1024,
            sweep_interval: Duration::from_secs(60),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Holds `server.sqlite` and `sessions/{id}/`.
    pub data_dir: PathBuf,
    pub bind: SocketAddr,
    /// Only requests with this token may create sessions (CLAUDE.md §7).
    pub host_token: String,
    /// Base of invite links, e.g. `https://dnd.example.org`; when unset, taken from the request.
    pub public_url: Option<String>,
    pub limits: Limits,
}

/// Shortest accepted host token: it guards uploads to the server.
pub const MIN_HOST_TOKEN_LEN: usize = 16;

impl Config {
    /// Reads `DND_*` variables:
    ///
    /// | Variable | Default |
    /// |---|---|
    /// | `DND_HOST_TOKEN` or `DND_HOST_TOKEN_FILE` | required, ≥ 16 characters |
    /// | `DND_DATA_DIR` | `/data` |
    /// | `DND_BIND` | `0.0.0.0:8080` |
    /// | `DND_PUBLIC_URL` | from the request |
    /// | `DND_MAX_SESSIONS`, `DND_MAX_PARTICIPANTS` | 100, 20 |
    /// | `DND_SESSION_TTL_SECS` | 3600 |
    /// | `DND_CONNECTIONS_PER_MINUTE`, `DND_MESSAGES_PER_MINUTE` | 10, 100 |
    /// | `DND_MAX_UPLOAD_MB` | 1024 |
    pub fn from_env() -> anyhow::Result<Self> {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        let host_token = match (var("DND_HOST_TOKEN"), var("DND_HOST_TOKEN_FILE")) {
            (Some(token), _) => token,
            (None, Some(file)) => std::fs::read_to_string(&file)
                .with_context(|| format!("reading DND_HOST_TOKEN_FILE {file}"))?,
            (None, None) => bail!("set DND_HOST_TOKEN (or DND_HOST_TOKEN_FILE)"),
        };
        let host_token = host_token.trim().to_owned();
        if host_token.chars().count() < MIN_HOST_TOKEN_LEN {
            bail!("the host token must be at least {MIN_HOST_TOKEN_LEN} characters");
        }
        let number = |name: &str, default: u64| -> anyhow::Result<u64> {
            var(name).map_or(Ok(default), |v| {
                v.trim()
                    .parse()
                    .with_context(|| format!("{name} must be a number"))
            })
        };
        let defaults = Limits::default();
        let limits = Limits {
            max_sessions: usize::try_from(number("DND_MAX_SESSIONS", 100)?)?,
            max_participants: usize::try_from(number("DND_MAX_PARTICIPANTS", 20)?)?,
            session_ttl: Duration::from_secs(number("DND_SESSION_TTL_SECS", 3600)?),
            connections_per_minute: u32::try_from(number("DND_CONNECTIONS_PER_MINUTE", 10)?)?,
            messages_per_minute: u32::try_from(number("DND_MESSAGES_PER_MINUTE", 100)?)?,
            max_upload_bytes: usize::try_from(number("DND_MAX_UPLOAD_MB", 1024)?)? * 1024 * 1024,
            ..defaults
        };
        Ok(Self {
            data_dir: var("DND_DATA_DIR").map_or_else(|| PathBuf::from("/data"), PathBuf::from),
            bind: var("DND_BIND")
                .as_deref()
                .unwrap_or("0.0.0.0:8080")
                .parse()
                .context("DND_BIND must be host:port")?,
            host_token,
            public_url: var("DND_PUBLIC_URL").map(|u| u.trim_end_matches('/').to_owned()),
            limits,
        })
    }
}
