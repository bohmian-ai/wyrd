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
    stored_response    JSONB,
    status             TEXT NOT NULL DEFAULT 'pending',
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT card_registration_operations_status_check
        CHECK (status IN ('pending', 'committed', 'expired')),
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

-- Registration lifecycle state and pending-card bookkeeping.

ALTER TABLE wyrd.cards
    DROP CONSTRAINT cards_status_check,
    ADD CONSTRAINT cards_status_check CHECK (
        status IN ('pending', 'active', 'deprecated', 'failed', 'expired', 'deleted')
    );

DROP INDEX IF EXISTS wyrd.cards_identity_unique;
CREATE UNIQUE INDEX cards_identity_unique
    ON wyrd.cards (data_tenant_id, kind, space, name, version)
    WHERE status NOT IN ('deleted', 'failed', 'expired');

DROP INDEX IF EXISTS wyrd.idx_cards_version_latest;
CREATE INDEX idx_cards_version_latest
    ON wyrd.cards (
        data_tenant_id, kind, space, name,
        version_major DESC, version_minor DESC, version_patch DESC
    )
    WHERE status = 'active' AND NOT version_is_prerelease;

DROP INDEX IF EXISTS wyrd.idx_cards_spec_hash;
CREATE INDEX idx_cards_spec_hash
    ON wyrd.cards (data_tenant_id, kind, space, name, spec_hash)
    WHERE status = 'active';

ALTER TABLE wyrd.cards
    ADD COLUMN registration_operation_id UUID
        REFERENCES wyrd.card_registration_operations(operation_id),
    ADD COLUMN pending_since TIMESTAMPTZ,
    ADD COLUMN finalized_at TIMESTAMPTZ,
    ADD COLUMN card_blob_uri TEXT,
    ADD COLUMN blob_failed_at TIMESTAMPTZ;

CREATE INDEX cards_pending_sweep_idx
    ON wyrd.cards (data_tenant_id, status, pending_since)
    WHERE status = 'pending';

-- Per-artifact registration expectations and post-commit initialization state.

CREATE TABLE wyrd.card_artifact_manifest (
    card_uid            UUID NOT NULL REFERENCES wyrd.cards(card_uid) ON DELETE CASCADE,
    relative_path       TEXT NOT NULL,
    expected_sha256     TEXT NOT NULL,
    expected_size_bytes BIGINT NOT NULL,
    content_type        TEXT,
    upload_status       TEXT NOT NULL,
    upload_id           UUID,
    verified_at         TIMESTAMPTZ,
    data_tenant_id      UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    PRIMARY KEY (card_uid, relative_path),
    CONSTRAINT card_artifact_manifest_upload_status_check
        CHECK (upload_status IN ('awaiting_init', 'pending', 'uploaded', 'verified'))
);

CREATE INDEX card_artifact_manifest_tenant_idx
    ON wyrd.card_artifact_manifest (data_tenant_id, card_uid);

CREATE INDEX card_artifact_manifest_awaiting_init_idx
    ON wyrd.card_artifact_manifest (data_tenant_id, upload_status)
    WHERE upload_status = 'awaiting_init';

ALTER TABLE wyrd.card_artifact_manifest ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.card_artifact_manifest FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON wyrd.card_artifact_manifest
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE POLICY admin_cross_tenant ON wyrd.card_artifact_manifest
    TO wyrd_platform_admin
    USING (true)
    WITH CHECK (true);

REVOKE ALL ON TABLE wyrd.card_artifact_manifest FROM wyrd_app;
REVOKE ALL ON TABLE wyrd.card_artifact_manifest FROM wyrd_platform_admin;
GRANT SELECT, INSERT, UPDATE ON wyrd.card_artifact_manifest TO wyrd_app;
GRANT SELECT, UPDATE, DELETE ON wyrd.card_artifact_manifest TO wyrd_platform_admin;

-- Composite Service/Agent registration projects its principal on the same
-- tenant transaction as the card row.
GRANT SELECT, INSERT, UPDATE ON wyrd.auth_service_accounts TO wyrd_app;
