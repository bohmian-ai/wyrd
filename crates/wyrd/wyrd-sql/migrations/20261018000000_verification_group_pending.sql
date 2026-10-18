-- Pending group scheduling for `group_end` bindings.
--
-- One row per tenant, binding, and caller group whose source evidence Scribe
-- acknowledged since the group was last handed to the execution queue.
-- `due_at` is PostgreSQL's statement time of the latest acknowledgement plus
-- the binding's `after`; new activity moves it. The scheduler claims expired
-- rows with `SKIP LOCKED`, pins one snapshot, and deletes the row in the same
-- transaction that enqueues the group execution. The row is scheduling
-- metadata only: evidence stays in Bifrost and snapshots in the queue.
CREATE TABLE wyrd.verification_group_pending (
    data_tenant_id UUID        NOT NULL,
    binding_id     UUID        NOT NULL,
    group_id       TEXT        NOT NULL
        CHECK (octet_length(group_id) BETWEEN 1 AND 256),
    due_at         TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_tenant_id, binding_id, group_id),
    FOREIGN KEY (data_tenant_id, binding_id)
        REFERENCES wyrd.verification_bindings(data_tenant_id, binding_id)
        ON DELETE CASCADE
);

-- The operator due-tenant query scans expired rows across tenants; a
-- tenant's claim scans its own expired rows in claim order.
CREATE INDEX verification_group_pending_due
    ON wyrd.verification_group_pending (due_at, data_tenant_id);
CREATE INDEX verification_group_pending_claim
    ON wyrd.verification_group_pending (data_tenant_id, due_at, binding_id, group_id);

ALTER TABLE wyrd.verification_group_pending ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.verification_group_pending FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.verification_group_pending
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.verification_group_pending TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

-- A pending group whose owner is inactive or whose authority is rejected is
-- recorded as one terminal errored group execution: it asserts no accepted
-- authority, pins no inputs or snapshot, and never runs. Only that shape may
-- omit `authority`; every runnable execution keeps its accepted authority
-- and frozen input.
ALTER TABLE wyrd.verifier_executions ALTER COLUMN authority DROP NOT NULL;
ALTER TABLE wyrd.verifier_executions
    DROP CONSTRAINT verifier_executions_input_shape,
    DROP CONSTRAINT verifier_executions_group_shape,
    ADD CONSTRAINT verifier_executions_refused_shape CHECK (
        authority IS NOT NULL
        OR (origin = 'group' AND status = 'errored' AND attempts = 0
            AND group_id IS NOT NULL AND result_id IS NULL
            AND lease_token IS NULL AND settled_at IS NOT NULL)
    ),
    ADD CONSTRAINT verifier_executions_input_shape CHECK (
        (window_start IS NOT NULL AND window_end IS NOT NULL AND window_start < window_end
         AND input_record_id IS NULL AND input_event_time IS NULL
         AND inputs IS NULL AND route_overrides IS NULL)
        OR (window_start IS NULL AND window_end IS NULL
            AND input_record_id IS NOT NULL AND input_event_time IS NOT NULL
            AND inputs IS NULL AND route_overrides IS NULL)
        OR (window_start IS NULL AND window_end IS NULL
            AND input_record_id IS NULL AND input_event_time IS NULL
            AND origin IN ('manual', 'group')
            AND jsonb_typeof(inputs) = 'object'
            AND jsonb_typeof(route_overrides) = 'object')
        OR (authority IS NULL
            AND window_start IS NULL AND window_end IS NULL
            AND input_record_id IS NULL AND input_event_time IS NULL
            AND inputs IS NULL AND route_overrides IS NULL)
    ),
    ADD CONSTRAINT verifier_executions_group_shape CHECK (
        (origin = 'group' AND authority IS NOT NULL)
            = (octet_length(group_snapshot_sha256) IS NOT DISTINCT FROM 32)
        AND (origin = 'group' AND authority IS NOT NULL)
            = (jsonb_typeof(group_sources) IS NOT DISTINCT FROM 'object')
        AND (origin <> 'group' OR group_id IS NOT NULL)
        AND (group_verdict IS NULL OR (origin = 'group' AND status = 'completed'))
    );
