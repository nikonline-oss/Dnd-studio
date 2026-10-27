-- ADR-0024: boards with nodes (notes, links, pictures) and edges. The row keeps the keys the
-- database needs (cascades, order); the rest of the entity is its JSON as the core sees it.
CREATE TABLE boards (
    id         TEXT PRIMARY KEY NOT NULL,
    sort_order INTEGER NOT NULL,
    dto        TEXT NOT NULL CHECK (json_valid(dto)),
    version    INTEGER NOT NULL DEFAULT 1
) STRICT;

CREATE TABLE board_nodes (
    id       TEXT PRIMARY KEY NOT NULL,
    board_id TEXT NOT NULL REFERENCES boards (id) ON DELETE CASCADE,
    dto      TEXT NOT NULL CHECK (json_valid(dto)),
    version  INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX board_nodes_board_id ON board_nodes (board_id);

CREATE TABLE board_edges (
    id       TEXT PRIMARY KEY NOT NULL,
    board_id TEXT NOT NULL REFERENCES boards (id) ON DELETE CASCADE,
    from_id  TEXT NOT NULL REFERENCES board_nodes (id) ON DELETE CASCADE,
    to_id    TEXT NOT NULL REFERENCES board_nodes (id) ON DELETE CASCADE,
    dto      TEXT NOT NULL CHECK (json_valid(dto)),
    version  INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX board_edges_board_id ON board_edges (board_id);
CREATE INDEX board_edges_from_id ON board_edges (from_id);
CREATE INDEX board_edges_to_id ON board_edges (to_id);
