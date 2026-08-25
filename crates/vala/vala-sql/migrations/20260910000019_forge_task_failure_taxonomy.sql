-- Durable bounded Forge failure qualification and worker scratch health.

ALTER TABLE vala.forge_tasks
    ADD COLUMN attempt_count integer NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    ADD COLUMN failure_class text CHECK (failure_class IN ('data_refusal','transient_object_store','transient_coordination','storage_health','capacity_refused','internal_invariant')),
    ADD COLUMN next_eligible_at timestamptz NOT NULL DEFAULT statement_timestamp(),
    ADD COLUMN failed_volume_identity text,
    ADD COLUMN envelope_version smallint NOT NULL DEFAULT 0,
    ADD COLUMN decoded_batch_bytes bigint,
    ADD COLUMN decoded_input_bytes bigint,
    ADD COLUMN sort_working_bytes bigint,
    ADD COLUMN sort_merge_reservation_bytes bigint,
    ADD COLUMN encoder_buffer_bytes bigint,
    ADD COLUMN upload_chunk_bytes bigint,
    ADD COLUMN sort_spill_bytes bigint,
    ADD CONSTRAINT forge_task_envelope_shape CHECK (
        (envelope_version = 0 AND decoded_batch_bytes IS NULL AND decoded_input_bytes IS NULL
            AND sort_working_bytes IS NULL AND sort_merge_reservation_bytes IS NULL
            AND encoder_buffer_bytes IS NULL AND upload_chunk_bytes IS NULL
            AND sort_spill_bytes IS NULL)
        OR
        (envelope_version = 1 AND decoded_batch_bytes > 0 AND decoded_input_bytes > 0
            AND sort_working_bytes > 0 AND sort_merge_reservation_bytes > 0
            AND encoder_buffer_bytes > 0 AND upload_chunk_bytes > 0
            AND sort_spill_bytes > 0
            AND decoded_input_bytes = estimated_parallelism * decoded_batch_bytes
            AND sort_working_bytes = 2 * decoded_batch_bytes + sort_merge_reservation_bytes
            AND estimated_memory_bytes = decoded_input_bytes + sort_working_bytes + encoder_buffer_bytes + upload_chunk_bytes
            AND estimated_spill_bytes = sort_spill_bytes)
    );

CREATE TABLE vala.forge_worker_registry (
    worker_id uuid PRIMARY KEY,
    scratch_volume_identity text NOT NULL,
    quarantined boolean NOT NULL DEFAULT false,
    quarantine_reason text CHECK (quarantine_reason IN ('storage_health')),
    quarantined_at timestamptz,
    heartbeat_at timestamptz NOT NULL DEFAULT statement_timestamp()
);

GRANT SELECT, INSERT, UPDATE ON vala.forge_worker_registry TO wyrd_platform_admin;

CREATE INDEX forge_tasks_retry_eligibility
    ON vala.forge_tasks (next_eligible_at, ready_at)
    WHERE state IN ('ready', 'retryable');
