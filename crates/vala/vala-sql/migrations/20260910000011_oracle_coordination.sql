-- Forward-only Oracle membership and admission control plane.
ALTER TABLE vala.cluster_nodes DROP CONSTRAINT cluster_nodes_pkey;
ALTER TABLE vala.cluster_nodes
    ADD COLUMN capability_version smallint NOT NULL DEFAULT 1,
    ADD COLUMN capabilities jsonb NOT NULL
        DEFAULT '{"kind":"scribe_v1","tail_protocol_version":1}'::jsonb,
    ADD COLUMN ready boolean NOT NULL DEFAULT false;
ALTER TABLE vala.cluster_nodes ADD PRIMARY KEY (node_id, role);
ALTER TABLE vala.cluster_nodes
    ADD CONSTRAINT cluster_nodes_capability_version_v1 CHECK (capability_version = 1),
    ADD CONSTRAINT cluster_nodes_capabilities_object CHECK (jsonb_typeof(capabilities) = 'object'),
    ADD CONSTRAINT cluster_nodes_nonempty_address CHECK (btrim(advertise_addr) <> '');
GRANT DELETE ON vala.cluster_nodes TO wyrd_platform_admin;

CREATE TABLE vala.oracle_admission_leases (
    query_id uuid PRIMARY KEY,
    data_tenant_id uuid NOT NULL,
    query_class text NOT NULL CHECK (query_class IN ('interactive', 'analytical')),
    slot_units integer NOT NULL CHECK (slot_units > 0),
    leader_node_id uuid NOT NULL,
    leader_fencing_token bigint NOT NULL,
    acquired_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL CHECK (expires_at > acquired_at)
);

CREATE TABLE vala.oracle_admission_nodes (
    query_id uuid NOT NULL REFERENCES vala.oracle_admission_leases(query_id) ON DELETE CASCADE,
    node_id uuid NOT NULL,
    PRIMARY KEY (query_id, node_id)
);

CREATE TABLE vala.oracle_admission_accounting (
    scope_kind text NOT NULL CHECK (scope_kind IN ('cluster', 'class', 'tenant')),
    scope_key text NOT NULL CHECK (btrim(scope_key) <> ''),
    accounting_class text NOT NULL CHECK (accounting_class IN ('all', 'interactive', 'analytical')),
    used_slots bigint NOT NULL DEFAULT 0 CHECK (used_slots >= 0),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (scope_kind, scope_key, accounting_class),
    CONSTRAINT oracle_admission_accounting_valid_scope CHECK (
        (scope_kind = 'cluster' AND scope_key = 'global' AND accounting_class = 'all')
        OR (scope_kind = 'class' AND scope_key = 'global'
            AND accounting_class IN ('interactive', 'analytical'))
        OR (scope_kind = 'tenant' AND accounting_class IN ('interactive', 'analytical')
            AND scope_key ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$')
    )
);

CREATE INDEX oracle_admission_leases_expiry_idx
    ON vala.oracle_admission_leases (expires_at, query_id);
CREATE INDEX oracle_admission_leases_reconcile_idx
    ON vala.oracle_admission_leases (data_tenant_id, query_class, expires_at);
CREATE INDEX oracle_admission_nodes_node_idx
    ON vala.oracle_admission_nodes (node_id, query_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON
    vala.oracle_admission_leases,
    vala.oracle_admission_nodes,
    vala.oracle_admission_accounting
TO wyrd_platform_admin;
