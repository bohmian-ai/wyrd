-- Wyrd external Postgres role bootstrap template.
--
-- Run this as a cluster administrator before starting wyrd-server in external
-- Postgres mode. Replace the password placeholders with values from the deploy
-- secret store. Migrations must run as wyrd_migrator after these roles exist.

CREATE ROLE wyrd_migrator
    LOGIN
    BYPASSRLS
    PASSWORD 'REPLACE_WITH_WYRD_MIGRATOR_PASSWORD';

CREATE ROLE wyrd_app
    LOGIN
    PASSWORD 'REPLACE_WITH_WYRD_APP_PASSWORD';

CREATE ROLE wyrd_platform_admin
    LOGIN
    BYPASSRLS
    PASSWORD 'REPLACE_WITH_WYRD_PLATFORM_ADMIN_PASSWORD';
