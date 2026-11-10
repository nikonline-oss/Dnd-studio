-- ADR-0053: a delta is rebuilt from the log, so a batch boundary must be stored, not guessed from
-- consecutive `seq` (separate commits are consecutive too). Rows written before this migration
-- have no value; a reader treats each such row as its own batch, which is safe (it can only make
-- the replay more conservative, never reveal something that was hidden).
ALTER TABLE event_log ADD COLUMN batch_seq INTEGER;
CREATE INDEX event_log_batch_seq ON event_log (batch_seq);
