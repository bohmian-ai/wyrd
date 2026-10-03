---
id: AUDIT-OUTBOX-T01
title: Move audit publication progress off the chain-head row
kind: implementation
status: ready
spec: SPEC-audit-outbox
spec_revision: 1
depends_on: []
requirements: [REQ-006, INV-002, INV-003]
acceptance: [AC-004]
---

# Move audit publication progress off the chain-head row

## Outcome and Value

`AuditPublisher` freezes, publishes, and settles a tenant's staged range while
appenders hold that tenant's `vala.audit_chain_head` row. Publication progress
(`published_seq`, `publishing_seq_hi`) lives in tenant-scoped state appenders
never lock, so publication proceeds during sustained appends (REQ-006, AC-004).

## Owners, Scope, Consumers, and Prohibited Changes

- Owner: `crates/vala/vala-sql` (migration, `queries/audit_staging.rs`).
- Consumers: `crates/wyrd/wyrd-server/src/audit/publication.rs`, test helpers
  in `crates/wyrd/wyrd-testing/src/server.rs`, Postgres tests that read the
  watermark.
- The new state keeps RLS tenant isolation and the existing grant pattern.
- Freeze still reuses an existing in-flight bound verbatim; settle still
  advances monotonically and garbage-collects only through the watermark.
- Prohibited: changing the hash chain, event content, retained history, or the
  single publisher. Migration prefix `20261003000000`.

## Approach

1. Add a migration creating per-tenant publication state, backfilling it from
   the chain head, and dropping the progress columns and their checks from the
   chain head.
2. Rewrite freeze and settle against the new state; publishers serialize on
   that state, never on the chain head.
3. Update test helpers and tests that read the watermark.

## Ordered Implementation Scenarios

### Scenario 1 — Freeze proceeds while an appender holds the chain head

**Behavior.** With a transaction holding the tenant's chain-head lock, a
publisher freezes a range (AC-004, REQ-006).

**RED.** A focused vala-sql Postgres test opens an append transaction that
holds the chain head, then freezes from a second connection; it fails today
with `55P03`.
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p vala-sql --test pg_audit_staging'`

**GREEN.** Migration plus freeze/settle on the new state.

**REFACTOR.** Remove dead NOWAIT/chain-head documentation.

## Acceptance Criteria

- Freeze never locks `vala.audit_chain_head`.
- Two publishers still derive the same frozen bound; a restart reuses it.
- Staged rows are deleted only through the watermark.

## Expected Write Set and Consumer Closure

`crates/vala/vala-sql/migrations/20261003000000_*.sql`,
`crates/vala/vala-sql/src/queries/audit_staging.rs`,
`crates/vala/vala-sql/tests/pg_audit_staging.rs`,
`crates/wyrd/wyrd-testing/src/server.rs`,
`crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs`.

## Verification and Evidence

- `mise run test:bifrost:integration:sql`
- `mise run test:bifrost:journey:server`
- `mise run fmt`, `mise run lints`

## Material Stop Conditions

A publisher correctness argument that requires appender-side locking of the
new state.

## Authority Links

`changes/active/audit-outbox/spec.md`, `AGENTS.md` §2,
`architecture/bifrost-design.md`.

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Freeze never locks `vala.audit_chain_head` (AC-004, REQ-006) | `crates/vala/vala-sql/migrations/20261003000000_audit_publication_progress.sql` (new `vala.audit_publication`, backfill, drop chain-head progress columns); `freeze_publication_range`/`settle_publication` in `crates/vala/vala-sql/src/queries/audit_staging.rs` | `pg_audit_staging::...::publication_proceeds_while_an_append_holds_the_chain_head` | PASS |
| Competing publishers derive one bound; restart reuses it | freeze locks only the progress row `FOR UPDATE NOWAIT`, reuses `publishing_seq_hi` | `held_progress_row_fails_immediately_and_retries_unchanged`, `frozen_range_survives_tail_growth_competition_and_stale_settlement`, journey `frozen_audit_range_replays_once_while_its_tail_waits` | PASS |
| Staged rows deleted only through the watermark | `settle_publication` upserts progress then deletes `seq <= published_seq` | `settlement_is_tenant_scoped`, `settled_tenant_drains_to_zero_and_owes_nothing`, journey `a_stalled_tenant_does_not_block_another_tenants_history` (now fences the progress row) | PASS |

Commands (all green):

- `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p vala-sql --test pg_audit_staging --test pg_migration --test pg_schema_usage'` — 18/18
- `mise run test:bifrost:journey:server` — 29/29
- `mise run fmt`, `mise run lints` (lints first failed on pre-existing
  `clippy::cast_*` errors in `sdks/wyrd-sdk-rust/tests/observe_run.rs`; fixed at
  the source by typing the burst constants `u16` and using lossless `From`
  conversions), `git diff --check`

RED note: the old freeze locked the chain head `FOR UPDATE NOWAIT`, so the new
`publication_proceeds_while_an_append_holds_the_chain_head` would have failed
with `55P03` (the previous `held_chain_head_fails_immediately...` test asserted
exactly that). Non-goals untouched: hash chain, event content, retained
history, single publisher.
