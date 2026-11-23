# Cleanup health through existing Scribe admission

Status: finalized — maintainer approved on 2026-11-20. This replaces the
previously selected outstanding-receipt quota with cleanup health through
existing Scribe admission. One-hour/10,000 retry history, recovery policy,
durable checkpoint, CPU design and two-task delivery remain locked.

## Purpose and separation

Use existing workload, memory, in-flight and unstaged-backlog admission for
ingestion pressure. Add a cleanup-health check to that same admission owner to
prevent indefinitely accepting more receipts when their cleanup is failing or
falling behind. Do not add a per-node receipt quota or reinterpret the per-table
10,000 retry-history setting as one.

The existing checks do not measure PostgreSQL cleanup debt: a completed/staged
write can release resources while its receipt remains. Cleanup-health behavior
is therefore new work, not an existing safeguard we can claim already covers
that debt. Do not reuse memory-accounting poison or process-wide readiness as
its signal; reads and other workloads must remain usable.

## Connected mechanism

1. The background Scribe cleaner reconstructs its scope from existing durable
   receipt ownership and custody checkpoints, including previous epochs of this
   stable node. Restart begins cleanup-unverified, rather than resetting history.
   Finish the initial due-work sweep before opening fresh write admission.
2. Each scheduled sweep establishes a finite scope and a policy cutoff from
   PostgreSQL time. It must process all receipts already eligible at that cutoff
   within its ownership scope, using bounded SQL batches. Eligibility remains
   outside the agreed age/count history AND past independent recovery handoff.
   Recheck the authoritative deletion predicate in SQL. New receipts arriving
   after the sweep boundary belong to subsequent sweeps. No per-batch job ledger.
3. A successful DELETE batch is not a successful sweep. Publish healthy progress
   only after every due portion of the captured scope has been reconciled and
   confirmed deletions committed. Ambiguous SQL outcomes require reconciliation.
   A sweep with no eligible receipts is a successful sweep; recovery-pinned
   receipts do not constitute cleanup failure merely because they are old.
4. The cleaner has a bounded sweep/progress deadline tied to its periodic
   scheduling cadence. Admission checks the freshness of completed sweep coverage
   locally. Completion freshness is anchored to the sweep boundary, not reset
   merely because an arbitrarily old sweep finally finishes. Thus an alive
   worker deleting a few rows cannot renew health while increasingly falling
   behind. Exact cadence/deadline and private representation are implementation
   choices, not new receipt-expiration settings or public tuning options.
5. Failed or ambiguous cleanup makes new admission retryable-busy. Stale coverage
   also makes it busy even if the cleaner hung or exited before reporting a
   failure. Already admitted work may complete through the unchanged durability
   boundary; no accepted write is revoked. The existing in-flight bound limits
   that work. The request path does no cleanup SQL and does not wait for a sweep.
6. Keep the cleaner, staging and recovery running under their existing resource
   owners while fresh writes are refused. The cleaner catches up to a current
   sweep boundary. Once that sweep completes within its progress deadline,
   admission resumes automatically, still subject to normal resource pressure.

Coalesced custody/count/age wake-ups request work; they are neither deletion
proof nor health proof. Missed wake-ups are recovered by scheduled durable-state
sweeps. If the writer produces cleanup work faster than the cleaner can drain
it, admission eventually pauses when completed coverage becomes stale, giving
the cleaner an opportunity to catch up. Do not mark health on heartbeat alone.

## Limits and tradeoff

This prevents indefinite continued acceptance during failed or inadequate
cleanup. It does not promise an instantaneous hard cap on PostgreSQL row count
or physical disk bytes. Rows can exceed the recent-history limit while pinned
for recovery or awaiting a sweep; PostgreSQL vacuum also affects physical reuse.
Do not present this amendment as the same guarantee as a fixed outstanding-row
quota. A strict independent row ceiling would still require a quota or stronger
synchronous admission coupling.

The cost is retryable refusal when metadata maintenance cannot keep up, including
cleanup failure while insert SQL itself remains available. Healthy writes add
only a local health/freshness check to existing admission, not a query, durable
write or cleanup wait. Queries, existing row visibility and unrelated workloads
do not inherit the write-admission refusal.

Temporary receipt/recovery coupling remains until the existing staged handoff.
This amendment changes pressure control, not the acceptance protocol or proof.
Removing that coupling entirely would be a separate architectural change.

## Proof within the locked fence scenario

Keep the existing three fence scenarios. In the background-cleanup scenario,
replace the superseded quota-specific cases with: SQL error/ambiguity,
hung/exited cleaner, successful small batches without full-sweep progress,
restart with old debt, entirely recovery-pinned scope, missed wakes, and catch-up
that reopens admission. Assert existing admitted writes finish, accepted data and
reads remain usable, and ordinary workload/unstaged pressure still applies.
Retain the agreed per-scenario logs/read-c64 checkpoints and final qualification.

No implementation, tests or fresh benchmarks have been performed for this
design; there is no claim that existing code already implements its health
signal or sweep coverage.
