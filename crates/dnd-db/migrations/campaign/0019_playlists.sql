-- Independent ordered audio queues; copied track hashes survive library metadata changes.
CREATE TABLE playlists (
    id TEXT PRIMARY KEY NOT NULL,
    dto TEXT NOT NULL CHECK (json_valid(dto)),
    version INTEGER NOT NULL DEFAULT 1
) STRICT;
