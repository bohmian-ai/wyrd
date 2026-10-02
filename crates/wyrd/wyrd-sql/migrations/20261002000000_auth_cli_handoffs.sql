-- One-use CLI login handoffs.
--
-- `wyrd auth login` begins a tenant human login through a handoff row that
-- binds the server-issued handoff id to the tenant, the Active connection the
-- login began through, and the SHA-256 of a 256-bit verifier only the CLI
-- holds. The login's own state row (wyrd.auth_login_state) carries the
-- handoff id as its initiation binding and, once the common callback has
-- issued the session, its sealed completion. This row stores no token: a
-- claim presents the verifier, redeems that sealed completion, and deletes
-- the handoff in one tenant transaction, so a second claim matches nothing.
-- PostgreSQL derives every expiry; a handoff lives at most five minutes.
CREATE TABLE wyrd.auth_cli_handoffs (
    handoff_id     UUID        PRIMARY KEY,
    data_tenant_id UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    connection_id  UUID        NOT NULL,
    verifier_hash  BYTEA       NOT NULL CHECK (octet_length(verifier_hash) = 32),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    expires_at     TIMESTAMPTZ NOT NULL,
    CHECK (expires_at > created_at AND expires_at <= created_at + interval '5 minutes')
);

CREATE INDEX auth_cli_handoffs_expires_at
    ON wyrd.auth_cli_handoffs (data_tenant_id, expires_at);

ALTER TABLE wyrd.auth_cli_handoffs ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_cli_handoffs FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_cli_handoffs
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
