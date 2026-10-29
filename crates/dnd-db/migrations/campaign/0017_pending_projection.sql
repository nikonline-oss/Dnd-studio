-- Recover the provisional local view after refreshing the authoritative database.
ALTER TABLE pending_ops ADD COLUMN projected INTEGER NOT NULL DEFAULT 0 CHECK (projected IN (0, 1));
UPDATE pending_ops SET projected = 1
WHERE EXISTS (SELECT 1 FROM event_log WHERE intent_id = json_extract(pending_ops.intent, '$.intent_id'));
INSERT OR REPLACE INTO campaign_meta (key, value)
SELECT 'local.speculative', '1' WHERE EXISTS (SELECT 1 FROM pending_ops WHERE projected = 1);
