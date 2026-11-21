-- Shared social sign-in providers and deployment login settings.
--
-- Authorized by the authorization-model specification, revision 3,
-- "Persistence and authority changes": new platform social-provider
-- configuration records (provider kind, client metadata, sealed secret,
-- tested/enabled state and revision) and new platform application-mail/signup
-- settings records (password availability and the signup switch; SMTP columns
-- join this record with application mail).
--
-- These are deployment-wide platform records, so they carry no tenant key and
-- admit only operator sessions, like every other `platform.*` table. Every
-- table still enables and forces row-level security.

-- ---------------------------------------------------------------------------
-- Shared providers
-- ---------------------------------------------------------------------------
-- One row per closed provider kind. A row is never deleted: replacing the
-- client advances `revision`, clears the test and disables the provider, so a
-- credential anchored to an earlier revision can never match a later one.
-- `client_secret_enc` is a sealing-keyring envelope; the plaintext never
-- leaves the unseal path.
CREATE TABLE platform.social_providers (
    provider            TEXT        PRIMARY KEY CHECK (provider IN ('google', 'github')),
    revision            BIGINT      NOT NULL CHECK (revision > 0),
    client_id           TEXT        NOT NULL CHECK (client_id <> ''),
    client_secret_enc   BYTEA       NOT NULL,
    tested_at           TIMESTAMPTZ,
    enabled             BOOLEAN     NOT NULL DEFAULT FALSE,
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Only a revision a real provider sign-in proved can be offered.
    CONSTRAINT social_providers_enabled_tested CHECK (NOT enabled OR tested_at IS NOT NULL)
);

-- ---------------------------------------------------------------------------
-- Operator provider-test transactions
-- ---------------------------------------------------------------------------
-- A provider test is a real sign-in begun by a platform principal for one
-- exact provider revision. The row is keyed by the SHA-256 of the provider
-- `state`, consumed once before any provider IO, and expires with the
-- PostgreSQL-derived five-minute login window.
CREATE TABLE platform.social_login_state (
    state_hash          BYTEA       PRIMARY KEY CHECK (length(state_hash) = 32),
    provider            TEXT        NOT NULL REFERENCES platform.social_providers(provider),
    provider_revision   BIGINT      NOT NULL,
    redirect_uri        TEXT        NOT NULL,
    code_verifier       TEXT        NOT NULL,
    nonce               TEXT,
    tester_principal_id UUID        NOT NULL REFERENCES platform.principals(id) ON DELETE CASCADE,
    expires_at          TIMESTAMPTZ NOT NULL,
    consumed_at         TIMESTAMPTZ
);

CREATE INDEX platform_social_login_state_expires_at
    ON platform.social_login_state (expires_at);

-- ---------------------------------------------------------------------------
-- Deployment login settings
-- ---------------------------------------------------------------------------
-- One row. Password starts enabled and public signup starts disabled.
CREATE TABLE platform.login_settings (
    singleton           BOOLEAN     PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    password_enabled    BOOLEAN     NOT NULL DEFAULT TRUE,
    signup_enabled      BOOLEAN     NOT NULL DEFAULT FALSE,
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO platform.login_settings (singleton) VALUES (TRUE) ON CONFLICT DO NOTHING;

-- ---------------------------------------------------------------------------
-- Row-level security
-- ---------------------------------------------------------------------------
ALTER TABLE platform.social_providers ENABLE ROW LEVEL SECURITY;
ALTER TABLE platform.social_providers FORCE ROW LEVEL SECURITY;
CREATE POLICY operator_access ON platform.social_providers TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

ALTER TABLE platform.social_login_state ENABLE ROW LEVEL SECURITY;
ALTER TABLE platform.social_login_state FORCE ROW LEVEL SECURITY;
CREATE POLICY operator_access ON platform.social_login_state TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

ALTER TABLE platform.login_settings ENABLE ROW LEVEL SECURITY;
ALTER TABLE platform.login_settings FORCE ROW LEVEL SECURITY;
CREATE POLICY operator_access ON platform.login_settings TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());
