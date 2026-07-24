-- Slice 01: maintenance-worker lease table + fencing sequence.
--
-- Maintenance workers (compaction, snapshot expiry, orphan GC, async query
-- executor) hold a named lease identified by `lease_key`, fenced by a monotonic
-- `fencing_token` stamped when the worker claimed it. This is a cross-tenant
-- control-plane table: lease keys are global (e.g. 'compaction:table:<uid>',
-- 'snapshot_expiry:global') and carry no tenant column, so there is NO RLS.
-- Access is via OperatorPool (wyrd_platform_admin BYPASSRLS) per plan §4.
--
-- Plan §9-02 forbids adding a lease-key format CHECK constraint (later slices
-- introduce their own key namespaces); `lease_key` is a plain PRIMARY KEY.
--
-- NEVER creates roles here — only asserts existence and GRANTs to existing roles.

DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_platform_admin') THEN
    RAISE EXCEPTION 'wyrd_platform_admin missing — run bootstrap/roles.sql / db:setup-roles';
  END IF;
END $$;

GRANT USAGE ON SCHEMA vala TO wyrd_platform_admin;

-- Monotonic fencing sequence. Token 0 is never issued; nextval starts at 1, so
-- any live lease carries a strictly positive fencing token.
CREATE SEQUENCE IF NOT EXISTS vala.maintenance_fencing_seq;

CREATE TABLE vala.maintenance_leases (
    lease_key      text        PRIMARY KEY,
    owner          uuid        NOT NULL,
    fencing_token  bigint      NOT NULL,
    expires_at     timestamptz NOT NULL,
    heartbeat_at   timestamptz NOT NULL
);

GRANT USAGE ON SEQUENCE vala.maintenance_fencing_seq TO wyrd_platform_admin;
GRANT SELECT, INSERT, UPDATE, DELETE ON vala.maintenance_leases TO wyrd_platform_admin;
