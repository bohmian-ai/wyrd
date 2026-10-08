# TASK-008 round-six invariant review

## Immutable subject and scope

- Base: `1d05642bf2c4d824de1aec27286048ec79b37e25`
- Candidate: `f8d6945041467d52020024d311ce8831a45ff4a1`
- Remediation range: `1d05642bf2c4d824de1aec27286048ec79b37e25..f8d6945041467d52020024d311ce8831a45ff4a1`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review: `changes/active/verified-change-contract/review/TASK-008-r5/`
- Remediation: `TASK-008-CLOSEOUT-R4-audit-handoff-and-owner-shape.md`

This closure review is limited to `FIND-TASK-008-CLOSEOUT-16`,
`FIND-TASK-008-CLOSEOUT-17`, and regressions introduced by the remediation
range. Earlier accepted code was not reopened. The deferred unmodified default
capacity run (`FIND-TASK-008-CLOSEOUT-13`) is not acceptance evidence or a
blocker in this review. The separately tracked intermittent
`verification_runtime::two_bindings_share_one_client_observation` failure is
not counted against this range because the range does not touch its Oracle
scan, live-route selection, or Drift-fold path.

The candidate remained `f8d6945041467d52020024d311ce8831a45ff4a1`
through this review. This checkout has no `.codegraph/` index, so navigation
used Git, `rg`, and direct source inspection.

## Navigation and invariant trace

The audit ownership trace is:

1. `OracleQueryAudit::stage` increments both the existing atomic pending owner
   and `audit_outbox_pending` before the non-blocking enqueue
   (`crates/wyrd/wyrd-server/src/oracle/query_audit.rs:101-117`).
2. `OracleAuditWriter::run` decrements both only after the drained batch has
   either committed through the canonical append or been counted lost
   (`query_audit.rs:155-179`, with commit/loss settlement at `187-214`). Queue
   refusal similarly counts the loss before the temporary pending ownership is
   released.
3. `Deployment::drain` scrapes every live replica before reading PostgreSQL
   (`crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:383-425`).
4. `Backlog::with_replicas` adds every replica's pending gauge to the durable
   audit cell (`crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:159-200`).
5. `Queue::backlog` counts every `vala.audit_staging` row above its tenant's
   `published_seq`, without the old stop-time exclusion
   (`evidence.rs:257-301`). A commit between scrape and durable read can
   therefore be counted twice but cannot disappear from both owners.
6. The unchanged drain judge and report consume that combined `Backlog`; audit
   must be zero before `Backlog::is_empty` can yield a passing drain
   (`evidence.rs:176-180`, `step.rs:419-423`,
   `crates/wyrd/wyrd-testing/src/bin/capacity/report.rs:245-263`).

The replica lifecycle trace is:

1. `Benchmark::clean_up` takes the deployment, shuts down its clients within
   the existing reserve, and only then invokes replica shutdown
   (`crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:581-605`).
2. The live replica collection is now consumed by the owning
   `Deployment::stop_replicas` method (`capacity/step.rs:427-454`).
3. It retains newest-first execution, per-ordinal log paths,
   `spawn_blocking` around synchronous `LocalServer::stop`, join/process error
   conversion, ordinal result alignment, and the prior cancellation behavior.

Sibling consumers remain consistent: test-support inspection and shutdown
continue to read the same atomic pending owner, while the new gauge is only a
metrics projection of that owner. Audit publication, the staging table, its
tenant hash chain, the publisher watermark, and shutdown settlement are not
changed by this range.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-008-CLOSEOUT-16`: replica-set shutdown has a concrete lifecycle owner | The module-level workflow is deleted. `Deployment::stop_replicas` owns and consumes `self.replicas`; `Benchmark::clean_up` preserves client-before-replica ordering (`main.rs:581-605`; `step.rs:427-454`). | Updated `tests::a_slow_replica_stop_leaves_the_runtime_free` constructs the owning `Deployment` and drives `Benchmark::clean_up`, asserting runtime progress, clean reap, log retention, and ordinal result (`main.rs:907-993`). Recorded remediation evidence reports the complete ignored capacity target passed. | PASS |
| Preserve the prior synchronous-stop correction and all adjacent stop behavior | The method keeps `spawn_blocking`, newest-first iteration, `server-<ordinal>.log`, process and join error conversion, reverse-to-ordinal output, and fallback drops on cancellation (`step.rs:427-454`). `LocalServer::stop` and `STOP_GRACE` are unchanged. | Focused owner-path proof above; release-server tests are recorded 2/2. Static comparison shows no change to the single-process owner. | PASS |
| `FIND-TASK-008-CLOSEOUT-17` pre-commit portion: a returned request cannot become invisible while its decision remains process-owned | Pending and the gauge rise before enqueue and fall only after commit or counted loss (`query_audit.rs:101-117,155-179`). Drain sums the gauge from every replica (`evidence.rs:182-200`; `step.rs:406-418`). | Held-chain-head integration test observes two returned Oracle reads as pending and the audit backlog as two (`evidence.rs:420-528`). Supporting arithmetic test passed locally. | PASS |
| `FIND-TASK-008-CLOSEOUT-17` handoff invariant: no zero gap between process ownership and durable ownership | Drain orders replica scrape before durable query (`step.rs:406-418`). The producer releases pending only after commit; PostgreSQL then counts the committed row. This permits over-count during handoff but excludes a false zero. | Held-chain-head test polls the transition and requires audit `>= 2` while pending drains (`evidence.rs:530-543`). | PASS |
| `FIND-TASK-008-CLOSEOUT-17` post-commit portion: rows committed after the captured stop remain backlog until publication | The audit subquery now selects all staging rows with `s.seq > h.published_seq`; it no longer filters `created_at <= stopped` (`evidence.rs:280-299`). | The integration test proves at least one row has `created_at > stopped`, the audit cell stays nonzero, and publication advancing the watermark clears it (`evidence.rs:544-563`). | PASS |
| REQ-171: the existing audit saturation cell cannot pass while either owner still owes work | `Backlog::is_empty` includes the combined audit value and the unchanged exact drain judge gates the recorded result (`evidence.rs:176-180`; `step.rs:419-423`). | `pending_decisions_add_to_staged_audit_rows`, `staged_members_hold_the_scribe_backlog`, and `a_backlog_drains_only_within_the_limit` passed locally. Recorded integration proof covers the real Oracle-to-staging-to-publication handoff. | PASS |
| Preserve non-blocking audit semantics, the canonical staging path, single publisher, tenant watermarks, and counted-loss behavior | The range adds only a projection of the existing pending owner. It does not await audit commit in requests, change `append_audit_batch`, create a queue or ledger, or alter publication (`query_audit.rs:101-117,182-214`). Architecture now names the gauge beside the existing failure counter (`architecture/bifrost-design.md:587-596`). | Source trace through queue refusal, commit failure, shutdown, staging, and publication; no changed persistence or request-wait path. | PASS |
| No remediation-range regression to workload, SLO, report schema/verdict, command deadline, or deferred empirical qualification | The range changes no workload, plan, SLO, report field, verdict rule, CLI option, timeout, task entry point, or benchmark claim. The audit value feeds the pre-existing report cell. | Complete range inspection and `git diff --check`; `FIND-13` remains explicitly deferred and no AC-040/AC-041 qualification is claimed. | PASS |
| No unrelated scope expansion | Production changes are limited to the approved pending metric projection and its architecture sentence; benchmark changes are the required owner move, audit aggregation, and focused proof. | Full remediation-range inspection; no public API, Card/schema, storage-format, authz, tenant, or verification-runtime path changed. | PASS |

## Proposed findings

None. The range closes both scoped prior findings, and the producer-to-sink,
sibling-consumer, lifecycle, failure, cancellation, and report traces found no
regression introduced by the remediation.

## Verification assessment and limits

- Static review covered every file in the immutable remediation range and the
  relevant owners, callers, sibling consumers, failure paths, and report sink.
- The three focused non-environment tests for audit arithmetic, Scribe backlog,
  and the exact drain edge passed locally: 3 passed, 0 failed.
- The Postgres-backed held-commit test could not be rerun in this sandbox
  because access to the configured Docker socket was denied before PostgreSQL
  started. The remediation record reports that focused test passing and also
  records a 20/20 ignored-inclusive capacity target. The source trace supports
  that result, but this reviewer does not claim an independent database rerun.
- The full default `bench:capacity` execution remains deferred as
  `FIND-TASK-008-CLOSEOUT-13` and supplies no empirical capacity result here.
- The known intermittent
  `verification_runtime::two_bindings_share_one_client_observation` failure is
  a separate integrated-branch blocker, not a regression in this range.

## Overall result

**PASS**
