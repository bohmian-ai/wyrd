-- Header-free tenant login state and Wyrd authorization codes.
--
-- The common provider callback carries no tenant selector: `Host`, forwarded
-- headers, and paths never choose a tenant. Tenant and connection come only
-- from the one-use login state the begin request wrote, so the state row is
-- now keyed globally by the SHA-256 of its random state value. The raw state
-- lives only in the provider authorization URL; the database never stores it.
--
-- A row records how the login was initiated by which one binding it carries.
-- A login begun at `GET /auth/authorize` (RFC 6749 §4.1.1) binds the OAuth
-- client, its exact redirect URI, its PKCE S256 challenge (RFC 7636 §4.3),
-- and its optional `state`. A CLI login binds its device authorization id
-- (RFC 8628); the device id is unique, so a device approves at most one login.
-- The callback consumes the row (consumed_at) before any provider IO. For an
-- authorize login it then records the signed-in principal and the SHA-256 of
-- a fresh Wyrd authorization code with a 60-second expiry; the token endpoint
-- deletes the row when it redeems the code (RFC 6749 §4.1.2), so a code is
-- used once. No token is stored: tokens are minted at redemption.
--
-- In-flight login state is transient (five minutes) and keyed by the raw
-- state, so it is discarded rather than migrated.
DROP TABLE wyrd.auth_login_state;

CREATE TABLE wyrd.auth_login_state (
    state_hash          BYTEA       PRIMARY KEY CHECK (octet_length(state_hash) = 32),
    data_tenant_id      UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    connection_id       UUID        NOT NULL,
    connection_revision BIGINT      NOT NULL,
    issuer              TEXT        NOT NULL,
    client_id           TEXT        NOT NULL,
    redirect_uri        TEXT        NOT NULL,
    code_verifier       TEXT        NOT NULL,
    nonce               TEXT        NOT NULL,
    oauth_client_id     TEXT        CHECK (oauth_client_id IN ('wyrd-ui', 'wyrd-cli')),
    client_redirect_uri TEXT,
    code_challenge      TEXT,
    client_state        TEXT,
    device_id           UUID,
    consumed_at         TIMESTAMPTZ,
    code_hash           BYTEA       UNIQUE CHECK (octet_length(code_hash) = 32),
    principal_id        UUID,
    expires_at          TIMESTAMPTZ NOT NULL,
    CONSTRAINT auth_login_state_initiation
        CHECK (num_nonnulls(oauth_client_id, device_id) = 1),
    CONSTRAINT auth_login_state_authorize
        CHECK ((oauth_client_id IS NULL) = (client_redirect_uri IS NULL)
               AND (oauth_client_id IS NULL) = (code_challenge IS NULL)
               AND (oauth_client_id IS NOT NULL OR client_state IS NULL)),
    CONSTRAINT auth_login_state_code
        CHECK ((code_hash IS NULL) = (principal_id IS NULL)
               AND (code_hash IS NULL
                    OR (oauth_client_id IS NOT NULL AND consumed_at IS NOT NULL)))
);

CREATE UNIQUE INDEX auth_login_state_device
    ON wyrd.auth_login_state (device_id) WHERE device_id IS NOT NULL;
CREATE INDEX auth_login_state_expires_at
    ON wyrd.auth_login_state (data_tenant_id, expires_at);

ALTER TABLE wyrd.auth_login_state ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_login_state FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_login_state
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.auth_login_state TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

-- The callback knows only the state. It learns which tenant to open a
-- row-level-secured transaction for through this one answer, and nothing
-- else: the tenant id of an unconsumed, unexpired row with exactly this hash,
-- or NULL. It returns no principal, connection, or binding, and a caller
-- without the random state cannot name a row.
CREATE FUNCTION wyrd.auth_login_state_tenant(p_state_hash bytea)
RETURNS uuid
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, wyrd
AS $$
DECLARE
    prior text := coalesce(current_setting('app.operator', true), '');
    tenant uuid;
BEGIN
    PERFORM set_config('app.operator', 'on', true);
    SELECT data_tenant_id INTO tenant FROM wyrd.auth_login_state
    WHERE state_hash = p_state_hash
      AND consumed_at IS NULL
      AND expires_at > statement_timestamp();
    PERFORM set_config('app.operator', prior, true);
    RETURN tenant;
END;
$$;

-- A human refresh family with no login-connection provenance predates
-- connection binding and cannot be attributed to any provider, least of all a
-- replacement. Revoke it outright rather than leaving it unbound.
UPDATE wyrd.auth_refresh_tokens
   SET revoked_at = now(),
       revoked_reason = 'login_connection_unbound'
 WHERE principal_kind = 'user'
   AND human_connection_id IS NULL
   AND revoked_at IS NULL;

-- A human is identified by the provider that authenticated them (issuer and
-- subject), never by email. A replacement provider's user who shares an email
-- with an existing user is a different User until an authorized link, so email
-- cannot be unique within a tenant. The non-unique lookup index remains.
ALTER TABLE wyrd.auth_users DROP CONSTRAINT auth_users_data_tenant_id_email_key;
