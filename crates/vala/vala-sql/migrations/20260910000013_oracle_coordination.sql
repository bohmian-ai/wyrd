-- Forward-only Oracle membership and admission control plane.
ALTER TABLE vala.cluster_nodes DROP CONSTRAINT cluster_nodes_pkey;
ALTER TABLE vala.cluster_nodes
    ADD COLUMN data_tenant_id uuid REFERENCES platform.tenants(data_tenant_id),
    ADD COLUMN capability_version smallint NOT NULL DEFAULT 1,
    ADD COLUMN capabilities jsonb NOT NULL
        DEFAULT '{"kind":"scribe_v1","tail_protocol_version":1}'::jsonb,
    ADD COLUMN ready boolean NOT NULL DEFAULT false;
UPDATE vala.cluster_nodes
SET data_tenant_id = '00000000-0000-0000-0000-000000000000'
WHERE data_tenant_id IS NULL;
ALTER TABLE vala.cluster_nodes ALTER COLUMN data_tenant_id SET NOT NULL;
ALTER TABLE vala.cluster_nodes ADD PRIMARY KEY (data_tenant_id, node_id, role);
ALTER TABLE vala.cluster_nodes
    ADD CONSTRAINT cluster_nodes_capability_version_v1 CHECK (capability_version = 1),
    ADD CONSTRAINT cluster_nodes_capabilities_object CHECK (jsonb_typeof(capabilities) = 'object'),
    ADD CONSTRAINT cluster_nodes_nonempty_address CHECK (btrim(advertise_addr) <> '');
ALTER TABLE vala.cluster_nodes ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.cluster_nodes FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.cluster_nodes
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

REVOKE SELECT, INSERT, UPDATE, DELETE ON vala.cluster_nodes FROM wyrd_platform_admin;
GRANT SELECT, INSERT, UPDATE, DELETE ON vala.cluster_nodes TO wyrd_app;

-- Oracle's cross-tenant catalog reconciliation authority is introduced with
-- the coordination migration so the older OLAP migration remains checksum-stable.
GRANT USAGE ON SCHEMA vala TO wyrd_platform_admin;
GRANT SELECT ON vala.bifrost_tables TO wyrd_platform_admin;
