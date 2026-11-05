-- Local password Users, their browser-session authority, enrollment,
-- operator-assisted reset, bounded guessing state, and User-owned API keys.
--
-- A local User is the existing wyrd.auth_users row with a canonical
-- tenant-local username and an Argon2id PHC verifier in the existing
-- password_hash column. The two are present together or not at all, so an
-- SSO User never acquires a password by a matching name. password_revision
-- increases on every password change or reset; a local browser session is
-- bound to the revision it signed in with and stops resolving once it moves.
-- Email uniqueness stays removed and SSO identity stays issuer + subject.
ALTER TABLE wyrd.auth_users
    ADD COLUMN username TEXT
        CONSTRAINT auth_users_username_canonical
        CHECK (username ~ '^[a-z0-9][a-z0-9._-]{2,63}$'),
    ADD COLUMN password_revision BIGINT NOT NULL DEFAULT 0
        CONSTRAINT auth_users_password_revision_nonnegative
        CHECK (password_revision >= 0),
    ADD CONSTRAINT auth_users_local_credential
        CHECK ((username IS NULL) = (password_hash IS NULL));

CREATE UNIQUE INDEX auth_users_tenant_username
    ON wyrd.auth_users (data_tenant_id, username)
    WHERE username IS NOT NULL;

-- A browser session's authority is exactly one of: the human connection
-- revision an SSO sign-in went through, the recovery API key, or the local
-- password revision of its User (principal_id).
ALTER TABLE wyrd.auth_browser_sessions
    ADD COLUMN password_revision BIGINT;

ALTER TABLE wyrd.auth_browser_sessions DROP CONSTRAINT auth_browser_sessions_authority;

ALTER TABLE wyrd.auth_browser_sessions
    ADD CONSTRAINT auth_browser_sessions_authority
        CHECK ((connection_id IS NULL) = (connection_revision IS NULL)
               AND num_nonnulls(connection_id, api_key_id, password_revision) = 1);

-- One-use, twenty-four-hour enrollment invitations. Only the SHA-256 of the
-- 256-bit code is stored. The issuer and Role ids are revalidated when the
-- invitation is consumed, in the transaction that creates the User.
CREATE TABLE wyrd.auth_user_invitations (
    id                  UUID        PRIMARY KEY,
    data_tenant_id      UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    code_hash           BYTEA       NOT NULL CHECK (octet_length(code_hash) = 32),
    issuer_principal_id UUID        NOT NULL,
    role_ids            UUID[]      NOT NULL CHECK (cardinality(role_ids) > 0),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    expires_at          TIMESTAMPTZ NOT NULL,
    revoked_at          TIMESTAMPTZ,
    consumed_at         TIMESTAMPTZ,
    consumed_by         UUID,
    UNIQUE (data_tenant_id, code_hash),
    CHECK (expires_at = created_at + interval '24 hours'),
    CHECK ((consumed_at IS NULL) = (consumed_by IS NULL)),
    FOREIGN KEY (data_tenant_id, consumed_by)
        REFERENCES wyrd.auth_users(data_tenant_id, id)
);

ALTER TABLE wyrd.auth_user_invitations ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_user_invitations FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_user_invitations
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.auth_user_invitations TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

-- One-use, fifteen-minute operator-issued password resets, bound to the User
-- and to the password revision current at issue, so any later change or reset
-- invalidates every outstanding code. Only the code's SHA-256 is stored.
CREATE TABLE wyrd.auth_password_resets (
    id                  UUID        PRIMARY KEY,
    data_tenant_id      UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    code_hash           BYTEA       NOT NULL CHECK (octet_length(code_hash) = 32),
    user_id             UUID        NOT NULL,
    password_revision   BIGINT      NOT NULL,
    issuer_principal_id UUID        NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    expires_at          TIMESTAMPTZ NOT NULL,
    consumed_at         TIMESTAMPTZ,
    UNIQUE (data_tenant_id, code_hash),
    CHECK (expires_at = created_at + interval '15 minutes'),
    FOREIGN KEY (data_tenant_id, user_id)
        REFERENCES wyrd.auth_users(data_tenant_id, id) ON DELETE CASCADE
);

ALTER TABLE wyrd.auth_password_resets ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_password_resets FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_password_resets
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.auth_password_resets TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

-- Fixed-window password failure counters shared by every replica: one row per
-- tenant and canonical username ('account') or trusted connection source
-- ('source'). A window ends a fixed fifteen minutes after its first failure
-- and is never extended; expired rows are purged when the tenant's next
-- failure is recorded.
CREATE TABLE wyrd.auth_password_attempts (
    data_tenant_id  UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    scope           TEXT        NOT NULL CHECK (scope IN ('account', 'source')),
    subject         TEXT        NOT NULL CHECK (octet_length(subject) BETWEEN 1 AND 256),
    failures        INTEGER     NOT NULL CHECK (failures > 0),
    window_ends_at  TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_tenant_id, scope, subject)
);

ALTER TABLE wyrd.auth_password_attempts ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_password_attempts FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_password_attempts
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.auth_password_attempts TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

-- A credential has exactly one tenant-bound relational owner: an existing
-- Service/Admin account or an existing User. principal_id stays the owner's
-- id for every existing reader; the owner columns carry the referential
-- integrity. Every existing key is a Service/Admin key, so the backfill keeps
-- each id, verifier, prefix, expiry and owner unchanged. A User key always has
-- a name, an explicit permission ceiling and an expiry.
ALTER TABLE wyrd.auth_api_keys
    ADD COLUMN service_account_id UUID,
    ADD COLUMN user_id UUID,
    ADD COLUMN name TEXT CHECK (char_length(name) BETWEEN 1 AND 64),
    ADD COLUMN permission_ceiling JSONB CHECK (jsonb_typeof(permission_ceiling) = 'array');

UPDATE wyrd.auth_api_keys SET service_account_id = principal_id;

DO $$
DECLARE
    constraint_name TEXT;
BEGIN
    FOR constraint_name IN
        SELECT conname
          FROM pg_constraint
         WHERE conrelid = 'wyrd.auth_api_keys'::regclass
           AND confrelid = 'wyrd.auth_service_accounts'::regclass
           AND contype = 'f'
    LOOP
        EXECUTE format('ALTER TABLE wyrd.auth_api_keys DROP CONSTRAINT %I', constraint_name);
    END LOOP;
END $$;

ALTER TABLE wyrd.auth_api_keys
    ADD CONSTRAINT auth_api_keys_service_account_owner
        FOREIGN KEY (data_tenant_id, service_account_id)
        REFERENCES wyrd.auth_service_accounts(data_tenant_id, id) ON DELETE CASCADE,
    ADD CONSTRAINT auth_api_keys_user_owner
        FOREIGN KEY (data_tenant_id, user_id)
        REFERENCES wyrd.auth_users(data_tenant_id, id) ON DELETE CASCADE,
    ADD CONSTRAINT auth_api_keys_one_owner
        CHECK (num_nonnulls(service_account_id, user_id) = 1
               AND principal_id = COALESCE(service_account_id, user_id)),
    ADD CONSTRAINT auth_api_keys_user_key_shape
        CHECK ((user_id IS NULL) = (name IS NULL)
               AND (user_id IS NULL) = (permission_ceiling IS NULL)
               AND (user_id IS NULL OR expires_at IS NOT NULL));

CREATE INDEX auth_api_keys_page
    ON wyrd.auth_api_keys (data_tenant_id, principal_id, created_at DESC, id DESC);
