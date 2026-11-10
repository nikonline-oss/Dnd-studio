/// Errors talking to a session server.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NetError {
    #[error("not a server address or invite link: {0}")]
    InvalidAddress(String),
    /// The server answered with an error (`code` as in the server's `{ code, message }`).
    #[error("server error {status} {code}: {message}")]
    Http {
        status: u16,
        code: String,
        message: String,
    },
    /// The server could not be reached, or the connection broke.
    #[error("connection failed: {0}")]
    Transport(String),
    #[error("download exceeds the limit of {limit} bytes")]
    DownloadTooLarge { limit: u64 },
    /// The server sent something unexpected.
    #[error("protocol error: {0}")]
    Protocol(String),
    /// Not connected, and this intent may not wait for the connection (CLAUDE.md §7).
    #[error("not connected to the server")]
    Offline,
    /// Sent, but the connection broke before the answer: it may or may not have been applied.
    #[error("the connection broke before the server answered")]
    ConnectionLost,
    #[error("the server did not answer in time")]
    Timeout,
    /// The connection is over for good (session stopped, access revoked, client outdated).
    #[error("the session connection is closed: {0}")]
    Closed(String),
    /// A catalog index or package failed validation or a signature check. The plugin error's
    /// stable code is kept so the UI can explain what was wrong (ADR-0060).
    #[error("plugin catalog {code}: {message}")]
    Plugin { code: String, message: String },
}

impl NetError {
    /// A stable code for the UI (`errors.<code>`).
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::InvalidAddress(_) => "invalid_address",
            Self::Http { code, .. } => code,
            Self::Transport(_) => "server_unreachable",
            Self::DownloadTooLarge { .. } => "download_too_large",
            Self::Protocol(_) => "protocol_error",
            Self::Offline => "offline",
            Self::ConnectionLost => "connection_lost",
            Self::Timeout => "timeout",
            Self::Closed(reason) => reason,
            Self::Plugin { code, .. } => code,
        }
    }
}

impl From<dnd_plugin::PluginError> for NetError {
    fn from(error: dnd_plugin::PluginError) -> Self {
        Self::Plugin {
            code: error.code().to_owned(),
            message: error.to_string(),
        }
    }
}
