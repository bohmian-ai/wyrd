# SQL Foundation

Tenant-scoped `wyrd.*` and `vala.*` rows use `data_tenant_id UUID NOT NULL` as
the leading tenant key. The Rust contract is `wyrd_spec::ids::DataTenantId`, a
UUIDv7-backed newtype. Human-facing tenant URLs use `TenantSlug`; the server
resolves that slug to `DataTenantId` at the auth/API boundary and does not
thread slugs through durable tenant-scoped rows.

`space` is not the tenant key. It remains a user namespace within a tenant,
primarily for registry card identity. Tenant isolation is keyed by
`data_tenant_id`.

Every tenant-scoped parent table should expose a composite foreign-key target
such as `(data_tenant_id, id)` or `(data_tenant_id, uid)`, and tenant-scoped
children should reference parents through the same composite key. This keeps
same-schema and cross-schema references from crossing tenant boundaries at the
database layer.

`platform.*` is above the tenant boundary and does not carry `data_tenant_id` on
every row. `platform.tenants` is the tenant catalog whose primary key is
`data_tenant_id`.

## Runtime Tenant Binding

Row-level security is the primary tenant boundary for tenant-scoped `wyrd.*`
and `vala.*` tables. Runtime request paths use the `wyrd_app` database role,
which does not bypass RLS. Migration paths use the boot-only `wyrd_migrator`
role, and audited cross-tenant support paths use `wyrd_platform_admin` when
that credential is provisioned.

Tenant-scoped query modules run inside `wyrd_sql::TenantConn`. Acquiring a
`TenantConn` opens a transaction on the runtime pool and binds
`app.current_tenant` with:

```sql
SELECT set_config('app.current_tenant', $1, true)
```

The tenant value is always parameter-bound from `DataTenantId`; callers do not
compose tenant SQL strings. The third `set_config` argument keeps the setting
local to the transaction, so commit or rollback clears the tenant before the
connection returns to the pool.

RLS policies call the shared SQL helper `wyrd.current_tenant()`, which reads
`app.current_tenant` and casts it to `uuid`. Policies use the strict
`current_setting` form so a missing tenant binding fails loudly instead of
returning an empty result set.

Every tenant-scoped table in `wyrd.*` and `vala.*` follows this policy shape.
The `wyrd-sql` `0002_auth.sql` migration applies it inline for every
`wyrd.auth_*` table:

```sql
ALTER TABLE wyrd.example ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.example FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.example
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
```

Live verification of policy behavior depends on a Postgres database whose
cluster roles have already been bootstrapped. The `wyrd-sql` migration tests
skip when `DATABASE_URL` is unset; when run against a live database they assert
role metadata, `platform.tenants`, `wyrd.current_tenant()`, tenant-scoped auth
tables, and RLS catalog state.

## Transaction Discipline

Every tenant-scoped logical operation opens exactly one `TenantConn` from the
runtime `wyrd_app` pool. Reads, writes, audit rows, relationship updates, and
same-crate cross-domain work for that operation share the transaction opened by
that wrapper. Even read-only handlers commit the `TenantConn` at the end so the
shape stays uniform; dropping it without commit rolls back through SQLx.

Tenant-scoped query functions take `&mut TenantConn<'_>`. They do not take a
raw `PgPool`, open their own SQLx transaction, or issue raw transaction-control
SQL. Wyrd v1 does not use nested transactions or savepoints. If a sub-operation
appears to need a savepoint, split or refactor the operation boundary instead
of hiding partial rollback inside the query layer.

Platform operations are the exception because `platform.*` is above the tenant
boundary. Audited platform-admin reads and writes run on the platform-admin
pool and may use a bare SQLx transaction for one platform-scoped operation.
Runtime tenant resolution remains the narrow `SECURITY DEFINER` bridge exposed
to `wyrd_app`; it is not a tenant-scoped data operation.

Cross-crate transactional coordination is not supported. `wyrd-sql` must not
import `vala-sql` query modules, and `vala-sql` must not call Wyrd query write
functions to extend a Wyrd transaction. Downstream Vala effects are propagated
after the Wyrd commit through the future outbox/event fanout path and are
handled idempotently by Vala.

## Connection Pools

Wyrd server boot uses three Postgres login roles but only runtime-safe pools
survive into `AppState`.

| Role | Pool lifetime | Default max | Statement cache | Purpose |
|---|---:|---:|---:|---|
| `wyrd_app` | runtime | 32 | 256 | Tenant-scoped HTTP, MCP, worker, and Vala query traffic. RLS applies. |
| `wyrd_migrator` | boot only | 2 | 0 | DDL and migrations for Wyrd and Vala schemas. Has `BYPASSRLS` and is closed before runtime state exists. |
| `wyrd_platform_admin` | optional runtime | 2 | 64 | Audited cross-tenant platform operations. Dedicated deployments may omit it. |

The server boot sequence is:

1. Resolve role DSNs from `WYRD_DATABASE_URL`,
   `WYRD_DATABASE_URL_MIGRATOR`, and optional
   `WYRD_DATABASE_URL_PLATFORM_ADMIN`, or derive all three from embedded
   Postgres.
2. Build the `wyrd_migrator` pool with migrator defaults.
3. Run `wyrd-sql` migrations and `vala-sql` migrations against that same
   migrator pool.
4. Close the migrator pool.
5. Build the runtime `wyrd_app` pool and optional `wyrd_platform_admin` pool.
6. Assemble `AppState { pool, platform_admin_pool }`.

`AppState` carries only `pool: PgPool` for runtime tenant-scoped traffic and
`platform_admin_pool: Option<PgPool>` for audited platform routes. The migrator
pool is never stored on `AppState`; keeping a long-lived `BYPASSRLS` migrator
connection available to handlers would bypass the tenancy model. `PgPool`
clones are cheap handles over shared pool state, so axum `State<AppState>`
threads those pools into request handlers.

Vala consumes the shared Wyrd runtime pool by reference through
`TenantConn<'_>` for tenant-scoped work. It does not build a fourth pool or own
a separate runtime connection budget. Vala migrations consume the same
boot-only migrator pool before it is closed.

Pool tuning lives in `wyrd-sql` `PoolConfig`. Unsuffixed `WYRD_DB_*` variables
tune the runtime `wyrd_app` pool. The same names suffixed with `_MIGRATOR` or
`_PLATFORM_ADMIN` tune the boot migrator and platform-admin pools
respectively. Missing suffixed variables fall back to that role's defaults, not
to the unsuffixed app value.

| Variable | Runtime default | Notes |
|---|---:|---|
| `WYRD_DB_MAX_CONNECTIONS` | 32 | Per server pod. |
| `WYRD_DB_MIN_CONNECTIONS` | 2 | Warm runtime connections. |
| `WYRD_DB_ACQUIRE_TIMEOUT_SECS` | 5 | Fail fast when the pool is exhausted. |
| `WYRD_DB_IDLE_TIMEOUT_SECS` | 300 | Use `off` to disable idle reaping. |
| `WYRD_DB_MAX_LIFETIME_SECS` | 1800 | Use `off` to disable lifetime recycling. |
| `WYRD_DB_STATEMENT_CACHE_CAPACITY` | 256 | Set to `0` behind transaction-mode PgBouncer. |
| `WYRD_DB_TEST_BEFORE_ACQUIRE` | true | Checks stale connections before reuse. |

The connection budget formula is:

```text
pods * (app_max + platform_admin_max) + migrator_max <= pg.max_connections - reserved
```

Reserve at least ten server-side connections for Postgres administration and
extension roles. The migrator budget is short-lived at boot; steady-state
runtime capacity is dominated by `app_max`.

For transaction-mode PgBouncer, set
`WYRD_DB_STATEMENT_CACHE_CAPACITY=0`. SQLx prepared statement caches live on a
physical upstream Postgres connection, while transaction pooling can reassign
that upstream connection between client transactions. Tenant binding remains
valid because Wyrd uses `set_config('app.current_tenant', $1, true)`, which is
transaction-scoped rather than session-scoped. Future worker code must avoid
session-scoped advisory locks and prefer transaction-scoped locking patterns.
`LISTEN`/`NOTIFY` is not compatible with transaction pooling and is not part of
the current SQL foundation.
