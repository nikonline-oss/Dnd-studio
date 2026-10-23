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
}

impl NetError {
    /// A stable code for the UI (`errors.<code>`).
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::InvalidAddress(_) => "invalid_address",
            Self::Http { code, .. } => code,
            Self::Transport(_) => "server_unreachable",
            Self::Protocol(_) => "protocol_error",
            Self::Offline => "offline",
            Self::ConnectionLost => "connection_lost",
            Self::Timeout => "timeout",
            Self::Closed(reason) => reason,
        }
    }
}
