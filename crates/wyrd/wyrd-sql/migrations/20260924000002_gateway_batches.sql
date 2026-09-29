-- Gateway Batches lifecycle state shared by every wyrd-server replica.
--
-- A batch input file is uploaded to one provider deployment; its row pins that
-- deployment and records the governed reference (size, digest, content type)
-- and the provider's file id. A batch row is the replay fence of batch
-- creation: it is claimed under the digest of the canonical create request
-- before the provider is called, so a replayed creation on any replica finds
-- the claimed row instead of creating a second upstream batch.

CREATE TABLE wyrd.gateway_batch_files (
    data_tenant_id      UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    file_id             UUID        NOT NULL,
    filename            TEXT        NOT NULL,
    size_bytes          BIGINT      NOT NULL CHECK (size_bytes >= 0),
    -- Base64 SHA-256 of the uploaded bytes.
    sha256              TEXT        NOT NULL,
    content_type        TEXT        NOT NULL,
    -- Endpoint every request line targets.
    endpoint            TEXT        NOT NULL,
    -- `<provider>/<model>` projection that served the upload.
    model               TEXT        NOT NULL,
    deployment          TEXT        NOT NULL,
    upstream_file_id    TEXT        NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_tenant_id, file_id)
);

ALTER TABLE wyrd.gateway_batch_files ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.gateway_batch_files FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.gateway_batch_files
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE TABLE wyrd.gateway_batches (
    data_tenant_id      UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    batch_id            UUID        NOT NULL,
    -- Hex SHA-256 of the RFC 8785 canonical create request.
    request_sha256      TEXT        NOT NULL,
    -- Input file; no foreign key, so deleting a file keeps its batches.
    file_id             UUID        NOT NULL,
    model               TEXT        NOT NULL,
    deployment          TEXT        NOT NULL,
    -- NULL while creation is pending; set once the provider created the batch.
    upstream_batch_id   TEXT,
    -- Last observed provider batch object.
    batch               JSONB,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_tenant_id, batch_id),
    UNIQUE (data_tenant_id, request_sha256),
    CHECK ((upstream_batch_id IS NULL) = (batch IS NULL))
);

ALTER TABLE wyrd.gateway_batches ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.gateway_batches FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.gateway_batches
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
