-- Cloud-issuer OIDC trust store and workload binding tables.
--
-- auth_trusted_issuers: lossless TrustedIssuer column superset for both Human
-- and Workload principal kinds, keyed on the natural composite
-- (data_tenant_id, issuer_url). Stores the full ClientAuth discriminant plus a
-- nullable BYTEA column for the encrypted secret (12-byte nonce || ciphertext
-- for SecretBasic/SecretPost; NULL for PrivateKeyJwt and Public).
--
-- auth_workload_bindings: per-tenant (issuer, subject) → CardRef mapping used
-- by the workload identity resolver. An inter-table FK on the same
-- (data_tenant_id, issuer_url) composite enforces fail-closed delete semantics:
-- deleting an issuer while live bindings reference it raises an FK violation
-- (ON DELETE RESTRICT, Decision 6). A bulk --cascade delete is the only path
-- to remove an issuer with bindings.
--
-- Both tables use a combined USING + WITH CHECK tenant_isolation policy so the
-- wyrd_app role can SELECT, INSERT, UPDATE, and DELETE its own tenant's rows.
-- The 04-seed and 05-CRUD tasks both write these tables; a read-only policy
-- would silently break both.

-- ---------------------------------------------------------------------------
-- Trusted OIDC issuer configuration
-- ---------------------------------------------------------------------------
CREATE TABLE wyrd.auth_trusted_issuers (
    data_tenant_id      UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    issuer_url          TEXT        NOT NULL,
    -- Resolved JWKS endpoint from OIDC discovery or direct configuration.
    jwks_uri            TEXT        NOT NULL,
    expected_audience   TEXT        NOT NULL,
    -- Wyrd's OAuth 2.0 client identifier at this IdP.
    client_id           TEXT        NOT NULL,
    -- ClientAuth discriminant: SecretBasic | SecretPost | PrivateKeyJwt | Public.
    -- The secret lives separately in client_secret_enc (nullable BYTEA).
    client_auth         TEXT        NOT NULL
        CHECK (client_auth IN ('SecretBasic', 'SecretPost', 'PrivateKeyJwt', 'Public')),
    -- Lossless ClaimMapping: subject path required; email and groups optional.
    claim_mapping       JSONB       NOT NULL,
    -- Per-issuer IdP group → Wyrd role mapping (HashMap<String, Vec<String>>).
    group_role_map      JSONB       NOT NULL,
    -- Baseline roles granted to every federated principal from this issuer (Vec<String>).
    default_roles       JSONB       NOT NULL,
    -- Whether tokens represent human users or machine workloads.
    principal_kind      TEXT        NOT NULL
        CHECK (principal_kind IN ('Human', 'Workload')),
    -- JWKS key cache TTL in seconds.
    jwks_ttl_secs       BIGINT      NOT NULL,
    -- Encrypted client secret: 12-byte AES-GCM nonce prepended to ciphertext.
    -- NULL for PrivateKeyJwt and Public variants. BYTEA preserves the nonce;
    -- storing ciphertext-only would make the 04 decrypt path irrecoverable.
    client_secret_enc   BYTEA,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Natural composite key; also the FK target for auth_workload_bindings.
    PRIMARY KEY (data_tenant_id, issuer_url)
);

ALTER TABLE wyrd.auth_trusted_issuers ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_trusted_issuers FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_trusted_issuers
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- ---------------------------------------------------------------------------
-- Workload identity binding: (tenant, issuer, subject) → CardRef
-- ---------------------------------------------------------------------------
CREATE TABLE wyrd.auth_workload_bindings (
    data_tenant_id  UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    issuer_url      TEXT        NOT NULL,
    subject         TEXT        NOT NULL,
    -- Optional audience override for this specific binding.
    audience        TEXT,
    -- Structured CardRef stored as JSONB for full lossless serde round-trip.
    -- Precedent: auth_service_accounts.card_ref JSONB + GIN index.
    card_ref        JSONB       NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Natural composite key.
    PRIMARY KEY (data_tenant_id, issuer_url, subject),
    -- Fail-closed: deleting a referenced issuer raises FK violation (Decision 6).
    -- The (data_tenant_id, issuer_url) composite ensures tenant-scoped matching:
    -- an issuer registered under tenant A cannot satisfy a binding for tenant B.
    FOREIGN KEY (data_tenant_id, issuer_url)
        REFERENCES wyrd.auth_trusted_issuers (data_tenant_id, issuer_url)
        ON DELETE RESTRICT
);

CREATE INDEX auth_workload_bindings_card_ref_gin
    ON wyrd.auth_workload_bindings USING GIN (card_ref);

ALTER TABLE wyrd.auth_workload_bindings ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_workload_bindings FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_workload_bindings
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
