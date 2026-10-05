# System-Resilience Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `eb22b03f2bb766886d839bda23aafbd4ba130ab3`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 9
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`

The candidate was still checked out at the stated commit when this report was
completed. The complete base-to-candidate diff was reviewed. No other reviewer's
conclusions were used.

## Deployed-path evidence

| Path | Ownership and failure boundary | Evidence |
|---|---|---|
| Local Rust Workflow | `Workflow::run_with_options` builds the complete plan before dispatch, then one `WorkflowExecutor` owns the run ledger and bounded `JoinSet`. | `crates/skald/skald-workflow/src/workflow_surface.rs:545-577`; `crates/skald/skald-workflow/src/workflow.rs:180-305` |
| Local Python Workflow | The Python wrapper enters the same Rust Workflow surface through the shared runtime bridge; it does not introduce another scheduler. Python observers execute through `spawn_blocking`. | `crates/skald/skald-workflow/src/python.rs`; `crates/skald/skald-observer/src/python.rs:248-339` |
| Native route | A step task calls the existing Agent loop with the shared native `ProviderRegistry`. Dropping or aborting the step drops the Agent/provider future. | `crates/skald/skald-workflow/src/workflow.rs:434-475`; `crates/skald/skald-workflow/src/route.rs:370-397` |
| WyrdGateway route | Each attempt gets an immutable adapter carrying fallback, remaining deadline, cancellation, and correlation. The caller is a narrow per-call trait; TASK-001 tests the seam with a fake, while the real public/in-process owners remain later-task work. | `crates/skald/skald-workflow/src/route.rs:62-95, 494-555`; `crates/skald/skald-workflow/src/workflow.rs:1239-1474` |
| ExtGateway route | Plan construction resolves the declared binding and builds one policy-screened client. Calls use the existing provider retry path; dropping the step drops the local request/retry future. | `crates/skald/skald-workflow/src/route.rs:267-333, 558-587`; `crates/skald/skald-providers/src/clients/external.rs:52-149`; `crates/skald/skald-providers/src/endpoint.rs:41-218` |
| Shared-process containment | Provider and route failures become typed step results. A spawned step panic becomes a failed step and stops later scheduling without panicking the executor task. | `crates/skald/skald-workflow/src/workflow.rs:264-297`; `crates/skald/skald-workflow/src/attempt.rs:39-183` |

TASK-001 adds the local execution engine and route seams; it does not add the
process-local server run registry. Therefore restart recovery, replica affinity,
server shutdown accounting, and gateway settlement are not falsely credited to
this candidate. Under the approved design, local execution is intentionally
non-durable: process loss or rolling replacement loses the in-flight local run
and does not resume or replay provider calls.

## Failure and recovery paths

| Failure or interruption | Observed behavior | Recovery and proof assessment |
|---|---|---|
| Provider or external dependency outage | Typed connection, timeout, decode, 408, 429, and 5xx outcomes use provider-internal retry within an attempt and the authored Workflow retry outside it. Terminal categories do not retry. | The classification, exhaustion, attempt counts, and deterministic backoff are directly covered by `workflow::tests::bounded_attempt_lifecycle`. |
| Ordinary step failure | New work stops; already-running peers drain; completed peer data is retained; dependent work remains unstarted. | Implemented at `workflow.rs:205-245, 273-297`; directly covered by the focused lifecycle test. With intentionally uncapped local defaults, a non-cooperative already-running peer can wait indefinitely; that is the approved ordinary-drain behavior, not an added finding. |
| Explicit cancellation | The executor selects cancellation ahead of deadline and join completion, aborts all step tasks, drains their join results, and returns a complete cancelled snapshot. | Implemented at `workflow.rs:194-203, 249-285`; the focused test verifies the provider future is dropped and no task remains in flight. The observer boundary exception is finding `SYS-001`. |
| Total deadline | The executor selects total deadline ahead of task completion, aborts and drains steps, and produces the exact timed-out snapshot. Backoff and each step attempt also race it. | Implemented at `workflow.rs:194-203, 249-285, 365-427`; directly covered on paused virtual time. The observer boundary exception is finding `SYS-001`. |
| Per-attempt timeout | The step attempt future is dropped, the timeout is retryable, and exhaustion produces `WYRD_WORKFLOW_504_STEP_TIMEOUT`. | Implemented at `workflow.rs:382-399`; the focused test verifies both abandoned provider futures and the final error. The attempt-start observer occurs before this timer is established, as described in `SYS-001`. |
| Parent future drop | Dropping `WorkflowExecutor::execute` drops its owned `JoinSet`, which aborts every spawned step. | Directly covered by the lifecycle test, including zero remaining fake-provider calls. No detached scheduler was introduced. |
| Step panic | Tokio returns a `JoinError`; the executor records a bounded internal step error and drains peers rather than crashing the shared runtime. | `workflow.rs:286-297`. No dedicated panic test was identified, but the containment path is explicit and local. |
| Process crash or rolling replacement | In-flight local work and its snapshot disappear; no call is resumed or replayed. | This matches the approved process-local/non-durable boundary. Server request affinity and restart behavior belong to later tasks. |

## Material finding

### SYS-001 — Workflow observers can bypass cancellation and deadline recovery

- Classification: `INCORRECT`
- Violated obligation: the total Workflow deadline and explicit cancellation
  must bound execution and return a complete terminal snapshot after owned work
  drains; per-attempt `timeout_seconds` is a wall-clock bound; observers are
  best-effort and must not turn observation into the run's availability owner.
- Exact locations:
  - `crates/skald/skald-workflow/src/workflow.rs:183-188`
  - `crates/skald/skald-workflow/src/workflow.rs:230-242`
  - `crates/skald/skald-workflow/src/workflow.rs:300-304`
  - `crates/skald/skald-workflow/src/workflow.rs:377-382`
  - `crates/skald/skald-workflow/src/workflow.rs:400-425`
  - `crates/skald/skald-observer/src/python.rs:248-339`
- Evidence: `on_workflow_start` is awaited before the ledger starts and before
  the executor enters either cancellation/deadline select. Binding-failure
  observer calls and `on_workflow_finish` are also awaited directly in the
  parent executor, outside those selects. A step increments its attempt count
  and awaits `on_workflow_step_attempt` before constructing the per-attempt
  deadline. The Python implementation awaits a `spawn_blocking` join for each
  callback, so a blocking Python observer is a concrete reachable instance,
  not a hypothetical trait implementation.
- Observable system consequence: a blocked start observer prevents an already
  signalled cancellation or expired total deadline from producing any snapshot;
  a blocked finish observer withholds an already-built terminal snapshot and,
  in a future server owner, retains the Workflow capacity slot; a blocked
  attempt-start observer can exceed `timeout_seconds` before its timer exists.
  Dropping the Rust future cannot cancel an already-running `spawn_blocking`
  Python callback, so callback work may also continue after the Workflow owner
  is gone. This affects local Rust and Python runs today and would affect any
  later server composition that installs a nontrivial observer.
- Testable correction: place every Workflow observer await inside the existing
  owning cancellation/deadline boundary. Establish the attempt deadline before
  emitting the attempt-start event and race that event with cancellation, total
  deadline, and attempt timeout. Start/binding/finish notifications must not
  prevent cancellation or an expired total deadline from returning the complete
  snapshot; terminal notification must not retain execution capacity after the
  snapshot is final. Reuse the existing cancellation token and `Instant`
  deadlines; do not add an observer scheduler or timeout configuration.
  Add one deterministic observer fixture that blocks start, attempt-start, and
  finish in turn and proves cancellation/deadline still settles the run and no
  new attempt begins.

## Affected capabilities

- Local Rust Workflow execution: cancellation/deadline response can stall.
- Local Python Workflow execution: a Python callback can occupy a blocking
  worker after the Workflow future is dropped.
- Future server composition of this executor: a stalled terminal observer can
  delay terminal publication and Workflow-capacity release unless corrected at
  this shared owner.
- Native, WyrdGateway, and ExtGateway provider futures themselves remain
  bounded by the owned step task once the executor reaches its select loop.
- Unrelated server and gateway capabilities do not crash or become unavailable
  from ordinary step/provider failures; the candidate contains those failures
  within the Workflow caller/task.

## Verification assessment

- Re-ran:
  `mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::bounded_attempt_lifecycle)'`
  — 1 selected, 1 passed.
- Reviewed the task's recorded successful scoped lanes and the lifecycle test's
  concurrency, retry, ordinary drain, cancellation, deadline, and parent-drop
  assertions.
- The healthy and interruption tests use observers that return immediately.
  They do not exercise a blocked start, attempt-start, binding-failure, or
  finish notification, so they cannot close `SYS-001`.
- Real public/in-process WyrdGateway cancellation and gateway-owned settlement
  are deliberately deferred to TASK-003/TASK-004 and are not treated as a
  TASK-001 failure.

## Overall result

**FAIL**

The owned step set, dependency-outage handling, panic containment, ordinary
peer drain, cancellation, total deadline, and parent-drop paths are otherwise
coherent. `SYS-001` leaves the public local engine without its required bounded
recovery when an observer blocks at a parent-owned callback boundary.
