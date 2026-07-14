-- vala.file_list: durable Parquet file index (CONTRACTS §3).
--
-- Scribe INSERTs one row per sealed file; Forge SELECTs + UPDATEs
-- (compacted = true, committed_snapshot_id); Oracle SELECTs for fused scan.
-- RLS on data_tenant_id. One row = one physical Parquet file = one partition_day.
-- Writer stream identity (node_id, writer_epoch) plus (wal_lsn_min, wal_lsn_max)
-- supports Oracle live-tail dedup (CONTRACTS §8).

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
    partition_day         date NOT NULL,
    tenant_bucket         int  NOT NULL,
    compacted             bool NOT NULL DEFAULT false,
    committed_snapshot_id bigint,
    -- Writer stream identity (CONTRACTS §8). (node_id, writer_epoch) identifies
    -- the pod-local WAL stream; wal_lsn_{min,max} are ONLY comparable within that
    -- stream. Cross-stream comparison is meaningless.
    node_id               uuid   NOT NULL,
    writer_epoch          bigint NOT NULL,
    wal_lsn_min           bigint NOT NULL,
    wal_lsn_max           bigint NOT NULL,
    created_at            timestamptz NOT NULL DEFAULT now()
);

-- Forge compaction lookup: uncompacted files grouped by partition.
CREATE INDEX file_list_group_idx
    ON vala.file_list (namespace, table_name, partition_day, tenant_bucket)
    WHERE NOT compacted;

-- Tenant-scoped table queries (RLS enforced).
CREATE INDEX file_list_tenant_idx
    ON vala.file_list (data_tenant_id, namespace, table_name);

-- Per-stream watermark lookup for Oracle live-tail dedup (CONTRACTS §8).
CREATE INDEX file_list_live_tail_watermark_idx
    ON vala.file_list (namespace, table_name, tenant_bucket, node_id, writer_epoch, wal_lsn_max);

-- Duplicate-range guard: prevents replay-driven re-seal from double-inserting
-- the same sealed range (regression guard for restart idempotency).
CREATE UNIQUE INDEX file_list_stream_range_uniq
    ON vala.file_list (node_id, writer_epoch, wal_lsn_min, wal_lsn_max);

-- RLS: cross-tenant SELECT under a tenant-scoped role returns zero rows.
ALTER TABLE vala.file_list ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.file_list FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.file_list
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- OperatorPool grants (wyrd_platform_admin BYPASSRLS). BYPASSRLS skips RLS
-- policies but NOT privilege checks. Scribe INSERTs via TenantConn (RLS-bound);
-- Forge/Oracle system-owner work goes through OperatorPool (audited).
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_platform_admin') THEN
    RAISE EXCEPTION 'wyrd_platform_admin missing — run bootstrap/roles.sql / db:setup-roles';
  END IF;
END $$;

GRANT SELECT, INSERT, UPDATE ON vala.file_list TO wyrd_platform_admin;

-- wyrd_app grants for tenant-scoped work via TenantConn.
GRANT SELECT, INSERT, UPDATE ON vala.file_list TO wyrd_app;
