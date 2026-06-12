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
