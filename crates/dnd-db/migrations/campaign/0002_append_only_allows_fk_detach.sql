-- Append-only tables reference users with ON DELETE SET NULL, but their "no update" triggers also
-- blocked that SET NULL, so a user with chat messages or dice rolls could never be deleted.
-- The triggers now allow exactly one change: detaching the user reference (non-NULL -> NULL)
-- with every other column unchanged. Any other UPDATE is still rejected.

DROP TRIGGER chat_messages_no_update;
CREATE TRIGGER chat_messages_no_update BEFORE UPDATE ON chat_messages
WHEN NOT (
    OLD.sender_user_id IS NOT NULL AND NEW.sender_user_id IS NULL
    AND NEW.id IS OLD.id AND NEW.kind IS OLD.kind AND NEW.body IS OLD.body
    AND NEW.whisper_to IS OLD.whisper_to AND NEW.created_at IS OLD.created_at
)
BEGIN SELECT RAISE(ABORT, 'chat_messages is append-only'); END;

DROP TRIGGER dice_rolls_no_update;
CREATE TRIGGER dice_rolls_no_update BEFORE UPDATE ON dice_rolls
WHEN NOT (
    OLD.user_id IS NOT NULL AND NEW.user_id IS NULL
    AND NEW.id IS OLD.id AND NEW.expression IS OLD.expression AND NEW.result IS OLD.result
    AND NEW.total IS OLD.total AND NEW.chat_message_id IS OLD.chat_message_id
    AND NEW.created_at IS OLD.created_at
)
BEGIN SELECT RAISE(ABORT, 'dice_rolls is append-only'); END;
