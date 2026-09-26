-- Tenant gateway administration: provider credentials, provider deployments,
-- tenant policies, versioned model pricing, and the append-only accounting
-- ledger whose pricing references keep pricing history alive.
--
-- Every table is tenant-owned and FORCE RLS. Referential rules are enforced by
-- composite foreign keys rather than application checks:
--
-- * A deployment's credential reference names a credential in the same tenant
--   with the same provider. ON DELETE/UPDATE RESTRICT makes deleting a
--   referenced credential, or replacing it with a different provider, a
--   conflict; there is no cascade.
-- * A priced ledger entry references its exact pricing version. Pricing is
--   retired, never removed, and ON DELETE RESTRICT backs that for history.

-- ---------------------------------------------------------------------------
-- Provider credentials
-- ---------------------------------------------------------------------------
CREATE TABLE wyrd.gateway_provider_credentials (
    data_tenant_id      UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    name                TEXT        NOT NULL,
    provider            TEXT        NOT NULL,
    -- Redacted ProviderCredentialSourceView JSON. Operator-owned sources name
    -- authority only; a ManagedSecret source is the bare unit view, and its
    -- value lives solely in the sealed envelope columns below.
    source              JSONB       NOT NULL,
    -- Tenant keyring version that sealed the envelope. Key material itself is
    -- never persisted: it is operator configuration read from the environment
    -- or a mounted file.
    secret_key_version  TEXT,
    -- AES-GCM nonce of the sealed payload.
    secret_nonce        BYTEA,
    -- Sealed payload binding tenant, name, provider, and the provider key.
    secret_ciphertext   BYTEA,
    state               TEXT        NOT NULL CHECK (state IN ('active', 'revoked')),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    rotated_at          TIMESTAMPTZ,
    revoked_at          TIMESTAMPTZ,
    PRIMARY KEY (data_tenant_id, name),
    -- Composite FK target so deployments bind both name and provider.
    UNIQUE (data_tenant_id, name, provider),
    CHECK ((state = 'revoked') = (revoked_at IS NOT NULL)),
    -- Exactly the managed source carries an envelope, and an envelope is
    -- always complete, so no row can hold a partial or orphaned ciphertext.
    CHECK ((source = '"managed_secret"'::jsonb) = (secret_ciphertext IS NOT NULL)),
    CHECK ((secret_key_version IS NULL) = (secret_ciphertext IS NULL)),
    CHECK ((secret_nonce IS NULL) = (secret_ciphertext IS NULL)),
    CHECK (secret_nonce IS NULL OR octet_length(secret_nonce) = 12)
);

ALTER TABLE wyrd.gateway_provider_credentials ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.gateway_provider_credentials FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.gateway_provider_credentials
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- ---------------------------------------------------------------------------
-- Provider deployments
-- ---------------------------------------------------------------------------
CREATE TABLE wyrd.gateway_provider_deployments (
    data_tenant_id      UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    name                TEXT        NOT NULL,
    provider            TEXT        NOT NULL,
    model               TEXT        NOT NULL,
    -- NULL when the deployment uses no upstream authentication; MATCH SIMPLE
    -- then skips the credential foreign key.
    credential_name     TEXT,
    -- Complete ProviderDeployment JSON.
    deployment          JSONB       NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_tenant_id, name),
    FOREIGN KEY (data_tenant_id, credential_name, provider)
        REFERENCES wyrd.gateway_provider_credentials (data_tenant_id, name, provider)
        ON DELETE RESTRICT ON UPDATE RESTRICT
);

CREATE INDEX gateway_provider_deployments_credential_idx
    ON wyrd.gateway_provider_deployments (data_tenant_id, credential_name, provider);
CREATE INDEX gateway_provider_deployments_model_idx
    ON wyrd.gateway_provider_deployments (data_tenant_id, provider, model);

ALTER TABLE wyrd.gateway_provider_deployments ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.gateway_provider_deployments FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.gateway_provider_deployments
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- ---------------------------------------------------------------------------
-- Tenant policies: one row per tenant, also the policy write lock.
-- ---------------------------------------------------------------------------
CREATE TABLE wyrd.gateway_policies (
    data_tenant_id      UUID        NOT NULL PRIMARY KEY REFERENCES platform.tenants(data_tenant_id),
    -- GatewayFallbackPolicy JSON; NULL is the empty default.
    fallback            JSONB,
    -- GatewayGovernancePolicy JSON without pricing; NULL is the empty default.
    governance          JSONB,
    -- GatewayCapturePolicy JSON with its version; NULL is the version-1 default.
    capture             JSONB,
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE wyrd.gateway_policies ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.gateway_policies FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.gateway_policies
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- ---------------------------------------------------------------------------
-- Versioned model pricing owned by the governance policy.
-- ---------------------------------------------------------------------------
CREATE TABLE wyrd.gateway_model_pricing (
    data_tenant_id      UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    provider            TEXT        NOT NULL,
    model               TEXT        NOT NULL,
    version             TEXT        NOT NULL,
    effective_at        TIMESTAMPTZ NOT NULL,
    active              BOOLEAN     NOT NULL,
    -- Complete GatewayModelPricing JSON; `active` above is authoritative.
    entry               JSONB       NOT NULL,
    PRIMARY KEY (data_tenant_id, provider, model, version),
    UNIQUE (data_tenant_id, provider, model, effective_at)
);

ALTER TABLE wyrd.gateway_model_pricing ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.gateway_model_pricing FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.gateway_model_pricing
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- ---------------------------------------------------------------------------
-- Append-only accounting ledger.
-- ---------------------------------------------------------------------------
CREATE TABLE wyrd.gateway_accounting_entries (
    data_tenant_id      UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    entry_id            UUID        NOT NULL,
    call_id             UUID        NOT NULL,
    kind                TEXT        NOT NULL CHECK (kind IN (
        'budget_reservation_created',
        'budget_reservation_settled',
        'attempt_accounted',
        'call_accounted'
    )),
    -- Priced attempt entries only: the exact pricing version used.
    provider            TEXT,
    model               TEXT,
    pricing_version     TEXT,
    -- Complete GatewayAccountingEntryV1 JSON.
    entry               JSONB       NOT NULL,
    recorded_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_tenant_id, entry_id),
    CHECK (pricing_version IS NULL OR (provider IS NOT NULL AND model IS NOT NULL)),
    FOREIGN KEY (data_tenant_id, provider, model, pricing_version)
        REFERENCES wyrd.gateway_model_pricing (data_tenant_id, provider, model, version)
        ON DELETE RESTRICT ON UPDATE RESTRICT
);

CREATE INDEX gateway_accounting_entries_pricing_idx
    ON wyrd.gateway_accounting_entries (data_tenant_id, provider, model, pricing_version)
    WHERE pricing_version IS NOT NULL;

ALTER TABLE wyrd.gateway_accounting_entries ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.gateway_accounting_entries FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.gateway_accounting_entries
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- The ledger is append-only for the runtime role.
REVOKE UPDATE, DELETE ON wyrd.gateway_accounting_entries FROM wyrd_app;
