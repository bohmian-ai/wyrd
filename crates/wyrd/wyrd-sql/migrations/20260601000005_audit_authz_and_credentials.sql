-- Durable audit rows for authz checks, credential issuance, and token exchange.
-- Numbered after 20260601000004_auth_builtin_role_immutability.sql on this branch.

ALTER TABLE wyrd.auth_api_keys
    ADD CONSTRAINT auth_api_keys_data_tenant_id_id_key UNIQUE (data_tenant_id, id);

CREATE TABLE wyrd.audit_credential_issuance (
    id                    UUID         PRIMARY KEY,
    data_tenant_id        UUID         NOT NULL REFERENCES platform.tenants(data_tenant_id),
    issued_at             TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    issuer_principal_id   UUID         NOT NULL,
    target_sa_id          UUID         NOT NULL,
    api_key_id            UUID         NOT NULL,
    request_id            TEXT         NOT NULL,
    expires_at            TIMESTAMPTZ  NOT NULL,
    FOREIGN KEY (data_tenant_id, api_key_id)
        REFERENCES wyrd.auth_api_keys(data_tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (data_tenant_id, target_sa_id)
        REFERENCES wyrd.auth_service_accounts(data_tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX audit_credential_issuance_by_target
    ON wyrd.audit_credential_issuance (data_tenant_id, target_sa_id, issued_at DESC);

ALTER TABLE wyrd.audit_credential_issuance ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.audit_credential_issuance FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.audit_credential_issuance
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY admin_cross_tenant
    ON wyrd.audit_credential_issuance
    TO wyrd_platform_admin
    USING (true)
    WITH CHECK (true);

CREATE TABLE wyrd.audit_token_exchange (
    id                    UUID         PRIMARY KEY,
    data_tenant_id        UUID         NOT NULL REFERENCES platform.tenants(data_tenant_id),
    issued_at             TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    subject_principal_id  UUID         NOT NULL,
    actor_principal_id    UUID         NOT NULL,
    act_chain             JSONB        NOT NULL,
    request_id            TEXT         NOT NULL,
    expires_at            TIMESTAMPTZ  NOT NULL
);

CREATE INDEX audit_token_exchange_by_subject
    ON wyrd.audit_token_exchange (data_tenant_id, subject_principal_id, issued_at DESC);

ALTER TABLE wyrd.audit_token_exchange ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.audit_token_exchange FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.audit_token_exchange
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY admin_cross_tenant
    ON wyrd.audit_token_exchange
    TO wyrd_platform_admin
    USING (true)
    WITH CHECK (true);

CREATE TABLE wyrd.audit_authz_check (
    id                    BIGSERIAL    PRIMARY KEY,
    data_tenant_id        UUID         NOT NULL,
    wyrd_request_id       TEXT         NOT NULL,
    callee_principal_id   UUID         NOT NULL,
    caller_principal_id   UUID         NOT NULL,
    delegation_chain      JSONB        NOT NULL,
    decision              TEXT         NOT NULL CHECK (decision IN ('allow','deny')),
    deny_reason           TEXT,
    occurred_at           TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    CONSTRAINT audit_authz_check_decision_consistency
        CHECK (
            (decision = 'allow' AND deny_reason IS NULL)
         OR (decision = 'deny'  AND deny_reason IS NOT NULL)
        )
);

CREATE INDEX audit_authz_check_request_id_idx
    ON wyrd.audit_authz_check (wyrd_request_id);
CREATE INDEX audit_authz_check_tenant_occurred_idx
    ON wyrd.audit_authz_check (data_tenant_id, occurred_at DESC);

ALTER TABLE wyrd.audit_authz_check ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.audit_authz_check FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.audit_authz_check
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY admin_cross_tenant
    ON wyrd.audit_authz_check
    TO wyrd_platform_admin
    USING (true)
    WITH CHECK (true);
