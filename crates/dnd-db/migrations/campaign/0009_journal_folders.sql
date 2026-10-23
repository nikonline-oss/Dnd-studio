-- ADR-0021: journal folders become rows (they may be empty, are renamed and moved as a whole).
-- Entries reference their folder; deleting a folder deletes its subfolders and entries.
CREATE TABLE journal_folders (
    id         TEXT PRIMARY KEY NOT NULL,
    parent_id  TEXT REFERENCES journal_folders (id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    version    INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX journal_folders_parent_id ON journal_folders (parent_id);

-- The old `folder` path column stays (a STRICT table cannot drop it without a rebuild) and is no
-- longer read; new rows leave it ''.
ALTER TABLE journal_entries
    ADD COLUMN folder_id TEXT REFERENCES journal_folders (id) ON DELETE CASCADE;
CREATE INDEX journal_entries_folder_id ON journal_entries (folder_id, sort_order);

-- Existing paths "a/b/c" become nested folders a > b > c. Every prefix of every path is a folder;
-- ids get the UUIDv7 layout (version 7, RFC variant) from random bits.
CREATE TEMP TABLE migrated_folders AS
WITH RECURSIVE prefixes (path, parent, name, rest, depth) AS (
    SELECT DISTINCT '', NULL, NULL, folder || '/', 0
    FROM journal_entries
    WHERE folder <> ''
    UNION ALL
    SELECT
        CASE WHEN path = '' THEN substr(rest, 1, instr(rest, '/') - 1)
             ELSE path || '/' || substr(rest, 1, instr(rest, '/') - 1) END,
        CASE WHEN path = '' THEN NULL ELSE path END,
        substr(rest, 1, instr(rest, '/') - 1),
        substr(rest, instr(rest, '/') + 1),
        depth + 1
    FROM prefixes
    WHERE rest <> ''
)
SELECT
    path,
    parent,
    name,
    depth,
    lower(
        hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-7' || substr(hex(randomblob(2)), 2)
        || '-' || substr('89ab', 1 + (random() & 3), 1) || substr(hex(randomblob(2)), 2)
        || '-' || hex(randomblob(6))
    ) AS id
FROM (SELECT DISTINCT path, parent, name, depth FROM prefixes WHERE path <> '');

INSERT INTO journal_folders (id, parent_id, name, sort_order, version)
SELECT child.id, parent.id, child.name, 0, 1
FROM migrated_folders AS child
LEFT JOIN migrated_folders AS parent ON parent.path = child.parent
ORDER BY child.depth, child.name;

UPDATE journal_entries
SET folder_id = (SELECT id FROM migrated_folders WHERE migrated_folders.path = journal_entries.folder)
WHERE folder <> '';

DROP TABLE migrated_folders;
