# Concurrency, Lifecycle, and Resource-Ownership Review

## Review Findings

### Critical

None.

### Important

- **DCL-001 — VIOLATION** — [`crates/wyrd/wyrd-server/src/components/workflow/host.rs:98`](../../../../../crates/wyrd/wyrd-server/src/components/workflow/host.rs) and [`crates/wyrd/wyrd-server/src/components/workflow/runs.rs:263`](../../../../../crates/wyrd/wyrd-server/src/components/workflow/runs.rs): a preparation reservation becomes visible and consumes capacity before its preparation is registered with the shutdown tracker. `WorkflowRuns::admit` installs the reservation and returns it; only later does `WorkflowRunHost::create` call `runs.spawn(preparation.run())` at `host.rs:111`. Concurrently, `WorkflowRuns::drain` cancels admission, closes the tracker, and waits at `runs.rs:393-398`. The installed `tokio-util` contract explicitly says `TaskTracker::close` does **not** prevent later spawning, and `wait` returns immediately when the tracker is closed and empty (`tokio-util-0.7.18/src/task/task_tracker.rs:301-338`). Therefore this reachable interleaving exists: admit installs a reservation; shutdown closes and observes an empty tracker and reports a clean Workflow drain; the request then spawns the cancelled preparation after that report. The late task will eventually drop the reservation, but it is untracked work surviving the claimed drain, directly violating the normative create order (`spec.md:1581-1586`), shutdown ownership (`spec.md:1619-1622`), REQ-034A, and INV-018. The current shutdown journey stalls only after the preparation task has started (`pg_workflow_runs.rs:1777-1795`), so it does not cover this window. **Observable consequence:** shutdown can proceed to gateway/Bifrost teardown and report `workflows_drained = true` while an admitted reservation and a newly spawned preparation still exist; capacity/waiter cleanup then occurs after the owner claimed to be drained. **Required correction:** make registration with the existing `WorkflowRuns` `TaskTracker` and reservation publication one coordinated owner operation, so either the tracker owns the preparation before the reservation can be observed or shutdown wins and no reservation is installed. Use the existing lock plus `TaskTracker` token/tracked-future facilities; do not add an actor, queue, lifecycle option, or custom check. Add focused concurrency proof that drain cannot complete between reservation installation and preparation ownership, and that the losing shutdown path publishes the unavailable error and releases the slot exactly once.

- **DCL-002 — VIOLATION** — [`crates/wyrd/wyrd-server/src/components/workflow/host.rs:373`](../../../../../crates/wyrd/wyrd-server/src/components/workflow/host.rs): the tracked preparation launches its bounded pure hydration/Skald preparation with bare `tokio::task::spawn_blocking`. When shutdown cancels `Preparation::run`, the biased select at `host.rs:262-266` drops `Self::prepare`; dropping the returned `JoinHandle` detaches an already-started blocking task rather than stopping it. That blocking closure retains the pinned graph, execution dependencies, `RunTools`, captured caller, and cloned `AppState` through `host.rs:373-382`, but it is not counted by `WorkflowRuns.tasks`. Consequently `WorkflowRuns::drain` can report success while this preparation child is still executing, and the Tokio runtime may later wait for it outside the shared shutdown budget. This contradicts the task's requirement that preparation use bounded repository-native blocking/cooperative execution while shutdown drains all Workflow preparation work under the one deadline (`TASK-004:151-157`), plus REQ-034A and INV-018. The recorded graph-bound journey proves sibling serviceability, and the shutdown journey cancels at the pre-graph test gate; neither holds the blocking preparation itself, so neither closes this gap. **Observable consequence:** a deep but admitted graph can continue consuming blocking-pool CPU and retaining the server dependency graph after Workflow shutdown has been declared clean; teardown ordering and the single shared deadline are no longer truthful. **Required correction:** register the blocking closure itself with the existing `WorkflowRuns` `TaskTracker` (the installed `TaskTracker::spawn_blocking` is the standard native mechanism) and ensure cancellation of the outer preparation does not detach that owned child. The child need not publish or accept after cancellation, but shutdown must continue to count it until it finishes or the shared deadline expires and is reported as an unclean drain. Add focused proof that a held blocking preparation prevents a clean drain until released, while cancellation still releases its reservation and wakes waiters exactly once.

### Suggestions

None.

## Reviewed Boundary

| Boundary | Source traced | Result |
|---|---|---|
| Admission, same-key waiters, capacity reservation | `components/workflow/host.rs::create`; `components/workflow/runs.rs::{WorkflowRuns::admit, Reservation}` | FAIL — DCL-001 |
| Atomic reservation promotion and accepted start | `Reservation::accept`; `Preparation::run`; Skald `PreparedWorkflowRun::execute` | PASS apart from the ownership gaps above |
| Whole-snapshot replacement and terminal ownership | `AcceptedRun::{observe, finish}`; `skald-workflow::WorkflowExecutor::drive`; `RunLedger::finish` | PASS |
| Active-capacity release, retention, expiry, eviction, key removal | `RunTable::{release, sweep, evict, oldest_terminal}`; `AcceptedRun::finish` | PASS |
| Run cancellation and Skald abort/drain | `WorkflowRuns::cancel`; `WorkflowRunHost::cancel`; `WorkflowExecutor::drive/settle` | PASS |
| Query waiter/owner transfer and settlement join | `components/workflow/tools.rs::QueryTool::invoke`; `RunTools::drain`; `query/collect.rs::{BoundedQuery, ResultCollector}`; `oracle/lifecycle_controls.rs` | PASS |
| Application shutdown ordering and shared deadline | `app/server.rs::BoundServer::run`; `WorkflowRuns::drain` | FAIL — DCL-001, DCL-002 |
| Oracle graph/resource release changes | `vala-bifrost-redux::oracle::{analytical, analytical_supervisor}`; `resources.rs`; `architecture/bifrost-design.md` | PASS for this task boundary; removal of the bespoke polling/poison-on-owner-release machinery uses ordinary reference-owned memory return and is not DRIFT under the standing direction |
| Recorded lifecycle evidence | `pg_workflow_runs.rs` scenarios 2, 4, 6, and 7; `wyrd-testing/tests/bifrost/oracle/workflow.rs`; TASK-004 implementation evidence | Credible for exercised paths, but does not cover either finding's ownership window |

## Authority and Source Coverage

- Immutable subject verified at candidate `96e993a16706d2fb759e4cdb7371ff490b198a35`, base `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`.
- Read and applied `AGENTS.md`, `architecture/agent-rules.md`, `architecture/references/languages/spec-driven-development.md`, `architecture/references/languages/maintainer-style.md`, the approved `changes/active/skald-workflow-runtime/spec.md`, TASK-004, and the applicable Bifrost authority and source.
- Reviewed the complete base-to-candidate changed-file inventory and the concurrency/lifecycle/resource-ownership portions of the cumulative diff. `.codegraph/` is absent, so CodeGraph was not used.
- The standing direction was applied: no bespoke mechanism is required as remediation. Both corrections reuse the installed, standard `tokio-util::TaskTracker` ownership model. The candidate's removal of bespoke Oracle graph-drain polling was not treated as a defect merely because prior code used it.

## Open Questions

None. The two findings follow from explicit source order and the installed dependency's documented close/wait/spawn behavior.

## Verification Notes

- Strictly read-only review: no builds, tests, Cargo, or mise commands were run.
- Recorded evidence reports the seven `pg_workflow_runs` scenarios, focused query lifecycle coverage, forwarded Oracle cancel/deadline/pod-loss coverage, and broader repository lanes as passing.
- Residual proof gap: the recorded shutdown test reaches the existing pre-graph preparation gate only after tracker registration, and the graph-bound test does not hold the `spawn_blocking` closure. It therefore cannot falsify DCL-001 or DCL-002.

## Overall Result

**FAIL**

The candidate implements the requested run, query, terminal, and retention state machines, but it does not yet make all admitted/preparation work part of the owner that shutdown drains. DCL-001 and DCL-002 must be closed before this domain can pass.
