-- Pending upload dedupe is an artifact identity, not a byte identity.

DROP INDEX IF EXISTS wyrd.storage_multipart_uploads_dedupe_pending;

CREATE UNIQUE INDEX storage_multipart_uploads_dedupe_pending
    ON wyrd.storage_multipart_uploads (
        data_tenant_id,
        card_uid,
        relative_path,
        expected_sha256
    )
    WHERE status = 'pending';
