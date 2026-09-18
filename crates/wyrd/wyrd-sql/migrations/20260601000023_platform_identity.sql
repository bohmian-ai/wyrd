-- Human identity at the platform control plane.
--
-- A deployment may let its platform administrators sign in individually instead
-- of sharing the one global credential. Three objects carry that, and all three
-- sit in `platform.*` outside row-level security, because a platform principal
-- has no tenant to key an RLS policy by.
--
-- The connection is deliberately singular. A tenant may trust many issuers; the
-- platform plane trusts at most one, so the entry point selects the connection
-- and the connection selects the principal. There is no post-login chooser and
-- no scope inferred from a token, header, or hostname.
--
-- Identity is pinned, not provisioned. A platform principal is pre-registered
-- against an expected claim by someone who already holds platform authority;
-- the first successful login records `(issuer, subject)` durably and every
-- later login matches on that alone. An unknown subject is denied, so no login
-- can create platform authority.

-- ---------------------------------------------------------------------------
-- Human platform principals
-- ---------------------------------------------------------------------------
-- `user` joins `global_admin` as a platform-scope kind. Kind still fixes scope
-- and grants still fix authority: a human platform principal holds exactly the
-- permissions its grant names, the same as the machine root.
ALTER TABLE platform.principals DROP CONSTRAINT principals_principal_kind_check;
ALTER TABLE platform.principals ADD CONSTRAINT principals_principal_kind_check
    CHECK (principal_kind IN ('global_admin','user'));

-- ---------------------------------------------------------------------------
-- The one platform-scope OIDC connection
-- ---------------------------------------------------------------------------
-- Column shape mirrors `wyrd.auth_trusted_issuers` so the same verification,
-- discovery, and secret-sealing mechanics apply unchanged. `client_secret_enc`
-- is a 12-byte AES-GCM nonce prepended to ciphertext, sealed with the same
-- process key; the plaintext never leaves the decrypt path.
--
-- `singleton` is a one-valued primary key. At most one row can exist, so
-- "replace the connection" is an upsert rather than a delete-and-insert race,
-- and no query has to decide which of several connections the platform plane
-- meant.
CREATE TABLE platform.oidc_connection (
    singleton           BOOLEAN     PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    issuer_url          TEXT        NOT NULL,
    jwks_uri            TEXT        NOT NULL,
    expected_audience   TEXT        NOT NULL,
    client_id           TEXT        NOT NULL,
    client_auth         TEXT        NOT NULL
        CHECK (client_auth IN ('SecretBasic', 'SecretPost', 'PrivateKeyJwt', 'Public')),
    claim_mapping       JSONB       NOT NULL,
    jwks_ttl_secs       BIGINT      NOT NULL,
    client_secret_enc   BYTEA,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- Platform login state
-- ---------------------------------------------------------------------------
-- The tenant equivalent is `wyrd.auth_login_state`, keyed by tenant under RLS.
-- The platform plane has no tenant, so its login state lives here. Single-use
-- consumption and expiry are what make PKCE and the nonce load-bearing.
CREATE TABLE platform.login_state (
    state           TEXT PRIMARY KEY,
    code_verifier   TEXT NOT NULL,
    nonce           TEXT NOT NULL,
    issuer          TEXT NOT NULL,
    redirect_uri    TEXT NOT NULL,
    expires_at      TIMESTAMPTZ NOT NULL
);

CREATE INDEX platform_login_state_expires_at
    ON platform.login_state (expires_at);

-- ---------------------------------------------------------------------------
-- Pinned federated identity for a platform principal
-- ---------------------------------------------------------------------------
-- Pre-registration writes `match_claim` with `subject` still NULL. The first
-- successful login fills `subject` in and from then on the subject alone
-- resolves the principal, so an administrator who changes their email keeps
-- their identity and someone who inherits that email does not acquire it.
--
-- `(issuer, subject)` is unique among filled rows, so one federated identity
-- can never resolve to two platform principals. `principal_id` is unique, so a
-- platform principal pins exactly one identity.
CREATE TABLE platform.principal_identities (
    principal_id  UUID PRIMARY KEY REFERENCES platform.principals(id) ON DELETE CASCADE,
    issuer        TEXT NOT NULL,
    match_claim   TEXT NOT NULL,
    subject       TEXT,
    pinned_at     TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- A pinned row records when it was pinned; an unpinned one records neither.
    CONSTRAINT principal_identities_pin_consistent
        CHECK ((subject IS NULL) = (pinned_at IS NULL))
);

CREATE UNIQUE INDEX platform_principal_identities_subject
    ON platform.principal_identities (issuer, subject) WHERE subject IS NOT NULL;
CREATE UNIQUE INDEX platform_principal_identities_match_claim
    ON platform.principal_identities (issuer, match_claim) WHERE subject IS NULL;

-- ---------------------------------------------------------------------------
-- Privileges
-- ---------------------------------------------------------------------------
-- Only the BYPASSRLS operator role reaches the platform plane. `wyrd_app` is
-- never granted here: that is what makes "no tenant-plane path confers platform
-- authority" a property of the database rather than of the server code.
GRANT SELECT, INSERT, UPDATE, DELETE
    ON platform.oidc_connection,
       platform.login_state,
       platform.principal_identities
    TO wyrd_platform_admin;
