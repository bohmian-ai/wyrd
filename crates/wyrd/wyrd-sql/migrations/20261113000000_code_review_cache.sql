-- Code-review pipeline cache metadata.
--
-- The code-review pipeline keeps source-free repository snapshots and
-- selected-source PR contexts as immutable objects in Wyrd object storage
-- under `<tenant>/code-review/`. These tables hold only the metadata its
-- store protocols need: the current snapshot pointer per compatibility scope,
-- one record per snapshot object for collection, and context records with
-- their publication and expiry stamps. No object bytes or execution payloads
-- live here, and this cache is never the accepted execution evidence, which
-- Bifrost retains.
--
-- Rows are keyed by tenant, GitHub installation and the stable numeric
-- repository id. Owner and name are mutable labels and are left out, so a
-- rename or transfer keeps its rows. `pointer`, `identity` and `record` are
-- the pipeline's own serialized types, stored whole. Every timestamp comes
-- from `statement_timestamp()`.

-- The default-branch snapshot each compatibility scope currently names.
-- Publication is a compare-and-swap on `tree`.
CREATE TABLE wyrd.code_review_snapshot_pointers (
    data_tenant_id     UUID   NOT NULL REFERENCES platform.tenants(data_tenant_id),
    installation_id    BIGINT NOT NULL CHECK (installation_id > 0),
    repository_id      BIGINT NOT NULL CHECK (repository_id > 0),
    extraction_version TEXT   NOT NULL,
    format_version     BIGINT NOT NULL CHECK (format_version >= 0),
    max_file_bytes     BIGINT NOT NULL CHECK (max_file_bytes >= 0),
    pointer            JSONB  NOT NULL CHECK (jsonb_typeof(pointer) = 'object'),
    tree               TEXT   NOT NULL,
    PRIMARY KEY (data_tenant_id, installation_id, repository_id,
                 extraction_version, format_version, max_file_bytes)
);

-- One row per snapshot object. A build touches its record before creating the
-- object; collection claims unreferenced, untouched records in one statement,
-- and a claimed record refuses touches until its object and row are deleted.
CREATE TABLE wyrd.code_review_snapshot_records (
    data_tenant_id  UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    installation_id BIGINT      NOT NULL CHECK (installation_id > 0),
    repository_id   BIGINT      NOT NULL CHECK (repository_id > 0),
    object_key      TEXT        NOT NULL,
    touched_at      TIMESTAMPTZ NOT NULL,
    claimed         BOOLEAN     NOT NULL DEFAULT false,
    PRIMARY KEY (data_tenant_id, installation_id, repository_id, object_key)
);

-- Published PR contexts. A repeated publication keeps the original stamps,
-- and a context at or past `expires_at` is never read even if its deletion lags.
CREATE TABLE wyrd.code_review_contexts (
    data_tenant_id  UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    installation_id BIGINT      NOT NULL CHECK (installation_id > 0),
    repository_id   BIGINT      NOT NULL CHECK (repository_id > 0),
    context_id      TEXT        NOT NULL,
    identity        JSONB       NOT NULL CHECK (jsonb_typeof(identity) = 'object'),
    record          JSONB       NOT NULL CHECK (jsonb_typeof(record) = 'object'),
    published_at    TIMESTAMPTZ NOT NULL,
    expires_at      TIMESTAMPTZ NOT NULL CHECK (expires_at > published_at),
    PRIMARY KEY (data_tenant_id, installation_id, repository_id, context_id)
);

CREATE INDEX code_review_contexts_expiry ON wyrd.code_review_contexts (expires_at);

ALTER TABLE wyrd.code_review_snapshot_pointers ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.code_review_snapshot_pointers FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.code_review_snapshot_pointers
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.code_review_snapshot_pointers TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

ALTER TABLE wyrd.code_review_snapshot_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.code_review_snapshot_records FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.code_review_snapshot_records
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.code_review_snapshot_records TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

ALTER TABLE wyrd.code_review_contexts ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.code_review_contexts FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.code_review_contexts
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.code_review_contexts TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());
