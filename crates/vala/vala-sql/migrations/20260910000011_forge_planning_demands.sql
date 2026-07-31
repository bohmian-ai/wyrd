-- Durable coalescing ingress for Forge planning.

CREATE TABLE vala.forge_planning_demands (
    data_tenant_id uuid NOT NULL,
    catalog_name text NOT NULL CHECK (catalog_name = 'wyrd-redux'),
    namespace_name text NOT NULL CHECK (namespace_name IN ('vala.system','vala.bifrost','vala.traces','vala.metrics','vala.logs','vala.genai','vala.eval','vala.drift','vala.dev','vala.datasets')),
    table_name text NOT NULL CHECK (table_name <> '' AND length(table_name) <= 63 AND table_name ~ '^[A-Za-z0-9_.-]+$' AND table_name NOT LIKE '%..%' AND table_name !~ '^\\.|\\.$'),
    first_requested_at timestamptz NOT NULL DEFAULT statement_timestamp(),
    last_requested_at timestamptz NOT NULL DEFAULT statement_timestamp(),
    last_source text NOT NULL CHECK (last_source IN ('hint','periodic')),
    generation bigint NOT NULL DEFAULT 1 CHECK (generation > 0),
    PRIMARY KEY (data_tenant_id, catalog_name, namespace_name, table_name)
);

ALTER TABLE vala.forge_planning_demands ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.forge_planning_demands FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.forge_planning_demands
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE INDEX forge_planning_demands_ring
    ON vala.forge_planning_demands (data_tenant_id, last_requested_at, catalog_name, namespace_name, table_name);

GRANT SELECT, INSERT, UPDATE ON vala.forge_planning_demands TO wyrd_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON vala.forge_planning_demands TO wyrd_platform_admin;
GRANT SELECT, INSERT, UPDATE ON vala.audit_chain_head TO wyrd_platform_admin;
GRANT SELECT, INSERT ON vala.audit_outbox TO wyrd_platform_admin;
