# Deployment

Wyrd SQL deployments size connection pools per server stack. The default pool
settings target a small self-hosted or low-replica SaaS deployment that connects
directly to Postgres.

The steady-state runtime budget is:

```text
pods * (app_max + platform_admin_max) + migrator_max <= pg.max_connections - reserved
```

Use `reserved = 10` as the baseline for Postgres administration, replication,
and extension roles. The default platform-admin pool max is `2`, and the
migrator max is `2` but exists only during boot.

For a tenant stack on Postgres with `max_connections = 100`:

| Pods | Formula result | Deployment guidance |
|---:|---:|---|
| 1 | `(100 - 10 - 2) / 1 - 2 = 86` | Default `WYRD_DB_MAX_CONNECTIONS=32` has headroom. |
| 2 | `(100 - 10 - 2) / 2 - 2 = 42` | Default `32` still fits. |
| 4 | `(100 - 10 - 2) / 4 - 2 = 20` | Set `WYRD_DB_MAX_CONNECTIONS=20`. |
| 8 | `(100 - 10 - 2) / 8 - 2 = 9` | Introduce PgBouncer or lower concurrency expectations. |

SaaS-per-tenant deployments run one Wyrd stack per tenant. Apply the same
formula independently to each tenant's stack and database budget. Pool config
does not carry a tenant dimension because each tenant stack owns its own
runtime pool.

When a deployment uses PgBouncer in transaction pooling mode, set
`WYRD_DB_STATEMENT_CACHE_CAPACITY=0` for the runtime pool and equivalent
suffixed variables for any pool routed through the bouncer. Keep tenant binding
transaction-scoped; do not introduce session-scoped SQL state in request paths.
