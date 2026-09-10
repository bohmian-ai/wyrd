-- Stage 1 OLAP control table.
-- vala.bifrost_tables: registry (one row per registered Bifrost table).
-- Tenant-scoped with RLS.

CREATE TABLE vala.bifrost_tables (
    data_tenant_id      UUID    NOT NULL REFERENCES platform.tenants(data_tenant_id),
    table_uid           BYTEA   NOT NULL CHECK (octet_length(table_uid) = 16),
    fqn                 TEXT    NOT NULL,
    fingerprint         BYTEA   NOT NULL CHECK (octet_length(fingerprint) = 32),
    status              TEXT    NOT NULL CHECK (status IN ('active', 'deprecated', 'quarantined')) DEFAULT 'active',
    -- Canonical PhysicalLayoutWire object: the exact JSON the describe route
    -- returns. Three keys, in this order: partition_granularity ('hour' or
    -- 'day', the system-owned partition on wyrd_event_time), and the fully
    -- populated sort_keys and bloom_columns arrays. No key names a partition
    -- column, and no SQL predicate reads inside this document.
    physical_layout     JSONB   NOT NULL,
    registered_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    origin              TEXT,
    actor               TEXT,
    PRIMARY KEY (data_tenant_id, table_uid),
    UNIQUE (data_tenant_id, fqn)
);

ALTER TABLE vala.bifrost_tables ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.bifrost_tables FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.bifrost_tables
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE INDEX bifrost_tables_fingerprint_idx
    ON vala.bifrost_tables (data_tenant_id, fingerprint);

GRANT SELECT, INSERT, UPDATE, DELETE
    ON vala.bifrost_tables
    TO wyrd_app;
