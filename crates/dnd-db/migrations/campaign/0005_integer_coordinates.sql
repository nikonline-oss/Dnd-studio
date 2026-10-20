-- ADR-0016: maps get a fixed logical size; token positions become INTEGER in 1/16 logical units.
-- Existing maps get 4000×3000. Existing positions are scaled by 16, rounded and clamped onto the
-- map. STRICT tables cannot change a column type, so `tokens` is rebuilt (nothing references it
-- through a foreign key).

ALTER TABLE maps ADD COLUMN width INTEGER NOT NULL DEFAULT 4000
    CHECK (width BETWEEN 1 AND 65536);
ALTER TABLE maps ADD COLUMN height INTEGER NOT NULL DEFAULT 3000
    CHECK (height BETWEEN 1 AND 65536);

CREATE TABLE tokens_v5 (
    id           TEXT PRIMARY KEY NOT NULL,
    map_id       TEXT NOT NULL REFERENCES maps (id) ON DELETE CASCADE,
    character_id TEXT REFERENCES characters (id) ON DELETE SET NULL,
    asset_id     TEXT REFERENCES assets (id) ON DELETE SET NULL,
    name         TEXT NOT NULL DEFAULT '',
    x            INTEGER NOT NULL CHECK (x >= 0),
    y            INTEGER NOT NULL CHECK (y >= 0),
    rotation     REAL NOT NULL DEFAULT 0,
    scale        REAL NOT NULL DEFAULT 1 CHECK (scale > 0),
    layer        TEXT NOT NULL CHECK (layer IN ('background', 'tokens', 'gm')),
    visible      INTEGER NOT NULL CHECK (visible IN (0, 1)),
    version      INTEGER NOT NULL DEFAULT 1
) STRICT;

INSERT INTO tokens_v5 (id, map_id, character_id, asset_id, name, x, y, rotation, scale, layer,
                       visible, version)
SELECT t.id, t.map_id, t.character_id, t.asset_id, t.name,
       MAX(0, MIN(CAST(round(t.x * 16) AS INTEGER), m.width * 16)),
       MAX(0, MIN(CAST(round(t.y * 16) AS INTEGER), m.height * 16)),
       t.rotation, t.scale, t.layer, t.visible, t.version
FROM tokens AS t JOIN maps AS m ON m.id = t.map_id;

DROP TABLE tokens;
ALTER TABLE tokens_v5 RENAME TO tokens;
CREATE INDEX tokens_map_id ON tokens (map_id);
CREATE INDEX tokens_character_id ON tokens (character_id);
CREATE INDEX tokens_asset_id ON tokens (asset_id);
