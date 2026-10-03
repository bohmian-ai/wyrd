---
id: TASK-008-CLOSEOUT-R4
kind: remediation
status: ready
spec: changes/active/verified-change-contract/spec.md
spec_revision: 57
original_task: changes/active/verified-change-contract/tasks/task-008-closeout.md
base: f6159606c5c959e8fcc3423574ab0e7e6c86ee13
reviewed_candidate: 0973a03e5a389ea0fb3635c6d9175e25db0a6da0
requirements: [REQ-171]
parent_task: TASK-008-CLOSEOUT
remediates: [FIND-TASK-008-CLOSEOUT-16, FIND-TASK-008-CLOSEOUT-17]
route_to: wyrd-implement
---

# Close the capacity audit handoff and restore lifecycle ownership

## Outcome

Finish TASK-008 closeout by keeping normal replica shutdown on a concrete
lifecycle owner and making the existing capacity audit-backlog cell cover the
complete non-blocking handoff from process-local ownership through durable
publication.

This remediates the cumulative candidate
`f6159606c5c959e8fcc3423574ab0e7e6c86ee13..0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
under approved specification revision 57. It preserves the accepted command
process-group fix, Tokio blocking boundary, report-before-teardown design, and
the caller-deferred default capacity run.

## Issue diagnosis

### FIND-TASK-008-CLOSEOUT-16 — replica-set shutdown is an ownerless workflow

`Benchmark::clean_up` owns the transition from client shutdown to replica
shutdown and reporting, while `Deployment` owns the live replica collection
and `LocalServer::stop` owns one synchronous process termination. The latest
remediation moved the collection operation to module-level async
`stop_replicas` in `capacity/main.rs:649-677`.

That function is not a stateless helper. It consumes live process owners and
the benchmark output destination, selects newest-first execution, derives log
paths, crosses the blocking IO boundary, converts join and process failures,
restores ordinal result order, and defines cancellation partial progress. Its
only production caller is `Benchmark::clean_up`; its other caller is the
focused test added for this workflow. This violates the repository's hard
struct-centered rule for materially changed Rust and splits one cleanup
invariant across an owner and a free orchestration function.

The behavioral correction from prior finding 14 is valid and must remain:
`LocalServer::stop` is synchronous and must continue to execute through
Tokio's blocking facility so runtime timers and sibling tasks progress.

### FIND-TASK-008-CLOSEOUT-17 — audit drain omits the pending-to-staging handoff

Revision-57 REQ-171 requires every audit-outbox backlog caused by a step to
drain within 60 seconds before that step passes. The shared
`OracleQueryAudit` used by Oracle reads, tenant tripwires, and direct
verification intentionally returns control without waiting for its database
commit. `stage` increments the existing process-local `pending` owner before
enqueue; the writer decrements it only after the decision commits to
`vala.audit_staging` or is counted lost.

The load driver joins its requests and then captures the database `stopped`
timestamp. `Deployment::drain` can therefore begin while a serving replica
still owns an uncommitted decision. Its replica scrape currently contributes
only Scribe backlog, while `Queue::backlog` counts only already-committed audit
rows above the publication watermark and with `created_at <= stopped`.

This leaves two reachable blind spots. Before commit, SQL can report zero while
the process-local pending count is nonzero. After commit, a row stamped later
than `stopped` remains excluded from every subsequent poll even while it is
unpublished. The existing commit-failure counter is cumulative and cannot
represent healthy pending work. Server shutdown separately waits on this
pending owner, and the existing held-chain-head journey proves that requests
can complete while it remains nonzero.

The observable consequence is a false PASS: the benchmark can stop its drain
timer while a replica still owns decisions, or while their later committed
rows remain unpublished and invisible to the timestamp-filtered query.

## Intended correction outcome

Replica-set shutdown remains discoverable on an existing lifecycle owner while
preserving every accepted stop, ordering, error, log, cancellation, and Tokio
progress property.

The capacity audit backlog is nonzero for the entire lifecycle of step-owned
audit work: while any serving replica still owns an uncommitted decision and
after handoff while any staged row remains above the publication watermark.
It reaches zero only when both owners are empty, using the existing exact
60-second decision and report cell.

## Decision-complete recommendation

Delete the module-level `stop_replicas` workflow. Keep `Benchmark` as the
client-before-replica cleanup owner, `Deployment` as the replica-collection
owner, and `LocalServer::stop` as the single-process owner. Place the
replica-collection shutdown on the existing owner that already holds that
state, with `Benchmark` supplying its output destination after client
shutdown. Preserve the installed `spawn_blocking` boundary, newest-first stop
execution, per-ordinal log destination, ordinal result order, join and process
error conversion, `STOP_GRACE`, kill/reap behavior, cancellation partial
progress, and report contents. Do not introduce a helper type, trait, replica
manager, process framework, additional concurrency, or async rewrite of
`LocalServer`.

For audit evidence, reuse `OracleQueryAudit::pending` as the sole source of
truth before commit and the existing replica metrics scrape as the transport.
Expose the current pending value as one process-local gauge per serving
replica, and sum that value into the existing capacity audit backlog. Preserve
`vala.audit_staging` and `vala.audit_chain_head.published_seq` as the durable
source after commit, but count every currently unpublished staged audit row
during drain rather than permanently excluding rows committed after the
captured stop timestamp. Combine the replica-local and durable values only at
the existing `Deployment::drain`/`Backlog` decision boundary.

This closes the defect at its two real owners without making requests wait or
creating downstream guards. Preserve non-blocking authorization decisions,
the one canonical staging path and publisher, watermark semantics, exact
60-second edge, existing workload, SLO, and report schema. The cumulative
commit-failure counter remains a loss signal; it is not a substitute for the
pending gauge.

## Constraints and preserved behavior

- Preserve the one `bench:capacity` entry point, four-tenant workload,
  direct/queued five-kind mix, Scribe ingest and Oracle query rates,
  warmup/ramp/knee/sustained/scale-out sequence, SLOs, report cells, verdict,
  and profiling behavior.
- Preserve the one absolute command deadline, process-group TERM-to-KILL
  escalation, later-phase prevention, wrapper teardown opportunity, and all
  current diagnostics.
- Preserve report-before-Postgres-teardown ordering and the unchanged meaning
  of `Report::total_seconds` accepted by the integrator.
- Preserve non-blocking audit semantics, canonical `vala.audit_staging`, the
  single `AuditPublisher`, per-tenant publication watermarks, tenant isolation,
  and counted audit loss behavior.
- Preserve `LocalServer::stop`, its stop grace, clean and forced reaping, log
  retention, and fallback `Drop` behavior.
- Preserve all focused queue, fairness, exact-once, report, process-boundary,
  and language-journey proof.
- Keep `FIND-TASK-008-CLOSEOUT-13` deferred to post-merge integration. This
  remediation does not run or claim the full performance qualification.

## Non-goals

- No public API, CLI option, Card/schema, storage format, production timeout,
  request wait, audit queue, audit ledger, publisher, configuration surface,
  workload, SLO, report column, or capacity claim.
- No general process supervisor, async process framework, new replica owner,
  trait abstraction, or concurrent replica-stop redesign.
- No change to report/teardown sequencing or post-report elapsed accounting.
- No unrelated audit, Gate, Scribe, Oracle, Forge, or verification-runtime
  refactor.
- No execution of the deferred unmodified default benchmark.

## Acceptance criteria

### AC-R4-1 — replica shutdown has a concrete owner

The free async replica-set shutdown workflow is removed. An existing concrete
lifecycle owner exposes the collection shutdown operation, and
`Benchmark::clean_up` invokes it only after client shutdown. Normal stop still
runs off Tokio workers and preserves newest-first execution, ordinal result
alignment, log destinations, errors, grace, kill/reap, cancellation, and
report behavior.

Closes `FIND-TASK-008-CLOSEOUT-16`.

### AC-R4-2 — process-local audit ownership participates in drain

Every serving replica exposes the existing shared audit outbox's current
pending ownership through the repository's metrics surface. The capacity
drain sums those replica-local values into its audit backlog and cannot accept
zero while a request-completed decision remains uncommitted or has not yet
been counted lost.

Closes the pre-commit portion of `FIND-TASK-008-CLOSEOUT-17`.

### AC-R4-3 — durable audit ownership survives the handoff

After a pending decision commits, every staged row above its tenant's
publication watermark continues to hold the same audit backlog nonzero,
including a row whose database timestamp is later than the step's captured
stop time. The backlog reaches zero only after publication advances past all
such rows.

Closes the post-commit portion of `FIND-TASK-008-CLOSEOUT-17`.

### AC-R4-4 — adjacent behavior remains unchanged

The existing exact 60-second edge, Scribe/run/Forge backlog behavior, command
deadline and descendant proofs, replica stop semantics, capacity workload and
report, audit request latency, staging and publication semantics, and focused
correctness journeys retain their behavior. No public or production contract
outside the existing pending metric is added.

## Focused proof and broader verification

Make the existing slow-replica-stop proof exercise the retained inherent owner
path. It must show that a Tokio heartbeat advances while the same replica is
stopped, reaped, logged, and represented in the same ordinal result.

Add focused proof across the real audit handoff. Hold a tenant audit-chain
commit after a public Oracle or direct-verification request returns and prove
the capacity drain remains nonzero from replica-local pending evidence. After
releasing the commit, prove the same audit cell remains nonzero while the
resulting staging row is unpublished, including when its `created_at` is later
than the captured stop time, and reaches zero only after the publication
watermark advances. Retain a focused committed-but-unpublished staging case
and the exact 60-second boundary proof. A metric parser or arithmetic unit test
may support this, but cannot replace the held-commit path.

Run every new specifically named Rust test with its complete repository-pinned
`mise exec -- cargo nextest run --locked` command, explicit package and target,
applicable features or environment wrapper, and exact selector. Then run the
complete `capacity` target, the owning server/audit integration target, the
focused `release_server` selection, `mise run fmt`, `mise run lints`, and
`git diff --check`. Do not substitute the deferred full benchmark for the
focused handoff proof and do not claim empirical AC-040/AC-041 qualification.

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-R4-1 replica shutdown has a concrete owner | Free `stop_replicas` deleted from `capacity/main.rs`; inherent `Deployment::stop_replicas(&mut self, output)` in `capacity/step.rs` keeps `spawn_blocking`, newest-first stop, `server-<ordinal>.log`, join/process error conversion, ordinal order, and cancellation semantics; `Benchmark::clean_up` calls it after client shutdown | `tests::a_slow_replica_stop_leaves_the_runtime_free` now builds a `Deployment` and drives `Benchmark::clean_up`: heartbeat ≥ 10 ticks, `[Ok(≥ 2 s)]`, replica reaped, `server-0.log` kept, no client failure | PASS |
| AC-R4-2 process-local audit ownership participates in drain | `OracleQueryAudit` mirrors its existing `pending` owner into the `audit_outbox_pending` gauge (raised before enqueue, lowered after commit or counted loss); `Backlog::with_replicas` sums it across replica scrapes into `audit`; `Deployment::drain` scrapes before the durable read so the handoff can over-count but never miss | `evidence::tests::the_audit_backlog_holds_from_a_pending_decision_until_its_publication`: chain head held, two public Oracle reads return, drain read = 2 from pending alone. Red check: dropping the pending term fails `left: 0, right: 2`. `evidence::tests::pending_decisions_add_to_staged_audit_rows` supports the arithmetic | PASS |
| AC-R4-3 durable audit ownership survives the handoff | `Queue::backlog` counts every staged row above its tenant's `published_seq`; the `created_at <= stopped` cut is removed | Same held-commit test: after release every drain read during the handoff is ≥ 2, a staged row has `created_at > stopped`, the cell stays 2 until hand-driven `AuditPublisher::publish_tenant` reaches `Idle`, then reads 0. Red check: restoring the cut fails `left: 1, right: 2` | PASS |
| AC-R4-4 adjacent behavior unchanged | No workload, SLO, report column, CLI, timeout, request wait, queue, ledger, or publisher change; one gauge added and named in `architecture/bifrost-design.md` | `step::tests::a_backlog_drains_only_within_the_limit` and `evidence::tests::staged_members_hold_the_scribe_backlog` pass; full `capacity` target 20/20 including deadline and descendant proofs; `test:bifrost:journey:server` (held-chain-head journey); `release_server::tests` 2/2 | PASS |

Commands (all from the worktree root):

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity                      # 15 passed, 5 skipped
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && \
  mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity --run-ignored=only \
  -E 'test(=evidence::tests::the_audit_backlog_holds_from_a_pending_decision_until_its_publication)'"  # 1 passed
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity \
  -E 'test(=evidence::tests::pending_decisions_add_to_staged_audit_rows)'                    # 1 passed
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && \
  mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity --run-ignored=all"  # 20 passed, 0 skipped
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(/^release_server::tests::/)'  # 2 passed
mise run test:bifrost:journey:server
mise run fmt
mise run lints
git diff --check
```

The slow-replica-stop proof ran in the `--run-ignored=all` capacity run
above. The default `bench:capacity` run stays deferred to integration
(FIND-TASK-008-CLOSEOUT-13); no AC-040/AC-041 qualification is claimed.

Non-goals held: no public API, CLI, Card/schema, storage, production timeout,
request wait, audit queue, ledger, publisher, configuration, workload, SLO, or
report column changed. Only `query_audit.rs`, the capacity binary, and one
sentence in `architecture/bifrost-design.md` changed. `Queue::unconnected` is
`#[cfg(test)]` only, so the cleanup proof can build a `Deployment` with no
database. Integration note: the bench reads pending audit work only through
the `audit_outbox_pending` scrape, so the audit-outbox branch only has to move
that gauge's source onto `AuditOutbox`.

### Lane failure diagnosis: `test:bifrost:journey:server`

- **Symptom:** 28/29 passed. `verification_runtime::two_bindings_share_one_client_observation`
  failed with both Custom Drift bindings `inconclusive`. A rerun of that test
  alone under `WYRD_LOG` also failed.
- **Evidence:** a fresh read-only diagnostician reran the same unchanged test
  binary. It failed twice early, then passed 6 times in a row with no rebuild.
  On a pass the published results were `failed`/`passed` as expected. On a
  failure both runs pinned one hot file through Oracle, yet folded an empty
  aggregate (`drift.rs:731-739`, `fold_custom` at `drift.rs:629-646`). Window,
  cron boundaries, event-time stamping, attribution, and SQL were each checked
  and are correct. The failures overlapped another session's concurrent
  capacity-binary run on the host.
- **Cause:** not proven. It is intermittent and confined to event-time-ranged
  Oracle reads right after flush and snapshot refresh. Suspects:
  `EventTimeQueryInterval::retains` (`oracle/pruning.rs:187`, used at
  `oracle/exec.rs:1987`) and `LiveScribeRoute::is_selected_by`
  (`oracle/live.rs:80`). Today nothing logs pruning outcomes or the folded
  aggregate.
- **Fix site:** outside this task's write set, in Oracle event-time pruning and
  live-route selection or the Drift fold. This remediation's diff does not
  touch verification, Drift, Scribe, or the Oracle scan path. Reported as a
  blocker for the integrator rather than patched here.
