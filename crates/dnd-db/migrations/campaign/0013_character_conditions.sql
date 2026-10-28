-- ADR-0024: the conditions of the campaign's system that apply to a character (a JSON array of
-- condition ids).
ALTER TABLE characters ADD COLUMN conditions TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(conditions));
