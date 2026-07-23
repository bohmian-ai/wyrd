-- Stage 1 OLAP control tables.
-- vala.bifrost_tables: registry (one row per registered Bifrost table).
-- vala.olap_commits:   2PC anchor (one row per commit attempt).
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


-- 2PC anchor keyed on batch_id. snapshot_id is library-generated and only
-- known AFTER the Iceberg commit (recovery scans snapshot_properties for
-- wyrd_batch_id), so it is nullable and never read for correctness.
-- Stage 1 FSM: precommit / committed / failed only. 'aborted' is Stage-2-only.
CREATE TABLE vala.olap_commits (
    data_tenant_id      UUID    NOT NULL REFERENCES platform.tenants(data_tenant_id),
    table_uid           BYTEA   NOT NULL CHECK (octet_length(table_uid) = 16),
    batch_id            BYTEA   NOT NULL CHECK (octet_length(batch_id) = 16),
    snapshot_id         BIGINT,
    state               TEXT    NOT NULL CHECK (state IN ('precommit', 'committed', 'failed')),
    precommit_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    committed_at        TIMESTAMPTZ,
    finalized_at        TIMESTAMPTZ,
    error_code          TEXT,
    error_detail        TEXT,
    origin              TEXT,
    actor               TEXT,
    PRIMARY KEY (data_tenant_id, table_uid, batch_id),
    FOREIGN KEY (data_tenant_id, table_uid)
        REFERENCES vala.bifrost_tables(data_tenant_id, table_uid)
);

ALTER TABLE vala.olap_commits ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.olap_commits FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.olap_commits
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE INDEX olap_commits_state_idx
    ON vala.olap_commits (data_tenant_id, table_uid, state)
    WHERE state IN ('precommit', 'failed');


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
    ON vala.bifrost_tables, vala.olap_commits, vala.refresh_epochs
    TO wyrd_app;
