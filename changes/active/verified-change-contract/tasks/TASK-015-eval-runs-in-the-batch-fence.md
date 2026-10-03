---
id: TASK-015
kind: implementation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 60
requirements: [REQ-077, REQ-108, AC-014]
depends_on: []
---

# Create Eval runs through a batched run-request outbox

## Outcome and Value

Scribe acknowledges an Eval observation batch on its own durable boundary and
the client never waits for run creation. After that acknowledgement the
server places one run request per committed record in an in-process outbox.
A background flusher writes queued requests in one multi-row insert per
tenant, one `verifier_runs` row per matching active `observations_ready`
binding, keyed by tenant, binding, and record, so a repeated request inserts
nothing. The outbox has no count limit and never drops a request because
Postgres is slow or down: a failed flush keeps its batch and retries with
backoff. Graceful shutdown flushes it; a hard kill loses only unflushed
requests, and every loss the process can observe is counted and logged
(REQ-077, AC-014). Bindings whose owner is not runtime-active create no run
(REQ-108).

(The file name is historical; revision 59's batch-fence design was replaced
by revision 60's outbox.)

## Owners, Scope, Consumers, and Prohibited Changes

- `wyrd-sql` `queries/verifier_runs.rs` owns the frame-free batch insert: one
  lock statement over the batch's subjects' bindings, per-binding resolution
  and activity, then one multi-row `INSERT ... SELECT FROM unnest(...)`
  with ordinals assigned in SQL and `ON CONFLICT DO NOTHING`.
- `wyrd-server` `verification/observations.rs` owns the outbox: unbounded
  queue, pending count, one writer task, per-tenant grouping, retained failed
  batches with exponential backoff, and `shutdown(deadline)` reporting
  unflushed requests. `app/server.rs` drains it at shutdown.
- Prohibited: a count limit, a drop on full or on failure, a Scribe or
  `vala-sql` write to `verifier_runs`, a cross-crate transaction, a client wait
  on run creation.

## Ordered Implementation Scenarios

### Scenario 1 — One insert per tenant, idempotent

**Behavior.** A batch of records for several subjects and bindings inserts one
run per (active binding, record) in one insert statement; resubmitting the
same batch inserts nothing and consumes no ordinal.

**RED.** `pg_verifier_runs` test.
`scripts/postgres/with-test-postgres.sh mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_verifier_runs -E 'test(=observation_batches_insert_once_per_binding_and_record)'`

**GREEN.** `VerifierRunQueue::enqueue_observation_batch`.

**REFACTOR.** Delete the per-row `enqueue_observations` path.

### Scenario 2 — Outbox retains, retries, and flushes at shutdown

**Behavior.** With Postgres unavailable the outbox keeps every request and
retries; when it returns, exactly one run per matching binding exists;
graceful shutdown flushes queued requests; a request still unflushed at the
deadline is counted and logged.

**RED.** Runtime test in `pg_verification_runtime.rs` forcing a flush outage
(closed pool fault) and then recovery, and a shutdown flush.

**GREEN.** Outbox owner and writer.

**REFACTOR.** Remove `PENDING_LIMIT`, the semaphore, and the per-frame task.

## Acceptance Criteria

AC-014's outbox bullets pass; the existing continuous Eval journey stays
green.

## Verification and Evidence

Exact focused commands above; `mise run fmt`, `mise run lints`,
`mise run test:sql`, `mise run test:bifrost:integration:server`.

## Authority Links

- [Approved spec revision 60](../spec.md): REQ-077, REQ-108, AC-014.
- `crates/wyrd/wyrd-server/src/oracle/query_audit.rs` (outbox shape).
