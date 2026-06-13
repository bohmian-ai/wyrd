# Postgres Layout

Wyrd uses one PostgreSQL database. Runtime request handling uses the
`wyrd_app` pool, boot migrations use a short-lived `wyrd_migrator` pool, and
audited cross-tenant operations use the optional `wyrd_platform_admin` pool.
SQL ownership is split by crate:

- `wyrd-sql` owns `platform` and `wyrd`.
- `vala-sql` owns `vala`.
- No `skald` schema exists in this phase.

`wyrd-sql` boots its schemas through two forward-only migrations:

1. `0001_platform.sql` creates the `platform` and `wyrd` schemas, validates the
   three pre-provisioned login roles, grants object privileges, defines
   `wyrd.current_tenant()`, creates `platform.tenants`, and installs the
   hardened `platform.resolve_tenant_by_slug(text)` lookup.
2. `0002_auth.sql` creates tenant-scoped `wyrd.auth_*` identity and credential
   tables. Every row carries `data_tenant_id UUID NOT NULL REFERENCES
   platform.tenants(data_tenant_id)`, tenant-aware child foreign keys use
   composite keys, and every table ships with the `ENABLE` + `FORCE` +
   `tenant_isolation` RLS policy block.

Migrations never create cluster roles. Role provisioning is handled by external
infrastructure bootstrap or embedded Postgres boot before SQL migrations run.

`wyrd-server` applies migrators in this order:

1. `wyrd_sql::migrate(pool)`
2. `vala_sql::migrate(pool)`

The order lets future `vala.*` migrations reference `wyrd.registry_cards` with
tenant-aware composite foreign keys after Wyrd control-plane tables exist.
`skald_sql::migrate(pool)` is deferred until the Skald storage phase.

Tenant-scoped query paths in both `wyrd.*` and `vala.*` use
`wyrd_sql::TenantConn` so each logical operation runs in a transaction with
`app.current_tenant` bound through `set_config(..., true)`. RLS policies are the
database-enforced tenant boundary; application-layer tenant filters are not the
source of truth.
