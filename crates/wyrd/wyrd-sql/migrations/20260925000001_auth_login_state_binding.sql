-- Header-free tenant login state and sealed login completions.
--
-- The common provider callback carries no tenant selector: `Host`, forwarded
-- headers, and paths never choose a tenant. Tenant and connection come only
-- from the one-use login state the begin request wrote, so the state row is
-- now keyed globally by the SHA-256 of its random state value. The raw state
-- lives only in the provider authorization URL; the database never stores it.
--
-- A row records how the login was initiated by which one binding it carries: a
-- browser login binds the hash of the BFF's random flow id; a CLI login binds its
-- device authorization id (RFC 8628). The binding is unique, so a flow id or device
-- can be recorded against exactly one login. The callback consumes the row (consumed_at) before any provider
-- IO and, once it has issued a Wyrd session, stores that session sealed under
-- the deployment keyring in completion_sealed with a fresh, short
-- expires_at. The BFF or the device-code poll redeems the completion once by its binding; the
-- redemption deletes the row. Neither the provider code nor a Wyrd token ever
-- reaches the browser.
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
    browser_flow_hash   BYTEA       CHECK (octet_length(browser_flow_hash) = 32),
    device_id           UUID,
    consumed_at         TIMESTAMPTZ,
    completion_sealed   BYTEA,
    expires_at          TIMESTAMPTZ NOT NULL,
    CHECK (num_nonnulls(browser_flow_hash, device_id) = 1),
    CHECK (completion_sealed IS NULL OR consumed_at IS NOT NULL)
);

CREATE UNIQUE INDEX auth_login_state_browser_flow
    ON wyrd.auth_login_state (browser_flow_hash) WHERE browser_flow_hash IS NOT NULL;
CREATE UNIQUE INDEX auth_login_state_device
    ON wyrd.auth_login_state (device_id) WHERE device_id IS NOT NULL;
CREATE INDEX auth_login_state_expires_at
    ON wyrd.auth_login_state (data_tenant_id, expires_at);

ALTER TABLE wyrd.auth_login_state ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_login_state FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_login_state
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- The callback knows only the state. It learns which tenant to open a
-- row-level-secured transaction for through this one answer, and nothing
-- else: the tenant id of an unconsumed, unexpired row with exactly this hash,
-- or NULL. It returns no principal, connection, or binding, and a caller
-- without the random state cannot name a row.
CREATE FUNCTION wyrd.auth_login_state_tenant(p_state_hash bytea)
RETURNS uuid
LANGUAGE sql
SECURITY DEFINER
STABLE
SET search_path = pg_catalog, wyrd
AS $$
    SELECT data_tenant_id FROM wyrd.auth_login_state
    WHERE state_hash = p_state_hash
      AND consumed_at IS NULL
      AND expires_at > statement_timestamp()
$$;

REVOKE EXECUTE ON FUNCTION wyrd.auth_login_state_tenant(bytea) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION wyrd.auth_login_state_tenant(bytea) TO wyrd_app;

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
