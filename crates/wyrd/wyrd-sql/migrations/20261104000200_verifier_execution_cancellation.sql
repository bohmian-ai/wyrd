-- Durable cancellation and worker compatibility of queued Verifier executions.
--
-- `worker_format` is the execution record format its admission wrote; a
-- worker claims only records whose format it understands, so a record
-- written for newer checkpoint semantics is never run by an older worker.
-- The format that introduces a newer record changes this default.
--
-- Cancelling a principal's unfinished executions on revocation reads them by
-- subject and by actor; these partial indexes keep that lookup bounded by the
-- principal's unfinished work rather than the tenant's history.
ALTER TABLE wyrd.verifier_executions
    ADD COLUMN worker_format INTEGER NOT NULL DEFAULT 1 CHECK (worker_format >= 1);

CREATE INDEX verifier_executions_unfinished_subject
    ON wyrd.verifier_executions (data_tenant_id, sub_principal_id)
    WHERE status IN ('pending', 'retrying', 'running');

CREATE INDEX verifier_executions_unfinished_actor
    ON wyrd.verifier_executions (data_tenant_id, act_principal_id)
    WHERE status IN ('pending', 'retrying', 'running');
