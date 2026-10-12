# Tenancy

Wyrd uses `DataTenantId` as the durable tenant isolation key. Human-facing
tenant slugs are resolved at the auth boundary and never become the row-level
tenant key for `wyrd.*` or `vala.*`.

Postgres tenant isolation is enforced with row-level security in the database,
and the pool decides scope. Every tenant-scoped operation
opens a `TenantConn`, binds `app.current_tenant` for that transaction, runs its
queries, and commits. Dropping the connection wrapper rolls the transaction
back and clears the transaction-local tenant binding.

The pool split is:

| Pool | Purpose |
|---|---|
| Tenant (`WYRD_DATABASE_URL`) | Runtime request, MCP, worker, and tenant-scoped maintenance queries through `TenantConn`. Only `tenant_isolation` admits rows. |
| Platform (`WYRD_PLATFORM_DATABASE_URL`, else `WYRD_DATABASE_URL`) | Audited cross-tenant support and the platform control plane: initialization, the tenant directory, provisioning, and tenant-admin recovery; the Bifrost Iceberg catalog. Connects with `app.operator=on`, which every table's `operator_access` policy admits for the owning login only. |

Operators create the logins; Wyrd creates no role and grants nothing to a
named role. DDL runs only in the one-off `wyrd-server migrate` process as the
platform login, which therefore owns every Wyrd object. With one login the
pool alone separates tenant from operator scope. With a separate tenant login
the database does too: that login is not the owner, so `operator_access` never
applies to it. Boot refuses superuser and `BYPASSRLS` logins, which Postgres
exempts from every policy.

Wyrd's auth and RBAC model is the single permission model. Postgres enforces
only tenant isolation and table-local invariants (constraints and triggers that
hold for every login); migrations add no per-command, per-column, or per-login
permissions.

Deployment modes share the same code path:

| Mode | Tenant catalog | Platform-admin posture |
|---|---|---|
| Self-hosted | Usually one `platform.tenants` row, with more allowed for internal multi-tenancy. | Required: the platform plane provisions and recovers tenants through it. |
| Cloud multi-tenant | One shared cluster with many tenant rows. | Required small audited pool. |
| Cloud dedicated | One customer cluster with one tenant row. | Required for the platform plane, held as a break-glass credential. |

`platform.*` remains above the tenant boundary. Narrow tenant lookup functions
may use `SECURITY DEFINER` with a pinned `search_path`, raising the operator
flag only for their own body, so a tenant session can resolve slugs without
reading the directory. Tenant-scoped tables in `wyrd.*` and
`vala.*` carry `data_tenant_id`, lead their tenant-aware indexes and composite
foreign keys with it, and use RLS policies against `wyrd.current_tenant()`.
