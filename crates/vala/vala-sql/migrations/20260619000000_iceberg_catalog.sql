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

-- wyrd_catalog must exist before any GRANT names it. Migrations must not
-- create cluster roles (they are cluster-scoped and would TOCTOU-flake under
-- parallel #[sqlx::test] DBs). Role creation lives in bootstrap/roles.sql,
-- role_bootstrap.rs, and wyrd_sql::testing. This migration only asserts
-- existence, mirroring the pattern in platform.sql.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_catalog') THEN
        RAISE EXCEPTION
            'role wyrd_catalog missing — infra/boot/testing role bootstrap incomplete (see bootstrap/roles.sql + role_bootstrap.rs + wyrd_sql::testing)';
    END IF;
END$$;

-- wyrd_catalog is the owner/privilege role for iceberg_catalog objects. It
-- needs CREATE because the catalog crate creates iceberg_tables +
-- iceberg_namespace_properties at runtime under this role identity (the
-- catalog URI sets role=wyrd_catalog so even the connecting wyrd_catalog_app
-- authenticates but acts as the owner).
GRANT USAGE, CREATE ON SCHEMA iceberg_catalog TO wyrd_catalog;

-- ALTER DEFAULT PRIVILEGES targets the role that will CREATE the tables.
-- The tables are created at runtime with wyrd_catalog as the effective
-- creator (via role= in the URI), so the FOR ROLE here must be wyrd_catalog.
-- The migrator must be a member of wyrd_catalog for this to fire — see
-- bootstrap/roles.sql + role_bootstrap.rs for the GRANT wyrd_catalog TO
-- wyrd_migrator that satisfies this.
ALTER DEFAULT PRIVILEGES FOR ROLE wyrd_catalog IN SCHEMA iceberg_catalog
    REVOKE ALL ON TABLES FROM wyrd_app;

-- Defense in depth: revoke access on any tables that already exist, and
-- prevent schema traversal.
REVOKE ALL ON ALL TABLES IN SCHEMA iceberg_catalog FROM wyrd_app;
REVOKE USAGE ON SCHEMA iceberg_catalog FROM wyrd_app;
REVOKE ALL ON SCHEMA iceberg_catalog FROM PUBLIC;
