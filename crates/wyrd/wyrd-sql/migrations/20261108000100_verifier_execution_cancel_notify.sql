-- Cancelling a running Verifier execution notifies the runner holding it.
--
-- A cancellation or revocation fences the run's lease in this table, but the
-- process holding that lease, on any replica, learns of it only when it next
-- renews. Each running execution that settles `cancelled` notifies
-- `wyrd_verification_cancelled` with payload '<data_tenant_id>', so every
-- runner renews its held leases at once, finds the fenced token gone, and
-- stops the run and frees what it holds. NOTIFY is transactional, so a
-- runner hears it only after the cancellation commits, and identical
-- payloads in one transaction, as a principal's revocation writes, are
-- delivered once.

CREATE FUNCTION wyrd.notify_verifier_execution_cancelled() RETURNS trigger
LANGUAGE plpgsql
SET search_path = ''
AS $$
BEGIN
    PERFORM pg_catalog.pg_notify('wyrd_verification_cancelled', NEW.data_tenant_id::text);
    RETURN NULL;
END
$$;

CREATE TRIGGER verifier_executions_notify_cancelled
    AFTER UPDATE OF status ON wyrd.verifier_executions
    FOR EACH ROW
    WHEN (OLD.status = 'running' AND NEW.status = 'cancelled')
    EXECUTE FUNCTION wyrd.notify_verifier_execution_cancelled();
