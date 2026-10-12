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

Postgres row-level security is the authoritative tenant boundary, and the pool
decides scope. Every table forces RLS. Tenant-keyed tables carry a
`tenant_isolation` policy for every role; every table carries an
`operator_access` policy, targeting only the login that ran the migrations,
which admits sessions with `app.operator=on`. Platform and catalog pools
connect with that flag; tenant pools never set it, and a separate tenant login
gains nothing by setting it because the policy does not apply to it. Every
tenant-scoped logical operation acquires one `TenantConn`, which binds the
verified `DataTenantId` through transaction-local configuration and opens the
transaction in one round trip:

```sql
SELECT set_config('app.current_tenant', '<tenant uuid>', true); BEGIN
```

The binding precedes `BEGIN` deliberately. Postgres runs the two statements as
one implicit transaction that `BEGIN` turns explicit, so the binding survives
into it; if the binding fails, `BEGIN` never runs and Postgres rolls everything
back, returning the pooled connection idle. `BEGIN` first would leave a failed
binding inside an aborted transaction that SQLx does not roll back, poisoning
the next borrower. A simple query takes no parameters, so the tenant is inlined
from the typed `DataTenantId`, whose UUID rendering cannot contain a quote.
`TenantConn` is the only code that builds this statement and the only code
allowed raw transaction control.

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
result. Outside `TenantConn`'s begin statement, tenant identity is
parameter-bound; callers do not build tenant SQL strings or add a second
hand-written tenant predicate.

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

## Logins, pools, and boot

| Login | Lifetime | Capability |
|---|---|---|
| platform (`WYRD_PLATFORM_DATABASE_URL`, else `WYRD_DATABASE_URL`) | `wyrd-server migrate` and runtime | Owns every Wyrd object; fenced, audited cross-tenant operator sessions through `OperatorPool`; Bifrost Iceberg catalog |
| tenant (`WYRD_DATABASE_URL`) | Runtime | Tenant-scoped RLS traffic through `TenantConn` |

`wyrd-server migrate` connects as the platform login, applies
`wyrd_sql::migrate` and then `vala_sql::migrate` under the migration advisory
lock, validates both ledgers and forced RLS, and exits.

Serving boot performs these steps in order:

1. Resolve the required `WYRD_DATABASE_URL` and the optional
   `WYRD_PLATFORM_DATABASE_URL`, which defaults to it.
2. Build the tenant pool and the platform operator pool. No DDL runs.
3. `WyrdPostgres::validate_schema` and `ValaPostgres::validate_schema` refuse
   a superuser or `BYPASSRLS` login and a platform login that does not own
   Wyrd's objects; with two logins they also refuse a tenant login that owns
   them, holds `TRUNCATE`, `REFERENCES`, or `TRIGGER`, or reaches a table RLS
   does not protect. Then they verify every embedded migration version and
   checksum and forced RLS under each table's exact policies.
4. Construct runtime `TenantConn` and approved `OperatorPool` owners and make
   only those capabilities available to services.

There is no embedded Postgres. The canonical test harness uses
repository-managed, lane-isolated Postgres databases owned and migrated by a
platform login and served with a separate tenant login.

Pool configuration has one canonical typed model. Unsuffixed `WYRD_DB_*`
settings tune the tenant pool; `_MIGRATOR` tunes the one-off migration
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
