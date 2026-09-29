# SQL Foundation

Wyrd uses one PostgreSQL control-plane database. `wyrd-sql` owns the
`platform` and `wyrd` schemas; `vala-sql` owns the `vala` schema. Analytical
payload bytes remain in Bifrost object storage rather than Postgres.

Tenant-scoped `wyrd.*` and `vala.*` rows use
`data_tenant_id UUID NOT NULL` as the leading tenant key. The Rust contract is
`wyrd_spec::ids::DataTenantId`. Human-facing `TenantSlug` values resolve to a
`DataTenantId` at the authenticated API boundary and do not enter durable
tenant-scoped rows. `space` is a registry namespace inside a tenant and is
never a tenancy boundary.

Tenant-scoped parents expose composite foreign-key targets such as
`(data_tenant_id, id)` or `(data_tenant_id, uid)`. Children carry the same
tenant key in their foreign keys, including cross-schema references.
`platform.*` is the explicitly privileged plane above tenant scope;
`platform.tenants` owns the tenant catalog.

## Runtime tenant binding

Postgres row-level security is the authoritative tenant boundary. Runtime
tenant traffic uses the `wyrd_app` login role without `BYPASSRLS`. Every
tenant-scoped logical operation acquires one `TenantConn`, which opens a
transaction and binds the verified `DataTenantId` through transaction-local
configuration:

```sql
SELECT set_config('app.current_tenant', $1, true)
```

RLS policies use the strict `wyrd.current_tenant()` helper and apply both
`USING` and `WITH CHECK` predicates:

```sql
ALTER TABLE wyrd.example ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.example FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.example
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
```

A missing tenant binding fails rather than returning an empty cross-tenant
result. Tenant identity is parameter-bound; callers do not build tenant SQL
strings or add a second hand-written tenant predicate.

## Transaction discipline

Every tenant-scoped logical operation opens exactly one transaction. The caller
owns that `TenantConn` transaction and its final commit or rollback.
Tenant-scoped query and service functions accept `&mut TenantConn<'_>` and
never accept a raw `PgPool`, `PgConnection`, or SQLx transaction. Callees do
not commit, roll back, open a nested transaction, or issue raw transaction
control. Related writes, relationships, and canonical audit rows compose in the
same caller-owned transaction.

Cross-tenant work uses only a named `OperatorPool` capability under explicit
platform authority. Each operator capability validates the target tenant,
lease generation, fence, operation identity, and audit before mutation. It
does not expose a raw pool, connection, transaction, or generic query method.
`SECURITY DEFINER` functions are narrow, reviewed bridges and never become a
general RLS bypass.

Cross-crate transactional coordination is not supported. Cross-crate work does
not extend a transaction by importing another crate's private query modules. A cross-owner durable effect uses its declared committed
handoff and idempotent consumer semantics.

## Roles, pools, and boot

| Role | Lifetime | Capability |
|---|---|---|
| `wyrd_app` | Runtime | Tenant-scoped RLS traffic through `TenantConn` |
| `wyrd_platform_admin` | Runtime | Fenced, audited cross-tenant work through `OperatorPool`; Bifrost Iceberg catalog owner |
| database owner | `wyrd-server migrate` only | Ordered DDL for Wyrd and Vala schemas; never given to a serving process |

`wyrd-server migrate` reads the owner DSN from `WYRD_DATABASE_URL`, applies
`wyrd_sql::migrate` and then `vala_sql::migrate` under the migration advisory
lock, validates both ledgers and forced RLS, and exits.

Serving boot performs these steps in order:

1. Resolve `WYRD_DATABASE_URL` (`wyrd_app`) and `WYRD_PLATFORM_DATABASE_URL`
   (`wyrd_platform_admin`); both are required.
2. Build the serving pools. No DDL runs.
3. `WyrdPostgres::validate_schema` and `ValaPostgres::validate_schema` verify
   login posture, every embedded migration version and checksum, forced RLS on
   tenant tables, and the catalog privilege boundary.
4. Construct runtime `TenantConn` and approved `OperatorPool` owners and make
   only those capabilities available to services.

There is no embedded Postgres. The canonical test harness uses
repository-managed, lane-isolated Postgres databases migrated by their owner
login.

Pool configuration has one canonical typed model. Unsuffixed `WYRD_DB_*`
settings tune the application pool; `_MIGRATOR` tunes the one-off migration
pool and `_PLATFORM_ADMIN` the platform pool. Missing suffixed settings use that role's
typed defaults and never inherit the application value.

The deployment proves this connection budget against the maximum replica and
rollout-surge count:

```text
replicas * (app_max + platform_admin_max)
  + concurrent_migrator_max
  + database_reserved
  <= postgres_max_connections
```

`database_reserved` is an explicit deployment decision covering
administration, replication, monitoring, failover, and extensions. No
universal numeric reserve substitutes for that calculation.

Transaction-mode PgBouncer requires statement-cache capacity `0` on each pool
routed through it. Tenant state remains transaction-local. Session-scoped
state, session advisory locks, and `LISTEN`/`NOTIFY` are not supported on a
transaction-pooled path.

## Migration and verification contract

Migration files are immutable, ordered, checksum-verified, and owned by their
schema crate. Architecture does not duplicate their filename or count. Restore
and release tooling derives the required set from the registered migration
sources and rejects missing, reordered, modified, or partially applied
migrations.

Verification covers role attributes, grants, schema ownership, migration
checksums, the system-tenant sentinel, tenant binding, RLS catalog state,
same-tenant success, cross-tenant denial, operator fencing, and transactional
audit behavior. Canonical repository `mise` tasks provision Postgres and run
these checks; tests do not silently skip required SQL proof because an
environment variable is absent.
