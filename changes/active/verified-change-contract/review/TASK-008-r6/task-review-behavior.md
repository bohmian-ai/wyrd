# TASK-008 closure behavior review, round 6

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `1d05642bf2c4d824de1aec27286048ec79b37e25`
- Candidate: `f8d6945041467d52020024d311ce8831a45ff4a1`
- Reviewed range: `1d05642bf2c4d824de1aec27286048ec79b37e25..f8d6945041467d52020024d311ce8831a45ff4a1`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review: `changes/active/verified-change-contract/review/TASK-008-r5/`
- Remediation task: `TASK-008-CLOSEOUT-R4-audit-handoff-and-owner-shape.md`

This is the caller-directed closure review. I inspected only whether
`FIND-TASK-008-CLOSEOUT-16` and `FIND-TASK-008-CLOSEOUT-17` are closed by the
named range and whether that range introduces a regression. Earlier passed
code was not reopened. `FIND-TASK-008-CLOSEOUT-13` remains deferred to
integration and is not acceptance evidence or a blocker here.

The checkout has no `.codegraph/` directory, so the review used the immutable
Git diff and direct source/caller inspection.

## Caller-to-result trace

### Replica shutdown ownership

`Benchmark::clean_up` still owns the cleanup transition: it removes the
`Deployment`, shuts down every client within the lifetime reserve, and only
then calls `Deployment::stop_replicas`. `Deployment` now owns the collection
workflow as an inherent method. It consumes its replica vector newest first,
derives the same per-ordinal log paths, runs each synchronous
`LocalServer::stop` through `tokio::task::spawn_blocking`, translates join and
process failures, restores ordinal result order, and leaves the deployment
without live replica owners. The slow-stop proof now reaches this method
through `Benchmark::clean_up`, rather than retaining a free workflow for test
access.

This closes the owner-shape violation without changing the accepted shutdown
behavior or the prior Tokio-worker correction.

### Audit ownership handoff

Every decision entering `OracleQueryAudit::stage` increments both the existing
atomic pending owner and the process-local `audit_outbox_pending` gauge before
the non-blocking enqueue. Queue refusal removes both immediately and records
the existing loss signal. The writer removes both only after the drained batch
has either committed to `vala.audit_staging` or been counted lost. The gauge
therefore covers the same interval as the pre-existing shutdown owner and does
not change request latency, queueing, commit, or failure semantics.

`Deployment::drain` scrapes all replicas before its durable query. It then
combines the summed pending gauges with the staged rows above each tenant's
publication watermark. `Queue::backlog` no longer excludes rows whose
`created_at` is later than the step's captured stop time. The ordering closes
both sides of the handoff: if a decision commits after its replica was scraped,
the later SQL read sees it; if it had not committed, the scrape sees it. A
handoff during the read may temporarily double-count but cannot produce a
false zero, and the next poll converges after publication.

The focused held-chain-head proof exercises a real public Oracle request,
process-local metrics, the canonical staging append, the publication
watermark, and the same scrape-then-query composition used by the drain. It
holds two returned requests pending, observes the pending count, releases the
commit, proves at least one staging timestamp is later than the captured stop,
and proves the cell becomes zero only after publication.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-008-CLOSEOUT-16`: replica-set shutdown must be an operation on an existing lifecycle owner | `capacity/step.rs:427-454` defines `Deployment::stop_replicas`; `capacity/main.rs:581-604` keeps client-before-replica ordering and calls the inherent method | `capacity/main.rs:907-1000` drives the owner path and checks Tokio progress, clean stop, reaping, log retention, and ordinal result | PASS |
| Preserve the accepted blocking boundary and shutdown semantics while closing finding 16 | `capacity/step.rs:443-453` retains newest-first sequential stop, `spawn_blocking`, error conversion, ordinal reversal, and emptied ownership | The slow-stop proof targets the same `Benchmark::clean_up` to `Deployment::stop_replicas` path | PASS |
| `FIND-TASK-008-CLOSEOUT-17`, pre-commit: returned decisions must hold the audit backlog nonzero while a replica still owns them | `oracle/query_audit.rs:53-55,101-117,155-179` mirrors the existing pending owner into a gauge across enqueue, commit, and counted-loss transitions; `capacity/evidence.rs:183-201` sums it across replicas | `capacity/evidence.rs:420-528` holds the chain head after public reads return and observes both pending decisions in the audit cell | PASS |
| `FIND-TASK-008-CLOSEOUT-17`, post-commit: late-committing staged rows must remain backlog until publication | `capacity/evidence.rs:249-295` counts every staged row above `published_seq`, without the former `created_at <= stopped` cut | `capacity/evidence.rs:530-563` proves a row stamped after `stopped` remains counted and clears only after the watermark advances | PASS |
| The pending-to-staging handoff must not permit a false zero | `capacity/step.rs:395-424` scrapes replicas before the SQL read and combines them through `Backlog::with_replicas` | Held-chain-head proof covers pending, transition, durable unpublished, and published states; `capacity/evidence.rs:568-588` separately pins replica aggregation arithmetic | PASS |
| The range must not regress audit request semantics or create another audit path | Gauge updates are observational around the existing atomic owner; `append_audit_batch`, the bounded channel, non-blocking `try_send`, loss counter, staging table, and publisher remain the same | Architecture text at `architecture/bifrost-design.md:586-598` documents the gauge without changing the one-path audit contract | PASS |
| The range must not regress benchmark workload, SLOs, report, deadline, or teardown boundary | Diff changes only audit-backlog evidence, replica-stop ownership/proof, matching architecture text, and remediation evidence; no workload, SLO, report, CLI, timeout, or public contract changes | Existing capacity target and focused evidence recorded in the remediation task; no default benchmark result is claimed | PASS |
| `FIND-TASK-008-CLOSEOUT-13` remains deferred | No default `mise run bench:capacity` qualification is added or claimed by the range | Explicit caller boundary | PASS (deferred, non-blocking) |

## Proposed findings

None. The reviewed range closes both assigned findings, and the traced changed
paths introduce no behavior regression within the caller-directed scope.

## Verification limits

- Static inspection covered every changed production, benchmark, test, and
  architecture path in the remediation range and traced their relevant
  callers and consumers.
- The focused non-Postgres aggregation test passed locally. A fresh local run
  of the held-chain-head integration proof could not start because this
  sandbox cannot access the configured Docker socket; it did not reach the
  behavior under test. The remediation record reports that proof passing under
  the repository-managed Postgres wrapper.
- The orchestrator owns final sequential verification for this review.
- The intermittent
  `verification_runtime::two_bindings_share_one_client_observation` failure in
  `test:bifrost:journey:server` is the caller-identified separate integrated-
  branch blocker. This range does not touch that verification/Oracle scan
  path, so it is noted but not counted against the candidate.
- The full default benchmark remains deferred as
  `FIND-TASK-008-CLOSEOUT-13`; this review makes no empirical AC-040/AC-041
  qualification claim.

## Result

**PASS**

`FIND-TASK-008-CLOSEOUT-16` and `FIND-TASK-008-CLOSEOUT-17` are closed, and no
regression was found in `1d05642bf..f8d694504` within the required closure
scope.
