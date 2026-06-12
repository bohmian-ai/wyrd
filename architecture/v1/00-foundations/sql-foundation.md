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
