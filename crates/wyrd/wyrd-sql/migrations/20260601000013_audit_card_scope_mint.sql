-- Durable audit rows for card-ref scope mint decisions.

CREATE TABLE wyrd.audit_card_scope_mint (
    id                    UUID         PRIMARY KEY,
    data_tenant_id        UUID         NOT NULL REFERENCES platform.tenants(data_tenant_id),
    occurred_at           TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    principal_id          UUID,
    mint_kind             TEXT         NOT NULL CHECK (mint_kind IN ('api_key_exchange', 'refresh', 'delegation', 'jwt_bearer')),
    root_card_ref         JSONB        NOT NULL,
    request_id            TEXT         NOT NULL,
    result                TEXT         NOT NULL CHECK (result IN ('success', 'failure')),
    scope_member_count    INTEGER,
    scope_hash            TEXT,
    scope_members         JSONB        NOT NULL DEFAULT '[]'::jsonb,
    failure_code          TEXT,
    failure_reason        TEXT,
    CONSTRAINT audit_card_scope_mint_result_consistency
        CHECK (
            (
                result = 'success'
                AND scope_member_count IS NOT NULL
                AND scope_hash IS NOT NULL
                AND failure_code IS NULL
                AND failure_reason IS NULL
            )
            OR
            (
                result = 'failure'
                AND scope_member_count IS NULL
                AND scope_hash IS NULL
                AND failure_code IS NOT NULL
                AND failure_reason IS NOT NULL
            )
        )
);

CREATE INDEX audit_card_scope_mint_by_root
    ON wyrd.audit_card_scope_mint (
        data_tenant_id,
        (root_card_ref->>'kind'),
        (root_card_ref->>'space'),
        (root_card_ref->>'name'),
        (root_card_ref->>'version'),
        occurred_at DESC
    );

CREATE INDEX audit_card_scope_mint_by_request
    ON wyrd.audit_card_scope_mint (data_tenant_id, request_id, occurred_at DESC);

ALTER TABLE wyrd.audit_card_scope_mint ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.audit_card_scope_mint FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.audit_card_scope_mint
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY admin_cross_tenant
    ON wyrd.audit_card_scope_mint
    TO wyrd_platform_admin
    USING (true)
    WITH CHECK (true);
