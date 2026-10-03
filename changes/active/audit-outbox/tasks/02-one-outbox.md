---
id: AUDIT-OUTBOX-T02
title: One process-wide batched audit outbox on every surface
kind: implementation
status: ready
spec: SPEC-audit-outbox
spec_revision: 1
depends_on: [AUDIT-OUTBOX-T01]
requirements: [REQ-001, REQ-002, REQ-003, REQ-005, REQ-007, INV-001, INV-002, INV-003, INV-004]
acceptance: [AC-001, AC-002, AC-003, AC-007]
---

# One process-wide batched audit outbox on every surface

## Outcome and Value

Every audited surface (Gate writes, `start_run`, Cards, operators, principals,
admin, platform, storage, OTLP, query, Oracle, peer authority, gateway, Bifrost
catalog registration, and `wyrd-auth` grants) stages its decision on the one
process outbox with a non-blocking enqueue. No request waits on or fails
because of an audit commit. The writer commits tenants concurrently on a
bounded number of connections, and shutdown drains the queue to a deadline.

## Owners, Scope, Consumers, and Prohibited Changes

- Owner of the outbox type: the crate that owns the canonical staging append
  (`vala-sql`), so `wyrd-auth`, Gate/Oracle composition, and `wyrd-server`
  share one concrete type without `wyrd-auth` depending on `wyrd-server`. The
  server constructs exactly one instance per process (`AppState::audit_outbox`)
  and hands it to Oracle, Gate, gateway, verification, and auth callers.
- The staging append stops being a production entry point outside the outbox
  (AC-001), proven by visibility or a test.
- Gate's audit trait becomes non-blocking (no `Result`), like Oracle's.
- Prohibited: changing event content, hash chaining, or the publisher.

## Approach

1. Generalize `OracleQueryAudit` into the shared outbox: concurrent bounded
   tenant commits, one failure counter labelled by surface, shutdown drain.
2. Construct one instance at boot; route Oracle, Gate, verification, gateway
   invocation (drop the per-event task), and every `audit::*` caller to it.
3. Convert `wyrd-auth` appenders and the Bifrost catalog registration append
   to staging after the decision; platform authorization stages and still
   returns its operator transaction, without an audit row in it.
4. `start_run`: stage the decision; never hold the chain head across enqueue.

## Ordered Implementation Scenarios

### Scenario 1 — Gate write and run start succeed with audit failing

**Behavior.** With inserts into `vala.audit_staging` revoked, a Gate write and a
`start_run` succeed and `audit_outbox_commit_failures_total` increments (AC-002,
REQ-003).

**RED.** A server journey revoking the staging insert, then writing and
starting a run; today both are refused with audit-unavailable.

**GREEN.** Route Gate and verification through the outbox.

**REFACTOR.** Delete `PostgresGateAudit` and the synchronous helpers.

### Scenario 2 — Concurrent same-tenant decisions from two outboxes stay gap-free

**Behavior.** Two outbox instances (two replicas) staging for one tenant
commit a gap-free, ordered chain (AC-003, INV-002).

**RED.** A Postgres integration test with two outboxes; it does not compile
until the outbox exists in its shared owner.

**GREEN.** Shared outbox with concurrent tenant commits.

**REFACTOR.** Keep one writer type.

### Scenario 3 — Shutdown drains queued events

**Behavior.** Events queued before shutdown commit within the deadline (AC-007).

**RED.** The same integration target stages events and calls shutdown; it
does not compile until the shared outbox exists.

**GREEN.** Shutdown closes the queue and waits for the writer to the deadline.

**REFACTOR.** None beyond the shared writer.

## Acceptance Criteria

- No production code outside the outbox writer calls the staging append.
- The surface families named in AC-002 succeed with audit failing (isolated
  tests where an end-to-end journey is not the owning tier).
- Two outboxes for one tenant produce a gap-free ordered chain.

## Expected Write Set and Consumer Closure

`crates/vala/vala-sql`,
`crates/wyrd/wyrd-server/src/{audit,oracle,bifrost,components,query,http,auth,boot,state.rs,app}`,
`crates/wyrd/wyrd-auth/src`, `crates/vala/vala-bifrost-redux/src/{gate,oracle,catalog}`,
`crates/wyrd/wyrd-testing`.

## Verification and Evidence

`mise run fmt`, `mise run lints`, `mise run test:wyrd`,
`mise run test:principals:integration`, `mise run test:bifrost:journey:server`,
`mise run test:bifrost:integration:redux`, `mise run test:bifrost:integration:sql`.

## Material Stop Conditions

A surface whose correctness requires the audit row to be atomic with its effect.

## Authority Links

`changes/active/audit-outbox/spec.md`, `AGENTS.md` §2-§6.
