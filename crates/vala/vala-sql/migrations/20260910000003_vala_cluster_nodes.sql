-- vala.cluster_nodes: unified Scribe/Oracle node registry.
--
-- Each Bifrost role (Scribe, Oracle) INSERTs a row on boot and UPDATEs
-- heartbeat_at every 5s. Gate is stateless (no row). Forge uses
-- vala.maintenance_leases instead.
--
-- fencing_token is the writer_epoch source: Scribe bumps it on every boot to
-- obtain a fresh LSN stream. Each (node_id, writer_epoch) pair identifies one
-- pod-local WAL. LSNs from different epochs (or different pods) are never
-- comparable — each epoch starts a fresh LSN sequence at 0.
--
-- NO RLS: This is a cluster-level node registry, not tenant-scoped data.
-- All writes via OperatorPool (BYPASSRLS, audited). Reads via OperatorPool
-- for discovery.

CREATE TABLE vala.cluster_nodes (
    node_id         uuid PRIMARY KEY,
    role            text NOT NULL CHECK (role IN ('scribe', 'oracle')),
    advertise_addr  text NOT NULL,
    fencing_token   bigint NOT NULL,
    started_at      timestamptz NOT NULL,
    heartbeat_at    timestamptz NOT NULL,
    meta            jsonb NOT NULL DEFAULT '{}'::jsonb
);

-- Discovery: list live nodes by role. A row with
-- heartbeat_at < now() - interval '15s' is not live (5s cadence, 15s TTL).
CREATE INDEX cluster_nodes_role_heartbeat_idx
    ON vala.cluster_nodes (role, heartbeat_at);

-- OperatorPool grants (wyrd_platform_admin BYPASSRLS). All writes via
-- OperatorPool (BYPASSRLS, audited); reads via OperatorPool for discovery.
-- TenantConn for anything that projects tenant-scoped columns (none today).
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_platform_admin') THEN
    RAISE EXCEPTION 'wyrd_platform_admin missing — run bootstrap/roles.sql / db:setup-roles';
  END IF;
END $$;

GRANT SELECT, INSERT, UPDATE ON vala.cluster_nodes TO wyrd_platform_admin;
