-- Per-database setup a DBA performs for a two-login Wyrd deployment, as the
-- test harness performs it for every test database. Runs as a cluster
-- administrator connected to a database owned by `wyrd_platform`.
--
-- `wyrd_platform` owns the database, runs `wyrd-server migrate`, and so owns
-- every Wyrd object. `wyrd_tenant` serves tenant traffic: it may call Wyrd's
-- functions and read and write rows in `wyrd` and `vala`, where row-level
-- security confines it to the bound tenant. It gets nothing in
-- `iceberg_catalog` and no table privileges in `platform`.
CREATE SCHEMA IF NOT EXISTS platform AUTHORIZATION wyrd_platform;
CREATE SCHEMA IF NOT EXISTS wyrd AUTHORIZATION wyrd_platform;
CREATE SCHEMA IF NOT EXISTS vala AUTHORIZATION wyrd_platform;
GRANT USAGE ON SCHEMA platform, wyrd, vala TO wyrd_tenant;
ALTER DEFAULT PRIVILEGES FOR ROLE wyrd_platform IN SCHEMA wyrd, vala
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO wyrd_tenant;
ALTER DEFAULT PRIVILEGES FOR ROLE wyrd_platform IN SCHEMA wyrd, vala
    GRANT USAGE, SELECT ON SEQUENCES TO wyrd_tenant;
