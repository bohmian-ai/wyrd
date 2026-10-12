# Deployment

Wyrd deploys as one logical `wyrd-server` serving surface backed by durable
Postgres and object storage. The logical server may use multiple replicas and
role-targeted pods behind one gateway. Supported tenant topologies are
self-hosted, multi-tenant SaaS, and single-tenant enterprise cloud; they share
one protocol and server architecture.

The normative operating contracts are:

- [`operations/deployment-and-release.md`](../../operations/deployment-and-release.md)
  for topology, TLS, configuration, database roles, migrations, rollout, and
  version skew;
- [`operations/reliability-and-recovery.md`](../../operations/reliability-and-recovery.md)
  for objectives, capacity, backup, restore, failure boundaries, and incidents;
  and
- [`wyrd-security-posture.md`](../../wyrd-security-posture.md) for identity,
  authorization, tenant isolation, credentials, peer trust, Sources, and audit.

## Database connection budget

Wyrd SQL deployments size pools across the largest permitted replica count,
including rollout surge:

```text
replicas * (app_max + platform_admin_max)
  + concurrent_migrator_max
  + database_reserved
  <= postgres_max_connections
```

`database_reserved` is an explicit deployment value covering administration,
replication, monitoring, failover, and extension roles. It is not silently
derived from the remainder.

Serving requires `WYRD_DATABASE_URL`; `WYRD_PLATFORM_DATABASE_URL` defaults
to it. Operators create both logins; production sets them to a separate
tenant login and the platform login that owns Wyrd's objects. Unsuffixed
`WYRD_DB_*`
settings tune the tenant application pool, `_PLATFORM_ADMIN` the platform pool,
and `_MIGRATOR` the pool of the one-off `wyrd-server migrate` process.

`wyrd-server migrate` runs as the platform login before serving replicas
start; serving replicas never run DDL and refuse
readiness until the migrated schema validates. Runtime tenant work uses `TenantConn` under RLS.
Cross-tenant operator work uses only the narrow `OperatorPool` capabilities
approved by repository architecture.

Transaction-mode PgBouncer requires statement-cache capacity `0` on every pool
that passes through it. Tenant state is transaction-local; session-scoped SQL
state, session advisory locks, and `LISTEN`/`NOTIFY` are not supported on that
path.

## Readiness

A replica reports ready only for roles whose dependencies, security material,
audit path, resource governors, persistent volumes, schema versions, and
recovery state are valid. The gateway routes a request only to a replica ready
for that surface. Liveness does not imply readiness.
