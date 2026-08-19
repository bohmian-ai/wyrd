-- Canonical Oracle policy ceilings and role-fenced delegated capacity blocks.

CREATE TABLE vala.oracle_admission_policies (
    scope_kind text NOT NULL CHECK (scope_kind IN ('global', 'tenant')),
    data_tenant_id uuid,
    query_class text NOT NULL CHECK (query_class IN ('interactive', 'analytical')),
    capacity integer NOT NULL CHECK (capacity > 0),
    CHECK ((scope_kind = 'global' AND data_tenant_id IS NULL)
        OR (scope_kind = 'tenant' AND data_tenant_id IS NOT NULL)),
    UNIQUE NULLS NOT DISTINCT (scope_kind, data_tenant_id, query_class)
);

CREATE TABLE vala.oracle_admission_blocks (
    block_id uuid PRIMARY KEY,
    allocation_id uuid NOT NULL,
    scope_kind text NOT NULL CHECK (scope_kind IN ('global', 'tenant', 'principal')),
    data_tenant_id uuid NOT NULL,
    principal_id uuid,
    query_class text NOT NULL CHECK (query_class IN ('interactive', 'analytical')),
    units integer NOT NULL CHECK (units > 0),
    holder_node_id uuid NOT NULL,
    holder_fencing_token bigint NOT NULL CHECK (holder_fencing_token > 0),
    valid_from timestamptz NOT NULL,
    expires_at timestamptz NOT NULL CHECK (expires_at > valid_from),
    closed_at timestamptz,
    CHECK ((scope_kind = 'principal' AND principal_id IS NOT NULL)
        OR (scope_kind <> 'principal' AND principal_id IS NULL)),
    UNIQUE (allocation_id, scope_kind)
);

CREATE INDEX oracle_admission_blocks_accounting
    ON vala.oracle_admission_blocks (scope_kind, data_tenant_id, query_class, expires_at);
CREATE INDEX oracle_admission_blocks_holder
    ON vala.oracle_admission_blocks (holder_node_id, holder_fencing_token, expires_at);

REVOKE ALL ON vala.oracle_admission_policies FROM PUBLIC, wyrd_app;
REVOKE ALL ON vala.oracle_admission_blocks FROM PUBLIC, wyrd_app;
GRANT SELECT, INSERT, UPDATE ON vala.oracle_admission_policies TO wyrd_platform_admin;
GRANT SELECT, INSERT, UPDATE ON vala.oracle_admission_blocks TO wyrd_platform_admin;
GRANT SELECT ON vala.cluster_nodes TO wyrd_platform_admin;
