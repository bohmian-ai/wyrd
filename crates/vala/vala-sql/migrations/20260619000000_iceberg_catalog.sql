-- Iceberg JDBC catalog schema.
--
-- iceberg-catalog-sql creates iceberg_tables + iceberg_namespace_properties at
-- runtime (CREATE TABLE IF NOT EXISTS); this migration creates only the schema
-- so those tables land in the right place.
--
-- The catalog tables are tenant-free. Tenancy is enforced at the application
-- layer (vala.bifrost_tables). metadata_location is a cross-tenant oracle, so a
-- separate tenant login is never granted access to this schema; tenant work
-- reaches a pointer only through vala.oracle_catalog_metadata_location.

CREATE SCHEMA IF NOT EXISTS iceberg_catalog;

-- The catalog crate creates its tables at runtime under the platform login
-- (its catalog URI pins search_path=iceberg_catalog), which owns this schema.
