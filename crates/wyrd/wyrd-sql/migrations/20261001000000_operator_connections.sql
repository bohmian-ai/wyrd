-- Operator connections and frozen dispatch failure context.
--
-- `wyrd.operator_connections` is the only home of an Operator credential. Each
-- row stores the nonsecret authority in the clear (`config`, the redacted
-- read shape) and the secret only as the common stored-secret envelope in
-- `secret_ciphertext`: the secret under a fresh per-version data key bound to
-- the tenant, connection, provider, name, and `secret_version`, and that data
-- key wrapped by the deployment's configured `[kms]` key. No plaintext secret
-- or wrapping key is ever written here. Rows are never deleted: DELETE
-- disables, so Cards, dispatches, audit, and key rotation keep their lineage.

CREATE TABLE wyrd.operator_connections (
    connection_id     UUID        NOT NULL PRIMARY KEY,
    data_tenant_id    UUID        NOT NULL
        REFERENCES platform.tenants(data_tenant_id),
    provider          TEXT        NOT NULL
        CHECK (provider IN ('slack','pager_duty','http')),
    name              TEXT        NOT NULL,
    config            JSONB       NOT NULL,
    status            TEXT        NOT NULL DEFAULT 'active'
        CHECK (status IN ('active','disabled')),
    secret_ciphertext BYTEA       NOT NULL,
    secret_version    INTEGER     NOT NULL CHECK (secret_version > 0),
    sub_principal_id  UUID        NOT NULL,
    act_principal_id  UUID        NOT NULL,
    updated_by        UUID        NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL,
    updated_at        TIMESTAMPTZ NOT NULL,
    UNIQUE (data_tenant_id, connection_id),
    UNIQUE (data_tenant_id, provider, name),
    CHECK (config ->> 'provider' = provider)
);

ALTER TABLE wyrd.operator_connections ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.operator_connections FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.operator_connections
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.operator_connections TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

-- Rewrap discovers which tenants hold connections; it reads each tenant's
-- envelopes only through that tenant's own session.

-- ---------------------------------------------------------------------------
-- Dispatch failure context
-- ---------------------------------------------------------------------------
-- The bounded, immutable context every attempt renders from, built in the
-- settlement transaction from the completed run and its exact Cards. A row
-- that predates this column keeps '{}', which the worker cannot render and
-- therefore fails terminally instead of sending a payload it cannot bound.
ALTER TABLE wyrd.operator_dispatches
    ADD COLUMN failure_context JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE wyrd.operator_dispatches
    ALTER COLUMN failure_context DROP DEFAULT;

-- Lease-expired dispatches are reclaimed like due ones.
CREATE INDEX operator_dispatches_leased
    ON wyrd.operator_dispatches (data_tenant_id, lease_expires_at)
    WHERE status = 'running';
