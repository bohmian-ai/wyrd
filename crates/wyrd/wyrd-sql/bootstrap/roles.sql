-- Wyrd external Postgres role bootstrap template.
--
-- Run this once as the database owner (a cluster administrator login) before
-- `wyrd-server migrate`. The caller supplies both serving passwords from the
-- deploy secret store through the environment variables
-- `WYRD_APP_PASSWORD` and `WYRD_PLATFORM_ADMIN_PASSWORD` (psql 15 or later),
-- which keeps them out of process arguments. A psql `--set app_password=...`
-- or `--set platform_admin_password=...` takes precedence and is meant only for
-- throwaway local and test databases. The same owner login then runs
-- `wyrd-server migrate`; serving Wyrd receives only the two roles below.

\set ON_ERROR_STOP on
\if :{?app_password}
\else
\getenv app_password WYRD_APP_PASSWORD
\endif
\if :{?platform_admin_password}
\else
\getenv platform_admin_password WYRD_PLATFORM_ADMIN_PASSWORD
\endif

SELECT format('CREATE ROLE wyrd_app LOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER PASSWORD %L', :'app_password')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_app')\gexec
SELECT format('CREATE ROLE wyrd_platform_admin LOGIN NOCREATEDB BYPASSRLS NOSUPERUSER PASSWORD %L', :'platform_admin_password')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_platform_admin')\gexec

ALTER ROLE wyrd_app WITH LOGIN NOCREATEDB NOBYPASSRLS NOSUPERUSER PASSWORD :'app_password';
ALTER ROLE wyrd_platform_admin WITH LOGIN NOCREATEDB BYPASSRLS NOSUPERUSER PASSWORD :'platform_admin_password';

REVOKE ALL PRIVILEGES ON DATABASE wyrd FROM wyrd_app, wyrd_platform_admin;
REVOKE wyrd_app, wyrd_platform_admin FROM wyrd_app, wyrd_platform_admin;

GRANT CONNECT ON DATABASE wyrd TO wyrd_app, wyrd_platform_admin;

-- Migrations hand the cross-tenant retirement-proof function to
-- wyrd_platform_admin, which requires the migrating owner to hold that role.
GRANT wyrd_platform_admin TO CURRENT_USER;
