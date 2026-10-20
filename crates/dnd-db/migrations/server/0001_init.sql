-- Session server index `/data/server.sqlite` v1 (CLAUDE.md §6). Secrets are stored as argon2 hashes only.

CREATE TABLE sessions (
    id               TEXT PRIMARY KEY NOT NULL,
    state            TEXT NOT NULL CHECK (state IN ('creating', 'active', 'stopped', 'expired')),
    params           TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(params)),
    gm_key_hash      TEXT NOT NULL,
    access_code_hash TEXT NOT NULL,
    created_at       INTEGER NOT NULL,
    last_activity    INTEGER NOT NULL,
    storage_dir      TEXT NOT NULL,
    campaign_path    TEXT NOT NULL
) STRICT;
CREATE INDEX sessions_state_last_activity ON sessions (state, last_activity);

CREATE TABLE participants (
    id           TEXT PRIMARY KEY NOT NULL,
    session_id   TEXT NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    role         TEXT NOT NULL CHECK (role IN ('gm', 'co_gm', 'player', 'spectator')),
    name         TEXT NOT NULL,
    connected_at INTEGER,
    status       TEXT NOT NULL CHECK (status IN ('connected', 'disconnected', 'kicked'))
) STRICT;
CREATE INDEX participants_session_id ON participants (session_id);

CREATE TABLE audit_log (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT REFERENCES sessions (id) ON DELETE SET NULL,
    actor      TEXT NOT NULL,
    action     TEXT NOT NULL,
    details    TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(details)),
    created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX audit_log_session_id ON audit_log (session_id, created_at);
CREATE TRIGGER audit_log_no_update BEFORE UPDATE ON audit_log
BEGIN SELECT RAISE(ABORT, 'audit_log is append-only'); END;
