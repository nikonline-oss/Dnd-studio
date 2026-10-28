-- ADR-0026: a journal entry may be a table instead of a text: its columns and cells as JSON
-- ({"columns": [{"name", "width"}], "rows": [[cell]]}); NULL for a text entry.
ALTER TABLE journal_entries ADD COLUMN table_data TEXT CHECK (table_data IS NULL OR json_valid(table_data));
