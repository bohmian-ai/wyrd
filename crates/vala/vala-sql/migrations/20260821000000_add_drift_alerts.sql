-- vala.drift_alerts — shared mutable alert table for all drift signal types.
-- Alerts are mutable (active flips on acknowledge/resolve), low-volume,
-- upsert-deduped, and accessed by point-lookup (B0/B5).
-- This is NOT a DomainTable; alerts live in Postgres, not Iceberg.

CREATE TABLE vala.drift_alerts (
    data_tenant_id  UUID        NOT NULL,
    drift_ref_kind  TEXT        NOT NULL,
    drift_ref_name  TEXT        NOT NULL,
    drift_ref_ver   TEXT        NOT NULL,
    drift_ref_space TEXT        NOT NULL,
    drift_type      TEXT        NOT NULL,
    series          TEXT,
    alert           JSONB       NOT NULL,
    active          BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (data_tenant_id, drift_ref_kind, drift_ref_name, drift_ref_ver,
                 drift_ref_space, drift_type, COALESCE(series, ''))
);

ALTER TABLE vala.drift_alerts ENABLE ROW LEVEL SECURITY;

CREATE POLICY drift_alerts_tenant ON vala.drift_alerts
    USING (data_tenant_id = current_setting('app.current_tenant')::UUID);

GRANT SELECT, INSERT, UPDATE ON vala.drift_alerts TO vala_app;
