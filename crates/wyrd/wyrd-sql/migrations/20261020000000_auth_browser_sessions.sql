-- Rust-owned opaque browser sessions.
--
-- A browser signs in to one tenant either through the tenant's provider
-- (the common callback completes a login whose state is bound to this
-- browser) or with a recovery API key. Either way the server generates a
-- random session identifier, sets it only in a host-only HttpOnly cookie, and
-- stores only its SHA-256 here. A row binds the tenant and principal, the
-- authority the session was created through (exactly one of the human
-- connection revision the sign-in went through, or the recovery API key's
-- credential id), and PostgreSQL-owned creation, absolute expiry and
-- revocation times. No API key, provider token, access token or refresh token
-- is stored. Every request re-resolves the principal's current authority
-- through the referenced connection revision or credential, so deactivating
-- either ends the session at once. A session never slides: twelve hours after
-- creation it is gone. Expired rows are purged when the tenant's next session
-- is created, as login state and device authorizations are.
CREATE TABLE wyrd.auth_browser_sessions (
    session_hash        BYTEA       PRIMARY KEY CHECK (octet_length(session_hash) = 32),
    data_tenant_id      UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    principal_id        UUID        NOT NULL,
    connection_id       UUID,
    connection_revision BIGINT,
    api_key_id          UUID,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    expires_at          TIMESTAMPTZ NOT NULL,
    revoked_at          TIMESTAMPTZ,
    CONSTRAINT auth_browser_sessions_authority
        CHECK ((connection_id IS NULL) = (connection_revision IS NULL)
               AND num_nonnulls(connection_id, api_key_id) = 1),
    CHECK (expires_at > created_at AND expires_at <= created_at + interval '12 hours')
);

CREATE INDEX auth_browser_sessions_expires_at
    ON wyrd.auth_browser_sessions (data_tenant_id, expires_at);

ALTER TABLE wyrd.auth_browser_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_browser_sessions FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_browser_sessions
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.auth_browser_sessions TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

-- A browser login's state is bound to the browser that began it: the
-- SHA-256 of a random login-transaction secret carried only in that browser's
-- short-lived HttpOnly cookie. The callback requires the presented cookie to
-- hash to this value before any provider IO, and on success creates the
-- browser session in the same transaction that records the login.
ALTER TABLE wyrd.auth_login_state
    ADD COLUMN browser_binding_hash BYTEA CHECK (octet_length(browser_binding_hash) = 32);

ALTER TABLE wyrd.auth_login_state DROP CONSTRAINT auth_login_state_initiation;

ALTER TABLE wyrd.auth_login_state
    ADD CONSTRAINT auth_login_state_initiation
        CHECK (num_nonnulls(oauth_client_id, device_id, tester_principal_id,
                            browser_binding_hash) = 1);
