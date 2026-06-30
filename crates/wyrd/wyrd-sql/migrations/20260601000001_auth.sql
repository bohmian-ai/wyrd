-- Wyrd tenant-scoped auth schema bootstrap.
--
-- Identity model (architecture/wyrd-design.md:100-127):
--   - PrincipalId is a UUID; discrimination lives on PrincipalKind, NOT in the id.
--   - Service / Agent principals are upserted by `wyrd apply` keyed on
--     (data_tenant_id, principal_kind, card_kind, card_uid). Re-apply preserves
--     principal_id.
--   - Refresh tokens are principal-generic: one table for User/Service/Agent.
--   - API keys carry the tenant in the prefix (wyrd_sk_<tenant_b64u>_<random>);
--     RLS scope is established from the prefix BEFORE hash verification.

CREATE SCHEMA IF NOT EXISTS wyrd;

-- ---------------------------------------------------------------------------
-- Users (human identity)
-- ---------------------------------------------------------------------------
CREATE TABLE wyrd.auth_users (
    id              UUID PRIMARY KEY,
    data_tenant_id  UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    email           TEXT NOT NULL,
    password_hash   TEXT,
    auth_type       TEXT NOT NULL CHECK (auth_type IN ('password','oauth_google','oauth_github')),
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

-- ---------------------------------------------------------------------------
-- Roles (RBAC)
-- ---------------------------------------------------------------------------
CREATE TABLE wyrd.auth_roles (
    id              UUID PRIMARY KEY,
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

-- ---------------------------------------------------------------------------
-- Non-human principals (Service / Agent) — card-bound
-- ---------------------------------------------------------------------------
-- Upserted by `wyrd apply` keyed on (data_tenant_id, principal_kind, card_kind, card_uid).
-- Re-apply preserves principal_id. card_ref JSONB is a denormalized projection
-- of the structured CardRef for lookup ergonomics — the durable key is
-- (card_kind, card_uid).
CREATE TABLE wyrd.auth_service_accounts (
    id              UUID PRIMARY KEY,
    data_tenant_id  UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    principal_kind  TEXT NOT NULL CHECK (principal_kind IN ('service','agent')),
    card_kind       TEXT NOT NULL CHECK (card_kind IN ('Service','Agent')),
    card_uid        UUID NOT NULL,
    card_ref        JSONB NOT NULL,
    space           TEXT NOT NULL,
    name            TEXT NOT NULL,
    version         TEXT NOT NULL,
    description     TEXT,
    status          TEXT NOT NULL CHECK (status IN ('active','suspended','deleted')),
    created_by      UUID NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (data_tenant_id, id),
    UNIQUE (data_tenant_id, principal_kind, card_kind, card_uid),
    UNIQUE (data_tenant_id, name),
    CHECK ((principal_kind = 'service' AND card_kind = 'Service')
        OR (principal_kind = 'agent'   AND card_kind = 'Agent')),
    FOREIGN KEY (data_tenant_id, created_by)
        REFERENCES wyrd.auth_users(data_tenant_id, id)
);

CREATE INDEX auth_service_accounts_card_ref_gin
    ON wyrd.auth_service_accounts USING GIN (card_ref);
CREATE INDEX auth_service_accounts_by_tenant_active
    ON wyrd.auth_service_accounts (data_tenant_id) WHERE status = 'active';

ALTER TABLE wyrd.auth_service_accounts ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_service_accounts FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_service_accounts
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- ---------------------------------------------------------------------------
-- Role grants (user-to-role and service-account-to-role)
-- ---------------------------------------------------------------------------
CREATE TABLE wyrd.auth_user_roles (
    data_tenant_id  UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    user_id         UUID NOT NULL,
    role_id         UUID NOT NULL,
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

CREATE TABLE wyrd.auth_service_account_roles (
    data_tenant_id        UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    service_account_id    UUID NOT NULL,
    role_id               UUID NOT NULL,
    granted_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_tenant_id, service_account_id, role_id),
    FOREIGN KEY (data_tenant_id, service_account_id)
        REFERENCES wyrd.auth_service_accounts(data_tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (data_tenant_id, role_id)
        REFERENCES wyrd.auth_roles(data_tenant_id, id) ON DELETE CASCADE
);

ALTER TABLE wyrd.auth_service_account_roles ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_service_account_roles FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_service_account_roles
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- ---------------------------------------------------------------------------
-- API keys (bootstrap credential for Service / Agent pods)
-- ---------------------------------------------------------------------------
-- Tenant is encoded in the key prefix; server parses tenant before this lookup
-- and runs the lookup under normal RLS scope. The prefix is the lookup key;
-- the full 74-char plaintext is Argon2-verified against key_hash inside RLS.
CREATE TABLE wyrd.auth_api_keys (
    id              UUID PRIMARY KEY,
    data_tenant_id  UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    sa_id           UUID NOT NULL,
    prefix          TEXT NOT NULL,
    key_hash        TEXT NOT NULL,
    created_by      UUID NOT NULL,
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

-- ---------------------------------------------------------------------------
-- Refresh tokens — principal-generic
-- ---------------------------------------------------------------------------
-- Single table covers User, Service, and Agent refresh chains.
-- (principal_kind, principal_id) is the owner key. principal_id matches
-- auth_users.id for users and auth_service_accounts.id for service/agent.
CREATE TABLE wyrd.auth_refresh_tokens (
    id              UUID PRIMARY KEY,
    data_tenant_id  UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    principal_kind  TEXT NOT NULL CHECK (principal_kind IN ('user','service','agent')),
    principal_id    UUID NOT NULL,
    token_hash      TEXT NOT NULL,
    issued_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at      TIMESTAMPTZ NOT NULL,
    rotated_from    UUID,
    revoked_at      TIMESTAMPTZ,
    revoked_reason  TEXT,
    UNIQUE (data_tenant_id, id),
    FOREIGN KEY (data_tenant_id, rotated_from)
        REFERENCES wyrd.auth_refresh_tokens(data_tenant_id, id)
);

CREATE INDEX auth_refresh_tokens_by_owner
    ON wyrd.auth_refresh_tokens (data_tenant_id, principal_kind, principal_id, expires_at);
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
