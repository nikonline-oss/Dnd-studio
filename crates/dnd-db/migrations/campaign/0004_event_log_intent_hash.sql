-- SHA-256 (hex) of the intent payload that produced each event, so that reusing an intent_id with
-- different data is detected after a restart too (ADR-0015). NULL for rows written before this
-- migration and for events without an intent.
ALTER TABLE event_log ADD COLUMN intent_hash TEXT
    CHECK (intent_hash IS NULL OR length(intent_hash) = 64);
