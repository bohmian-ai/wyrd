-- olap_derivations — the durable registry for cross-table
-- derivations (e.g. the genai-from-spans worker).
--
-- One row per (source table → target table) derivation for a given tenant /
-- control bind. Each row carries a durable WATERMARK: the last source commit
-- position (batch_id) fully consumed by the derivation. This mirrors exactly
-- how vala.olap_commits tracks commit progress — by opaque 16-byte batch_id,
-- never by a numeric snapshot-id compare. A source position is identified by
-- the source table's committed batch_id; advancing the watermark records that
-- everything up to and including that batch has been derived into the target.
--
-- `registered_watermark` is the source position captured at registration: the
-- earliest position the derivation is pinned to. When `watermark` is still
-- NULL (nothing consumed yet), the pin resolves to `registered_watermark` so
-- the worker replays from the registered/earliest position rather than an
-- empty/undefined start.
--
-- RLS mirrors vala.olap_projections: tenant isolation on data_tenant_id via
-- wyrd.current_tenant(). `control_bind` is the logical commit identity that
-- spans staging tenants (see olap_commits control_bind migration); it is stored
-- for parity with the commit path but tenant visibility is governed by
-- data_tenant_id, as in olap_projections.

CREATE TABLE vala.olap_derivations (
    data_tenant_id          UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    -- Unique identifier for this derivation registration.
    derivation_uid          BYTEA       NOT NULL CHECK (octet_length(derivation_uid) = 16),
    -- The source table this derivation reads from.
    source_table_uid        BYTEA       NOT NULL CHECK (octet_length(source_table_uid) = 16),
    -- The target table this derivation writes to (one row per target).
    target_table_uid        BYTEA       NOT NULL CHECK (octet_length(target_table_uid) = 16),
    -- Logical commit identity spanning staging tenants (control_bind of the
    -- source commit path). control_bind = data_tenant_id for TenantOwned tables,
    -- nil UUID for SystemShared tables. Stored for parity with olap_commits.
    control_bind            UUID        NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    -- Durable watermark: the last SOURCE commit position (batch_id) fully
    -- consumed and derived into the target. NULL means nothing consumed yet.
    -- Never compared numerically; it is an opaque commit-position identity that
    -- matches vala.olap_commits.batch_id.
    watermark               BYTEA       CHECK (watermark IS NULL OR octet_length(watermark) = 16),
    -- The source position captured at registration: the earliest position this
    -- derivation is pinned to. When `watermark` is NULL the pin resolves here.
    registered_watermark    BYTEA       CHECK (registered_watermark IS NULL OR octet_length(registered_watermark) = 16),
    -- Health / lifecycle state of the derivation worker for this row.
    derivation_state        TEXT        NOT NULL CHECK (derivation_state IN ('idle', 'deriving', 'failed', 'degraded'))
                            DEFAULT 'idle',
    -- Human-readable identity (namespace.derivation_name), for observability.
    fqn                     TEXT        NOT NULL,
    -- Wall-clock timestamps.
    registered_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Set on each successful watermark advance; drives freshness/lag.
    derived_at              TIMESTAMPTZ,
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (data_tenant_id, derivation_uid),
    -- One derivation per (source, target, control_bind) triple so registry
    -- upserts are idempotent via ON CONFLICT DO NOTHING.
    UNIQUE (data_tenant_id, source_table_uid, target_table_uid, control_bind),
    FOREIGN KEY (data_tenant_id, source_table_uid)
        REFERENCES vala.bifrost_tables(data_tenant_id, table_uid)
);

ALTER TABLE vala.olap_derivations ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.olap_derivations FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.olap_derivations
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- Fast lookup: all derivations reading from a given source table.
CREATE INDEX olap_derivations_source_idx
    ON vala.olap_derivations (data_tenant_id, source_table_uid);

-- Partial index for derivations that still have work (candidate scan).
CREATE INDEX olap_derivations_active_idx
    ON vala.olap_derivations (data_tenant_id, derivation_state)
    WHERE derivation_state IN ('idle', 'deriving');

GRANT SELECT, INSERT, UPDATE, DELETE
    ON vala.olap_derivations
    TO wyrd_app;
