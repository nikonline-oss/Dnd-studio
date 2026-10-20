-- Chat messages and dice rolls keep the author's name as it was when they were written, so the
-- history stays readable after the user is deleted (user id becomes NULL, ON DELETE SET NULL).
-- Existing rows are backfilled from `users`. The append-only triggers are recreated so the new
-- column is immutable as well; detaching the user reference remains the only allowed update.

DROP TRIGGER chat_messages_no_update;
ALTER TABLE chat_messages ADD COLUMN sender_name TEXT NOT NULL DEFAULT '';
UPDATE chat_messages
SET sender_name = COALESCE((SELECT name FROM users WHERE users.id = chat_messages.sender_user_id), '');
CREATE TRIGGER chat_messages_no_update BEFORE UPDATE ON chat_messages
WHEN NOT (
    OLD.sender_user_id IS NOT NULL AND NEW.sender_user_id IS NULL
    AND NEW.id IS OLD.id AND NEW.kind IS OLD.kind AND NEW.body IS OLD.body
    AND NEW.whisper_to IS OLD.whisper_to AND NEW.created_at IS OLD.created_at
    AND NEW.sender_name IS OLD.sender_name
)
BEGIN SELECT RAISE(ABORT, 'chat_messages is append-only'); END;

DROP TRIGGER dice_rolls_no_update;
ALTER TABLE dice_rolls ADD COLUMN user_name TEXT NOT NULL DEFAULT '';
UPDATE dice_rolls
SET user_name = COALESCE((SELECT name FROM users WHERE users.id = dice_rolls.user_id), '');
CREATE TRIGGER dice_rolls_no_update BEFORE UPDATE ON dice_rolls
WHEN NOT (
    OLD.user_id IS NOT NULL AND NEW.user_id IS NULL
    AND NEW.id IS OLD.id AND NEW.expression IS OLD.expression AND NEW.result IS OLD.result
    AND NEW.total IS OLD.total AND NEW.chat_message_id IS OLD.chat_message_id
    AND NEW.created_at IS OLD.created_at AND NEW.user_name IS OLD.user_name
)
BEGIN SELECT RAISE(ABORT, 'dice_rolls is append-only'); END;
