-- audit_log.session_id is ON DELETE SET NULL, but the append-only trigger blocked that SET NULL,
-- so a session with audit entries could never be deleted. The trigger now allows exactly the
-- detach (non-NULL -> NULL, all other columns unchanged); any other UPDATE is still rejected.

DROP TRIGGER audit_log_no_update;
CREATE TRIGGER audit_log_no_update BEFORE UPDATE ON audit_log
WHEN NOT (
    OLD.session_id IS NOT NULL AND NEW.session_id IS NULL
    AND NEW.id IS OLD.id AND NEW.actor IS OLD.actor AND NEW.action IS OLD.action
    AND NEW.details IS OLD.details AND NEW.created_at IS OLD.created_at
)
BEGIN SELECT RAISE(ABORT, 'audit_log is append-only'); END;
