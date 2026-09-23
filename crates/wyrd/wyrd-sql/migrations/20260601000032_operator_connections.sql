-- Operator connections and frozen dispatch failure context.
--
-- `wyrd.operator_connections` is the only home of an Operator credential. Each
-- row stores the nonsecret authority in the clear (`config`, the redacted
-- read shape) and the secret only as envelope ciphertext: the secret under a
-- fresh per-version data key, and that data key wrapped under the tenant's
-- externally held key-encryption key `key_version`. No plaintext secret or
-- key-encryption key is ever written here. Rows are never deleted: DELETE
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
    secret_nonce      BYTEA       NOT NULL CHECK (length(secret_nonce) = 12),
    wrapped_dek       BYTEA       NOT NULL,
    dek_nonce         BYTEA       NOT NULL CHECK (length(dek_nonce) = 12),
    key_version       INTEGER     NOT NULL CHECK (key_version > 0),
    secret_version    INTEGER     NOT NULL CHECK (secret_version > 0),
    created_by        UUID        NOT NULL,
    updated_by        UUID        NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL,
    updated_at        TIMESTAMPTZ NOT NULL,
    UNIQUE (data_tenant_id, connection_id),
    UNIQUE (data_tenant_id, provider, name),
    CHECK (config ->> 'provider' = provider)
);

-- Key rotation finds rows still wrapped under an older key version.
CREATE INDEX operator_connections_key_version
    ON wyrd.operator_connections (data_tenant_id, key_version);

ALTER TABLE wyrd.operator_connections ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.operator_connections FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.operator_connections
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

REVOKE ALL ON TABLE wyrd.operator_connections FROM wyrd_app, wyrd_platform_admin;
GRANT SELECT, INSERT, UPDATE ON wyrd.operator_connections TO wyrd_app;
-- Rotation discovers which tenants and key versions are still referenced;
-- it never needs ciphertext across tenants.
GRANT SELECT (data_tenant_id, key_version) ON wyrd.operator_connections TO wyrd_platform_admin;

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
