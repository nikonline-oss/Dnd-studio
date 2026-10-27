-- ADR-0024: zones on maps, watched by triggers. They go with their map.
CREATE TABLE zones (
    id      TEXT PRIMARY KEY NOT NULL,
    map_id  TEXT NOT NULL REFERENCES maps (id) ON DELETE CASCADE,
    dto     TEXT NOT NULL CHECK (json_valid(dto)),
    version INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX zones_map_id ON zones (map_id);
