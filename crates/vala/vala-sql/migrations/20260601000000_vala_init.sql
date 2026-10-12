-- Vala observability schema bootstrap.
-- The migration owner creates this schema before running sqlx so
-- vala._sqlx_migrations is routed into the Vala-owned namespace.

CREATE SCHEMA IF NOT EXISTS vala;

-- The ledger is a table like any other: only operator sessions, which the
-- migration runs as, may read or write it.
ALTER TABLE vala._sqlx_migrations ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala._sqlx_migrations FORCE ROW LEVEL SECURITY;
CREATE POLICY operator_access ON vala._sqlx_migrations TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());
