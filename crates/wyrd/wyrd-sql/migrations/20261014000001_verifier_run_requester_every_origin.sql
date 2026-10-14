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
