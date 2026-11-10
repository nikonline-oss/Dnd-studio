-- ADR-0061: a token may override which fields players see in the inspector.
-- NULL means "follow the campaign policy"; the campaign's own policy lives in campaign_meta.

ALTER TABLE tokens ADD COLUMN inspector TEXT
    CHECK (inspector IS NULL OR json_valid(inspector));
