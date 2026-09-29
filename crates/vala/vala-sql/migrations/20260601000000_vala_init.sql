-- Vala observability schema bootstrap.
-- The migration owner creates this schema before running sqlx so
-- vala._sqlx_migrations is routed into the Vala-owned namespace.

CREATE SCHEMA IF NOT EXISTS vala;

-- Serving boot proves the applied migrations match its binary.
GRANT SELECT ON TABLE vala._sqlx_migrations TO wyrd_app, wyrd_platform_admin;
