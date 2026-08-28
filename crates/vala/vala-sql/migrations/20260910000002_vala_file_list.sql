-- vala.file_list: durable Parquet file index for Bifrost write/read tier.
--
-- Scribe INSERTs one row per sealed file; Forge SELECTs + UPDATEs
-- (compacted = true, committed_snapshot_id); Oracle SELECTs for fused scan.
-- RLS on data_tenant_id. One row = one physical Parquet file = one time partition.
--
-- Writer stream identity: Each Scribe pod runs a writer stream identified by
-- (node_id, writer_epoch). node_id is the pod's stable UUID; writer_epoch is
-- a per-boot monotonic counter from vala.cluster_nodes.fencing_token. LSNs
-- (wal_lsn_min, wal_lsn_max) are only comparable within one stream — comparing
-- LSNs across pods or across epochs is meaningless. Oracle uses this to compute
-- per-stream watermarks for live-tail dedup.

CREATE TABLE vala.file_list (
    id                    uuid PRIMARY KEY,
    data_tenant_id        uuid NOT NULL REFERENCES platform.tenants(data_tenant_id),
    namespace             text NOT NULL,
    table_name            text NOT NULL,
    file_path             text NOT NULL,
    file_size             bigint NOT NULL,
    row_count             bigint NOT NULL,
    min_event_time        timestamptz NOT NULL,
    max_event_time        timestamptz NOT NULL,
    -- Exact time partition: the granularity token plus its UTC start boundary.
    -- The CHECK makes a noncanonical start unrepresentable, so the pair is a
    -- true identity rather than two loosely related columns.
    partition_granularity text NOT NULL CHECK (partition_granularity IN ('hour', 'day')),
    partition_start       timestamptz NOT NULL,
    CONSTRAINT file_list_partition_start_is_canonical
        CHECK (date_trunc(partition_granularity, partition_start AT TIME ZONE 'UTC')
               = partition_start AT TIME ZONE 'UTC'),
    compacted             bool NOT NULL DEFAULT false,
    committed_snapshot_id bigint,
    -- Writer stream identity: (node_id, writer_epoch) identifies the pod-local
    -- WAL stream; wal_lsn_{min,max} are ONLY comparable within that stream.
    -- Cross-stream comparison is meaningless.
    node_id               uuid   NOT NULL,
    writer_epoch          bigint NOT NULL,
    wal_lsn_min           bigint NOT NULL,
    wal_lsn_max           bigint NOT NULL,
    -- Iceberg-ready promotion evidence for this exact object, as the writer
    -- that closed its footer computed it. Written in the same fenced
    -- transaction as the row and never updated afterward, so a promoter reads
    -- statistics it can trust without reopening the object. NOT NULL: a row
    -- with no evidence would be a file nothing can promote.
    promotion_record      jsonb NOT NULL
        CHECK (jsonb_typeof(promotion_record) = 'object'),
    created_at            timestamptz NOT NULL DEFAULT now()
);

-- Forge compaction lookup: uncompacted files grouped by partition.
CREATE INDEX file_list_group_idx
    ON vala.file_list (data_tenant_id, namespace, table_name,
                       partition_granularity, partition_start)
    WHERE NOT compacted;

-- Tenant-scoped table queries (RLS enforced).
CREATE INDEX file_list_tenant_idx
    ON vala.file_list (data_tenant_id, namespace, table_name);

-- Per-stream watermark lookup for Oracle live-tail dedup.
CREATE INDEX file_list_live_tail_watermark_idx
    ON vala.file_list (data_tenant_id, namespace, table_name,
                       node_id, writer_epoch, wal_lsn_max);

-- Duplicate-range guard: prevents replay-driven re-seal from double-inserting
-- the same sealed range (regression guard for restart idempotency).
CREATE UNIQUE INDEX file_list_stream_range_uniq
    ON vala.file_list (data_tenant_id, node_id, writer_epoch, wal_lsn_min, wal_lsn_max);

-- RLS: cross-tenant SELECT under a tenant-scoped role returns zero rows.
ALTER TABLE vala.file_list ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.file_list FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.file_list
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- OperatorPool grants (wyrd_platform_admin BYPASSRLS). BYPASSRLS skips RLS
-- policies but NOT privilege checks. Scribe INSERTs via TenantConn (RLS-bound);
-- Forge/Oracle maintenance work goes through OperatorPool (audited).
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_platform_admin') THEN
    RAISE EXCEPTION 'wyrd_platform_admin missing — run bootstrap/roles.sql / db:setup-roles';
  END IF;
END $$;

GRANT SELECT, INSERT, UPDATE ON vala.file_list TO wyrd_platform_admin;

-- wyrd_app grants for tenant-scoped work via TenantConn.
GRANT SELECT, INSERT, UPDATE ON vala.file_list TO wyrd_app;
