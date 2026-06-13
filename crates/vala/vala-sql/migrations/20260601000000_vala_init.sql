-- Vala observability schema bootstrap.
-- The migrator creates this schema before running sqlx so
-- vala._sqlx_migrations is routed into the Vala-owned namespace.

CREATE SCHEMA IF NOT EXISTS vala;
