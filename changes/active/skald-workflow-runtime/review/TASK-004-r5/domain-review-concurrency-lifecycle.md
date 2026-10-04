# Concurrency and Lifecycle Domain Review — TASK-004 R5

## Review Findings

### Critical

None.

### Important

None.

### Suggestions

None.

## Reviewed Boundary

Immutable subject: base `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`,
candidate `d4d4e2da53abfc677abdb804e71517c3b6849f49`, approved
`SPEC-skald-workflow-runtime` Revision 13, original TASK-004, and remediation
tasks R1 through R4. `.codegraph/` is absent, so immutable Git objects and
direct source inspection were used.

| Boundary | Authority and source traced | Result |
|---|---|---|
| Preparation deduplication and waiter ownership | Revision 13 normative create flow, INV-018/019, AC-021; `WorkflowRunHost::create`; `WorkflowRuns::admit`; `Reservation::{outcome,accept,fail,drop}`; `tracked_preparation_replay_and_disconnect` | **PASS.** The scoped key is inspected and reserved under the one run-table lock. Matching waiters share the preparation outcome; conflicting requests fail; request cancellation does not cancel the tracked owner. |
| Reservation tracking, promotion, and capacity | Server run-owner contract, REQ-034A/034C, TASK-004 R1; `WorkflowRuns::{admit,spawn,drain}`; `Reservation::accept`; `AcceptedRun::finish`; owner tests in `runs.rs` | **PASS.** A reservation acquires a `TaskTrackerToken` before it is published, atomic promotion transfers rather than increments the active slot, every pre-accept failure/drop releases once, and terminal commit releases after owned work settles. |
| Prepared run and total deadline ownership | Revision 13 retry/deadline contract, TASK-004 R4; `Workflow::prepare`; `WorkflowExecutor::new`; `PreparedWorkflowRun::{deadline,execute}`; `Preparation::prepare`; `RunTools::{new,bind_deadline}` | **PASS.** `WorkflowExecutor::new` is the sole sampler of the total absolute deadline. The exact `PreparedWorkflowRun` instant is bound once through `Arc<OnceLock<Instant>>` into all already-hydrated tool clones before `Reservation::accept`; no earlier run-tool clock sample remains. |
| Run and step snapshot publication | REQ-018–023/045, R3's fixed human decision; `WorkflowExecutor::{drive,settle}`; `StepTask::run`; `RunLedger::{start,step_started,step_cancelled,finish}`; `AcceptedRun::{observe,finish}` | **PASS.** Whole snapshots are published from the scheduling owner. Attempt one is stored and published before a step becomes `Running`; an interruption before the task's first poll therefore settles that published step as `Cancelled` with attempt one and timestamps. Pending-only steps become `Unstarted`. |
| Cancellation, timeout, and terminal races | Normative cancellation flow, REQ-019/020/048; `WorkflowRuns::cancel`; `WorkflowRunHost::cancel`; `WorkflowExecutor::drive`; `RunLedger::finish`; lifecycle race journey | **PASS.** Cancellation is recorded synchronously, the executor aborts and joins every Skald step, cancellation wins the biased ready boundary, and total expiry alone proposes `TimedOut`. The consumed `AcceptedRun` is the sole terminal publisher, so later non-terminal observations cannot regress committed state. |
| Built-in query ownership and settlement | TASK-004/R1/R4 query-owner requirements, INV-021; `QueryTool::invoke`; `RunTools::drain`; `BoundedQuery::run`; `RunningQueryControls`; forwarded-Oracle journey | **PASS.** Each admitted query runs on the per-run `TaskTracker`; dropping/aborting the Agent waiter signals the child token without destroying the owner; the run joins every query owner before terminal publication and capacity release. Omitted and longer query durations are clipped from the one prepared deadline, while the existing `min` keeps an explicit shorter duration shorter. |
| Shutdown and tracked work | Normative shutdown flow, REQ-034A, INV-018; `WorkflowRuns::{spawn_blocking,drain}`; `Preparation::run`; `BoundServer::run`; shutdown owner/unit and server journeys | **PASS.** The reservation covers the pre-spawn interval, blocking preparation remains tracked after its awaiting future is cancelled, Workflow admission closes under the run-table lock, and Workflow work drains first while gateway/query/Bifrost dependencies remain available, all under the one process deadline. An exhausted Workflow drain is reported as unclean rather than hidden. |
| Retention and process-local semantics | REQ-034A/B/C; `RunTable::{sweep,evict,oldest_terminal}`; `WorkflowRuns::{get,cancel}`; lifecycle journey | **PASS.** Only terminal entries are retention candidates; eviction removes their scoped keys; active work is never evicted. Restart or a non-owning replica has no shared run state and returns the common not-found response, with no cross-replica owner or recovery mechanism added. |
| Fixed Oracle and harness decisions | Human standing decisions; current Bifrost authority and cumulative Oracle diff; R1–R4 preserved behavior | **PASS.** Deleted graph-drain polling and supervisor idle refusal remain deleted. Follower release remains grant-stream close with no leader acknowledgement. The foreign-tenant journey retains its fixture credential limitation. No replacement protocol, check, setting, option, or bespoke lifecycle mechanism was introduced. |

## Authority and Source Coverage

- Read and applied `AGENTS.md`, `architecture/agent-rules.md`, the
  spec-driven-development and maintainer-style references,
  `architecture/wyrd-design.md`, the Workflow-relevant shutdown/query portions
  of `architecture/bifrost-design.md`, Revision 13, TASK-004, and remediation
  tasks R1 through R4.
- Reviewed the complete cumulative base-to-candidate changed-file inventory
  and traced the concurrency/lifecycle owners and their callers through
  `components/workflow/{host,runs,tools}.rs`, Skald's
  `workflow.rs`, `workflow_surface.rs`, and `run.rs`, `query/collect.rs`,
  `app/server.rs`, and the relevant server and forwarded-Oracle journeys.
- Rechecked prior concurrency findings and their corrections. R1's reservation
  token and tracked blocking work, R3's attempt-one publication rule, and R4's
  prepared-deadline projection remain closed at their producing owners.
- Applied the standing DRIFT direction. The retained mechanisms are ordinary
  repository/Tokio ownership primitives: one lock, `TaskTracker` and its token,
  `CancellationToken`, `JoinSet`, watch snapshots, and a one-time standard
  library binding required by the established hydration order. No remediation
  or novel mechanism is warranted.

## Open Questions

None.

## Verification Notes

- Strict read-only review: no builds, tests, Cargo, or mise commands were run.
- Recorded evidence reports the focused deadline-owner test, Skald family
  tests, Wyrd server Workflow journeys, and the forwarded-Oracle journey as
  passing. The R4 red proof establishes that the focused deadline test fails
  when the earlier tool-side sample is restored.
- The focused R4 unit test proves exact deadline identity across a deliberate
  one-second hydration/preparation interval; the updated forwarded-Oracle case
  supplies a post-tool `DONE` continuation, so an early unqualified query
  timeout would make the run succeed instead of satisfying its unchanged
  `TimedOut` assertions.
- Residual verification limit: the focused unit does not invoke
  `QueryTool::invoke` or independently exercise an explicit shorter
  `deadline_ms`. The current source directly uses the bound instant and the
  pre-existing `requested.min(remaining)` projection, and the recorded broader
  query/server lanes passed; this is not a source-backed defect in the reviewed
  candidate.
- Candidate identity was rechecked before writing and remained
  `d4d4e2da53abfc677abdb804e71517c3b6849f49`.

## Overall Result

**PASS**

The cumulative candidate satisfies the reviewed concurrency and lifecycle
obligations. No material finding remains in this domain.
