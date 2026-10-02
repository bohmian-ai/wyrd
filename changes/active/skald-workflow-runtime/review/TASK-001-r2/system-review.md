# System-Resilience Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `28473e049705595306f2934cf4bc664168254086`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 10
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`

The candidate was checked out at the stated commit before review. The complete
base-to-candidate diff was reviewed. Production source is unchanged from the
round-one candidate `eb22b03f2bb766886d839bda23aafbd4ba130ab3`; revision 10
changes the approved public seams to the implemented
`HashMap<HeaderName, SecretString>` and `RemoteProblem(Box<RemoteProblem>)`
shapes but does not alter runtime failure or recovery behavior. Round-one
findings were treated as hypotheses and checked against current source.

## Deployed-path evidence

| Path | Ownership and failure boundary | Evidence |
|---|---|---|
| Local Rust Workflow | `Workflow::run_with_options` completes synchronous planning before one `WorkflowExecutor` owns the run ledger, cancellation token, limits, and bounded `JoinSet`. | `crates/skald/skald-workflow/src/workflow_surface.rs:531-579`; `crates/skald/skald-workflow/src/workflow.rs:80-304` |
| Local Python Workflow | The retained Python surface enters the same Rust executor through the shared runtime bridge. Python observers run callbacks in `spawn_blocking` and await the blocking-task join. | `crates/skald/skald-workflow/src/python.rs:136-154,225-295`; `crates/skald/skald-observer/src/python.rs:248-339` |
| Native route | A `StepTask` invokes the existing Agent loop with the shared native registry. Aborting or dropping the task drops the Agent/provider future. | `crates/skald/skald-workflow/src/workflow.rs:430-480`; `crates/skald/skald-workflow/src/route.rs:370-397` |
| WyrdGateway route | An attempt-local adapter carries fallback, absolute deadline, cancellation, and correlation into the narrow caller trait. TASK-001 supplies the seam; public and in-process implementations remain later-task work. | `crates/skald/skald-workflow/src/route.rs:356-397,494-555` |
| ExtGateway route | Plan construction fixes one screened, pinned client for the declared binding and origin. Provider-internal retry remains inside one Workflow attempt; aborting the step drops the request/retry future. | `crates/skald/skald-workflow/src/route.rs:267-333,558-595`; `crates/skald/skald-providers/src/clients/external.rs:52-149`; `crates/skald/skald-providers/src/endpoint.rs:41-218` |
| Shared-process containment | Typed provider and route failures become step results. A panic inside a spawned step is converted to a bounded internal failure, but observer awaits executed by the parent are not panic-isolated. | `crates/skald/skald-workflow/src/workflow.rs:264-297`; `crates/skald/skald-observer/src/observer.rs:19-23` |

TASK-001 does not add the server run registry. Local execution is deliberately
process-local and non-durable: process crash or rolling replacement loses the
in-flight local run and does not resume or replay provider calls. Replica
affinity, retained terminal state, shutdown capacity accounting, and gateway
settlement belong to later tasks and are not credited to this candidate.

## Failure and recovery paths

| Failure or interruption | Observed behavior | Recovery and proof assessment |
|---|---|---|
| Provider or gateway outage | Connection, timeout, decode, 408, 429, and 5xx categories can retry inside the provider and again at the authored Workflow boundary; terminal categories stop. | The normal classification/exhaustion path is covered by `workflow::tests::bounded_attempt_lifecycle`. An accepted `u32::MAX` retry count does not terminate safely; see `SYS-003`. |
| Ordinary step failure | New work stops, already-running peers drain, succeeded peer data remains, and dependent work terminalizes unstarted. | `workflow.rs:205-245,273-297`; covered by the focused lifecycle test. Uncapped local execution can intentionally wait for a non-cooperative peer, matching the approved ordinary-drain behavior. |
| Explicit cancellation | The parent select gives cancellation priority, aborts the `JoinSet`, drains join results, and returns a complete cancelled snapshot. | `workflow.rs:194-203,249-285`. Observer awaits can prevent the select or terminal return (`SYS-001`), and a task aborted before first poll can be misclassified (`SYS-004`). |
| Total deadline | The parent aborts and drains active tasks, then produces a timed-out snapshot; active attempt/backoff futures also race the deadline. | `workflow.rs:194-203,249-285,365-427`. Observer boundaries and unchecked deadline construction leave reachable exceptions (`SYS-001`, `SYS-002`). |
| Per-attempt timeout | The attempt future is dropped and exhaustion projects `WYRD_WORKFLOW_504_STEP_TIMEOUT`. | `workflow.rs:382-399`. The timer is constructed only after the attempt-start observer returns, so that callback is outside the wall-clock attempt bound (`SYS-001`). |
| Parent future drop | Dropping `WorkflowExecutor::execute` drops its owned `JoinSet`, aborting step futures. | The task evidence records focused parent-drop coverage. A Python callback already running in `spawn_blocking` is not cancelled by dropping its awaiting future and may outlive the run (`SYS-001`). |
| Step or observer panic | A panic in a spawned `StepTask` becomes a failed step through `JoinError`; a panic in parent-owned start, binding-error, or finish observation unwinds the public run future. | `workflow.rs:183-186,230-242,286-297,300-304`. This contradicts the observer contract that callbacks are best-effort and panic-contained (`SYS-001`). |
| Process crash or restart | In-flight local work and its snapshot disappear with no replay. | This is the approved local non-durable boundary, not a candidate defect. |

## Material proposed findings

### SYS-001 — Workflow observation can own availability and escape panic containment

- Classification: `REGRESSION`
- Violated obligation: REQ-016/019/043/048 and the observer contract require
  wall-clock attempt bounds, bounded cancellation/deadline settlement, owned
  task cleanup, and best-effort observation whose failures do not become run
  failures.
- Exact locations:
  - `crates/skald/skald-workflow/src/workflow.rs:183-188,230-242,300-304,377-425`
  - `crates/skald/skald-observer/src/observer.rs:7-23`
  - `crates/skald/skald-observer/src/composite.rs:132-173`
  - `crates/skald/skald-observer/src/python.rs:248-339`
- Evidence and reachability: workflow start, binding-failure events, and finish
  are awaited directly by the parent outside its cancellation/deadline select.
  Attempt-start is awaited before the per-attempt deadline exists; result and
  backoff events are also awaited before the step can settle or retry.
  `CompositeObserver` awaits observers serially. `PythonObserver` supplies a
  concrete blocking implementation by awaiting `spawn_blocking`; dropping that
  await does not stop an already-running blocking callback. No source boundary
  catches a Rust observer panic despite the public trait contract saying the
  runtime catches and logs one.
- Observable system consequence: a blocked start or binding-error observer can
  prevent cancellation or an expired deadline from reaching the parent select;
  a blocked finish observer withholds an already-terminal snapshot and would
  retain a later server capacity slot; an attempt-start observer extends the
  attempt beyond `timeout_seconds`; and a Rust observer panic can unwind the
  public Workflow future or turn a healthy step into an internal step failure.
  Python callback work can remain alive after the Workflow future is gone.
- Testable correction: route every Workflow observer call through one existing
  owner-level, panic-isolated best-effort boundary. That boundary must respect
  the current cancellation and absolute deadlines, establish the attempt
  deadline before attempt-start observation, and never let terminal observation
  retain a finalized snapshot or capacity. Reuse the current cancellation
  token/deadlines; add no observer scheduler or timeout configuration. Prove
  blocking and panicking start, attempt, result/backoff, binding-error, and
  finish observers cannot prevent the specified terminal result or start a new
  attempt.

### SYS-002 — Public timeout values can panic the runtime during deadline construction

- Classification: `INCORRECT`
- Violated obligation: public authored/local inputs must produce stable
  pre-dispatch or terminal behavior; they must not unwind the shared runtime.
- Exact locations:
  - `crates/wyrd-spec/src/card/workflow.rs:349-368`
  - `crates/skald/skald-workflow/src/workflow.rs:183-188,382,436-444`
  - `crates/skald/skald-workflow/src/plan.rs:278-282`
- Evidence and reachability: the contract accepts the full `u64` range for
  authored step timeouts and an arbitrary public `Duration` for the local run
  deadline. The executor constructs total, step, and Agent deadlines with
  unchecked `Instant + Duration`. Values that cannot be represented by the
  monotonic clock panic before a typed outcome can be produced.
- Observable system consequence: loading a validly decoded extreme timeout or
  passing an extreme local limit can unwind the caller task/process boundary
  instead of returning the promised `WorkflowResult` or complete
  `WorkflowRun`; in later server composition this is a shared-service
  availability risk.
- Testable correction: use checked deadline construction at the existing
  validation/execution owner. Reject an unrepresentable authored timeout through
  the field-specific Workflow validation path and an unrepresentable local
  deadline through the existing pre-dispatch error path; do not clamp or remove
  the deadline. Prove maximum-value step, Agent, and run durations never panic
  or dispatch.

### SYS-003 — The accepted maximum retry count cannot terminate safely

- Classification: `INCORRECT`
- Violated obligation: REQ-016 and the snapshot contract define exact
  `max_retries + 1` accounting in a `u32` attempt field and require bounded,
  terminating retry behavior.
- Exact locations:
  - `crates/wyrd-spec/src/card/workflow.rs:360-368`
  - `crates/skald/skald-workflow/src/workflow.rs:368-427`
  - `crates/skald/skald-workflow/src/run.rs:318-340`
- Evidence and reachability: `u32::MAX` is accepted, but the executor increments
  `attempt` before every attempt and tests `attempt > max_retries` only after a
  retryable failure. The required final attempt cannot fit. Overflow panics
  when checked or wraps to zero and continues when unchecked; local defaults
  have no total deadline, and zero/absent backoff is explicitly immediate.
- Observable system consequence: an authored workflow with a fast retryable
  failure can panic or spin/provider-loop without reaching the declared final
  attempt, consuming CPU and external-call capacity rather than settling a
  run.
- Testable correction: reject only `max_retries == u32::MAX` in the existing
  pure `WorkflowSpec` validation owner, because its mandatory first attempt is
  unrepresentable in the public counter. Prove `u32::MAX - 1` remains valid,
  `u32::MAX` fails before dispatch with the retry field named, and ordinary
  exhaustion accounting remains exact.

### SYS-004 — Cancellation before first task poll emits an impossible active-step snapshot

- Classification: `INCORRECT`
- Violated obligation: the exact snapshot invariants require pending/unstarted
  work to have zero attempts and no timestamps, while a cancelled active step
  must retain timestamps and at least one begun attempt.
- Exact locations:
  - `crates/skald/skald-workflow/src/workflow.rs:205-228,249-285`
  - `crates/skald/skald-workflow/src/run.rs:120-125,166-172,204-221`
- Evidence and reachability: the parent marks a step running and timestamps it
  before spawning. The child increments the shared attempt counter only when it
  is first polled. Cancellation can win and abort the task before that poll;
  the cancelled `JoinError` then calls `step_cancelled(index, 0)`, preventing
  `RunLedger::finish` from converting the still-running entry to unstarted.
- Observable system consequence: equivalent cancellations produce contract-
  distinct snapshots based only on Tokio scheduling, including a cancelled
  step with zero attempts, which no valid lifecycle state permits.
- Testable correction: at the executor settlement owner, leave a spawned task
  with a zero attempt counter for `RunLedger::finish` to terminalize as
  unstarted with cleared timestamps; record cancelled only after an attempt has
  begun. Deterministically cancel after spawn but before first poll, and contrast
  it with cancellation after attempt start.

## Affected capabilities

- Local Rust and Python Workflow execution can stall, panic, overrun a step
  timeout, spin on an unrepresentable retry policy, or return a malformed
  cancellation snapshot.
- Native, WyrdGateway, and ExtGateway routes all share the executor findings;
  the provider futures themselves remain owned once execution reaches the
  select loop.
- A later `wyrd-server` host would inherit these shared-owner defects: blocked
  terminal observation can retain capacity, unchecked public durations can
  unwind tracked work, and unbounded retry can consume provider capacity.
- Ordinary provider errors and step panics are otherwise contained within the
  Workflow caller; unrelated server/gateway capabilities are not taken down by
  the normal typed-failure path.

## Recovery and proof assessment

- The task records green `test:skald`, `test:shared`, `test:wyrd`, Python,
  codegen, boundary, formatting, and lint lanes, plus the focused
  `workflow::tests::bounded_attempt_lifecycle` result.
- The focused lifecycle test uses immediately returning observers and ordinary
  duration/retry values. It does not exercise blocked or panicking callbacks,
  unrepresentable `Instant` arithmetic, `u32::MAX` retries, or cancellation
  between spawn and first poll.
- No additional Cargo command was run in this concurrent review; the source and
  recorded evidence are sufficient to establish the reachable gaps without
  competing for the shared target directory.
- Revision 10 closes the former public-seam conflicts for external binding
  storage and boxed remote problems. It changes no evidence for `SYS-001`
  through `SYS-004`.

## Overall result

**FAIL**

The candidate correctly centralizes local execution in one owned task set and
has coherent normal outage, ordinary-failure, abort-and-drain, parent-drop, and
non-durable restart boundaries. The four findings above leave reachable paths
where observation controls availability, public limits panic or fail to
terminate, and cancellation emits a state forbidden by the portable contract.

The candidate remained at
`28473e049705595306f2934cf4bc664168254086` when this report was completed.
