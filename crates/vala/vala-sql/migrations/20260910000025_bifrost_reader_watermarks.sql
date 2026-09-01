-- Snapshots that live Oracle cuts and Scribe live-tail leases still need.
--
-- Forge maintenance runs in a different process from every reader, so a pinned
-- cut has to leave a durable trace or destructive maintenance would be deciding
-- from age alone. One row per reader node per table carries the oldest snapshot
-- that node currently depends on; it is republished on the node's ordinary
-- heartbeat and lapses with it, so a reader that dies stops protecting anything
-- on the same schedule its role lease already expires.
CREATE TABLE vala.bifrost_reader_watermarks (
    data_tenant_id        uuid NOT NULL REFERENCES platform.tenants(data_tenant_id),
    node_id               uuid NOT NULL,
    namespace             text NOT NULL,
    table_name            text NOT NULL,
    snapshot_id           bigint NOT NULL CHECK (snapshot_id > 0),
    snapshot_timestamp_ms bigint NOT NULL CHECK (snapshot_timestamp_ms >= 0),
    published_at          timestamptz NOT NULL,
    expires_at            timestamptz NOT NULL,
    CHECK (expires_at > published_at),
    CHECK (btrim(namespace) <> '' AND btrim(table_name) <> ''),
    PRIMARY KEY (data_tenant_id, node_id, namespace, table_name)
);

CREATE INDEX bifrost_reader_watermarks_table_liveness
    ON vala.bifrost_reader_watermarks (data_tenant_id, namespace, table_name, expires_at);

ALTER TABLE vala.bifrost_reader_watermarks ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.bifrost_reader_watermarks FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.bifrost_reader_watermarks
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

REVOKE ALL ON vala.bifrost_reader_watermarks FROM PUBLIC;
GRANT SELECT, INSERT, UPDATE, DELETE ON vala.bifrost_reader_watermarks TO wyrd_app;
