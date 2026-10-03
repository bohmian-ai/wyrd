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
