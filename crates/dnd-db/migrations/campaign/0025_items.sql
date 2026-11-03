CREATE TABLE items (
    id TEXT PRIMARY KEY NOT NULL,
    dto TEXT NOT NULL CHECK (json_valid(dto))
) STRICT;
