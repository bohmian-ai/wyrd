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

CREATE TABLE vala.oracle_admission_leases (
    data_tenant_id uuid NOT NULL REFERENCES platform.tenants(data_tenant_id),
    query_id uuid NOT NULL,
    query_class text NOT NULL CHECK (query_class IN ('interactive', 'analytical')),
    slot_units integer NOT NULL CHECK (slot_units > 0),
    leader_node_id uuid NOT NULL,
    leader_fencing_token bigint NOT NULL,
    acquired_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL CHECK (expires_at > acquired_at),
    PRIMARY KEY (data_tenant_id, query_id)
);

ALTER TABLE vala.oracle_admission_leases ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.oracle_admission_leases FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.oracle_admission_leases
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE TABLE vala.oracle_admission_nodes (
    data_tenant_id uuid NOT NULL,
    query_id uuid NOT NULL,
    node_id uuid NOT NULL,
    PRIMARY KEY (data_tenant_id, query_id, node_id),
    FOREIGN KEY (data_tenant_id, query_id)
        REFERENCES vala.oracle_admission_leases(data_tenant_id, query_id) ON DELETE CASCADE
);

ALTER TABLE vala.oracle_admission_nodes ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.oracle_admission_nodes FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.oracle_admission_nodes
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE TABLE vala.oracle_admission_accounting (
    data_tenant_id uuid NOT NULL REFERENCES platform.tenants(data_tenant_id),
    scope_kind text NOT NULL CHECK (scope_kind IN ('cluster', 'class', 'tenant')),
    scope_key text NOT NULL CHECK (btrim(scope_key) <> ''),
    accounting_class text NOT NULL CHECK (accounting_class IN ('all', 'interactive', 'analytical')),
    used_slots bigint NOT NULL DEFAULT 0 CHECK (used_slots >= 0),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (data_tenant_id, scope_kind, scope_key, accounting_class),
    CONSTRAINT oracle_admission_accounting_owner CHECK (
        scope_kind = 'tenant'
        OR data_tenant_id = '00000000-0000-0000-0000-000000000000'::uuid
    ),
    CONSTRAINT oracle_admission_accounting_valid_scope CHECK (
        (scope_kind = 'cluster' AND scope_key = 'global' AND accounting_class = 'all')
        OR (scope_kind = 'class' AND scope_key = 'global'
            AND accounting_class IN ('interactive', 'analytical'))
        OR (scope_kind = 'tenant' AND accounting_class IN ('interactive', 'analytical')
            AND scope_key ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$')
    )
);

ALTER TABLE vala.oracle_admission_accounting ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.oracle_admission_accounting FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.oracle_admission_accounting
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY tenant_global_accounting ON vala.oracle_admission_accounting
    USING (
        data_tenant_id = '00000000-0000-0000-0000-000000000000'::uuid
        AND scope_key = 'global'
        AND (
            (scope_kind = 'cluster' AND accounting_class = 'all')
            OR (
                scope_kind = 'class'
                AND accounting_class IN ('interactive', 'analytical')
            )
        )
    )
    WITH CHECK (
        data_tenant_id = '00000000-0000-0000-0000-000000000000'::uuid
        AND scope_key = 'global'
        AND (
            (scope_kind = 'cluster' AND accounting_class = 'all')
            OR (
                scope_kind = 'class'
                AND accounting_class IN ('interactive', 'analytical')
            )
        )
    );

CREATE INDEX oracle_admission_leases_expiry_idx
    ON vala.oracle_admission_leases (data_tenant_id, expires_at, query_id);
CREATE INDEX oracle_admission_leases_reconcile_idx
    ON vala.oracle_admission_leases (data_tenant_id, query_class, expires_at);
CREATE INDEX oracle_admission_nodes_node_idx
    ON vala.oracle_admission_nodes (data_tenant_id, node_id, query_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON
    vala.oracle_admission_leases,
    vala.oracle_admission_nodes,
    vala.oracle_admission_accounting
TO wyrd_app;
