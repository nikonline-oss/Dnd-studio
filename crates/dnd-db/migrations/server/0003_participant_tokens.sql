-- ADR-0022: a participant is a user of the session's campaign and proves who it is with a token.
-- Tokens are 256 random bits, stored as SHA-256 (a slow hash adds nothing to random secrets).
ALTER TABLE participants ADD COLUMN user_id TEXT;
ALTER TABLE participants ADD COLUMN token_hash TEXT;
CREATE UNIQUE INDEX participants_token_hash ON participants (token_hash);

-- The campaign name, for the session info page and logs.
ALTER TABLE sessions ADD COLUMN name TEXT NOT NULL DEFAULT '';
