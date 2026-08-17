-- Durable per-batch Scribe acknowledgment fence. The row and its canonical
-- audit event commit together under the caller's tenant-bound transaction.
CREATE TABLE vala.scribe_batch_commits (
    data_tenant_id uuid NOT NULL REFERENCES platform.tenants(data_tenant_id),
    logical_table_fqn text NOT NULL,
    batch_id uuid NOT NULL,
    slice_set_digest bytea NOT NULL CHECK (octet_length(slice_set_digest) = 32),
    slice_count integer NOT NULL CHECK (slice_count > 0),
    wal_node_id uuid NOT NULL,
    wal_writer_epoch bigint NOT NULL,
    wal_shard_id smallint NOT NULL CHECK (wal_shard_id >= 0),
    wal_segment_sequence bigint NOT NULL CHECK (wal_segment_sequence >= 0),
    wal_lsn_min bigint NOT NULL CHECK (wal_lsn_min >= 0),
    wal_lsn_max bigint NOT NULL CHECK (wal_lsn_max >= wal_lsn_min),
    request_id uuid NOT NULL,
    committed_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (data_tenant_id, logical_table_fqn, batch_id)
);

ALTER TABLE vala.scribe_batch_commits ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.scribe_batch_commits FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.scribe_batch_commits
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

GRANT SELECT, INSERT ON vala.scribe_batch_commits TO wyrd_app;
