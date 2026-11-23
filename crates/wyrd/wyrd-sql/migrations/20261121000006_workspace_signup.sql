-- Public workspace signup (authorization-model spec revision 7, "Persistence
-- and authority changes": new platform pre-tenant signup records and
-- admission counters; platform.tenants and existing provisioning state).
-- The admission scopes are the windows REQ-014 names: signup start in its
-- own shared source window ("Admission before work"), and completion's
-- separate source and proven-identity windows. Admission keys and the
-- creator key are 32-byte HMAC-SHA-256 pseudonyms ("no plain identity or
-- address in admission keys"; the creating identity's keyed pseudonym, so the
-- owned-workspace cap survives attempt purge). The expiry indexes serve the
-- bounded background purge ("Pseudonymous, bounded counters", REQ-014).
-- 'reset' and 'reset_account' are password-reset request's separate source
-- and account windows (revision 6, "Recovery and admission metadata": shared
-- counters with email-purpose/source HMAC scopes).
-- Email proofs follow "Proven email signup/discovery/contact proof and
-- one-use redemption": no pre-proof row, one-use identifier digests.
--
-- A signup attempt is one visitor's verified proof and the creation it may
-- complete; nothing is stored before proof ("No pending row before proof",
-- revision 5): the provider round-trip, and an email proof's envelope, ride a
-- sealed browser cookie. A proof is unique on the SHA-256 of its provider
-- state or email identifier, so a replayed or concurrent callback records at
-- most one, and the spent digest stays until expiry. A proof is held by the
-- SHA-256 of a completion secret only its browser holds, and the browser
-- binding's digest lets that browser alone recover a lost response. A social
-- proof records the provider identity; an email proof records only its
-- verified address. A tenant a signup creates names its attempt, so only
-- that attempt can resume the claim after a failure. Admission windows count
-- starts per trusted source and per address, and completions per source and
-- proven identity, shared by every replica ("Apply separate shared source and
-- address/account budgets").
--
-- Platform records carry no tenant key and admit only operator sessions; every
-- table still enables and forces row-level security.
CREATE TABLE platform.signup_attempts (
    id                    UUID        PRIMARY KEY,
    proof_kind            TEXT        NOT NULL CHECK (proof_kind IN ('email', 'social')),
    proof_hash            BYTEA       NOT NULL UNIQUE CHECK (length(proof_hash) = 32),
    provider              TEXT        REFERENCES platform.social_providers(provider),
    provider_revision     BIGINT      CHECK (provider_revision > 0),
    browser_binding_hash  BYTEA       NOT NULL CHECK (length(browser_binding_hash) = 32),
    completion_hash       BYTEA       NOT NULL UNIQUE CHECK (length(completion_hash) = 32),
    issuer                TEXT,
    subject               TEXT,
    email                 TEXT,
    status                TEXT        NOT NULL DEFAULT 'proven'
        CHECK (status IN ('proven', 'creating', 'failed', 'completed')),
    expires_at            TIMESTAMPTZ NOT NULL,
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    -- A social proof names its provider revision and identity; an email
    -- proof names only its verified address, which completion clears.
    CONSTRAINT signup_attempts_proof CHECK (
        CASE proof_kind
            WHEN 'social' THEN num_nonnulls(provider, provider_revision, issuer, subject) = 4
            ELSE num_nonnulls(provider, provider_revision, issuer, subject) = 0
                 AND (email IS NOT NULL OR status = 'completed')
        END
    )
);

CREATE INDEX platform_signup_attempts_expires_at
    ON platform.signup_attempts (expires_at);

CREATE TABLE platform.signup_admissions (
    scope         TEXT        NOT NULL CHECK (scope IN ('start', 'address', 'source', 'identity', 'sign_in', 'reset', 'reset_account')),
    key           BYTEA       NOT NULL CHECK (length(key) = 32),
    window_start  TIMESTAMPTZ NOT NULL,
    attempts      INTEGER     NOT NULL CHECK (attempts > 0),
    PRIMARY KEY (scope, key)
);

CREATE INDEX platform_signup_admissions_window_start
    ON platform.signup_admissions (window_start);

ALTER TABLE platform.tenants
    ADD COLUMN signup_attempt_id UUID UNIQUE
        REFERENCES platform.signup_attempts(id) ON DELETE SET NULL,
    ADD COLUMN signup_creator_key BYTEA
        CHECK (length(signup_creator_key) = 32),
    ADD CONSTRAINT tenants_signup_creator
        CHECK (signup_attempt_id IS NULL OR signup_creator_key IS NOT NULL);

-- The owned-workspace cap counts a creator's unretired signup tenants.
CREATE INDEX platform_tenants_signup_creator_key
    ON platform.tenants (signup_creator_key) WHERE signup_creator_key IS NOT NULL;

-- Each proven identity may own at most this many unretired signup-created
-- tenants (revision 5 owned-workspace cap, default 3).
ALTER TABLE platform.login_settings
    ADD COLUMN signup_workspace_cap INTEGER NOT NULL DEFAULT 3
        CHECK (signup_workspace_cap BETWEEN 1 AND 65535),
    -- The optional Turnstile challenge: site key and sealed secret together
    -- (revision 5 "optional challenge site key with sealed secret").
    ADD COLUMN signup_challenge_site_key TEXT,
    ADD COLUMN signup_challenge_secret BYTEA,
    ADD CONSTRAINT login_settings_signup_challenge
        CHECK ((signup_challenge_site_key IS NULL) = (signup_challenge_secret IS NULL));

ALTER TABLE platform.signup_attempts ENABLE ROW LEVEL SECURITY;
ALTER TABLE platform.signup_attempts FORCE ROW LEVEL SECURITY;
CREATE POLICY operator_access ON platform.signup_attempts TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

ALTER TABLE platform.signup_admissions ENABLE ROW LEVEL SECURITY;
ALTER TABLE platform.signup_admissions FORCE ROW LEVEL SECURITY;
CREATE POLICY operator_access ON platform.signup_admissions TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());
