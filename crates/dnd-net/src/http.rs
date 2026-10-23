//! HTTP side of the session server (CLAUDE.md §7, ADR-0022).

use std::path::Path;

use dnd_protocol::{Role, SessionId, UserId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::NetError;

/// Answer to creating a session. The secrets are shown once.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedSession {
    pub session_id: SessionId,
    pub name: String,
    pub gm_key: String,
    pub access_code: String,
    pub invite_url: String,
}

/// Public facts about a session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: SessionId,
    pub name: String,
    /// `creating` / `active` / `stopped` / `expired`.
    pub state: String,
    pub protocol: u32,
}

/// Answer to joining: the participant's token and identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Joined {
    pub session_id: SessionId,
    pub participant_id: uuid::Uuid,
    pub token: String,
    pub user_id: UserId,
    pub role: Role,
    pub name: String,
    pub session_name: String,
}

#[derive(Deserialize)]
struct ErrorBody {
    code: String,
    #[serde(default)]
    message: String,
}

/// A server and session from what a user pastes: an invite link
/// (`https://host/api/sessions/<id>`), or a server address plus a session id.
pub fn parse_invite(input: &str) -> Result<(String, SessionId), NetError> {
    let input = input.trim();
    let invalid = || NetError::InvalidAddress(input.to_owned());
    let (base, id) = input.rsplit_once("/api/sessions/").ok_or_else(invalid)?;
    let id = id.trim_end_matches('/');
    let id = uuid::Uuid::parse_str(id).map_err(|_| invalid())?;
    Ok((normalize_base(base)?, SessionId(id)))
}

/// `host:port` or `http(s)://host:port/` → `http(s)://host:port` (http when no scheme).
pub fn normalize_base(input: &str) -> Result<String, NetError> {
    let input = input.trim().trim_end_matches('/');
    let with_scheme = if input.starts_with("http://") || input.starts_with("https://") {
        input.to_owned()
    } else {
        format!("http://{input}")
    };
    let host = with_scheme
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or_default();
    if host.is_empty() || host.contains(char::is_whitespace) {
        return Err(NetError::InvalidAddress(input.to_owned()));
    }
    Ok(with_scheme)
}

/// Installs the TLS crypto provider (ring) once per process: reqwest and the WS client need one.
pub(crate) fn tls_ready() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Calls the server's HTTP API.
#[derive(Clone, Debug)]
pub struct HttpClient {
    http: reqwest::Client,
    base: String,
}

impl HttpClient {
    pub fn new(base: &str) -> Result<Self, NetError> {
        tls_ready();
        let http = reqwest::Client::builder()
            .user_agent(concat!("dndstudio/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| NetError::Transport(e.to_string()))?;
        Ok(Self {
            http,
            base: normalize_base(base)?,
        })
    }

    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// Uploads a campaign archive with the server's host token.
    pub async fn create_session(
        &self,
        host_token: &str,
        archive: &Path,
    ) -> Result<CreatedSession, NetError> {
        let bytes = tokio::fs::read(archive)
            .await
            .map_err(|e| NetError::Transport(e.to_string()))?;
        let form = reqwest::multipart::Form::new().part(
            "archive",
            reqwest::multipart::Part::bytes(bytes).file_name("campaign.zip"),
        );
        let response = self
            .http
            .post(self.url("/api/sessions"))
            .bearer_auth(host_token)
            .multipart(form)
            .send()
            .await;
        json(response).await
    }

    pub async fn session_info(&self, id: SessionId) -> Result<SessionInfo, NetError> {
        json(
            self.http
                .get(self.url(&format!("/api/sessions/{id}")))
                .send()
                .await,
        )
        .await
    }

    pub async fn join_gm(&self, id: SessionId, gm_key: &str) -> Result<Joined, NetError> {
        self.join(id, serde_json::json!({ "gm_key": gm_key })).await
    }

    pub async fn join_player(
        &self,
        id: SessionId,
        access_code: &str,
        name: &str,
    ) -> Result<Joined, NetError> {
        self.join(
            id,
            serde_json::json!({ "access_code": access_code, "name": name }),
        )
        .await
    }

    async fn join(&self, id: SessionId, body: serde_json::Value) -> Result<Joined, NetError> {
        json(
            self.http
                .post(self.url(&format!("/api/sessions/{id}/join")))
                .json(&body)
                .send()
                .await,
        )
        .await
    }

    /// Stops the session (GM token).
    pub async fn stop(&self, id: SessionId, token: &str) -> Result<(), NetError> {
        let response = self
            .http
            .post(self.url(&format!("/api/sessions/{id}/stop")))
            .bearer_auth(token)
            .send()
            .await;
        checked(response).await.map(drop)
    }

    /// Downloads the session's campaign archive into `out` (GM token).
    pub async fn export(&self, id: SessionId, token: &str, out: &Path) -> Result<(), NetError> {
        let response = self
            .http
            .get(self.url(&format!("/api/sessions/{id}/export")))
            .bearer_auth(token)
            .send()
            .await;
        let bytes = checked(response)
            .await?
            .bytes()
            .await
            .map_err(|e| NetError::Transport(e.to_string()))?;
        tokio::fs::write(out, bytes)
            .await
            .map_err(|e| NetError::Transport(e.to_string()))
    }

    /// A served asset file; its bytes must hash to `sha256` (content addressing, ADR-0004).
    pub async fn asset(
        &self,
        id: SessionId,
        token: &str,
        sha256: &str,
    ) -> Result<Vec<u8>, NetError> {
        let response = self
            .http
            .get(self.url(&format!("/api/sessions/{id}/assets/{sha256}")))
            .bearer_auth(token)
            .send()
            .await;
        let bytes = checked(response)
            .await?
            .bytes()
            .await
            .map_err(|e| NetError::Transport(e.to_string()))?;
        let actual: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if actual != sha256 {
            return Err(NetError::Protocol(format!(
                "asset {sha256} arrived with hash {actual}"
            )));
        }
        Ok(bytes.to_vec())
    }
}

async fn checked(
    response: Result<reqwest::Response, reqwest::Error>,
) -> Result<reqwest::Response, NetError> {
    let response = response.map_err(|e| NetError::Transport(e.to_string()))?;
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.json::<ErrorBody>().await.ok();
    Err(NetError::Http {
        status: status.as_u16(),
        code: body
            .as_ref()
            .map_or_else(|| "http_error".to_owned(), |b| b.code.clone()),
        message: body.map(|b| b.message).unwrap_or_default(),
    })
}

async fn json<T: serde::de::DeserializeOwned>(
    response: Result<reqwest::Response, reqwest::Error>,
) -> Result<T, NetError> {
    checked(response)
        .await?
        .json()
        .await
        .map_err(|e| NetError::Protocol(e.to_string()))
}
