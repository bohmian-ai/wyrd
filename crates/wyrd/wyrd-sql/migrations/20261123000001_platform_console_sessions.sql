-- Platform console sign-in (authorization-model spec revision 7, REQ-018;
-- "Persistence and authority changes", revision-7 purposes).
--
-- "Platform-user local password credential": a platform `user` principal
-- may hold one Argon2id verifier with a password revision. The columns sit on
-- the principal row, so ownership is unique by construction and no new
-- principal kind, tenant credential or person store exists. Advancing the
-- revision is the invalidation state: every browser session anchored to an
-- older revision stops resolving.
ALTER TABLE platform.principals
    ADD COLUMN password_hash     TEXT,
    ADD COLUMN password_revision BIGINT NOT NULL DEFAULT 0
        CHECK (password_revision >= 0),
    ADD CONSTRAINT principals_password_user
        CHECK (password_hash IS NULL OR principal_kind = 'user');

-- "exact password or platform OIDC connection/revision anchor": every
-- install or replacement of the one connection advances its revision, so a
-- session begun through an earlier connection stops resolving.
ALTER TABLE platform.oidc_connection
    ADD COLUMN revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0);

-- "Platform browser session": the SHA-256 of an opaque cookie secret, the
-- principal, exactly one anchor (password revision or connection revision),
-- creation and last activity, the idle and absolute deadlines PostgreSQL
-- derives from bound durations, revocation, and the last fresh human
-- authentication. No raw session identifier, bootstrap key or provider token
-- is stored. The principal and expiry indexes serve suspension and the
-- bounded cleanup.
CREATE TABLE platform.browser_sessions (
    id                   UUID        PRIMARY KEY,
    session_hash         BYTEA       NOT NULL UNIQUE CHECK (length(session_hash) = 32),
    principal_id         UUID        NOT NULL REFERENCES platform.principals(id) ON DELETE CASCADE,
    password_revision    BIGINT      CHECK (password_revision > 0),
    connection_revision  BIGINT      CHECK (connection_revision > 0),
    created_at           TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    last_active_at       TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    idle_expires_at      TIMESTAMPTZ NOT NULL,
    expires_at           TIMESTAMPTZ NOT NULL,
    fresh_at             TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    revoked_at           TIMESTAMPTZ,
    CONSTRAINT browser_sessions_one_anchor
        CHECK (num_nonnulls(password_revision, connection_revision) = 1),
    CONSTRAINT browser_sessions_idle_within_absolute
        CHECK (idle_expires_at <= expires_at)
);

CREATE INDEX platform_browser_sessions_principal
    ON platform.browser_sessions (principal_id);
CREATE INDEX platform_browser_sessions_expires_at
    ON platform.browser_sessions (expires_at);

-- "Platform browser login and re-authentication binding": the existing
-- login state gains its purpose (the API-token login, a browser login, or a
-- same-operator re-authentication), the SHA-256 of the browser binding, the
-- session a re-authentication refreshes and the exact connection revision the
-- login began against. Consumption and expiry stay as they are.
ALTER TABLE platform.login_state
    ADD COLUMN purpose TEXT NOT NULL DEFAULT 'token'
        CHECK (purpose IN ('token', 'login', 'reauth')),
    ADD COLUMN browser_binding_hash BYTEA
        CHECK (length(browser_binding_hash) = 32),
    ADD COLUMN session_id UUID
        REFERENCES platform.browser_sessions(id) ON DELETE CASCADE,
    ADD COLUMN connection_revision BIGINT,
    ADD CONSTRAINT login_state_browser_binding CHECK (
        CASE purpose
            WHEN 'token' THEN num_nonnulls(browser_binding_hash, session_id, connection_revision) = 0
            WHEN 'login' THEN browser_binding_hash IS NOT NULL AND session_id IS NULL
                              AND connection_revision IS NOT NULL
            ELSE num_nonnulls(browser_binding_hash, session_id, connection_revision) = 3
        END
    );

ALTER TABLE platform.browser_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE platform.browser_sessions FORCE ROW LEVEL SECURITY;
CREATE POLICY operator_access ON platform.browser_sessions TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());
