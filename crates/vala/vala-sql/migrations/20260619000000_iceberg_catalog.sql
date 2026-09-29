-- Iceberg JDBC catalog schema.
--
-- iceberg-catalog-sql creates iceberg_tables + iceberg_namespace_properties at
-- runtime (CREATE TABLE IF NOT EXISTS); this migration creates only the schema
-- and the privilege boundary so those tables land in the right place.
--
-- The catalog tables are tenant-free. Tenancy is enforced at the application
-- layer (vala.bifrost_tables). metadata_location is a cross-tenant oracle:
-- the request-path role (wyrd_app) must NOT be able to SELECT it.

CREATE SCHEMA IF NOT EXISTS iceberg_catalog;

-- wyrd_platform_admin owns Iceberg catalog work. The catalog crate creates
-- iceberg_tables + iceberg_namespace_properties at runtime under that login
-- (its catalog URI pins search_path=iceberg_catalog), so it owns them and
-- needs CREATE here. The request-path wyrd_app role gets nothing.
GRANT USAGE, CREATE ON SCHEMA iceberg_catalog TO wyrd_platform_admin;

-- Defense in depth: revoke access on any tables that already exist, and
-- prevent schema traversal.
REVOKE ALL ON ALL TABLES IN SCHEMA iceberg_catalog FROM wyrd_app;
REVOKE USAGE ON SCHEMA iceberg_catalog FROM wyrd_app;
REVOKE ALL ON SCHEMA iceberg_catalog FROM PUBLIC;
