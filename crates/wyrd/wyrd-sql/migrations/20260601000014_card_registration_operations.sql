-- Idempotency state for card registration.
--
-- The operation row is committed with the card and audit row before any
-- storage backend call. Replays use this row to resume post-commit work.

CREATE TABLE wyrd.card_registration_operations (
    operation_id       UUID PRIMARY KEY,
    data_tenant_id     UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    principal_id        UUID NOT NULL,
    idempotency_key    TEXT NOT NULL,
    request_hash       TEXT NOT NULL,
    card_uid           UUID NOT NULL,
    upload_plans       JSONB NOT NULL DEFAULT '[]'::jsonb,
    outcome            TEXT NOT NULL,
    status             TEXT NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT card_registration_operations_outcome_check
        CHECK (outcome IN ('created', 'idempotent_noop', 'deduplicated')),
    CONSTRAINT card_registration_operations_status_check
        CHECK (status IN ('pending', 'active', 'failed', 'expired')),
    CONSTRAINT card_registration_operations_idempotency_key_unique
        UNIQUE (data_tenant_id, principal_id, idempotency_key)
);

CREATE INDEX card_registration_operations_sweep_idx
    ON wyrd.card_registration_operations (status, updated_at)
    WHERE status = 'pending';

ALTER TABLE wyrd.card_registration_operations ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.card_registration_operations FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON wyrd.card_registration_operations
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE POLICY admin_cross_tenant ON wyrd.card_registration_operations
    TO wyrd_platform_admin
    USING (true)
    WITH CHECK (true);

REVOKE ALL ON TABLE wyrd.card_registration_operations FROM wyrd_app;
REVOKE ALL ON TABLE wyrd.card_registration_operations FROM wyrd_platform_admin;
GRANT SELECT, INSERT, UPDATE ON wyrd.card_registration_operations TO wyrd_app;
GRANT SELECT, UPDATE, DELETE ON wyrd.card_registration_operations TO wyrd_platform_admin;
