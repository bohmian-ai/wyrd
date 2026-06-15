-- Wyrd upload-init idempotency cache.

CREATE TABLE wyrd.storage_idempotency_keys (
    data_tenant_id   UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    idempotency_key  TEXT        NOT NULL,
    body_sha256      BYTEA       NOT NULL,
    response_status  SMALLINT    NOT NULL,
    response_body    JSONB       NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at       TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_tenant_id, idempotency_key),
    CONSTRAINT storage_idempotency_seed_only CHECK (
        response_body::text !~ (
            '"(' ||
            'put_' || 'url' || '|' ||
            'session_' || 'uri' || '|' ||
            'session_' || 'url' || '|' ||
            's' || 'as_url' || '|' ||
            'pre' || 'signed_' || 'url' || '|' ||
            'signed_' || 'url' ||
            ')"'
        )
    )
);

CREATE INDEX storage_idempotency_keys_expires
    ON wyrd.storage_idempotency_keys (expires_at);

ALTER TABLE wyrd.storage_idempotency_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.storage_idempotency_keys FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON wyrd.storage_idempotency_keys
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

REVOKE ALL ON TABLE wyrd.storage_idempotency_keys FROM wyrd_app;
REVOKE ALL ON TABLE wyrd.storage_idempotency_keys FROM wyrd_platform_admin;
GRANT SELECT, INSERT ON wyrd.storage_idempotency_keys TO wyrd_app;
GRANT SELECT, DELETE ON wyrd.storage_idempotency_keys TO wyrd_platform_admin;
