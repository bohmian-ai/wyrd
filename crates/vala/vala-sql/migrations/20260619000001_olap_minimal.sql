-- Stage 1 OLAP control tables.
-- vala.bifrost_tables: registry (one row per registered Bifrost table).
-- vala.refresh_epochs: cache invalidation watermark.
-- All tenant-scoped with RLS.

CREATE TABLE vala.bifrost_tables (
    data_tenant_id      UUID    NOT NULL REFERENCES platform.tenants(data_tenant_id),
    table_uid           BYTEA   NOT NULL CHECK (octet_length(table_uid) = 16),
    fqn                 TEXT    NOT NULL,
    fingerprint         BYTEA   NOT NULL CHECK (octet_length(fingerprint) = 32),
    status              TEXT    NOT NULL CHECK (status IN ('active', 'deprecated', 'quarantined')) DEFAULT 'active',
    partition_columns   TEXT[]  NOT NULL DEFAULT ARRAY[]::TEXT[],
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

CREATE TABLE vala.refresh_epochs (
    data_tenant_id      UUID    NOT NULL REFERENCES platform.tenants(data_tenant_id),
    table_uid           BYTEA   NOT NULL CHECK (octet_length(table_uid) = 16),
    epoch               BIGINT  NOT NULL DEFAULT 0,
    bumped_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_tenant_id, table_uid),
    FOREIGN KEY (data_tenant_id, table_uid)
        REFERENCES vala.bifrost_tables(data_tenant_id, table_uid)
);

ALTER TABLE vala.refresh_epochs ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.refresh_epochs FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.refresh_epochs
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

GRANT SELECT, INSERT, UPDATE, DELETE
    ON vala.bifrost_tables, vala.refresh_epochs
    TO wyrd_app;
