-- ADR-0024: temporary effects on tokens, stored as their JSON as the core sees them.
CREATE TABLE token_effects (
    id      TEXT PRIMARY KEY NOT NULL,
    dto     TEXT NOT NULL CHECK (json_valid(dto)),
    version INTEGER NOT NULL DEFAULT 1
) STRICT;
