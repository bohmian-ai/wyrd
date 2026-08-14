-- Writer-v2 Parquet identity and phase-aware Forge resource envelopes.

ALTER TABLE vala.forge_tasks
    DROP CONSTRAINT forge_task_envelope_shape,
    ADD COLUMN footer_encoded_bytes bigint,
    ADD COLUMN footer_decode_workspace_bytes bigint,
    ADD CONSTRAINT forge_task_envelope_shape CHECK (
        (envelope_version = 0 AND decoded_batch_bytes IS NULL AND decoded_input_bytes IS NULL
            AND sort_working_bytes IS NULL AND sort_merge_reservation_bytes IS NULL
            AND encoder_buffer_bytes IS NULL AND upload_chunk_bytes IS NULL
            AND footer_encoded_bytes IS NULL AND footer_decode_workspace_bytes IS NULL
            AND sort_spill_bytes IS NULL AND output_scratch_bytes IS NULL)
        OR
        (envelope_version = 1 AND decoded_batch_bytes > 0 AND decoded_input_bytes > 0
            AND sort_working_bytes > 0 AND sort_merge_reservation_bytes > 0
            AND encoder_buffer_bytes > 0 AND upload_chunk_bytes > 0
            AND footer_encoded_bytes IS NULL AND footer_decode_workspace_bytes IS NULL
            AND sort_spill_bytes > 0 AND output_scratch_bytes > 0
            AND decoded_input_bytes = estimated_parallelism * decoded_batch_bytes
            AND sort_working_bytes = 2 * decoded_batch_bytes + sort_merge_reservation_bytes
            AND estimated_memory_bytes = decoded_input_bytes + sort_working_bytes + encoder_buffer_bytes + upload_chunk_bytes
            AND estimated_spill_bytes = sort_spill_bytes + output_scratch_bytes)
        OR
        (envelope_version = 2 AND decoded_batch_bytes > 0 AND decoded_input_bytes > 0
            AND sort_working_bytes > 0 AND sort_merge_reservation_bytes > 0
            AND encoder_buffer_bytes > 0 AND upload_chunk_bytes > 0
            AND footer_encoded_bytes = 8388608 AND footer_decode_workspace_bytes = 33554432
            AND sort_spill_bytes > 0 AND output_scratch_bytes > 0
            AND decoded_input_bytes = estimated_parallelism * decoded_batch_bytes
            AND sort_working_bytes = 2 * decoded_batch_bytes + sort_merge_reservation_bytes
            AND estimated_memory_bytes = GREATEST(
                decoded_input_bytes + sort_working_bytes + encoder_buffer_bytes + upload_chunk_bytes + footer_encoded_bytes,
                footer_encoded_bytes + footer_decode_workspace_bytes)
            AND estimated_spill_bytes = sort_spill_bytes + output_scratch_bytes)
    );

ALTER TABLE vala.file_list
    ADD COLUMN file_ordinal smallint NOT NULL DEFAULT 0 CHECK (file_ordinal >= 0),
    ADD COLUMN file_checksum text CHECK (file_checksum IS NULL OR file_checksum ~ '^[0-9a-f]{64}$');

DROP INDEX vala.file_list_stream_range_uniq;
CREATE UNIQUE INDEX file_list_stream_range_ordinal_uniq
    ON vala.file_list (
        data_tenant_id, node_id, writer_epoch, wal_lsn_min, wal_lsn_max, file_ordinal
    );

CREATE TABLE vala.bifrost_layout_inventory (
    data_tenant_id uuid NOT NULL REFERENCES platform.tenants(data_tenant_id),
    table_uid uuid NOT NULL,
    current_snapshot_id bigint,
    current_schema_id integer NOT NULL,
    metadata_location text NOT NULL,
    unresolved_hot_manifest_digest bytea NOT NULL CHECK (octet_length(unresolved_hot_manifest_digest) = 32),
    table_identity_digest bytea NOT NULL CHECK (octet_length(table_identity_digest) = 32),
    unmarked_file_count bigint NOT NULL CHECK (unmarked_file_count >= 0),
    writer_v1_file_count bigint NOT NULL CHECK (writer_v1_file_count >= 0),
    writer_v2_file_count bigint NOT NULL CHECK (writer_v2_file_count >= 0),
    complete boolean NOT NULL,
    observed_at timestamptz NOT NULL,
    attestation_state text NOT NULL CHECK (attestation_state IN ('unverified', 'verified', 'blocked_legacy')),
    PRIMARY KEY (data_tenant_id, table_uid)
);

CREATE TABLE vala.bifrost_layout_inventory_state (
    inventory_name text PRIMARY KEY CHECK (inventory_name = 'oracle_activation'),
    global_cut_digest bytea NOT NULL CHECK (octet_length(global_cut_digest) = 32),
    roster_count bigint NOT NULL CHECK (roster_count >= 0),
    complete boolean NOT NULL,
    observed_at timestamptz NOT NULL
);

GRANT SELECT, INSERT, UPDATE, DELETE ON vala.bifrost_layout_inventory TO wyrd_platform_admin;
GRANT SELECT, INSERT, UPDATE, DELETE ON vala.bifrost_layout_inventory_state TO wyrd_platform_admin;
