-- A single original may be a tiled map AND a smaller library picture (ADR-0027).
-- Rebuilding assets fires SET NULL on maps/tokens, even with deferred foreign keys.
-- Preserve and restore those references explicitly inside the migration transaction.
CREATE TEMP TABLE asset_map_references AS
    SELECT id, background_asset_id FROM maps WHERE background_asset_id IS NOT NULL;
CREATE TEMP TABLE asset_token_references AS
    SELECT id, asset_id FROM tokens WHERE asset_id IS NOT NULL;

CREATE TABLE assets_v18 (
    id               TEXT PRIMARY KEY NOT NULL,
    type             TEXT NOT NULL CHECK (type IN ('image', 'audio', 'video', 'document', 'other')),
    sha256           TEXT NOT NULL CHECK (length(sha256) = 64),
    mime             TEXT NOT NULL,
    size_bytes       INTEGER NOT NULL CHECK (size_bytes >= 0),
    original_name    TEXT,
    width            INTEGER,
    height           INTEGER,
    thumbnail_sha256 TEXT CHECK (thumbnail_sha256 IS NULL OR length(thumbnail_sha256) = 64),
    created_at       INTEGER NOT NULL,
    version          INTEGER NOT NULL DEFAULT 1,
    usage            TEXT NOT NULL DEFAULT 'map' CHECK (usage IN ('map', 'token')),
    display_sha256   TEXT CHECK (display_sha256 IS NULL OR length(display_sha256) = 64),
    tiles            TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(tiles)),
    folder_id        TEXT REFERENCES library_folders (id) ON DELETE SET NULL,
    UNIQUE (type, usage, sha256)
) STRICT;
INSERT INTO assets_v18 SELECT * FROM assets;
DROP TABLE assets;
ALTER TABLE assets_v18 RENAME TO assets;
CREATE INDEX assets_folder_id ON assets (folder_id);
UPDATE maps SET background_asset_id = (
    SELECT background_asset_id FROM asset_map_references WHERE asset_map_references.id = maps.id
) WHERE id IN (SELECT id FROM asset_map_references);
UPDATE tokens SET asset_id = (
    SELECT asset_id FROM asset_token_references WHERE asset_token_references.id = tokens.id
) WHERE id IN (SELECT id FROM asset_token_references);
DROP TABLE asset_map_references;
DROP TABLE asset_token_references;
