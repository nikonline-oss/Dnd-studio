-- ADR-0024: triggers, stored as their JSON as the core sees them. Firings are only in event_log.
CREATE TABLE triggers (
    id      TEXT PRIMARY KEY NOT NULL,
    dto     TEXT NOT NULL CHECK (json_valid(dto)),
    version INTEGER NOT NULL DEFAULT 1
) STRICT;
