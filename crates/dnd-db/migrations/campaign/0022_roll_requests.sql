-- Manual roll requests triggers asked for, stored as their JSON as the core sees them.
CREATE TABLE roll_requests (
    id      TEXT PRIMARY KEY NOT NULL,
    dto     TEXT NOT NULL CHECK (json_valid(dto)),
    version INTEGER NOT NULL DEFAULT 1
) STRICT;
