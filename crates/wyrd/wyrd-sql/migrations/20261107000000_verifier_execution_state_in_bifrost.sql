-- Verifier execution state moves to Bifrost.
--
-- An execution's invocation attempts, accepted publications, and selected
-- terminal outcome are now one recoverable state, each accepted revision one
-- `bifrost.verification.execution_events` row fenced by its writer's
-- `vala.scribe_state_heads` claim. PostgreSQL keeps only queue claims,
-- leases, deadlines, and input custody, so the per-execution invocation,
-- publication, and event copies, their history triggers, and the
-- publication accounting columns are dropped.
--
-- Worker format 2 reads execution state only from Bifrost. A format-1
-- execution that already recorded an invocation or publication here would be
-- claimed by a format-2 worker as fresh, losing that custody, so the
-- migration refuses to run while any such row exists: drain format-1 work
-- with format-1 workers first. Executions without recorded progress move to
-- format 2 unchanged.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM wyrd.verifier_execution_invocations)
       OR EXISTS (SELECT 1 FROM wyrd.verifier_execution_publications) THEN
        RAISE EXCEPTION 'format-1 Verifier executions still hold invocation or publication custody; drain them before upgrading';
    END IF;
END
$$;

DROP TRIGGER verifier_executions_record_insert_event ON wyrd.verifier_executions;
DROP TRIGGER verifier_executions_record_update_event ON wyrd.verifier_executions;
DROP TABLE wyrd.verifier_execution_events;
DROP TABLE wyrd.verifier_execution_publications;
DROP TABLE wyrd.verifier_execution_invocations;
DROP FUNCTION wyrd.record_verifier_execution_event();
DROP FUNCTION wyrd.record_verifier_invocation_event();
DROP FUNCTION wyrd.record_verifier_publication_event();

ALTER TABLE wyrd.verifier_executions
    DROP COLUMN publication_count,
    DROP COLUMN publication_bytes,
    ALTER COLUMN worker_format SET DEFAULT 2;
UPDATE wyrd.verifier_executions SET worker_format = 2 WHERE worker_format = 1;
