-- Device authorizations (RFC 8628) for CLI human login.
--
-- `wyrd auth login` asks for a device code and a short user code. The person
-- approves the user code on Wyrd's verification page, which begins an
-- ordinary tenant login whose state row (wyrd.auth_login_state) carries the
-- device_id as its initiation binding. When the common callback has verified
-- the sign-in it records only the approval here: the principal, and the exact
-- connection revision the login went through. This row stores no token and
-- only the SHA-256 of the device code. The CLI's token poll presents the
-- device code, and in one tenant transaction the token endpoint locks this
-- row, mints the tokens from the approval, and deletes the row, so a device
-- code issues once (RFC 8628 §3.4-3.5). A denial marks the row so the next
-- poll ends the login. last_polled_at enforces the poll interval. PostgreSQL
-- derives every expiry; a device code lives at most ten minutes.
CREATE TABLE wyrd.auth_device_authorizations (
    device_id        UUID        PRIMARY KEY,
    data_tenant_id   UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    device_code_hash BYTEA       NOT NULL UNIQUE CHECK (octet_length(device_code_hash) = 32),
    user_code        TEXT        NOT NULL,
    denied           BOOLEAN     NOT NULL DEFAULT false,
    principal_id     UUID,
    connection_id    UUID,
    connection_revision BIGINT,
    last_polled_at   TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    expires_at       TIMESTAMPTZ NOT NULL,
    UNIQUE (data_tenant_id, user_code),
    CONSTRAINT auth_device_authorizations_approval
        CHECK (num_nonnulls(principal_id, connection_id, connection_revision) IN (0, 3)
               AND NOT (denied AND principal_id IS NOT NULL)),
    CHECK (expires_at > created_at AND expires_at <= created_at + interval '10 minutes')
);

CREATE INDEX auth_device_authorizations_expires_at
    ON wyrd.auth_device_authorizations (data_tenant_id, expires_at);

ALTER TABLE wyrd.auth_device_authorizations ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_device_authorizations FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_device_authorizations
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
