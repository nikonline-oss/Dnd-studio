-- Campaign schema v1 (CLAUDE.md §6). Shared by the desktop client and server sessions (ADR-0002).
-- Conventions:
--   * ids are UUID strings; timestamps are unix milliseconds (INTEGER);
--   * booleans are INTEGER 0/1; JSON columns are TEXT validated with json_valid();
--   * every mutable entity has `version INTEGER NOT NULL DEFAULT 1` (ADR-0003);
--   * forward-only migrations.

CREATE TABLE campaign_meta (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) STRICT, WITHOUT ROWID;

CREATE TABLE users (
    id         TEXT PRIMARY KEY NOT NULL,
    name       TEXT NOT NULL,
    role       TEXT NOT NULL CHECK (role IN ('gm', 'co_gm', 'player', 'spectator')),
    created_at INTEGER NOT NULL,
    version    INTEGER NOT NULL DEFAULT 1
) STRICT;

CREATE TABLE assets (
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
    UNIQUE (type, sha256)
) STRICT;

CREATE TABLE worlds (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    sort_order  INTEGER NOT NULL DEFAULT 0,
    version     INTEGER NOT NULL DEFAULT 1
) STRICT;

CREATE TABLE maps (
    id                  TEXT PRIMARY KEY NOT NULL,
    world_id            TEXT REFERENCES worlds (id) ON DELETE SET NULL,
    name                TEXT NOT NULL,
    background_asset_id TEXT REFERENCES assets (id) ON DELETE SET NULL,
    grid_kind           TEXT NOT NULL CHECK (grid_kind IN ('none', 'square', 'hex_flat', 'hex_pointy')),
    grid_cell_size      REAL NOT NULL CHECK (grid_cell_size > 0),
    grid_offset_x       REAL NOT NULL DEFAULT 0,
    grid_offset_y       REAL NOT NULL DEFAULT 0,
    fog                 TEXT CHECK (fog IS NULL OR json_valid(fog)),
    visible_to_players  INTEGER NOT NULL CHECK (visible_to_players IN (0, 1)),
    sort_order          INTEGER NOT NULL DEFAULT 0,
    version             INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX maps_world_id ON maps (world_id);
CREATE INDEX maps_background_asset_id ON maps (background_asset_id);

CREATE TABLE characters (
    id             TEXT PRIMARY KEY NOT NULL,
    kind           TEXT NOT NULL CHECK (kind IN ('pc', 'npc')),
    name           TEXT NOT NULL,
    sheet_template TEXT,
    sheet_data     TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(sheet_data)),
    owner_user_id  TEXT REFERENCES users (id) ON DELETE SET NULL,
    status         TEXT NOT NULL DEFAULT 'draft'
                   CHECK (status IN ('draft', 'pending_approval', 'active', 'archived')),
    version        INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX characters_owner_user_id ON characters (owner_user_id);

CREATE TABLE tokens (
    id           TEXT PRIMARY KEY NOT NULL,
    map_id       TEXT NOT NULL REFERENCES maps (id) ON DELETE CASCADE,
    character_id TEXT REFERENCES characters (id) ON DELETE SET NULL,
    asset_id     TEXT REFERENCES assets (id) ON DELETE SET NULL,
    name         TEXT NOT NULL DEFAULT '',
    x            REAL NOT NULL,
    y            REAL NOT NULL,
    rotation     REAL NOT NULL DEFAULT 0,
    scale        REAL NOT NULL DEFAULT 1 CHECK (scale > 0),
    layer        TEXT NOT NULL CHECK (layer IN ('background', 'tokens', 'gm')),
    visible      INTEGER NOT NULL CHECK (visible IN (0, 1)),
    version      INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX tokens_map_id ON tokens (map_id);
CREATE INDEX tokens_character_id ON tokens (character_id);
CREATE INDEX tokens_asset_id ON tokens (asset_id);

CREATE TABLE journal_entries (
    id               TEXT PRIMARY KEY NOT NULL,
    folder           TEXT NOT NULL DEFAULT '',
    title            TEXT NOT NULL,
    markdown         TEXT NOT NULL DEFAULT '',
    sort_order       INTEGER NOT NULL DEFAULT 0,
    visibility       TEXT NOT NULL DEFAULT 'gm' CHECK (visibility IN ('gm', 'players')),
    players_can_edit INTEGER NOT NULL DEFAULT 0 CHECK (players_can_edit IN (0, 1)),
    author_user_id   TEXT REFERENCES users (id) ON DELETE SET NULL,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    version          INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX journal_entries_folder ON journal_entries (folder, sort_order);
CREATE INDEX journal_entries_author_user_id ON journal_entries (author_user_id);

CREATE TABLE journal_links (
    id                 TEXT PRIMARY KEY NOT NULL,
    source_id          TEXT NOT NULL REFERENCES journal_entries (id) ON DELETE CASCADE,
    target_type        TEXT NOT NULL CHECK (target_type IN
                       ('journal_entry', 'map', 'token', 'character', 'compendium_entry', 'asset', 'world')),
    target_id          TEXT NOT NULL,
    link_type          TEXT NOT NULL DEFAULT 'reference',
    directed           INTEGER NOT NULL DEFAULT 1 CHECK (directed IN (0, 1)),
    weight             REAL NOT NULL DEFAULT 1,
    label              TEXT,
    meta               TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(meta)),
    visible_to_players INTEGER NOT NULL DEFAULT 0 CHECK (visible_to_players IN (0, 1)),
    version            INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX journal_links_source_id ON journal_links (source_id);
CREATE INDEX journal_links_target ON journal_links (target_type, target_id);

CREATE TABLE compendia (
    id         TEXT PRIMARY KEY NOT NULL,
    plugin_id  TEXT,
    name       TEXT NOT NULL,
    entry_type TEXT NOT NULL,
    version    INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX compendia_plugin_id ON compendia (plugin_id);

CREATE TABLE compendium_entries (
    id            TEXT PRIMARY KEY NOT NULL,
    compendium_id TEXT NOT NULL REFERENCES compendia (id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    data          TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(data)),
    sort_order    INTEGER NOT NULL DEFAULT 0,
    version       INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX compendium_entries_compendium_id ON compendium_entries (compendium_id, sort_order);

-- `version` is the plugin's semver (§6); `row_version` is the optimistic-concurrency version.
CREATE TABLE installed_plugins (
    id             TEXT PRIMARY KEY NOT NULL,
    version        TEXT NOT NULL,
    active         INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    config         TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(config)),
    installed_at   INTEGER NOT NULL,
    compat_warning TEXT,
    row_version    INTEGER NOT NULL DEFAULT 1
) STRICT;

-- Append-only.
CREATE TABLE chat_messages (
    id             TEXT PRIMARY KEY NOT NULL,
    sender_user_id TEXT REFERENCES users (id) ON DELETE SET NULL,
    kind           TEXT NOT NULL CHECK (kind IN ('text', 'system', 'roll')),
    body           TEXT NOT NULL,
    whisper_to     TEXT CHECK (whisper_to IS NULL OR json_valid(whisper_to)),
    created_at     INTEGER NOT NULL
) STRICT;
CREATE INDEX chat_messages_created_at ON chat_messages (created_at);
CREATE TRIGGER chat_messages_no_update BEFORE UPDATE ON chat_messages
BEGIN SELECT RAISE(ABORT, 'chat_messages is append-only'); END;
CREATE TRIGGER chat_messages_no_delete BEFORE DELETE ON chat_messages
BEGIN SELECT RAISE(ABORT, 'chat_messages is append-only'); END;

-- Append-only.
CREATE TABLE dice_rolls (
    id              TEXT PRIMARY KEY NOT NULL,
    user_id         TEXT REFERENCES users (id) ON DELETE SET NULL,
    expression      TEXT NOT NULL,
    result          TEXT NOT NULL CHECK (json_valid(result)),
    total           INTEGER NOT NULL,
    chat_message_id TEXT REFERENCES chat_messages (id),
    created_at      INTEGER NOT NULL
) STRICT;
CREATE INDEX dice_rolls_user_id ON dice_rolls (user_id);
CREATE INDEX dice_rolls_chat_message_id ON dice_rolls (chat_message_id);
CREATE TRIGGER dice_rolls_no_update BEFORE UPDATE ON dice_rolls
BEGIN SELECT RAISE(ABORT, 'dice_rolls is append-only'); END;

-- One row per event; `seq` is the protocol sequence number. Events are never edited.
CREATE TABLE event_log (
    seq           INTEGER PRIMARY KEY NOT NULL CHECK (seq > 0),
    intent_id     TEXT,
    actor_user_id TEXT,
    event         TEXT NOT NULL CHECK (json_valid(event)),
    created_at    INTEGER NOT NULL
) STRICT;
CREATE INDEX event_log_intent_id ON event_log (intent_id);
CREATE TRIGGER event_log_no_update BEFORE UPDATE ON event_log
BEGIN SELECT RAISE(ABORT, 'event_log is append-only'); END;

-- Client only (ADR-0004): GM edits made offline while the campaign lives on a server.
CREATE TABLE pending_ops (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    intent       TEXT NOT NULL CHECK (json_valid(intent)),
    base_version INTEGER,
    status       TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'conflict')),
    conflict     TEXT CHECK (conflict IS NULL OR json_valid(conflict)),
    created_at   INTEGER NOT NULL
) STRICT;
