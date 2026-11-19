-- Per-server Gateway quota usage contributions.
--
-- Each wyrd-server process admits Gateway calls against its own memory and
-- persists what it charged in the background as absolute per-server rows,
-- so a retried or uncertain write replaces its own row instead of adding to
-- it, and no server overwrites another's usage. A process reads the other
-- servers' rows as its shared view; quotas are therefore approximate across
-- servers. Budget spend stays in the idempotent accounting ledger.
--
-- The strict admission tables and functions of 20260924000001 and
-- 20261023000001 are no longer written by current servers and stay for
-- servers of the previous release during a rolling deployment.

-- ---------------------------------------------------------------------------
-- Requests and tokens one server charged to one limit's fixed minute window.
-- ---------------------------------------------------------------------------
CREATE TABLE wyrd.gateway_usage_windows (
    data_tenant_id      UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    -- Canonical JSON of the limit's (subject, target) key.
    limit_key           TEXT        NOT NULL,
    window_start        TIMESTAMPTZ NOT NULL,
    -- Random identity of the contributing server process.
    server_id           UUID        NOT NULL,
    requests            BIGINT      NOT NULL CHECK (requests >= 0),
    tokens              BIGINT      NOT NULL CHECK (tokens >= 0),
    PRIMARY KEY (data_tenant_id, limit_key, window_start, server_id)
);

ALTER TABLE wyrd.gateway_usage_windows ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.gateway_usage_windows FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.gateway_usage_windows
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.gateway_usage_windows TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

-- ---------------------------------------------------------------------------
-- Calls one server is running under one concurrency limit. The server renews
-- the row while it runs calls and deletes it when none remain; a crashed
-- server's row lapses at `expires_at`.
-- ---------------------------------------------------------------------------
CREATE TABLE wyrd.gateway_usage_concurrency (
    data_tenant_id      UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    limit_key           TEXT        NOT NULL,
    server_id           UUID        NOT NULL,
    active              BIGINT      NOT NULL CHECK (active > 0),
    expires_at          TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_tenant_id, limit_key, server_id)
);

ALTER TABLE wyrd.gateway_usage_concurrency ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.gateway_usage_concurrency FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.gateway_usage_concurrency
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.gateway_usage_concurrency TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());
