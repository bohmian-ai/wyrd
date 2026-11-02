-- PostgreSQL coordinates verification work and stores no input values. Each
-- execution's input declarations, accepted route overrides, and a group's
-- source selection live once, in its `bifrost.verification.execution_inputs`
-- record, which the runner resolves on every attempt. A row keeps its
-- identities, lease, attempts, status, idempotency key, accepted authority
-- and attribution, Drift window, selected `result_id`, and a group's
-- selection digest.
ALTER TABLE wyrd.verifier_executions
    DROP CONSTRAINT verifier_executions_input_shape,
    DROP CONSTRAINT verifier_executions_run_shape,
    DROP CONSTRAINT verifier_executions_group_shape,
    DROP CONSTRAINT verifier_executions_refused_shape;

ALTER TABLE wyrd.verifier_executions
    DROP COLUMN inputs,
    DROP COLUMN route_overrides,
    DROP COLUMN group_sources;

-- Exactly one frozen input: a Drift window, a historical Eval record, or a
-- caller submission or group selection recorded in Bifrost. A group refused
-- before admission is one terminal errored execution that selects nothing.
ALTER TABLE wyrd.verifier_executions
    ADD CONSTRAINT verifier_executions_input_shape CHECK (
        (window_start IS NOT NULL AND window_end IS NOT NULL AND window_start < window_end
         AND input_record_id IS NULL AND input_event_time IS NULL)
        OR (window_start IS NULL AND window_end IS NULL
            AND input_record_id IS NOT NULL AND input_event_time IS NOT NULL)
        OR (window_start IS NULL AND window_end IS NULL
            AND input_record_id IS NULL AND input_event_time IS NULL
            AND origin IN ('manual', 'group'))
    ),
    ADD CONSTRAINT verifier_executions_run_shape CHECK (
        run_id IS NULL OR origin = 'manual'
    ),
    ADD CONSTRAINT verifier_executions_group_shape CHECK (
        (origin = 'group' AND authority IS NOT NULL)
            = (octet_length(group_snapshot_sha256) IS NOT DISTINCT FROM 32)
        AND (origin <> 'group' OR group_id IS NOT NULL)
        AND (group_verdict IS NULL OR (origin = 'group' AND status = 'completed'))
    ),
    ADD CONSTRAINT verifier_executions_refused_shape CHECK (
        authority IS NOT NULL
        OR (origin = 'group' AND status = 'errored' AND attempts = 0
            AND group_id IS NOT NULL AND result_id IS NULL
            AND lease_token IS NULL AND settled_at IS NOT NULL
            AND window_start IS NULL AND window_end IS NULL
            AND input_record_id IS NULL AND input_event_time IS NULL
            AND group_snapshot_sha256 IS NULL)
    );
