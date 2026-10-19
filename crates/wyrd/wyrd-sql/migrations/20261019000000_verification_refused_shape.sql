-- A group refused before admission is one terminal errored execution that
-- pins nothing: besides its terminal metadata, every input, window, record,
-- and snapshot field is NULL. Constraining the authority-free alternative
-- itself keeps it from also satisfying an ordinary input or snapshot shape.
ALTER TABLE wyrd.verifier_executions
    DROP CONSTRAINT verifier_executions_refused_shape,
    ADD CONSTRAINT verifier_executions_refused_shape CHECK (
        authority IS NOT NULL
        OR (origin = 'group' AND status = 'errored' AND attempts = 0
            AND group_id IS NOT NULL AND result_id IS NULL
            AND lease_token IS NULL AND settled_at IS NOT NULL
            AND window_start IS NULL AND window_end IS NULL
            AND input_record_id IS NULL AND input_event_time IS NULL
            AND inputs IS NULL AND route_overrides IS NULL
            AND group_snapshot_sha256 IS NULL AND group_sources IS NULL)
    );
