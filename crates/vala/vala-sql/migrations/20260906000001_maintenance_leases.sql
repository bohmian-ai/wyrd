-- Slice 01: maintenance-worker lease table + fencing sequence.
--
-- Maintenance workers (compaction, snapshot expiry, orphan GC, async query
-- executor) hold a named lease identified by `lease_key`, fenced by a monotonic
-- `fencing_token` stamped when the worker claimed it. This is a cross-tenant
-- control-plane table: lease keys are global (e.g. 'compaction:table:<uid>',
-- 'snapshot_expiry:global') and carry no tenant column, so only operator
-- sessions pass its row-level security.
--
-- Plan §9-02 forbids adding a lease-key format CHECK constraint (later slices
-- introduce their own key namespaces); `lease_key` is a plain PRIMARY KEY.

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

ALTER TABLE vala.maintenance_leases ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.maintenance_leases FORCE ROW LEVEL SECURITY;
CREATE POLICY operator_access ON vala.maintenance_leases TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());
