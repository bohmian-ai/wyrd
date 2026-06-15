-- Wyrd artifact storage control-plane tables.

CREATE TABLE wyrd.storage_multipart_uploads (
    id                     UUID        PRIMARY KEY,
    data_tenant_id         UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    card_uid               TEXT        NOT NULL
        CHECK (card_uid ~ '^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[0-9a-f]{4}-[0-9a-f]{12}$'),
    relative_path          TEXT        NOT NULL,
    storage_path           TEXT        NOT NULL UNIQUE,
    backend                TEXT        NOT NULL CHECK (backend IN ('local', 's3', 'gcs', 'azure')),
    wire_protocol          TEXT        NOT NULL CHECK (wire_protocol IN (
                                      'single_put_v1',
                                      's3_multipart_v1',
                                      'gcs_resumable_v1',
                                      'azure_block_blob_v1',
                                      'local_fs_v1'
                                  )),
    expected_sha256        TEXT        NOT NULL CHECK (expected_sha256 ~ '^[A-Za-z0-9+/]{43}=$'),
    expected_size_bytes    BIGINT      NOT NULL CHECK (expected_size_bytes >= 0),
    content_type           TEXT,
    part_count_planned     INTEGER     NOT NULL CHECK (part_count_planned >= 0),
    part_size_bytes        BIGINT      NOT NULL CHECK (part_size_bytes >= 0),
    backend_upload_id      TEXT,
    block_count_planned    INTEGER     CHECK (block_count_planned IS NULL OR block_count_planned >= 0),
    status                 TEXT        NOT NULL DEFAULT 'initiating'
                                      CHECK (status IN ('initiating', 'pending', 'completed', 'aborted', 'failed')),
    failure_reason         TEXT,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at           TIMESTAMPTZ,
    aborted_at             TIMESTAMPTZ,
    expires_at             TIMESTAMPTZ NOT NULL
);

CREATE INDEX storage_multipart_uploads_tenant_card
    ON wyrd.storage_multipart_uploads (data_tenant_id, card_uid);

CREATE INDEX storage_multipart_uploads_status_expires
    ON wyrd.storage_multipart_uploads (status, expires_at)
    WHERE status = 'pending';

CREATE INDEX storage_multipart_uploads_initiating_created
    ON wyrd.storage_multipart_uploads (status, created_at)
    WHERE status = 'initiating';

CREATE UNIQUE INDEX storage_multipart_uploads_dedupe_pending
    ON wyrd.storage_multipart_uploads (data_tenant_id, card_uid, expected_sha256)
    WHERE status = 'pending';

ALTER TABLE wyrd.storage_multipart_uploads ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.storage_multipart_uploads FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON wyrd.storage_multipart_uploads
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE TABLE wyrd.storage_artifact_metadata (
    data_tenant_id UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    storage_path   TEXT        NOT NULL,
    card_uid       TEXT        NOT NULL
        CHECK (card_uid ~ '^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[0-9a-f]{4}-[0-9a-f]{12}$'),
    size_bytes     BIGINT      NOT NULL CHECK (size_bytes >= 0),
    sha256         TEXT        NOT NULL CHECK (sha256 ~ '^[A-Za-z0-9+/]{43}=$'),
    content_type   TEXT,
    sse_marker     TEXT,
    backend        TEXT        NOT NULL CHECK (backend IN ('local', 's3', 'gcs', 'azure')),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_tenant_id, storage_path)
);

CREATE INDEX storage_artifact_metadata_card
    ON wyrd.storage_artifact_metadata (data_tenant_id, card_uid);

ALTER TABLE wyrd.storage_artifact_metadata ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.storage_artifact_metadata FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON wyrd.storage_artifact_metadata
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE TABLE wyrd.storage_access_ledger (
    id             BIGSERIAL   PRIMARY KEY,
    data_tenant_id UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    subject_id     TEXT        NOT NULL,
    operation      TEXT        NOT NULL CHECK (operation IN (
                                 'upload_init',
                                 'upload_complete',
                                 'upload_abort',
                                 'download_init',
                                 'sweeper_abort'
                             )),
    upload_id      UUID,
    storage_path   TEXT        NOT NULL,
    backend        TEXT        NOT NULL CHECK (backend IN ('local', 's3', 'gcs', 'azure')),
    status_code    INTEGER     NOT NULL,
    error_code     TEXT,
    request_id     TEXT        NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX storage_access_ledger_tenant_created
    ON wyrd.storage_access_ledger (data_tenant_id, created_at DESC);

CREATE INDEX storage_access_ledger_upload
    ON wyrd.storage_access_ledger (upload_id)
    WHERE upload_id IS NOT NULL;

ALTER TABLE wyrd.storage_access_ledger ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.storage_access_ledger FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON wyrd.storage_access_ledger
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

REVOKE ALL ON TABLE wyrd.storage_multipart_uploads FROM wyrd_app;
REVOKE ALL ON TABLE wyrd.storage_multipart_uploads FROM wyrd_platform_admin;
GRANT SELECT, INSERT, UPDATE ON wyrd.storage_multipart_uploads TO wyrd_app;
GRANT SELECT, UPDATE ON wyrd.storage_multipart_uploads TO wyrd_platform_admin;

REVOKE ALL ON TABLE wyrd.storage_artifact_metadata FROM wyrd_app;
REVOKE ALL ON TABLE wyrd.storage_artifact_metadata FROM wyrd_platform_admin;
GRANT SELECT, INSERT, UPDATE ON wyrd.storage_artifact_metadata TO wyrd_app;
GRANT SELECT ON wyrd.storage_artifact_metadata TO wyrd_platform_admin;

REVOKE ALL ON TABLE wyrd.storage_access_ledger FROM wyrd_app;
REVOKE ALL ON TABLE wyrd.storage_access_ledger FROM wyrd_platform_admin;
GRANT SELECT, INSERT ON wyrd.storage_access_ledger TO wyrd_app;
GRANT SELECT, INSERT ON wyrd.storage_access_ledger TO wyrd_platform_admin;

REVOKE ALL ON SEQUENCE wyrd.storage_access_ledger_id_seq FROM wyrd_app;
REVOKE ALL ON SEQUENCE wyrd.storage_access_ledger_id_seq FROM wyrd_platform_admin;
GRANT USAGE, SELECT ON SEQUENCE wyrd.storage_access_ledger_id_seq TO wyrd_app;
GRANT USAGE, SELECT ON SEQUENCE wyrd.storage_access_ledger_id_seq TO wyrd_platform_admin;
