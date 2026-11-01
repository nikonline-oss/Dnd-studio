-- Tokens carry the GM's side marking (ally / neutral / enemy).
ALTER TABLE tokens
    ADD COLUMN disposition TEXT NOT NULL DEFAULT 'neutral'
        CHECK (disposition IN ('ally', 'neutral', 'enemy'));
