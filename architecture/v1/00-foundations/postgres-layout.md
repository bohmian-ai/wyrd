# Postgres Layout

Wyrd uses one PostgreSQL database. Runtime request handling uses the
`wyrd_app` pool, boot migrations use a short-lived `wyrd_migrator` pool, and
audited cross-tenant operations use the optional `wyrd_platform_admin` pool.
SQL ownership is split by crate:

- `wyrd-sql` owns `platform` and `wyrd`.
- `vala-sql` owns `vala`.
- No `skald` schema exists in this phase.

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
