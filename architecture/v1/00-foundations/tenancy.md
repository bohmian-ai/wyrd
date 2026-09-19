# Tenancy

Wyrd uses `DataTenantId` as the durable tenant isolation key. Human-facing
tenant slugs are resolved at the auth boundary and never become the row-level
tenant key for `wyrd.*` or `vala.*`.

Postgres tenant isolation is enforced with row-level security in the database.
The runtime `wyrd_app` role does not bypass RLS. Every tenant-scoped operation
opens a `TenantConn`, binds `app.current_tenant` for that transaction, runs its
queries, and commits. Dropping the connection wrapper rolls the transaction
back and clears the transaction-local tenant binding.

The role split is:

| Role | Purpose |
|---|---|
| `wyrd_migrator` | Boot-only DDL and migrations. Has `BYPASSRLS`; migration connections are closed after use. |
| `wyrd_app` | Runtime request, MCP, worker, and tenant-scoped maintenance queries. RLS applies. |
| `wyrd_platform_admin` | Audited cross-tenant support and the platform control plane: initialization, the tenant directory, provisioning, and tenant-admin recovery. Has `BYPASSRLS`. |

Deployment modes share the same code path:

| Mode | Tenant catalog | Platform-admin posture |
|---|---|---|
| Self-hosted | Usually one `platform.tenants` row, with more allowed for internal multi-tenancy. | Required: the platform plane provisions and recovers tenants through it. |
| Cloud multi-tenant | One shared cluster with many tenant rows. | Required small audited pool. |
| Cloud dedicated | One customer cluster with one tenant row. | Required for the platform plane, held as a break-glass credential; no silent fallback to `wyrd_app`. |

`platform.*` remains above the tenant boundary. Narrow tenant lookup functions
may use `SECURITY DEFINER` with a pinned `search_path` so the runtime role can
resolve slugs without direct table access. Tenant-scoped tables in `wyrd.*` and
`vala.*` carry `data_tenant_id`, lead their tenant-aware indexes and composite
foreign keys with it, and use RLS policies against `wyrd.current_tenant()`.
