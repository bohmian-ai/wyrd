# Concurrency domain review — TASK-002

Result: **FAIL** (one proposed finding requiring independent validation).

## Immutable subject and boundary

Base `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`; candidate
`d342c54749a871dcacf5b0c7ec29e3b458f2d9e2`; HEAD was reconfirmed at the latter.
Reviewed the cumulative range and the frozen revision-6 specification,
original task, and recorded verification evidence in this review directory.
This pass owns only execution concurrency, retry, deadline, and cancellation
semantics. No source or task packet was modified.

Authorities: AGENTS.md §§5–6/10–12/16; architecture/agent-rules.md;
architecture/wyrd-design.md doctrine, Verifier runtime and coordination-clock
sections; architecture/wyrd-doctrine.mdx; reference router and Rust-core,
evaluation, testing-workflows, spec-driven-development, and maintainer-style
references. The approved spec explicitly supersedes the older typed Verifier
split. Relevant obligations are REQ-003/004, INV-007, AC-002 and TASK-002
scenarios 2/4 plus its engine-owned deadline handoff.

CodeGraph was used first to navigate the engine and contract planner. Expanded
source coverage includes blocks.rs; complete engine orchestration/attempt/check
paths; scope production and stage writes; the action/check scripted harness and
ordering, overlap, retries, timeout and cancellation tests; VerifierSpec planner
and retry validation; existing execute_plan/execute_stage/TaskExecutor/
Executors/RunLedger; changed assertion/trace/capture helper diffs; Cargo.lock.
The resolved Tokio is 1.52.3; its local timeout implementation was inspected to
resolve the deadline behavior below.

## Execution and failure paths

| Obligation / path | Source evidence | Proof and assessment |
|---|---|---|
| Independent actions overlap under one limit | engine.rs:207–245 spawns a stage; :265 acquires the engine's Arc semaphore; clones retain that same semaphore | Barrier test proves two actions overlap at 2 and cannot both enter at 1. PASS. |
| Requirements share the block limit | engine.rs:308–359 spawns checks; :380 acquires the same semaphore | Source establishes a common limit, including across engine clones. Checks are not scheduled until actions settle. PASS. |
| Dependencies wait for action plus attached judgments | engine.rs:177–197 settles actions, then requirements, then advances the plan stage; scope writes occur only on the driver | dependents_wait_for_upstream_step_requirements exercises the action → step-check → dependent-action → final-check call stream. PASS. |
| Retryable errors spend a bounded budget | engine.rs:263–296; approved contract validates retries ≤5 in blocks/validate.rs:305 | Retry test distinguishes recovery, exhaustion, terminal refusal and attempt counts. The permit remains owned through retries, so retry fanout cannot exceed the block limit. PASS. |
| Per-attempt timeout drops the expired action future | engine.rs:270–287 races earlier step deadline; :282 maps expiry to retryable timed out | Timeout test proves two 5-second attempts and unrelated final check completion. No timeout becomes a subject failure. PASS. |
| Overall deadline interrupts pending stage work | engine.rs:139–143 wraps drive; run_steps/run_requirements own JoinSets, which abort their spawned tasks on drop | execution_deadline_cancels_running_work observes a hung action's DropSignal. The corresponding check JoinSet has the same cancellation ownership. No detached engine-owned worker is introduced. PASS for pending work. |
| Expired deadline yields an execution error on immediate completion paths | engine.rs:139–143 and :165–201 | FAIL; CONC-001 below. |

A panic in any spawned block becomes EngineError::Join and drops its local
JoinSet, aborting sibling tasks; it does not panic the calling service. Caller
cancellation drops drive and its JoinSet. Permits are RAII-owned inside spawned
futures, so task cancellation releases them when those futures are dropped.
Executor-owned underlying IO resources remain the executor's responsibility
under the supplied BlockCall contract; this task does not implement that IO.
There are no new server, process-pool, SQL, persistent queue, or tenant state
owners in this cumulative range.

The existing stage driver already owns topological barrier walking, fanout,
and ordered collection. The new engine uses the existing VerifierSpec::plan
and native JoinSet/Semaphore, but adds a separate drive/stage/scope orchestration
rather than extending execute_plan. Its new failure and action/check positions
are absent from the old EvalTask driver. Whether the prescribed reuse boundary
allows that separate owner is a whole-task acceptance question; this domain
pass does not use leaf helper reuse as proof of compliance or propose another
concurrency abstraction.

## Proposed finding

### CONC-001 — INCORRECT: an elapsed execution deadline can return a verdict

**Obligation.** TASK-002's contract handoff makes the engine own the deadline;
its scenario 4 requires bounded timeout/cancellation. EngineError and run's own
contract promise an execution error when the overall deadline has passed,
without a verdict. INV-007 requires execution deadlines to bound work.

**Changed location.** `crates/vala/vala-eval/src/blocks/engine.rs:139–143`
(`VerificationEngine::run`), with the immediate path at :165–201.

**Producer → consumer and reachability.** The public run method accepts an
absolute Instant, and performs no expired-deadline check. A caller can supply
an elapsed deadline, or construct the future and first poll it after the
deadline. A valid empty Verifier has no stage and no requirement tasks; drive
and run_requirements complete without suspension. The same path exists for
requirements whose missing bindings avoid spawning checks. Tokio 1.52.3's
`time/timeout.rs:211–221` polls the inner future first and immediately returns
its ready result before polling the deadline timer. Consequently timeout_at
returns Ok and run returns a completed VerificationOutcome (inconclusive for
an empty Verifier), although the deadline preceded execution. This follows
from the complete source path, not a scheduling race or an uncooperative
injected executor. Existing tests use a future deadline plus a pending action,
so they cannot exercise this path.

**Observable consequence.** The caller receives a verdict/completed execution
instead of DeadlineExceeded for work that had no execution time remaining.
The hosted owner consuming this engine cannot consistently distinguish timed
out execution from completed execution on no-work/missing-evidence paths.

**Smallest correction boundary.** Enforce an already elapsed overall deadline
in VerificationEngine::run, the owner of that invariant, before evaluating the
graph; retain timeout_at for pending work and the existing step timeout/retry
semantics. Also prevent publishing a verdict after the overall deadline if
synchronous execution/planning consumes its remaining time. Reuse Instant and
EngineError::DeadlineExceeded; no timer abstraction, configuration, executor
change, or downstream caller guard is needed. Keep normal empty execution
inconclusive when it finishes within budget.

**Focused closure proof.** Add a paused-time engine test supplying an already
elapsed deadline to a valid empty Verifier and assert DeadlineExceeded rather
than any VerificationOutcome. Include a no-spawn missing-evidence case if it
can share that focused test without new harness machinery. Preserve the
existing future-deadline empty-result and pending-action cancellation tests.
Run the exact newly named test through mise/nextest and the isolated vala-eval
library lane; no journey suite is needed for this correction.

## Verification limits

Recorded library, exact-selector and lint claims were inspected; this agent
ran no Cargo command to avoid conflicting with the orchestrator's independent
library checks. The deadline finding is source-proven using the pinned Tokio
implementation, not a newly executed reproduction. No integration, journey,
provider, server, sandbox or multi-process qualification was performed; those
are outside this task's isolated domain evidence. No synthetic load was used.
