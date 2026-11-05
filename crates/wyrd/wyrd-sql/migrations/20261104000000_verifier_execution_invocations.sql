-- Durable invocation checkpoints of queued Verifier executions.
--
-- One row is one attempt of one logical block invocation: the execution, the
-- step or requirement task, and the zero-based fanout item and repeat
-- iteration when those constructs exist (NULL otherwise), with a one-based
-- attempt. The row commits under the execution's lease before the attempt is
-- dispatched, and its settlement commits under the same lease before the
-- block's dependents run. A reclaimed execution restores every settled
-- invocation instead of running it again and counts each unsettled attempt as
-- spent. Settlements hold bounded block results, never payload bytes.
--
-- `deadline_at` is fixed by the first claim, so a reclaim or restart never
-- replenishes an execution's deadline.
ALTER TABLE wyrd.verifier_executions ADD COLUMN deadline_at TIMESTAMPTZ;

CREATE TABLE wyrd.verifier_execution_invocations (
    data_tenant_id  UUID        NOT NULL,
    execution_id    UUID        NOT NULL,
    task_id         TEXT        NOT NULL,
    item_index      INTEGER     CHECK (item_index >= 0),
    iteration_index INTEGER     CHECK (iteration_index >= 0),
    attempt         INTEGER     NOT NULL CHECK (attempt >= 1),
    settlement      JSONB,
    started_at      TIMESTAMPTZ NOT NULL,
    settled_at      TIMESTAMPTZ,
    FOREIGN KEY (data_tenant_id, execution_id)
        REFERENCES wyrd.verifier_executions(data_tenant_id, execution_id) ON DELETE CASCADE,
    CHECK ((settlement IS NULL) = (settled_at IS NULL))
);

CREATE UNIQUE INDEX verifier_execution_invocations_identity
    ON wyrd.verifier_execution_invocations
       (data_tenant_id, execution_id, task_id, item_index, iteration_index, attempt)
    NULLS NOT DISTINCT;

ALTER TABLE wyrd.verifier_execution_invocations ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.verifier_execution_invocations FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.verifier_execution_invocations
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.verifier_execution_invocations TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());
