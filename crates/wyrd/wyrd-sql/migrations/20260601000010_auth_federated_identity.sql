-- Federated human identity and login-state support for OIDC human login.

CREATE TABLE wyrd.auth_user_identities (
    data_tenant_id  UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    issuer          TEXT NOT NULL,
    subject         TEXT NOT NULL,
    user_id         UUID NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_tenant_id, issuer, subject),
    FOREIGN KEY (data_tenant_id, user_id)
        REFERENCES wyrd.auth_users(data_tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX auth_user_identities_by_user
    ON wyrd.auth_user_identities (data_tenant_id, user_id);

ALTER TABLE wyrd.auth_user_identities ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_user_identities FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_user_identities
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

ALTER TABLE wyrd.auth_users ALTER COLUMN email DROP NOT NULL;
ALTER TABLE wyrd.auth_users DROP CONSTRAINT auth_users_auth_type_check;
ALTER TABLE wyrd.auth_users ADD CONSTRAINT auth_users_auth_type_check
    CHECK (auth_type IN ('password','oidc'));

CREATE TABLE wyrd.auth_login_state (
    data_tenant_id  UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    state           TEXT NOT NULL,
    code_verifier   TEXT NOT NULL,
    nonce           TEXT NOT NULL,
    issuer          TEXT NOT NULL,
    redirect_uri    TEXT NOT NULL,
    expires_at      TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_tenant_id, state)
);

CREATE INDEX auth_login_state_expires_at
    ON wyrd.auth_login_state (expires_at);

ALTER TABLE wyrd.auth_login_state ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_login_state FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_login_state
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
