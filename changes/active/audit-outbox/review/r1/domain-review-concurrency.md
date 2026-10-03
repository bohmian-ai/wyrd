# Concurrency and lifecycle domain review

## Immutable subject

- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `f451d52be01acba66bae003b8509c95086fc8558`
- Range reviewed: `base..candidate` only
- Approved change: `changes/active/audit-outbox/spec.md`, revision 1
- Tasks: `01-publication-progress.md`, `02-one-outbox.md`, and
  `03-remove-audit-unavailable.md`
- Candidate identity was rechecked before this report and remained unchanged.

## Reviewed boundary

This review traced the concurrency and lifecycle boundary end to end:

- non-blocking enqueue, logical queue capacity, full and closed behavior;
- receiver draining, per-tenant grouping, per-tenant ordering, the fixed writer
  connection bound, and one in-flight commit per tenant;
- completion notification, `settle`, shutdown close/drain, deadline reporting,
  and ownership of the writer task;
- one process-owned outbox construction and its place in server shutdown;
- publication-range freeze/read/settle transactions, competing publishers,
  stale settlement, and deterministic replay across replicas;
- bounded tenant-cycle scheduling and cancellation of publication work.

The approved eventual-consistency window, acknowledgement-as-receipt boundary,
and accepted loss of process-memory work on hard process kill were treated as
intentional and are not findings.

## Authority and source coverage

| Area | Authority and source inspected | Result |
|---|---|---|
| Async ownership and bounded work | `AGENTS.md` §§6, 11; `architecture/agent-rules.md`; `architecture/bifrost-design.md` resource/failure invariants | Covered |
| Audit capture semantics | Direct user-approved repository principle; `AGENTS.md` §2; `architecture/wyrd-design.md` runtime identity; `architecture/bifrost-design.md` read audit and terminal contract | Finding CONC-001 |
| Outbox writer | `crates/vala/vala-sql/src/audit_outbox.rs` in full; callers located across Gate, Oracle, auth, server components, query, gateway, and verification | Covered |
| Hash-chain append | `crates/vala/vala-sql/src/queries/audit_staging.rs`; `pg_audit_outbox.rs`; `pg_audit_staging.rs` | Covered |
| Publication concurrency | migration `20261003000000_audit_publication_progress.sql`; freeze/read/settle SQL; `wyrd-server/src/audit/publication.rs`; publication journeys | Covered |
| Process composition and shutdown | `wyrd-server/src/boot/mod.rs`, `state.rs`, and `app/server.rs`; test-server shutdown/inspection paths | Covered |

The healthy path is otherwise coherent: accepted work is logically bounded by
`pending`; each process has at most one commit in flight per tenant; different
tenants use at most four concurrent commits; same-tenant replicas serialize on
the chain-head row; shutdown closes intake and waits against the process
deadline; publication progress is isolated from the append lock; a frozen
range is reused across competitors; and guarded settlement cannot clear a
newer bound or move the watermark backwards.

## Material proposed findings

### CONC-001 — Transient commit failures are finalized as loss instead of retried

- **Classification:** INCORRECT / repository-authority conflict
- **Violated obligation:** The user-approved repository principle says derived
  work such as audit uses batched server-owned outboxes that retry rather than
  drop on dependency slowness. The non-blocking request boundary does not make
  a transient database failure a terminal event outcome.
- **Exact locations:**
  - `crates/vala/vala-sql/src/audit_outbox.rs:292-309`
  - `crates/vala/vala-sql/src/audit_outbox.rs:271-288`
  - corroborating asserted-loss journey:
    `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:871-877`
- **Evidence:** `commit_tenant` collapses every connection-acquire, append, and
  transaction-commit error into logging/counting each event and then returns
  `()`. `AuditOutboxWriter::settle` consequently removes the in-flight tenant
  entry and decrements `pending` for the whole batch regardless of whether the
  transaction committed. No failed batch is restored to the tenant queue and
  no later attempt can recover it. This path is reachable whenever Postgres is
  temporarily unavailable, a connection acquisition fails, or a transaction
  fails during a dependency interruption; it is not limited to a malformed
  event or hard process kill.
- **Observable consequence:** A short Postgres interruption permanently loses
  every accepted authorization decision in each affected batch. `settle` and
  graceful shutdown can report zero pending after that loss, so recovery of the
  dependency cannot replay the decisions even though the process and its
  outbox remain alive.
- **Required testable correction:** Preserve a failed tenant batch as pending
  work and retry it with bounded backoff while the outbox is alive, without
  letting a later batch for that tenant overtake it. Keep one in-flight commit
  per tenant, the global accepted-event bound, cross-tenant concurrency, and
  the non-blocking `stage` contract. Decrement `pending` only after a committed
  batch or an explicitly authorized terminal-loss boundary. Shutdown must keep
  retrying queued work within its existing deadline and report any remainder.
  Replace the permanent-loss proof with a transient dependency-failure proof
  that restores the dependency and observes the original batch commit once,
  in order, while requests remain successful throughout.
- **Authority note:** Revision-1 `REQ-003` currently says a failed commit loses
  the event, while the later explicit user-approved repository principle says
  dependency slowness retries instead of dropping. The higher-priority user
  authority is unambiguous, but the orchestrator should reconcile this
  contradiction in its final verdict/artifact path rather than silently
  treating the task text as current authority.

## Verification limits

- I inspected the cumulative source diff and the relevant production owners,
  callers, SQL, and tests. I did not rerun the repository's long Postgres,
  server-journey, capacity, or aggregate lanes; their recorded green results
  establish the current drop-on-failure behavior, not retry behavior.
- Existing outbox integration coverage proves healthy two-replica chain
  serialization, one held tenant versus one healthy tenant, and healthy
  shutdown drain. It has no transient commit-failure recovery scenario.
- Existing publication tests cover progress-row contention, frozen-bound reuse,
  stale settlement, and crash/replay deduplication. I found no material defect
  in those changed concurrency paths.
- Queue-full loss, bounded eventual visibility, and deadline-bounded shutdown
  remainder were not reported as defects because they are explicit approved
  boundaries. CONC-001 is narrower: it concerns accepted work discarded while
  the process remains alive solely because its dependency was temporarily
  slow or unavailable.

## Overall result

**FAIL**

The publication concurrency and lifecycle changes are otherwise supported by
the reviewed source, but the outbox finalizes transient dependency failures as
loss, contrary to the governing user-approved retry principle.
