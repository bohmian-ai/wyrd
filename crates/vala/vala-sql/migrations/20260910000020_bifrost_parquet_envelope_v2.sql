-- Writer-v2 Parquet file identity.

ALTER TABLE vala.file_list
    ADD COLUMN file_ordinal smallint NOT NULL DEFAULT 0 CHECK (file_ordinal >= 0),
    ADD COLUMN file_checksum text CHECK (file_checksum IS NULL OR file_checksum ~ '^[0-9a-f]{64}$');

DROP INDEX vala.file_list_stream_range_uniq;
CREATE UNIQUE INDEX file_list_stream_range_ordinal_uniq
    ON vala.file_list (
        data_tenant_id, node_id, writer_epoch, wal_lsn_min, wal_lsn_max, file_ordinal
    );
