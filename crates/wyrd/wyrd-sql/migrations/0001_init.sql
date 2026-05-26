-- Wyrd control-plane identity, RBAC, and credentials.
-- Stage 0 commits the schema but does not execute or seed it.

CREATE TABLE users (
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT,
    auth_type TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE roles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    scopes JSONB NOT NULL,
    builtin BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE TABLE user_roles (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, role_id)
);

CREATE TABLE service_accounts (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    owning_space TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE service_account_roles (
    sa_id TEXT NOT NULL REFERENCES service_accounts(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    PRIMARY KEY (sa_id, role_id)
);

CREATE TABLE api_keys (
    id TEXT PRIMARY KEY,
    sa_id TEXT NOT NULL REFERENCES service_accounts(id) ON DELETE CASCADE,
    prefix TEXT NOT NULL UNIQUE,
    key_hash TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ
);

CREATE TABLE refresh_tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    rotated_from TEXT REFERENCES refresh_tokens(id),
    revoked_at TIMESTAMPTZ
);

CREATE TABLE governance_tokens (
    token_id TEXT PRIMARY KEY,
    card_uid TEXT NOT NULL,
    issuer TEXT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    status TEXT NOT NULL,
    rotated_from TEXT REFERENCES governance_tokens(token_id),
    revoked_at TIMESTAMPTZ
);

CREATE INDEX idx_api_keys_sa ON api_keys(sa_id);
CREATE INDEX idx_api_keys_prefix ON api_keys(prefix);
CREATE INDEX idx_refresh_tokens_user ON refresh_tokens(user_id);
CREATE INDEX idx_governance_tokens_card ON governance_tokens(card_uid);
