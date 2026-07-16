-- Per-artifact registration expectations and post-commit init state.

CREATE TABLE wyrd.card_artifact_manifest (
    card_uid           UUID NOT NULL REFERENCES wyrd.cards(card_uid) ON DELETE CASCADE,
    relative_path      TEXT NOT NULL,
    expected_sha256    TEXT NOT NULL,
    expected_size_bytes BIGINT NOT NULL,
    content_type       TEXT,
    upload_status      TEXT NOT NULL,
    upload_id          UUID,
    verified_at        TIMESTAMPTZ,
    data_tenant_id     UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
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
