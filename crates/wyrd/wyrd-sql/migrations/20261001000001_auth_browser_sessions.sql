-- Server-owned browser sessions for the production UI BFF.
--
-- The BFF holds only an opaque random session id in an HttpOnly cookie; this
-- row holds the Wyrd credential that id stands for. It is storage for the
-- BFF's session, not a new identity issuer: every access token it carries was
-- minted by the ordinary tenant issuance path.
--
-- id_hash is the SHA-256 of the raw 256-bit session id; the raw id is returned
-- once to the BFF and never stored. Every recoverable credential (access
-- token, refresh token, bootstrap API key, CSRF token) is a keyring envelope
-- ("wsk1" || key_id || nonce || ciphertext) under the deployment sealing
-- keyring. A session is one of two modes: oidc_refresh holds the human
-- refresh token its SSO login issued; api_key_exchange holds the operator API
-- key an OIDC-off sign-in presented and re-exchanges it at each access expiry.
-- Revocation (logout or a refused renewal) stamps revoked_at and wipes every
-- sealed value in the same statement, so a revoked row carries no credential.
-- Rows past their absolute expiry are purged when the tenant next creates a
-- session.
CREATE TABLE wyrd.auth_browser_sessions (
    id_hash              BYTEA       PRIMARY KEY CHECK (octet_length(id_hash) = 32),
    data_tenant_id       UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    principal_id         UUID        NOT NULL,
    -- The human connection an SSO session logged in through; NULL for an
    -- API-key session. The foreign key keeps a removed connection's tombstone.
    connection_id        UUID        REFERENCES wyrd.auth_human_connections(connection_id),
    mode                 TEXT        NOT NULL
        CHECK (mode IN ('oidc_refresh', 'api_key_exchange')),
    access_token_sealed  BYTEA,
    refresh_token_sealed BYTEA,
    api_key_sealed       BYTEA,
    access_expires_at    TIMESTAMPTZ NOT NULL,
    refresh_expires_at   TIMESTAMPTZ,
    absolute_expires_at  TIMESTAMPTZ NOT NULL,
    csrf_hash            BYTEA       NOT NULL CHECK (octet_length(csrf_hash) = 32),
    csrf_token_sealed    BYTEA,
    revoked_at           TIMESTAMPTZ,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    -- A live session carries exactly its mode's credentials; a revoked one
    -- carries none.
    CHECK (
        (revoked_at IS NULL
            AND access_token_sealed IS NOT NULL
            AND csrf_token_sealed IS NOT NULL
            AND (mode = 'oidc_refresh') = (refresh_token_sealed IS NOT NULL)
            AND (mode = 'oidc_refresh') = (refresh_expires_at IS NOT NULL)
            AND (mode = 'oidc_refresh') = (connection_id IS NOT NULL)
            AND (mode = 'api_key_exchange') = (api_key_sealed IS NOT NULL))
        OR (revoked_at IS NOT NULL
            AND access_token_sealed IS NULL
            AND refresh_token_sealed IS NULL
            AND api_key_sealed IS NULL
            AND csrf_token_sealed IS NULL)
    )
);

CREATE INDEX auth_browser_sessions_expiry
    ON wyrd.auth_browser_sessions (data_tenant_id, absolute_expires_at);

ALTER TABLE wyrd.auth_browser_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_browser_sessions FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_browser_sessions
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- The BFF presents only the raw session id. This answers the one question
-- needed to open the owning tenant's row-level-secured transaction: the tenant
-- id of a live (unrevoked, not absolutely expired) session with exactly this
-- id hash, or NULL. No other column is exposed, and a caller without the
-- random id cannot name a row.
CREATE FUNCTION wyrd.auth_browser_session_tenant(p_id_hash bytea)
RETURNS uuid
LANGUAGE sql
SECURITY DEFINER
STABLE
SET search_path = pg_catalog, wyrd
AS $$
    SELECT data_tenant_id FROM wyrd.auth_browser_sessions
    WHERE id_hash = p_id_hash
      AND revoked_at IS NULL
      AND absolute_expires_at > statement_timestamp()
$$;

REVOKE EXECUTE ON FUNCTION wyrd.auth_browser_session_tenant(bytea) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION wyrd.auth_browser_session_tenant(bytea) TO wyrd_app;

-- BFF login completion presents only its HttpOnly flow id. This names the
-- tenant of a completed, unexpired browser login bound to that flow hash, or
-- NULL; the redemption itself then runs under that tenant's RLS.
CREATE FUNCTION wyrd.auth_login_completion_tenant(p_browser_flow_hash bytea)
RETURNS uuid
LANGUAGE sql
SECURITY DEFINER
STABLE
SET search_path = pg_catalog, wyrd
AS $$
    SELECT data_tenant_id FROM wyrd.auth_login_state
    WHERE browser_flow_hash = p_browser_flow_hash
      AND completion_sealed IS NOT NULL
      AND expires_at > statement_timestamp()
$$;

REVOKE EXECUTE ON FUNCTION wyrd.auth_login_completion_tenant(bytea) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION wyrd.auth_login_completion_tenant(bytea) TO wyrd_app;
