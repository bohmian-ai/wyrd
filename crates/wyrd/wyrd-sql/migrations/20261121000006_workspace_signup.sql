-- Public workspace signup (authorization-model spec revision 7, "Persistence
-- and authority changes": new platform pre-tenant signup records and
-- admission counters; platform.tenants and existing provisioning state).
-- The admission scopes are the windows REQ-014 names: signup start in its
-- own shared source window ("Admission before work"), and completion's
-- separate source and proven-identity windows.
--
-- A signup attempt is one visitor's social proof and the creation it may
-- complete. The provider round-trip is keyed by the SHA-256 of its `state`
-- and consumed once; a proven attempt is then held by the SHA-256 of a
-- completion secret only its browser holds, and records the proven identity.
-- A tenant a signup creates names its attempt, so only that attempt can resume
-- the claim after a failure. Admission windows count creation attempts per
-- trusted source and per provider identity, shared by every replica.
--
-- Platform records carry no tenant key and admit only operator sessions; every
-- table still enables and forces row-level security.
CREATE TABLE platform.signup_attempts (
    id                    UUID        PRIMARY KEY,
    state_hash            BYTEA       NOT NULL UNIQUE CHECK (length(state_hash) = 32),
    provider              TEXT        NOT NULL REFERENCES platform.social_providers(provider),
    provider_revision     BIGINT      NOT NULL CHECK (provider_revision > 0),
    redirect_uri          TEXT        NOT NULL,
    code_verifier         TEXT,
    nonce                 TEXT,
    browser_binding_hash  BYTEA       NOT NULL CHECK (length(browser_binding_hash) = 32),
    completion_hash       BYTEA       UNIQUE CHECK (length(completion_hash) = 32),
    issuer                TEXT,
    subject               TEXT,
    email                 TEXT,
    status                TEXT        NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'proven', 'creating', 'failed', 'completed')),
    expires_at            TIMESTAMPTZ NOT NULL,
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    -- A pending attempt still holds its PKCE verifier; a proven one holds the
    -- identity and completion secret instead.
    CONSTRAINT signup_attempts_proof CHECK (
        (status = 'pending') = (code_verifier IS NOT NULL)
        AND (status = 'pending') = (completion_hash IS NULL)
        AND num_nonnulls(issuer, subject) = CASE WHEN status = 'pending' THEN 0 ELSE 2 END
    )
);

CREATE INDEX platform_signup_attempts_expires_at
    ON platform.signup_attempts (expires_at);

CREATE TABLE platform.signup_admissions (
    scope         TEXT        NOT NULL CHECK (scope IN ('start', 'source', 'identity')),
    key           TEXT        NOT NULL,
    window_start  TIMESTAMPTZ NOT NULL,
    attempts      INTEGER     NOT NULL CHECK (attempts > 0),
    PRIMARY KEY (scope, key)
);

ALTER TABLE platform.tenants
    ADD COLUMN signup_attempt_id UUID UNIQUE
        REFERENCES platform.signup_attempts(id) ON DELETE SET NULL;

ALTER TABLE platform.signup_attempts ENABLE ROW LEVEL SECURITY;
ALTER TABLE platform.signup_attempts FORCE ROW LEVEL SECURITY;
CREATE POLICY operator_access ON platform.signup_attempts TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

ALTER TABLE platform.signup_admissions ENABLE ROW LEVEL SECURITY;
ALTER TABLE platform.signup_admissions FORCE ROW LEVEL SECURITY;
CREATE POLICY operator_access ON platform.signup_admissions TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());
