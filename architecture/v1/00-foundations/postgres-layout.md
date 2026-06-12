# Postgres Layout

Wyrd uses one PostgreSQL database and one shared `sqlx::PgPool` for the server
control plane. SQL ownership is split by crate:

- `wyrd-sql` owns `platform` and `wyrd`.
- `vala-sql` owns `vala`.
- No `skald` schema exists in this phase.

`wyrd-server` applies migrators in this order:

1. `wyrd_sql::migrate(pool)`
2. `vala_sql::migrate(pool)`

The order lets future `vala.*` migrations reference `wyrd.registry_cards` with
tenant-aware composite foreign keys after Wyrd control-plane tables exist.
`skald_sql::migrate(pool)` is deferred until the Skald storage phase.
