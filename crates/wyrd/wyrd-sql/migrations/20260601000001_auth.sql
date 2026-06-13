-- Wyrd tenant-scoped auth schema bootstrap.

CREATE SCHEMA IF NOT EXISTS wyrd;

CREATE TABLE wyrd.auth_users (
    id              TEXT PRIMARY KEY,
    data_tenant_id  UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    email           TEXT NOT NULL,
    password_hash   TEXT,
    auth_type       TEXT NOT NULL CHECK (auth_type IN ('password','oauth_google','oauth_github','service_account')),
    status          TEXT NOT NULL CHECK (status IN ('active','suspended','deleted')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (data_tenant_id, email),
    UNIQUE (data_tenant_id, id)
);

CREATE INDEX auth_users_by_tenant_email
    ON wyrd.auth_users (data_tenant_id, email);
CREATE INDEX auth_users_by_tenant_active
    ON wyrd.auth_users (data_tenant_id) WHERE status = 'active';

ALTER TABLE wyrd.auth_users ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_users FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_users
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE TABLE wyrd.auth_roles (
    id              TEXT PRIMARY KEY,
    data_tenant_id  UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    name            TEXT NOT NULL,
    permissions     JSONB NOT NULL,
    builtin         BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (data_tenant_id, name),
    UNIQUE (data_tenant_id, id)
);

ALTER TABLE wyrd.auth_roles ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_roles FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_roles
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE TABLE wyrd.auth_user_roles (
    data_tenant_id  UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    user_id         TEXT NOT NULL,
    role_id         TEXT NOT NULL,
    granted_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_tenant_id, user_id, role_id),
    FOREIGN KEY (data_tenant_id, user_id)
        REFERENCES wyrd.auth_users(data_tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (data_tenant_id, role_id)
        REFERENCES wyrd.auth_roles(data_tenant_id, id) ON DELETE CASCADE
);

ALTER TABLE wyrd.auth_user_roles ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_user_roles FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_user_roles
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE TABLE wyrd.auth_service_accounts (
    id              TEXT PRIMARY KEY,
    data_tenant_id  UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    name            TEXT NOT NULL,
    owning_space    TEXT NOT NULL,
    description     TEXT,
    status          TEXT NOT NULL CHECK (status IN ('active','suspended','deleted')),
    created_by      TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (data_tenant_id, name),
    UNIQUE (data_tenant_id, id),
    FOREIGN KEY (data_tenant_id, created_by)
        REFERENCES wyrd.auth_users(data_tenant_id, id)
);

CREATE INDEX auth_service_accounts_by_tenant_space
    ON wyrd.auth_service_accounts (data_tenant_id, owning_space);

ALTER TABLE wyrd.auth_service_accounts ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_service_accounts FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_service_accounts
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE TABLE wyrd.auth_service_account_roles (
    data_tenant_id  UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    sa_id           TEXT NOT NULL,
    role_id         TEXT NOT NULL,
    PRIMARY KEY (data_tenant_id, sa_id, role_id),
    FOREIGN KEY (data_tenant_id, sa_id)
        REFERENCES wyrd.auth_service_accounts(data_tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (data_tenant_id, role_id)
        REFERENCES wyrd.auth_roles(data_tenant_id, id) ON DELETE CASCADE
);

ALTER TABLE wyrd.auth_service_account_roles ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_service_account_roles FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_service_account_roles
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE TABLE wyrd.auth_api_keys (
    id              TEXT PRIMARY KEY,
    data_tenant_id  UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    sa_id           TEXT NOT NULL,
    prefix          TEXT NOT NULL,
    key_hash        TEXT NOT NULL,
    created_by      TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at    TIMESTAMPTZ,
    expires_at      TIMESTAMPTZ NOT NULL,
    revoked_at      TIMESTAMPTZ,
    UNIQUE (data_tenant_id, prefix),
    FOREIGN KEY (data_tenant_id, sa_id)
        REFERENCES wyrd.auth_service_accounts(data_tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (data_tenant_id, created_by)
        REFERENCES wyrd.auth_users(data_tenant_id, id)
);

CREATE INDEX auth_api_keys_by_tenant_prefix
    ON wyrd.auth_api_keys (data_tenant_id, prefix);
CREATE INDEX auth_api_keys_active
    ON wyrd.auth_api_keys (data_tenant_id, sa_id) WHERE revoked_at IS NULL;

ALTER TABLE wyrd.auth_api_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_api_keys FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_api_keys
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE TABLE wyrd.auth_refresh_tokens (
    id              TEXT PRIMARY KEY,
    data_tenant_id  UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    user_id         TEXT NOT NULL,
    token_hash      TEXT NOT NULL,
    issued_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at      TIMESTAMPTZ NOT NULL,
    rotated_from    TEXT,
    revoked_at      TIMESTAMPTZ,
    UNIQUE (data_tenant_id, id),
    FOREIGN KEY (data_tenant_id, user_id)
        REFERENCES wyrd.auth_users(data_tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (data_tenant_id, rotated_from)
        REFERENCES wyrd.auth_refresh_tokens(data_tenant_id, id)
);

CREATE INDEX auth_refresh_tokens_by_tenant_user
    ON wyrd.auth_refresh_tokens (data_tenant_id, user_id, expires_at);
CREATE UNIQUE INDEX auth_refresh_tokens_token_hash
    ON wyrd.auth_refresh_tokens (data_tenant_id, token_hash);
CREATE INDEX auth_refresh_tokens_active_hash
    ON wyrd.auth_refresh_tokens (data_tenant_id, token_hash)
    WHERE revoked_at IS NULL;

ALTER TABLE wyrd.auth_refresh_tokens ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_refresh_tokens FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_refresh_tokens
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE TABLE wyrd.auth_governance_tokens (
    token_id            TEXT PRIMARY KEY,
    data_tenant_id      UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    -- CardRef: references a registered Card by its immutable UUIDv7 UID.
    -- A foreign key to wyrd.registry_cards will be added when that table lands.
    card_uid            TEXT NOT NULL
        CHECK (card_uid ~ '^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[0-9a-f]{4}-[0-9a-f]{12}$'),
    issuer              TEXT NOT NULL,
    issued_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at          TIMESTAMPTZ NOT NULL,
    revoked_at          TIMESTAMPTZ,
    rotated_from        TEXT,
    status              TEXT NOT NULL CHECK (status IN ('active','revoked','expired')),
    workload_binding    TEXT,
    UNIQUE (data_tenant_id, token_id),
    FOREIGN KEY (data_tenant_id, rotated_from)
        REFERENCES wyrd.auth_governance_tokens(data_tenant_id, token_id)
);

CREATE INDEX auth_governance_tokens_by_tenant_card
    ON wyrd.auth_governance_tokens (data_tenant_id, card_uid);

ALTER TABLE wyrd.auth_governance_tokens ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_governance_tokens FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_governance_tokens
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
