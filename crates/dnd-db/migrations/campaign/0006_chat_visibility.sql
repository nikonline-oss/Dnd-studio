-- Hidden GM rolls (stage 2a): who may see a chat message. The append-only trigger is recreated so
-- the new column is immutable too; detaching the sender (ON DELETE SET NULL) stays allowed.

DROP TRIGGER chat_messages_no_update;
ALTER TABLE chat_messages ADD COLUMN visibility TEXT NOT NULL DEFAULT 'all'
    CHECK (visibility IN ('all', 'gm'));
CREATE TRIGGER chat_messages_no_update BEFORE UPDATE ON chat_messages
WHEN NOT (
    OLD.sender_user_id IS NOT NULL AND NEW.sender_user_id IS NULL
    AND NEW.id IS OLD.id AND NEW.kind IS OLD.kind AND NEW.body IS OLD.body
    AND NEW.whisper_to IS OLD.whisper_to AND NEW.created_at IS OLD.created_at
    AND NEW.sender_name IS OLD.sender_name AND NEW.visibility IS OLD.visibility
)
BEGIN SELECT RAISE(ABORT, 'chat_messages is append-only'); END;
