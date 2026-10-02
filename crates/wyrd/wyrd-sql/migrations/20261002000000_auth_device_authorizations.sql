-- Device authorizations (RFC 8628) for CLI human login.
--
-- `wyrd auth login` asks for a device code and a short user code. The person
-- approves the user code on Wyrd's verification page, which begins an
-- ordinary tenant login whose state row (wyrd.auth_login_state) carries the
-- device_id as its initiation binding and, once the common callback has
-- issued the session, its sealed completion. This row stores no token and only
-- the SHA-256 of the device code: the CLI's token poll presents the device
-- code, redeems that sealed completion, and deletes this row in one tenant
-- transaction, so the code is redeemed once. A denial marks the row so the
-- next poll ends the login. last_polled_at enforces the poll interval.
-- PostgreSQL derives every expiry; a device code lives at most ten minutes.
CREATE TABLE wyrd.auth_device_authorizations (
    device_id        UUID        PRIMARY KEY,
    data_tenant_id   UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    device_code_hash BYTEA       NOT NULL UNIQUE CHECK (octet_length(device_code_hash) = 32),
    user_code        TEXT        NOT NULL,
    denied           BOOLEAN     NOT NULL DEFAULT false,
    last_polled_at   TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    expires_at       TIMESTAMPTZ NOT NULL,
    UNIQUE (data_tenant_id, user_code),
    CHECK (expires_at > created_at AND expires_at <= created_at + interval '10 minutes')
);

CREATE INDEX auth_device_authorizations_expires_at
    ON wyrd.auth_device_authorizations (data_tenant_id, expires_at);

ALTER TABLE wyrd.auth_device_authorizations ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_device_authorizations FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_device_authorizations
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
