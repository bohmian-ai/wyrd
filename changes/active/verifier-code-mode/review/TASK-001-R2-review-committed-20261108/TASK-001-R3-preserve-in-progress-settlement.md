---
id: TASK-001-R3
kind: remediation
status: ready
spec: SPEC-verifier-code-mode
spec_revision: 2
parent_task: TASK-001
remediates: [FIND-TASK-001-2]
depends_on: [TASK-001-R2]
---

# Give invocation cleanup one owning capability

## Outcome and authority

Finish the same cleanup guarantee by giving the invocation caller exclusive
ownership of cleanup. The engine owns timeout, retry and result publication;
executors may register started work but cannot drain or take its settlement.
After execution stops, the engine finishes that work before advancing. Keep
R2's immediate registration and actual work-owned guards.

This supersedes this task's earlier retained-future polling recommendation at
the maintainer's direction. The finding and approved behavior are unchanged;
the correction removes duplicate cleanup ownership rather than protecting it.

- Approved spec: revision 2 at
  `9abeaf124:changes/active/verifier-code-mode/spec.md`, also preserved exactly
  in [revision-2.md](../../spec-revisions/revision-2.md).
- Original [TASK-001](../../tasks/TASK-001-bounded-code-orchestration.md).
- Prior candidate: `f8b45a7208dbc3e8ec6c3cdac3605f6f84b6cc18`.
- Reviewed candidate: `9abeaf124585cd0d726d2554965b105fc0c0ef86`;
  runtime `66f4d54ca14d9da0257aa6a72239687e7abef004`.
- [R2 task/evidence](../TASK-001-R1-review-20261108/TASK-001-R2-settle-before-retry.md),
  [validated ledger](findings-validation.md), [verdict](verdict.md).

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode`.
The diagnosis/recommendation here plus original task and approved spec define
the work; no conversation or separate report supplies hidden requirements.
Implement on the caller's subsequent candidate; Git actions remain caller-owned.

## Diagnosis and selected correction

REQ-012/AC-009 and R2 require settlement before a timeout result or retry.
R2 moves the session's settlement into engine-owned ToolCalls when the session
opens, then waits both in AgentExecutor and after the engine's timed attempt.
This works when the attempt times out while the Agent is still running.

It fails if the Agent begins cleanup before that timeout:

1. A generated `code.run` awaits a started tracked extraction. Its configured
   Python operation deadline can precede the authored step deadline.
2. The earlier Python timeout ends the sandbox driver/child wait and raises
   the existing typed host stop. Started blocking extraction keeps running
   under TaskTracker with its workspace/writer/permit, as intended.
3. AgentExecutor enters `calls.settle().await` at `blocks/agent.rs:413–426`.
   `ToolCalls::settle` at `blocks/tools.rs:170–180` removes the settlement
   from shared state before awaiting it. The pending future is now owned
   only by the timed Agent call again.
4. The later authored timeout at `blocks/engine.rs:675–701` drops that call
   and its removed future. The engine's fallback settlement finds an empty
   list, so it retries or produces its result while extraction still runs.

This is the original finding's same premature progression. A retry gets fresh
session admission while old work still occupies capacity; a no-retry failure
can be published before cleanup finishes. It needs no arbitrary future drop,
new shutdown expectation or unsupported input. The new passing test holds the
Agent's only tool operation until the attempt timeout, so its first settlement
poll is the outer engine wait. It misses interruption of the inner Agent wait.

The root cause is duplicated lifecycle authority, not merely removal before
an await. `BlockCall.tool_calls` is cloneable and currently exposes settlement
draining to the timed Agent executor as well as the outer engine. That makes
it possible for the child to take the very cleanup obligation its parent must
retain. Safer polling would preserve that duplicate ownership.

Select one owning invocation capability at the existing engine/ToolCalls
boundary. The engine retains that non-clone owning capability; the executor's
cloneable tool-call handle exposes only request accounting, recording and work
registration. It must expose no operation that takes or drains the owner's
settlement. This owner/handle separation should make the problematic child
drain impossible to express through the executor-facing API. Exact type names,
fields and private organization remain implementation-owned.

Remove cleanup draining from `AgentExecutor::invoke`. The engine alone finishes
registered work after the action execution/attempt timeout, before retry or
Attempted/result creation. Apply the same ownership to checks: finish registered
work after `checks.execute` and before handling Busy, constructing an
inconclusive/cited result or reading settled records. Both success and error
paths pass through their caller-owned completion boundary. Capture the executor's
result/error before finishing cleanup. Cleanup time must not replace an already
returned terminal/Busy result with a later authored timeout: preserve the
captured classification after cleanup, with no retry for terminal/Busy. Authored
timeout retry/no-retry policy applies when execution itself is still pending at
that deadline.

Keep immediate session registration and the existing TaskTracker, writer,
permit and workspace retention. No second guard, polling-based fallback,
additional drain in the child, generic lifecycle framework or dependency is
needed. The engine already owns orchestration; the small owning capability
restricts who may finish cleanup rather than adding another orchestrator.

Low-level direct executor calls in this repository are test seams, not another
production lifecycle owner. `ActionExecutor`/`CheckExecutor` in `blocks.rs`
define one-attempt execution under a caller; Hosted Blocks only forwards engine
calls. Update direct fixture helpers in `blocks_agent.rs`, `pg_hosted_artifacts`
and other actual affected constructors to retain the caller's owning capability
and finish it before exposing their result, or use an existing engine helper.
Do not silently drop their cleanup assertions. This preserves action/check
coverage without keeping a production child drain solely for test fixtures.

## Preserved behavior and non-goals

Keep session handoff and caller-owned action/check/direct-fixture settlement,
timeout classification and retry count, final failed-step/inconclusive-check semantics, retry-spanning
request/record/Python-budget state, exact Busy deferral, catchable ordinary domain
errors, work-owned writer/permit/workspace, allowed partial scratch during
cleanup, no late delivered value/digest, scratch limits/no-overwrite, query
cancellation and sibling availability. Other nine findings remain closed;
CodeTool finding 7 remains withdrawn.

Do not add an arbitrary-Drop, whole-engine no-result timeout, abandonment,
lease-loss or shutdown-drain promise. Preserve those existing callers. No
blocking async Drop, runtime-thread join, rollback, journal, process termination,
new concurrency policy or TASK-002/003 work is authorized.

## Ordered closure scenario

### The invocation caller finishes work before publishing or retrying

**Behavior.** Through real Engine::attempt and AgentExecutor, the same held
bounded work must settle before retry/result whether the authored timeout arrives
while the Agent is executing or after an earlier host stop with work still active.
In the second phase, observe the Agent stop while tracked work remains active
before the authored timeout, and hold work across that timeout/progression
boundary. Do not require an inner cleanup wait after the fix: executors must
no longer have that capability. No new session/model/tool operation or completed
result may advance until release. For timeout-during-execution retain the
retry/no-retry outcomes. For an earlier host stop, preserve its captured
terminal/Busy classification after release; cleanup must not convert it into
a retryable timeout. Retain cancelled no-value records and reusable admission. Also exercise an Agent
requirement's success/error completion with started work still held: the engine
cannot publish its judgment, error classification or records before cleanup.
Fixture calls likewise retain an explicit caller-owned completion boundary.

**RED.** Extend the existing focused V1 regression with this missing phase.
Reuse HeldWork, real engine/AgentExecutor and existing start/release signals.
Synchronize phase order rather than add sleeps. On the candidate, an earlier
host stop reaches inner cleanup and the later authored timeout discards that
cleanup, allowing early progression. Record that failure. Retain the current
working timeout-during-execution cases. Check coverage protects the other
engine completion path when the Agent drain is removed; do not manufacture
RED for behavior that already works. Inspect the executor-facing capability:
it must lack settlement extraction/draining, rather than merely testing a
runtime check on a method it can still call.

**GREEN.** Retain one caller-owned cleanup capability and hand executors only
registration/recording/accounting access. Finish action/check cleanup in the
engine before progression/classification. The execution-timeout cases, early-stop
cleanup case, check completion and direct fixtures must preserve their respective
captured outcomes and the V2 regressions.

**REFACTOR.** Delete the child drain and redundant recovery/polling mechanics.
Keep one owning capability and the existing work/record producers; simplify
only while the same proofs stay green. No new framework.

## Acceptance and verification

| Obligation | Focused observation |
| --- | --- |
| FIND-TASK-001-2, REQ-012/AC-009 | Executor-facing handles cannot drain cleanup; the engine retains the owning capability outside the timed action and cannot retry/publish before registered work ends. |
| Same cause, retained R2 behavior | Execution-timeout cases retain retry/no-retry behavior; an earlier completed host stop retains its terminal/Busy result after cleanup. All cases retain cancelled no-value records and fresh capacity. |
| Adjacent consumers/non-goals | Engine action/check completion and direct fixture helpers finish their caller-owned work before classifying/exposing results; existing timeout/error/queue/budget and excluded no-result cancellation semantics remain. |

V1 — extend the confirmed existing selector and record RED/GREEN:

```bash
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::timed_out_attempt_settles_before_retry_or_result)'
```

Add focused check-completion coverage in the existing `blocks_agent` test
owner. Planned selector below; confirm its actual selector after adding it and
before recording any RED. Cover success and error outcomes as cases in the
same caller-owned lifecycle proof, not separate test harnesses.

```bash
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::agent_check_settles_before_result)'
```

V2 — retained focused regressions:

```bash
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::interrupted_blocking_work_keeps_its_permit) | test(=blocks_agent::admitted_waiters_survive_halt_without_dispatch) | test(=blocks_agent::agent_tool_budget_spans_attempts_without_refund) | test(=blocks_agent::agent_records_survive_failed_steps_and_inconclusive_checks)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::tools::pg_tests::extraction_settles_before_workspace_release) | test(=verification::hosted::tools::pg_tests::code_workspace_wait_obeys_operation_deadline)'
```

Run only the narrow owner lane needed for actual changed modules, recording
its exact command, plus:

```bash
mise run fmt:check
mise run lints
mise run check:deps
mise exec -- git diff --check
```

No full SDK journey suite, broad gate, benchmark, synthetic load or new harness
is required. Add another exact focused selector only for an actual affected
consumer. Append finding-to-source/proof evidence, exact results, RED diagnosis,
consumer tracing and non-goal confirmation here. Follow wyrd-implement's
unexpected-failure diagnosis rules; no weakened assertions or wider timeouts.

## Convergence boundary

R1 retained actual work guards but left the join inside a dropped session.
R2 retains the join during execution but removes it when cleanup begins.
Both incomplete fixes share the same ownership cause; the green tests omitted
cancellation of an in-progress join. The maintainer rejected retaining duplicate
draining capability with safer polling. The next review is limited to exclusive caller ownership, both timeout
cases and affected action/check/direct-fixture consumers. Carry all other
closures forward; do not restart broad discovery.

## Implementation evidence

Base: `9abeaf124` (runtime `66f4d54ca`); diff uncommitted on
`change/verifier-code-mode`.

### Owner, gap, extension

- **Owner:** the engine/`ToolCalls` boundary in `vala-verify` `blocks`.
  `HostedTools::open` is still the only producer of a settlement, and
  `Engine::attempt`/`judge` are the only production creators.
- **Gap:** `ToolCalls` was `Clone + Default` and exposed `settle`/`settled`.
  So `AgentExecutor::invoke` could pop the settlement inside the timed
  attempt, and the authored timeout then dropped it.
- **Extension:** `ToolCallsOwner` (`blocks/tools.rs`) is not `Clone`. It is
  the only constructor of the shared state and the only holder of `settle`
  and `settled`.
  - The engine keeps the owner. Each attempt receives `ToolCallsOwner::handle()`.
  - `ToolCalls` (the handle) now exposes no public method. Spending,
    recording and `defer` are private to `tools`.
  - `ToolSession::register_with` (renamed from `settle_with`) only defers.
  - `ToolCalls` has no `Default`, so the only way to get one is from an owner.
  - The executor's drain is deleted. Nothing new was added: no polling, no
    second guard, no dependency.

| Acceptance obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-2, REQ-012/AC-009: executor handles cannot drain; the engine owns cleanup outside the timed action | `ToolCallsOwner` + `ToolCalls` handle (`blocks/tools.rs`); drain removed from `AgentExecutor::invoke` (`blocks/agent.rs`); `attempt` keeps the owner outside `timeout_at` and settles before `timer.finish` and the retry/result decision (`blocks/engine.rs`) | V1 `Stopped` cases; API inspection: `ToolCalls` has no pub fn, no `Default`, and no path from a handle to the owner | PASS |
| Execution-timeout retry/no-retry retained | unchanged `timeout_at` classification | V1 `(1, Direct)` Succeeded with 2 attempts, reopened `[true]`; `(0, Direct)` Failed "timed out" | PASS |
| Earlier host stop keeps its captured Terminal/Busy, no retry | result captured before `settle` | V1 `(1, Stopped(Terminal))`: Failed, 1 attempt, own reason. `(1, Stopped(Busy))`: `EngineError::Busy("no worker")`, 1 open. Settlement survives the attempt-timeout instant; engine does not advance before release | PASS |
| Check completion settles before Busy/classification/records | `judge` captures `checks.execute`, then `settle`, then matches | `agent_check_settles_before_result` (Ok → Failed "judged"; Terminal stop → Inconclusive with own reason) | PASS |
| Cancelled no-value records, reusable admission | unchanged | V1/check `cancelled_extraction`; reopened `[true]` | PASS |
| Direct fixtures keep a caller-owned completion boundary | `blocks_agent` `step_recording`/`check`/inline check; `pg_hosted_artifacts` closures (async closures) and `HostedAgent::call`/`step` hold the owner and `settle` before returning. Callers with no tool session (Code/extract executors, `blocks.rs` unit test, `pg_hosted_files`) use `ToolCallsOwner::default().handle()` | V2, `pg_hosted_artifacts::` and `pg_hosted_files::` | PASS |

### RED / GREEN

- **RED.** I ran V1 extended with the `Stopped` phase against the HEAD runtime
  (the candidate's `src` restored, the new test in place).
  - The two `Direct` cases passed.
  - `(1, Stopped(Terminal))` failed with `the settlement was dropped while the
    work was held`. This is the inner drain being cancelled by the authored
    timeout, as the diagnosis predicted.
- **Check completion.** `agent_check_settles_before_result` passes on HEAD,
  because the inner drain covered checks. I did not manufacture a RED for it.
  - Mutation check: with R3 applied but the engine's check-path `settle`
    removed, it fails with `Returned: judged while the work was held`.
- **GREEN.**
  - V1: 1/1 passed (6.0s).
  - Check test: 1/1 passed.

### Fixture-authoring failures (new test only, before RED was valid)

Each of these was a defect in the new fixture. No product code or existing
assertion changed.

1. **Hang with no output.**
   - Cause: the `started` wait had no engine arm.
   - Cause: the `Direct` cases declared `code.run` with no code tool, so
     `resolve` refused before any work started.
   - Fix: both waits select on the engine, and `Direct` declares only
     `fs.extract`.
2. **`ended == 0` and the check failing with `DeadlineExceeded`.**
   - Cause: the test consumed (`forget`) the `started` permit that `DropHeld`
     peeks, so the program never saw it.
   - Fix: the test peeks too (drops the permit).
3. **The check was Inconclusive rather than Passed.**
   - Cause: the executor has no evidence reader, so no citation survives.
   - Fix: the success case answers a `failed` judgment, which is still an Ok
     executor return.

### Commands (all passed)

```bash
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::timed_out_attempt_settles_before_retry_or_result)'   # 1 passed
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::agent_check_settles_before_result)'                 # 1 passed
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::interrupted_blocking_work_keeps_its_permit) | test(=blocks_agent::admitted_waiters_survive_halt_without_dispatch) | test(=blocks_agent::agent_tool_budget_spans_attempts_without_refund) | test(=blocks_agent::agent_records_survive_failed_steps_and_inconclusive_checks)'  # 4 passed
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::tools::pg_tests::extraction_settles_before_workspace_release) | test(=verification::hosted::tools::pg_tests::code_workspace_wait_obeys_operation_deadline)'  # 2 passed
mise exec -- cargo nextest run --locked -p vala-verify --lib                     # 56 passed
mise exec -- cargo nextest run --locked -p vala-verify --test integration        # 21 passed
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^verification::/)'   # 65 passed
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(/^pg_hosted_artifacts::/) | test(/^pg_hosted_files::/)'  # 20 passed
mise run fmt:check && mise run lints && mise run check:deps && mise exec -- git diff --check
```

### Consumers traced and non-goals

- **Consumers.** Production creators of the invocation state: `Engine::attempt`
  and `Engine::judge` only.
  - The Hosted Blocks executor forwards the engine's `BlockCall` unchanged.
  - The sole settlement producer is `HostedTools::open`, through
    `ToolSession::with_settlement` → `register_with`.
  - The execution deadline, queued-runner drop, abandonment and shutdown paths
    are unchanged.
- **Non-goals.** All remained excluded: no Drop/abandonment/lease-loss/shutdown
  promise, no rollback, no process termination, no new concurrency policy, no
  dependency, no TASK-002/003 work.
- **Files changed:**
  - `vala-verify` `src/blocks{.rs,/tools.rs,/agent.rs,/engine.rs}` and
    `tests/integration/blocks_agent.rs`
  - `wyrd-server` `tests/integration/pg_hosted_{artifacts,files}.rs`
  - this evidence
  - `spec.md`, `spec-revisions/`, TASK-003 and the review folders remain the
    maintainer's uncommitted files, untouched by this remediation.
