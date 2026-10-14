-- Every Verifier run records its actor.
--
-- A manual run still freezes its authenticated caller. Work Wyrd starts
-- internally, a scheduled window or an observation-triggered run, records the
-- tenant SYSTEM principal as its actor; its origin is recorded only as
-- references (binding, owner, Trigger, subject, input record), never as a
-- second principal. The original table-level check limited a requester to
-- manual runs; it is unnamed, so it is found by its definition and replaced by
-- one that only requires the manual requester. A tenant without a SYSTEM
-- principal records a null requester on internal runs, so the requester is
-- not required for every origin.
DO $$
DECLARE
    manual_only TEXT;
BEGIN
    SELECT conname INTO manual_only
      FROM pg_constraint
     WHERE conrelid = 'wyrd.verifier_runs'::regclass
       AND contype = 'c'
       AND pg_get_constraintdef(oid) LIKE '%requested_by_principal_id IS NOT NULL%'
       AND pg_get_constraintdef(oid) NOT LIKE '%idempotency_key%';
    IF manual_only IS NOT NULL THEN
        EXECUTE format('ALTER TABLE wyrd.verifier_runs DROP CONSTRAINT %I', manual_only);
    END IF;
END
$$;

ALTER TABLE wyrd.verifier_runs
    ADD CONSTRAINT verifier_runs_manual_requester_check
    CHECK (origin <> 'manual' OR requested_by_principal_id IS NOT NULL);

-- A manual run records the span of the request that enqueued it, so its
-- execution, which runs later in a trace of its own, links to its cause.
-- Internal runs have no enqueuing request and record none.
ALTER TABLE wyrd.verifier_runs
    ADD COLUMN cause_trace_id BYTEA,
    ADD COLUMN cause_span_id BYTEA,
    ADD CONSTRAINT verifier_runs_cause_check CHECK (
        (cause_trace_id IS NULL AND cause_span_id IS NULL)
        OR (octet_length(cause_trace_id) = 16 AND octet_length(cause_span_id) = 8)
    );
