-- ADR-0026: folders of the image and audio libraries (the row keeps what cascades need, the rest
-- is the entity's JSON), and the folder of each asset. Deleting a folder deletes its subfolders;
-- its assets go back to the library's root.
CREATE TABLE library_folders (
    id        TEXT PRIMARY KEY NOT NULL,
    parent_id TEXT REFERENCES library_folders (id) ON DELETE CASCADE,
    dto       TEXT NOT NULL CHECK (json_valid(dto)),
    version   INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX library_folders_parent_id ON library_folders (parent_id);

ALTER TABLE assets ADD COLUMN folder_id TEXT REFERENCES library_folders (id) ON DELETE SET NULL;
CREATE INDEX assets_folder_id ON assets (folder_id);

-- Sounds are assets of type 'audio' (allowed since 0001). The `usage` column's CHECK from 0007
-- cannot gain 'audio' without rebuilding `assets`, which would null the map and token references
-- to it, so audio rows keep `usage = 'token'` and are told apart by `type`.
