-- Stage-5 slice 04: olap_projections scaffolding.
-- Projection candidates: one row per registered projection (Rollup, MV, LookupSet).
-- Freshness tracking drives the matcher/rewriter substitution decision.
-- source_schema_fingerprint is the F14 guard input (source-vs-source only).

CREATE TABLE vala.olap_projections (
    data_tenant_id              UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    -- Unique identifier for this projection registration.
    projection_uid              BYTEA       NOT NULL CHECK (octet_length(projection_uid) = 16),
    -- The source table this projection is derived from.
    source_table_uid            BYTEA       NOT NULL CHECK (octet_length(source_table_uid) = 16),
    -- Human-readable name (namespace.projection_name).
    fqn                         TEXT        NOT NULL,
    -- One of: 'rollup', 'mv', 'lookup_set'
    projection_kind             TEXT        NOT NULL CHECK (projection_kind IN ('rollup', 'mv', 'lookup_set')),
    -- Current lifecycle state.
    projection_state            TEXT        NOT NULL CHECK (projection_state IN ('idle', 'refreshing', 'failed', 'degraded'))
                                DEFAULT 'idle',
    -- The refresh epoch at the time of last successful refresh.
    -- Must equal source_refresh_epoch for a LookupSet to be substitutable (exact freshness).
    refresh_epoch               BIGINT      NOT NULL DEFAULT 0,
    -- The source table's refresh epoch at last successful refresh.
    source_refresh_epoch        BIGINT      NOT NULL DEFAULT 0,
    -- Iceberg snapshot id the projection was built for (LookupSet exact-freshness pin).
    -- Never compared numerically; used as an identity anchor.
    built_for_snapshot_id       BIGINT,
    -- Most recent snapshot id used during refresh (informational).
    last_refreshed_snapshot_id  BIGINT,
    -- Commit-lag counter: number of source commits since last refresh.
    -- Rollup/MV staleness decisions use this knob.
    commit_lag                  BIGINT      NOT NULL DEFAULT 0,
    -- F14: source schema fingerprint at last refresh (source-vs-source guard only).
    source_schema_fingerprint   BYTEA       NOT NULL CHECK (octet_length(source_schema_fingerprint) = 32),
    -- Projection output schema fingerprint (not used for F14; informational).
    projection_schema_fingerprint BYTEA,
    -- Wall-clock timestamps.
    registered_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    refreshed_at                TIMESTAMPTZ,
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (data_tenant_id, projection_uid),
    FOREIGN KEY (data_tenant_id, source_table_uid)
        REFERENCES vala.bifrost_tables(data_tenant_id, table_uid)
);

ALTER TABLE vala.olap_projections ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.olap_projections FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.olap_projections
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- Fast lookup: all projections for a given source table (matcher's primary access pattern).
CREATE INDEX olap_projections_source_idx
    ON vala.olap_projections (data_tenant_id, source_table_uid);

-- Partial index for projections in non-terminal states (refresh worker).
CREATE INDEX olap_projections_active_idx
    ON vala.olap_projections (data_tenant_id, projection_state)
    WHERE projection_state IN ('idle', 'refreshing');

GRANT SELECT, INSERT, UPDATE, DELETE
    ON vala.olap_projections
    TO wyrd_app;
