-- ADR-0020: combat (initiative order). Tables beyond CLAUDE.md §6, added there with this ADR.
-- One combat per map; it goes away with its map. A combatant outlives its token or character
-- (the reference is cleared) and goes away with its combat.
CREATE TABLE combats (
    id                TEXT PRIMARY KEY NOT NULL,
    map_id            TEXT NOT NULL UNIQUE REFERENCES maps (id) ON DELETE CASCADE,
    round             INTEGER NOT NULL CHECK (round >= 1),
    -- Not a foreign key (combatants reference combats); the core keeps it valid.
    turn_combatant_id TEXT,
    version           INTEGER NOT NULL DEFAULT 1
) STRICT;

CREATE TABLE combatants (
    id           TEXT PRIMARY KEY NOT NULL,
    combat_id    TEXT NOT NULL REFERENCES combats (id) ON DELETE CASCADE,
    token_id     TEXT REFERENCES tokens (id) ON DELETE SET NULL,
    character_id TEXT REFERENCES characters (id) ON DELETE SET NULL,
    name         TEXT NOT NULL,
    initiative   INTEGER,
    sort_order   INTEGER NOT NULL,
    version      INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX combatants_combat_id ON combatants (combat_id);
CREATE INDEX combatants_token_id ON combatants (token_id);
CREATE INDEX combatants_character_id ON combatants (character_id);
