//! WS/HTTP client for the session server (ADR-0022) and, later, the plugin index.
//!
//! See `README.md` for scope and dependency boundaries.

mod connection;
mod error;
mod http;

pub use connection::{ConnectOptions, Connection, Incoming, Status};
pub use error::NetError;
pub use http::{CreatedSession, HttpClient, Joined, SessionInfo, normalize_base, parse_invite};
