ALTER TABLE chat_messages ADD COLUMN related_characters TEXT NOT NULL DEFAULT '[]'
    CHECK (json_valid(related_characters));
