-- Wyrd external Postgres role bootstrap template.
--
-- Run this as a cluster administrator before starting wyrd-server in external
-- Postgres mode. The psql caller supplies four variables from the deploy secret
-- store (`--set migrator_password=...`, etc.). Migrations must run as
-- wyrd_migrator after these roles exist.

\set ON_ERROR_STOP on

SELECT format('CREATE ROLE wyrd_migrator LOGIN NOCREATEDB BYPASSRLS NOSUPERUSER PASSWORD %L', :'migrator_password')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_migrator')\gexec
SELECT format('CREATE ROLE wyrd_app LOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER PASSWORD %L', :'app_password')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_app')\gexec
SELECT format('CREATE ROLE wyrd_platform_admin LOGIN NOCREATEDB BYPASSRLS NOSUPERUSER PASSWORD %L', :'platform_admin_password')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_platform_admin')\gexec
SELECT 'CREATE ROLE wyrd_catalog NOLOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER'
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_catalog')\gexec
SELECT format('CREATE ROLE wyrd_catalog_app LOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER PASSWORD %L', :'catalog_app_password')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_catalog_app')\gexec

ALTER ROLE wyrd_migrator WITH LOGIN NOCREATEDB BYPASSRLS NOSUPERUSER PASSWORD :'migrator_password';
ALTER ROLE wyrd_app WITH LOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER PASSWORD :'app_password';
ALTER ROLE wyrd_platform_admin WITH LOGIN NOCREATEDB BYPASSRLS NOSUPERUSER PASSWORD :'platform_admin_password';
ALTER ROLE wyrd_catalog WITH NOLOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER;
ALTER ROLE wyrd_catalog_app WITH LOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER PASSWORD :'catalog_app_password';

REVOKE ALL PRIVILEGES ON DATABASE wyrd FROM wyrd_migrator, wyrd_app, wyrd_platform_admin, wyrd_catalog, wyrd_catalog_app;
REVOKE wyrd_migrator, wyrd_app, wyrd_platform_admin, wyrd_catalog_app FROM wyrd_migrator, wyrd_app, wyrd_platform_admin, wyrd_catalog, wyrd_catalog_app;

DO $$
DECLARE membership record;
BEGIN
    FOR membership IN
        SELECT parent.rolname AS parent_name, member.rolname AS member_name
        FROM pg_auth_members memberships
        JOIN pg_roles parent ON parent.oid = memberships.roleid
        JOIN pg_roles member ON member.oid = memberships.member
        WHERE member.rolname IN ('wyrd_migrator', 'wyrd_app', 'wyrd_platform_admin', 'wyrd_catalog_app')
          AND NOT (
              parent.rolname = 'wyrd_catalog'
              AND member.rolname IN ('wyrd_migrator', 'wyrd_catalog_app')
          )
    LOOP
        EXECUTE format('REVOKE %I FROM %I', membership.parent_name, membership.member_name);
    END LOOP;
END $$;

GRANT CONNECT ON DATABASE wyrd TO wyrd_migrator, wyrd_app, wyrd_platform_admin, wyrd_catalog_app;
GRANT CREATE ON DATABASE wyrd TO wyrd_migrator;

GRANT wyrd_catalog TO wyrd_catalog_app;
GRANT wyrd_catalog TO wyrd_migrator;
