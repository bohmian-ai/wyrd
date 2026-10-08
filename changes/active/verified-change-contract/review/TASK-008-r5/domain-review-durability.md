# Durability, backpressure, and persistent-state domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
- Cumulative range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review inputs: `review/TASK-008-r1/` through `review/TASK-008-r4/`, including the R3 remediation and its appended evidence

`HEAD` resolved to the candidate before source inspection. CodeGraph is not
indexed in this checkout, so tracing used Git and repository-native search.
Prior reports and remediation evidence were treated as hypotheses rather than
accepted conclusions. The caller-directed deferral of
`FIND-TASK-008-CLOSEOUT-13` and the integrator's rejection of post-report
teardown in the report total are preserved and are not findings in this pass.

## Reviewed boundary

This review traced the task's durability-sensitive paths end to end:

- default-queue observation admission, byte ownership, backpressure,
  flush/shutdown, Scribe acknowledgement, and persisted Oracle read-back;
- exact-once cross-replica Verifier claims and durable result settlement;
- benchmark observation of the durable run queue, Scribe persistence and
  staging, the non-blocking audit path, and Forge demands through the exact
  60-second drain edge;
- cancellation and cleanup disposition of admitted client work and already
  durable effects; and
- the revised process-boundary proof where it intersects durable cleanup.

The candidate changes no production queue, claim, Scribe, Oracle, audit, or
Forge algorithm. Its only production-file change in this boundary updates the
default queue's benchmark-reference rustdoc. The material behavior is the
benchmark and test evidence that decides whether production-owned state has
drained.

## Authority and source coverage

| Boundary | Authority and inspected source | Assessment |
|---|---|---|
| Default queue/backpressure | REQ-172 through REQ-177, INV-019, AC-041/AC-042; `wyrd-queue` config, producer, queue, and budget owners; `wyrd-client` Bifrost handle; `capacity/load.rs`; Rust `observe_run.rs` journeys | **PASS.** The benchmark uses `WyrdState` with `QueueConfig::default`, treats `QUEUE_FULL` as an error, and times an explicit flush. The public real-server journey checks zero client-owned bytes after flush and durable grouping of every logical observation. The cumulative diff changes only the default's benchmark-name documentation, not byte or retry semantics. |
| Exactly-once queued claims | REQ-171's correctness/test split; durable claim, lease, and settlement rules; `pg_verification_runtime.rs::two_replicas_claim_each_queued_run_exactly_once`; claim loop, PostgreSQL claim SQL, runner settlement, and result publication | **PASS.** Two runtime instances hold all executions until all 100 claims exist, both must participate, every run remains on attempt one, execution count remains exactly 100, and exactly 100 durable summaries remain. This is an appropriate integration proof for state that a release-process journey cannot directly observe. |
| Scribe backlog and exact drain edge | REQ-171 saturation SLO; Bifrost durability and staging telemetry authority; `capacity/evidence.rs::scribe_backlog`; `capacity/step.rs::{Drain,Deployment::drain}` | **PASS.** Waiting persistence work, immutable generations, and restored/claimed durable staging members all prevent a zero Scribe backlog. Empty is accepted only at or before 60 seconds; a nonempty read at the boundary and a later first empty read expire. |
| Durable run and Forge-demand probes | REQ-171; `capacity/evidence.rs::Queue::backlog`; Verifier run rows; `vala.forge_planning_demands` lifecycle | **PASS within the named durable boundaries.** Expected-but-not-yet-created queued runs and nonterminal durable rows participate in drain. Forge demands remain present until their generation is acknowledged and deleted. |
| Non-blocking audit outbox through retained publication | REQ-171 saturation SLO; AGENTS audit rules; `wyrd-design.md` runtime identity; `bifrost-design.md` read-audit contract; `oracle/query_audit.rs`; `audit/publication.rs`; `capacity/evidence.rs::Queue::backlog`; `capacity/step.rs::Deployment::drain` | **FAIL.** The benchmark observes only committed `vala.audit_staging` rows and can report the audit backlog empty while decisions are still queued or being committed by each replica's process-local outbox. See `DUR-R5-001`. |
| Cancellation and shutdown disposition | Bifrost structured-cancellation rules; `capacity/{load,step,main}.rs`; `release_server.rs`; R3 process-group and blocking-stop proofs | **PASS for this domain.** Request cancellation retains server-accepted durable effects, client admission is later flushed/shut down or explicitly documented as lost on owner drop, replica stop now leaves Tokio workers free, and the outer phase supervisor kills descendant groups. The report-total exclusion of subsequent teardown is caller-approved design and is not reopened here. |

## Material proposed finding

### `DUR-R5-001` — INCORRECT — the audit drain can declare zero before the non-blocking outbox has committed

- **Violated obligation:** Revision-57 REQ-171 requires every audit-outbox
  backlog after load stops to drain within 60 seconds before a step passes.
  Repository authority makes authorization decisions non-blocking: the request
  queues the event and does not wait for its commit, while shutdown explicitly
  waits for pending commits.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:223-264` and
  `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:395-419`. The producer
  state this omits is
  `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:48-57,92-121,142-164`;
  shutdown's independent drain is
  `crates/wyrd/wyrd-server/src/app/server.rs:876-883`.
- **Evidence:** `OracleQueryAudit::stage` increments `pending` and performs a
  non-blocking `try_send`; its writer decrements `pending` only after the batch
  has committed or been counted lost. `Queue::backlog`, however, counts only
  rows already present in `vala.audit_staging` above `published_seq`, and
  `Deployment::drain` accepts zero from that SQL probe without observing any
  replica's `pending` count. This is a reachable interval after a benchmark
  request returns because the request is deliberately not held for audit
  commit. Moreover, a decision committed after the captured `stopped` database
  timestamp receives a later `created_at` and is excluded by
  `s.created_at <= $2`, so subsequent polls do not repair the omission for that
  step. The server's shutdown path separately waiting on
  `audit_outbox.shutdown` confirms that committed staging is not the complete
  pre-shutdown ownership boundary.
- **Observable consequence:** A step can mark the saturation cell `PASS` and
  stop its drain timer while one or both replicas still own uncommitted audit
  decisions caused by that step. Those decisions may commit and require
  publication later, or be counted lost, after the benchmark has already
  recorded a false zero. The report therefore does not establish REQ-171's
  audit-outbox drain SLO and can understate two-replica saturation at exactly
  the non-blocking boundary introduced to remove audit lock contention.
- **Required testable correction:** Extend the existing benchmark drain
  evidence to include each serving replica's process-local audit decisions
  until they are committed or counted lost, together with the existing durable
  staging-above-watermark count. Reuse the existing outbox `pending` owner and
  release-server metric scrape rather than adding a benchmark-side audit queue
  or waiting in requests. Preserve non-blocking request semantics, the one
  canonical `vala.audit_staging` path, publication watermarks, the exact
  60-second edge, and all workload/SLO/report behavior. A focused proof must
  hold an audit commit after its request completes and show that the step
  remains nonempty until the held process-local decision settles; another
  proof must show that committed-but-unpublished staging still holds the same
  audit cell nonzero.

## Positive controls and prior-finding closure

- Prior `FIND-TASK-008-CLOSEOUT-8` remains closed: Scribe staging live members
  are part of the zero-backlog decision.
- Prior `FIND-TASK-008-CLOSEOUT-9` remains closed: the 60-second boundary is
  judged correctly.
- Prior `FIND-TASK-008-CLOSEOUT-12` remains closed: duplicate-claim judgment is
  test-owned, while the benchmark retains only the approved request-loss and
  wrong-judgment error categories.
- The real-server AC-041 journey continues to use the public state/queue path,
  reads through the server after Scribe flush, requires one 100-row group per
  observation, samples client-owned bytes during sustained emission, and
  requires zero ownership after flush.
- R3 process-group supervision and `spawn_blocking` replica stops do not alter
  queue identity, retry, acknowledgement, or persisted-state semantics.

## Verification evidence and limits

This pass inspected the complete cumulative diff, current source owners and
callers, approved revision-57 authority, the original task's evidence, and all
four prior review/remediation rounds. It did not rerun the expensive Postgres
journeys or capacity command. The candidate records the focused cross-replica
claim, fairness, AC-041 real-server, capacity-unit, process-boundary, replica
stop, release-server, formatting, and lint results; these are supporting
evidence, not a substitute for the missing audit handoff proof.

No existing focused test holds a non-blocking audit commit while exercising
`Deployment::drain`. The existing backlog unit tests cover Scribe staged members
and the exact time edge, not the process-local audit-to-Postgres handoff.

`FIND-TASK-008-CLOSEOUT-13`, the unmodified full default benchmark, remains
deferred by explicit caller sequencing and is not a blocker or empirical proof
for this candidate. Running it cannot by itself repair `DUR-R5-001`, because a
short process-local pending interval can be missed nondeterministically while
the SQL-only probe still reports zero.

## Overall result

**FAIL**

The queue, Scribe, exact-once, and persistent-run evidence is otherwise
coherent, but the benchmark's saturation verdict omits a real state owner in
the approved audit path. The correction is bounded to exposing and consuming
the existing pending-outbox evidence; it does not require a new durability or
concurrency decision.
