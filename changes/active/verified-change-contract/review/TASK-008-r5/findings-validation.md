# TASK-008 round-five findings validation

## Immutable subject and validation scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
- Cumulative range:
  `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
- Approved authority: `changes/active/verified-change-contract/spec.md`,
  approved revision 57
- Original task:
  `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior reviews: `review/TASK-008-r1/` through `review/TASK-008-r4/`
- Latest remediation:
  `review/TASK-008-r4/TASK-008-CLOSEOUT-R3-process-boundary-and-proof.md`

I read every round-five discovery report and the focused follow-up, inspected
the complete cumulative changed-path set, and independently traced the two
proposed failures through their producers, sibling consumers, owners, and
tests. The candidate remained at the named commit. This checkout has no
`.codegraph/` directory, so source navigation used Git, `rg`, and direct source
inspection.

The caller's fixed review boundaries are preserved:

- `FIND-TASK-008-CLOSEOUT-13`, the full unmodified default benchmark run, is
  **DEFERRED** to integration after the other workstreams merge. It is neither
  a candidate blocker nor evidence for an empirical capacity claim.
- The report is intentionally written before Postgres-wrapper teardown. The
  rejected prior request to include post-report teardown in
  `Report::total_seconds` is excluded and is not revived here.

## Producer-to-consumer and sibling trace

### Replica shutdown ownership

`Benchmark` owns the setup-to-report lifecycle, its output directory, and the
optional `Deployment`. During `Benchmark::clean_up`, it removes the deployment,
shuts down its clients under the lifetime reserve, then passes the deployment's
entire replica collection plus the output path to module-level
`stop_replicas`. That function consumes live `LocalServer` process owners,
selects newest-first order, derives each log path from replica identity and the
output root, crosses Tokio's blocking boundary, awaits each stop, converts join
and process failures, and restores ordinal result order. Cancellation leaves
the active blocking stop running and later replicas to their `Drop` fallback.

This is a stateful, multi-step lifecycle workflow with two existing natural
owners: `Benchmark` owns the cleanup transition and output, while `Deployment`
owns the replica set. `LocalServer::stop` correctly retains one process's
synchronous TERM/grace/kill/reap/log-copy behavior. The only production caller
of `stop_replicas` is `Benchmark::clean_up`; its second caller is the focused
test introduced to exercise the extracted production workflow. The test does
not make the workflow stateless or ownerless.

The hard struct-centered rule in `AGENTS.md` section 5 and
`architecture/agent-rules.md` admits free functions only for deterministic
helpers, narrow conversions, or algorithms with no natural owner. This helper
does IO, consumes lifecycle state, owns ordering and partial progress, and
threads its dependencies through parameters. The standards report's narrower
description of it as a blocking-boundary adapter is therefore incomplete: the
`spawn_blocking` call is a narrow adapter, but the surrounding function is
collection shutdown orchestration.

### Audit decision handoff and drain

Oracle reads, tenant tripwires, and direct verification share one
`OracleQueryAudit`. `OracleQueryAudit::stage` increments its process-local
`pending` count before `try_send`; the writer decrements that count only after
the drained batch has committed to `vala.audit_staging` or has been counted
lost. Requests deliberately do not wait for that commit. Direct verification
and Oracle query operations in the REQ-171 mix can therefore return while a
serving replica still owns an uncommitted decision.

The workload joins every request task before `Deployment::run` captures the
database `stopped` timestamp and enters `Deployment::drain`. The drain scrapes
replica metrics, but combines only Scribe metric families with
`Queue::backlog`. That SQL probe counts audit rows already committed to
`vala.audit_staging`, above the publication watermark, and with
`created_at <= stopped`. It does not observe `OracleQueryAudit::pending`.
Production `/metrics` currently exposes the cumulative audit commit-failure
counter, not the pending owner; the only direct pending inspection is
`test-support` state used by in-process tests. Server shutdown independently
waits on `audit_outbox.shutdown`, confirming that process-local pending work is
real owned state outside committed staging.

The omission spans both sides of one handoff:

1. before commit, SQL can read zero while `pending` is nonzero; and
2. after such a decision commits, its database `created_at` can be later than
   the captured `stopped` timestamp, so the current upper-time predicate can
   exclude the unpublished row from every later poll even after `pending`
   returns to zero.

The existing audit-publication journey makes the first state reachable by
locking a tenant chain head, completing reads, and observing a nonzero pending
count before the lock is released. The same shared producer also receives
direct-verification decisions. Gate's transactional audit path is a sibling
with different ownership and does not close this process-local gap.

`Deployment::drain` is the correct consumer boundary for the final guard: it
already owns the REQ-171 decision that all replica-local and durable backlogs
are empty. The correction must expose the existing process-local owner at its
source and bridge it to the existing staging-above-watermark query; it must not
add a second audit queue, make requests wait, or move audit ownership into the
benchmark.

## Proposal dispositions

| Discovery proposal | Disposition | Validation |
|---|---|---|
| Behavior review's empty proposal set | **CONFIRMED as empty for that lens** | Its caller-to-result trace closes the prior process-group, async-worker, and evidence-command hypotheses, but it did not trace the process-local audit handoff or apply the hard owner rule to the extracted helper. |
| Invariant review's empty proposal set | **CONFIRMED as empty for that lens** | Its prior-finding and process-lifecycle conclusions stand; the later durability trace supplies a distinct reachable invariant gap. |
| Standards review's empty proposal set | **REVISED** by `MNT-R5-001` | `spawn_blocking` is correct, but the whole `stop_replicas` function is not merely an adapter under the explicit struct-centered rule. |
| `MNT-R5-001` | **CONFIRMED** as new `FIND-TASK-008-CLOSEOUT-16` | The module-level async workflow consumes live process owners, output state, ordering, error translation, and cancellation semantics despite having existing natural owners. |
| System review's empty proposal set | **CONFIRMED as empty for its resilience lens** | No additional crash, signal, timeout, or recovery defect remains after the round-four remediation. |
| Capacity review's empty proposal set | **CONFIRMED as empty for its measured-capacity lens** | Workload, report, SLO, and deferred empirical boundaries are unchanged; the audit omission is a durability-evidence handoff, not a new capacity-shape proposal. |
| Process-lifecycle review's empty proposal set | **CONFIRMED as empty for that domain** | Process-group ownership and the Tokio blocking boundary are behaviorally closed. The retained owner-shape violation does not reopen those behaviors. |
| `DUR-R5-001` | **REVISED** and consolidated with `FOLLOWUP-R5-001` as new `FIND-TASK-008-CLOSEOUT-17` | The finding is reachable and required by REQ-171. Its correction must cover both process-local pending work and durable rows committed after the stop timestamp. |
| `FOLLOWUP-R5-001` | **REVISED** into `FIND-TASK-008-CLOSEOUT-17` | It correctly sharpens the shared source and timestamp handoff; it is not a second defect and receives no separate stable ID. |

## Ponytail correction analysis

For `FIND-TASK-008-CLOSEOUT-16`, deletion of the extracted free workflow is
the first and sufficient simplification. Keep the installed Tokio
`spawn_blocking` mechanism and the existing `Benchmark`, `Deployment`, and
`LocalServer` owners. Put collection shutdown on the existing owner of the
replica collection (with `Benchmark` retaining client-before-replica cleanup
and supplying the output destination), or keep the collection loop directly
inside the owning cleanup transition. Do not introduce a replica-manager type,
trait, general process abstraction, extra concurrency, or async rewrite of the
process harness. The focused proof must exercise the chosen owner path rather
than retaining a free workflow solely for test access.

For `FIND-TASK-008-CLOSEOUT-17`, the repository already has both necessary
owners and the installed evidence transport. `OracleQueryAudit::pending` is
the source of truth before commit; each release replica already exposes a
metrics scrape; `vala.audit_staging` plus `vala.audit_chain_head.published_seq`
is the durable source of truth after commit; and `Deployment::drain` already
combines replica and SQL evidence for the exact 60-second decision. Reuse those
mechanisms. Export the existing pending owner as a process-local gauge, sum it
across serving replicas, and count all currently unpublished staged rows during
the drain rather than permanently excluding rows whose commit occurs after the
captured stop timestamp. Preserve the one canonical staging path,
non-blocking requests, publication watermark, exact drain edge, and current
report cell. No new endpoint, queue, ledger, request wait, or public
configuration is justified.

## Final deduplicated finding ledger

### `FIND-TASK-008-CLOSEOUT-16` — CONFIRMED — VIOLATION

- **Discovery source IDs:** `MNT-R5-001`; follow-up uncertainty 1.
- **Violated obligation:** `AGENTS.md` section 5,
  `architecture/agent-rules.md`, and the Rust/maintainer authority require
  stateful multi-step IO orchestration to be an inherent operation on its
  concrete owner; free functions must be genuinely stateless and
  deterministic, narrow conversions, or ownerless algorithms.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:589-604,649-677`;
  owner context at
  `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:38-56`; single-process
  owner at
  `crates/wyrd/wyrd-testing/src/release_server.rs:365-405`.
- **Evidence:** `stop_replicas` consumes the deployment's live process-owner
  collection and the benchmark output dependency, determines stop and report
  order, crosses a blocking IO boundary, translates failures, and defines
  cancellation partial progress. Its only production caller is
  `Benchmark::clean_up`; its only other caller is its focused test. Both
  `Benchmark` and `Deployment` already provide natural, stateful owners.
- **Observable consequence:** the cleanup invariant is split between the
  lifecycle owner and an ownerless module workflow. A maintainer changing
  cleanup must discover and preserve client-before-replica order, newest-first
  process stop, ordinal report alignment, output naming, and cancellation
  behavior across two shapes, and the newly modified Rust remains
  structurally incomplete under a hard repository acceptance rule.
- **Decision-complete correction:** delete the module-level workflow while
  preserving its behavior. Keep `LocalServer::stop` as the owner of one
  synchronous process stop and keep Tokio's installed blocking boundary. Put
  replica-collection shutdown on the existing `Deployment` owner, invoked by
  `Benchmark::clean_up` after client shutdown, or inline that collection stage
  in the owning cleanup transition if no separate owner method is needed.
  Preserve newest-first execution, per-ordinal log destinations, ordinal
  result order, join/process error conversion, `STOP_GRACE`, kill/reap,
  cancellation partial progress, and report contents. Add no helper type,
  trait, manager, process framework, or behavior/configuration change.
- **Focused closure proof:** make
  `a_slow_replica_stop_leaves_the_runtime_free` exercise the retained inherent
  owner path and show that the heartbeat advances while the same replica is
  cleanly stopped, reaped, logged, and reported. Retain the complete capacity
  target and release-server tests.

### `FIND-TASK-008-CLOSEOUT-17` — REVISED — INCORRECT

- **Discovery source IDs:** `DUR-R5-001`, `FOLLOWUP-R5-001`; follow-up
  uncertainty 2.
- **Violated obligation:** Revision-57 REQ-171 requires every audit-outbox
  backlog after load stops to drain within 60 seconds before a step passes.
  Repository audit authority makes the request non-blocking while the shared
  process outbox retains ownership until commit or counted loss.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:159-180,223-264` and
  `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:320-361,383-420`;
  omitted producer at
  `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:48-121,142-199,203-243`;
  direct-verification sibling at
  `crates/wyrd/wyrd-server/src/components/verification/service.rs:500-536`;
  independent lifecycle evidence at
  `crates/wyrd/wyrd-server/src/state.rs:634-641` and
  `crates/wyrd/wyrd-server/src/app/server.rs:876-883`.
- **Evidence:** `stage` increments `pending` before non-blocking enqueue, and
  the writer decrements it only after commit or counted loss. The load driver
  joins requests before capturing `stopped`, so completed step traffic can
  still own pending decisions. `Deployment::drain` reads no pending value and
  accepts a SQL-only zero. Its audit query counts only committed unpublished
  rows with `created_at <= stopped`; a decision committed after that timestamp
  is omitted from every later poll. Existing production metrics expose only a
  cumulative failure counter, while the held-chain-head journey demonstrates
  completed reads with nonzero pending ownership.
- **Observable consequence:** a judged step can record an empty audit backlog,
  stop its drain timer, and pass saturation while one or both replicas still
  own uncommitted decisions. Once those decisions commit, unpublished rows can
  remain invisible because their timestamps are later than `stopped`. The
  report therefore does not establish REQ-171's audit-outbox drain SLO,
  especially on the required two-replica steps.
- **Decision-complete correction:** reuse `OracleQueryAudit`'s existing
  pending owner and the existing replica metrics scrape to expose and sum one
  process-local pending gauge per serving replica. In the same audit backlog,
  retain the publication-watermark test but remove the stop-time exclusion so
  every currently unpublished staged row participates in the handoff. Combine
  the replica-local and durable counts only in the existing
  `Deployment::drain`/`Backlog` owner. Preserve non-blocking request semantics,
  the canonical `vala.audit_staging` write path, `AuditPublisher`, watermark
  semantics, the exact 60-second boundary, workload, report schema, and all
  sibling service behavior. Do not add a benchmark-side queue, second ledger,
  endpoint, request wait, or configuration surface.
- **Focused closure proof:** hold a tenant audit-chain commit after a public
  Oracle or direct-verification request returns and prove the step's audit
  backlog remains nonzero from the serving replica's process-local evidence.
  Release the commit and prove the same cell remains nonzero while the staged
  row is above the publication watermark, including when its `created_at` is
  later than the captured stop time, then reaches zero only after publication.
  Also retain a focused committed-but-unpublished staging case and the exact
  60-second drain-edge proof. A scrape/parser unit test may support but cannot
  replace the held-commit path.

## Prior-finding closure

| Prior finding | Validation result |
|---|---|
| `FIND-TASK-008-CLOSEOUT-1` | **CLOSED.** `bench:capacity` is the one server-capacity entry point. |
| `FIND-TASK-008-CLOSEOUT-2` | **CLOSED for the accepted process-boundary portion.** One absolute deadline now owns every mandatory phase's descendant group with finite escalation and focused task-entry proof. The integrator-rejected post-report elapsed sub-part is excluded and not reopened. |
| `FIND-TASK-008-CLOSEOUT-3` | **CLOSED.** Judge-provider wait remains separate from engine overhead. |
| `FIND-TASK-008-CLOSEOUT-4` | **CLOSED.** The active task and approved specification both identify revision 57. |
| `FIND-TASK-008-CLOSEOUT-5` | **CLOSED for its original correction.** `Benchmark` and `Deployment` remain the lifecycle owners. New finding 16 is limited to the ownerless workflow introduced by the later remediation. |
| `FIND-TASK-008-CLOSEOUT-6` | **CLOSED.** Cooperative cancellation, surviving effects, cleanup ownership, and retry boundaries are documented at their owners. |
| `FIND-TASK-008-CLOSEOUT-7` | **CLOSED.** Step identity is typed. |
| `FIND-TASK-008-CLOSEOUT-8` | **CLOSED.** Scribe's durable staging live members participate in its backlog. New finding 17 concerns the separate process-local audit handoff. |
| `FIND-TASK-008-CLOSEOUT-9` | **CLOSED.** The exact 60-second drain edge is judged correctly. |
| `FIND-TASK-008-CLOSEOUT-10` | **CLOSED.** The report uses one common table and SLI schema. |
| `FIND-TASK-008-CLOSEOUT-11` | **CLOSED.** CPU and peak memory share one measured resource interval. |
| `FIND-TASK-008-CLOSEOUT-12` | **CLOSED.** Duplicate-claim correctness remains test-owned outside the benchmark verdict. |
| `FIND-TASK-008-CLOSEOUT-13` | **DEFERRED** by explicit caller sequencing to post-merge integration. It is non-blocking here and supplies no empirical qualification. |
| `FIND-TASK-008-CLOSEOUT-14` | **CLOSED behaviorally.** Normal synchronous replica stop runs through Tokio's blocking pool. New finding 16 preserves this correction and addresses only its extracted owner shape. |
| `FIND-TASK-008-CLOSEOUT-15` | **CLOSED.** Every current named capacity test carries a complete pinned exact command and recorded result. |

## Verification assessment

- The recorded complete capacity target passed 14 tests with four
  environment/process tests skipped.
- The two actual-task process-group proofs passed, as did the recorded
  compatible-host slow-replica-stop heartbeat proof and stalled-setup proof.
- Round-five reviewers freshly ran the six named focused capacity tests, the
  release-server selection, formatting, and diff check successfully; one
  sandbox rerun of the slow-stop proof could not start its systemd scope and
  did not reach the behavior under test.
- The available process and heartbeat evidence closes prior findings 2 and 14
  within the caller-approved boundary, but no current proof exercises
  replica-local audit pending through `Deployment::drain` or the post-stop
  commit-to-publication handoff.
- The full unmodified `mise run bench:capacity` remains explicitly deferred as
  `FIND-TASK-008-CLOSEOUT-13`. Running it later may qualify capacity, but a
  nondeterministic full run is not focused proof of finding 17 and does not
  close either retained structural/correctness finding.

## Validated result

**FIX_REQUIRED**

The validated ledger contains two bounded findings:
`FIND-TASK-008-CLOSEOUT-16` and `FIND-TASK-008-CLOSEOUT-17`. The first deletes
an ownerless workflow introduced by the latest remediation while preserving
its blocking-boundary fix. The second completes the existing audit-backlog
evidence across the already-approved non-blocking process-to-Postgres handoff.
Neither requires a new product, public API, security, compatibility,
concurrency-semantics, or persistent-data decision.
