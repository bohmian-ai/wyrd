# Postgres Layout

Wyrd uses one PostgreSQL control-plane database with schema ownership split by
crate:

- `wyrd-sql` owns `platform` and `wyrd`;
- `vala-sql` owns `vala`; and
- Skald owns no schema in the Wyrd control-plane database.

Operators create every login; Wyrd creates no role and grants nothing to a
named role. The platform login (`WYRD_PLATFORM_DATABASE_URL`, else
`WYRD_DATABASE_URL`) runs the one-off `wyrd-server migrate` process, so it owns
every Wyrd object, including the Bifrost Iceberg catalog, and serves audited
cross-tenant operator sessions. The tenant login (`WYRD_DATABASE_URL`) serves
tenant traffic; a one-login deployment uses the platform login for both.

## Migration ownership and order

Migration sources are the ordered, immutable migration registries owned by
`wyrd-sql` and `vala-sql`. Architecture does not duplicate filenames, counts,
or a snapshot of the table inventory. The deployment gate:

1. acquires one deployment migration lease;
2. verifies applied and source checksums;
3. applies `wyrd_sql::migrate`;
4. applies `vala_sql::migrate`;
5. verifies forced RLS under each table's exact policies and migration
   state; and
6. exits; serving boot re-verifies applied versions, checksums, login
   posture, and forced RLS read-only and refuses readiness when they fail.

The order allows tenant-aware `vala.*` references to Wyrd control-plane state.
There is no `skald_sql::migrate` step in the Wyrd server sequence.

## Tenant layout

Every tenant-scoped row carries
`data_tenant_id UUID NOT NULL REFERENCES platform.tenants(data_tenant_id)`.
Tenant-aware parent/child references use composite keys that include
`data_tenant_id`. Each tenant table enables and forces RLS and installs both
read and write tenant predicates through `wyrd.current_tenant()`.

Tenant-scoped paths in both schemas use `TenantConn`. The caller owns its
transaction; callees neither commit nor roll back. RLS is the database-enforced
boundary, so query modules do not add a parallel tenant predicate. Cross-tenant
work uses a narrow `OperatorPool` capability rather than a raw pool,
connection, or transaction.

See [`sql-foundation.md`](sql-foundation.md) for the complete binding and
transaction contract and
[`../../operations/deployment-and-release.md`](../../operations/deployment-and-release.md)
for migration, release, and connection-budget behavior.
