# wyrd-sql

`wyrd-sql` is Wyrd's durable Postgres layer. It owns the `platform` and `wyrd`
schemas, `TenantConn`, `OperatorPool`, and the `WYRD_SQL_*` error family;
`vala-sql` owns `vala` and builds on this crate. The design these rules
protect is
[Postgres tenant binding](../../../architecture/wyrd-security-posture.md#postgres-tenant-binding).

## Connections

- ALWAYS take `&mut TenantConn<'_>` for tenant-scoped work and `&OperatorPool`
  for cross-tenant work. These are the only connection types allowed in
  function signatures and struct fields.
- NEVER accept `&sqlx::PgPool`, `sqlx::Pool<Postgres>`, a bare `PgConnection`,
  or a caller-supplied `sqlx::Transaction<'_, Postgres>`.
- NEVER build a pool outside the boundary constructors:
  `WyrdPostgres::connect_from_dsns` and `ValaPostgres::connect_from_dsns` in
  production, `from_pools` in tests. `from_pools` compiles only under
  `cfg(test)` or `test-support`, and `check:deps` proves the shipped server
  never enables `test-support`. `PgPool::connect_lazy` is banned in domain
  crates.
- Production query, auth, and boot code reaches the database through
  `WyrdPostgres::tenant_conn`, `WyrdPostgres::resolve_tenant_slug`, or an
  `OperatorPool`. `mise run check:tenant-isolation` refuses a raw pool or
  `app_pool()` borrow there.

## Transactions

- Every tenant-scoped logical operation acquires one `TenantConn`, and the
  caller owns its transaction, commit, and rollback.
- A function that takes `&mut TenantConn<'_>` NEVER calls `commit()` or
  `rollback()`, opens a nested transaction, or issues raw transaction control.
  This lets a caller chain several writes, relationships, and audit-adjacent
  rows into one atomic transaction.
- Cross-crate work does not extend a transaction by importing another crate's
  private query modules. A cross-owner durable effect uses its declared
  committed handoff and an idempotent consumer.
- Cross-tenant work uses a named `OperatorPool` capability that validates the
  target tenant, lease generation, fence, and operation identity before it
  mutates. NEVER widen a tenant query instead.

## Queries

- NEVER add a manual `data_tenant_id` predicate on a `TenantConn` path. RLS
  already enforces it, and a hand-written filter drifts.
- Bind tenant identity as a parameter. Only `TenantConn`'s begin statement
  inlines it.
- PostgreSQL owns coordination timestamps: bind a duration and derive the
  deadline in SQL with `statement_timestamp()`. Domain event times and
  caller-supplied absolute dates stay producer-owned.
- `SECURITY DEFINER` functions are narrow, reviewed bridges with a pinned
  `search_path`. NEVER use one as a general RLS bypass.
- Map columns by name, never by position.
- Migrations are immutable, ordered, and checksum-verified. NEVER edit an
  applied migration; add a new one.

## Imports across tiers

- Code outside this crate imports through its tier's re-exports:
  `use vala_sql::{OperatorPool, SqlError, TenantConn};`, never
  `use wyrd_sql::TenantConn;` from Vala or Bifrost code.

## Errors

`SqlError` is the single `WYRD_SQL_*` family for every server-tier SQL failure;
`vala-sql` re-exports it. Map constraint failures by SQLSTATE, never by message
text: `23505` unique, `23503` foreign key, and `23514` check violations are
`409`. SQLSTATE `42501` maps to `WYRD_SQL_403_RLS_DENIED` only when the message
names row-level security, because Postgres reports ordinary permission denials
with the same state.

## Tests

SQL proof runs through the crate's `mise` task, which provisions
repository-managed Postgres with separate platform and tenant logins. Tests
needing Postgres live in `mod pg_tests` or a `pg_*` file and never silently
skip because an environment variable is absent.
