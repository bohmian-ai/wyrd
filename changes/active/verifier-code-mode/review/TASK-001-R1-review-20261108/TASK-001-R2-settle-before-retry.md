---
id: TASK-001-R2
kind: remediation
status: ready
spec: SPEC-verifier-code-mode
spec_revision: 2
parent_task: TASK-001
remediates: [FIND-TASK-001-2]
depends_on: [TASK-001-R1]
---

# Settle started extraction before a timeout result or retry

## Outcome and authority

A Verifier Agent attempt that times out must stop execution and settle its
already-started bounded host work before the engine retries or produces the
attempt result. Keep the R1 guards, tracker, record and error fixes. This closes
the remaining part of FIND-TASK-001-2; it adds no new runtime or shutdown policy.

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode`.
- Approved authority: [spec revision 2](../../spec-revisions/revision-2.md),
  also available exactly at candidate `f8b45a720:changes/active/verifier-code-mode/spec.md`.
- Original [TASK-001](../../tasks/TASK-001-bounded-code-orchestration.md).
- Prior candidate: `0e51af2a2332f362ab44b7c5b88b51c3cb4f8011`.
- Reviewed cumulative candidate: `f8b45a7208dbc3e8ec6c3cdac3605f6f84b6cc18`;
  runtime source is `0393740405bb47ab95ff1a404a20be44597fbdeb`.
- [R1 correction and evidence](../TASK-001-first-review-20261108/TASK-001-R1-restore-execution-and-evidence.md),
  [validated diagnosis](findings-validation.md), [verdict](verdict.md).

The implementer needs this task, the original task and approved spec. Review
reports support the diagnosis below; they do not contain hidden requirements.
Git actions remain caller-owned. Preserve the reviewed candidate and implement
on the caller's subsequent candidate.

## Diagnosis

REQ-012 and AC-009 require started bounded extraction to settle before
invocation cleanup completes. R1 repaired premature resource release:
`Invocation::blocking` in server `verification/hosted/tools.rs:304–317` moves
the owned writer and active-operation permit into a tracked blocking closure;
the closure also retains its Workspace. `HostedTools::open:139–179` places
`work.close(); work.wait().await` in ToolSession. AgentExecutor normally awaits
that session's close at Vala `blocks/agent.rs:410–422`.

The remaining producer is the engine's authored attempt timeout. Vala
`blocks/engine.rs:658–702` wraps `actions.execute(call)` in `timeout_at`.
Expiry drops AgentExecutor, including its session or an in-progress close,
then immediately treats the attempt as retryable timed out. The engine retries
or produces Attempted/settled records without another settlement await.
ToolSession's settlement future (`blocks/tools.rs:242–250`) is owned by the
future that was just destroyed. TaskTracker does not join work when dropped.

A supported Agent attempt can therefore start `fs.extract` and time out while
the bounded blocking operation still runs. With a retry, the new attempt opens
a new session/semaphore while old work still owns the previous permit; cap 1
can admit new work alongside the old extraction. Without a retry, a failed
step can be produced while scratch mutation and workspace occupancy continue.
This violates the original cleanup requirement despite preventing premature
directory deletion or writer release.

The new owner extraction proof manually closes/waits the tracker. The Vala
permit proof drops a child while the enclosing Agent remains alive and releases
work before it ends. Both are useful and pass, but neither drives the real
engine timeout that discards session settlement. Their relocation is accepted;
the missing consumer-level observation is what remains.

## Selected correction and reuse

Complete the existing engine/attempt-to-ToolSession/Invocation lifecycle
connection. The existing owner applying the attempt timeout must retain access
to settlement of that attempt's started host work beyond destruction of its
timed Agent future. Stop execution/new dispatch, await the existing tracked
bounded work, then apply the established timeout classification and retry or
Attempted settlement. Reuse TaskTracker, ToolSession/Invocation and the engine's
existing attempt lifecycle. Private integration and helper choices remain
implementation-owned.

Retain the owned writer/permit/workspace guards. Partial scratch may still be
published while bounded extraction finishes during cleanup; rollback is not
required. Moving guards again, extending deadlines, adding a late result guard
or spawning unowned cleanup cannot repair the timeout owner's early progression.
The correction is one bounded lifecycle connection, not a replacement extractor,
scheduler, transport, journal or cleanup service.

Inspect all consumers of changed lifecycle state, including the engine's
execution deadline and queued runner's abandonment/lease-loss select, to
preserve their semantics. This finding does not separately require arbitrary
future Drop to block, a result after caller abandonment or a new shutdown-drain
contract. Do not generalize this fix into those additions.

## Preserved behavior and non-goals

Keep retryable timeout identity, authored retry counts and final failed-step /
inconclusive-check behavior; ToolCalls accounting/records across retries;
retry-spanning hosted Python budget; exact Busy deferral without a subject
judgment; catchable ordinary domain errors; query cancellation on drop;
no late returned value/digest; scratch limits/no-overwrite; healthy sibling
service availability. Existing no-result cancellation and queue lease safety
remain. No general Skald repair change, blocking async Drop, synchronous runtime
thread wait, new dependency/feature/public contract or TASK-002/003 work.
Other nine closed findings stay closed; ID 7 remains withdrawn.

## Ordered closure scenario

### A timed-out attempt finishes cleanup before it advances

**Behavior.** Through real AgentExecutor and Engine::attempt, hold started
tracked blocking work across an authored attempt deadline earlier than the
whole-execution deadline. No retry/new-session operation, later model/tool
request or produced failed-attempt result may occur before release. After
release, cleanup completes and the existing timeout policy proceeds. Cover
retry and no-retry as equivalent cases: retry starts only after settlement;
no-retry produces the existing failed step. The interrupted call keeps its
cancellation/no-value evidence and fresh work can acquire released capacity.

**RED.** Add the planned focused engine regression in V1, confirming its actual
selector after adding it and before recording RED. Reuse the existing
`blocks_agent` engine/held-work seam, real AgentExecutor, ToolSession settlement
and deterministic start/release synchronization. The current candidate should
advance the retry or return the failed result while work is held. Record that
failure; do not assert a new type or rely on timing sleeps as synchronization.

**GREEN.** Connect the existing termination and settlement owners as selected
above. Pass both cases and the retained guard/permit regressions in V2. Prove
no mutation or delivered value after the completed cleanup boundary and
recovered admission. Existing error/result semantics must remain.

**REFACTOR.** Keep one settlement owner; remove duplicated joining or downstream
checks only when the same proofs stay green. Preserve all retained R1 behavior.

## Acceptance and focused verification

| Finding / obligation | Required observation |
| --- | --- |
| FIND-TASK-001-2; REQ-012/AC-009 | Real engine attempt timeout cannot produce a result or retry until started bounded host work settles; both retry/no-retry cases exercise the consumer. |
| Same cause, shared active cap / records | Old and new attempts do not overlap started non-code work; interrupted-call records remain cancelled with no delivered digest/bytes; capacity is reusable after cleanup. |
| Preserved callers/non-goals | Source tracing and focused affected regressions retain execution deadline, no-result cancellation, queue safety, budgets and ordinary failure behavior. No new drop/shutdown promise or unrelated machinery. |

### V1 — New consumer regression

Planned name in the confirmed existing `vala-verify` integration target;
confirm actual selector after adding it, before RED. The implementer owns the
smallest fixture organization.

```bash
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::timed_out_attempt_settles_before_retry_or_result)'
```

### V2 — Retained affected regressions

```bash
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::interrupted_blocking_work_keeps_its_permit) | test(=blocks_agent::admitted_waiters_survive_halt_without_dispatch) | test(=blocks_agent::agent_tool_budget_spans_attempts_without_refund) | test(=blocks_agent::agent_records_survive_failed_steps_and_inconclusive_checks)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::tools::pg_tests::extraction_settles_before_workspace_release) | test(=verification::hosted::tools::pg_tests::code_workspace_wait_obeys_operation_deadline)'
```

Run the existing narrow Vala/server owner lane only for the resulting changed
modules; record its exact command. If lifecycle edits affect another consumer,
add its focused selector and explain the affected property. No full SDK journey
suite, broad gate, benchmark or synthetic host load is required.

```bash
mise run fmt:check
mise run lints
mise run check:deps
mise exec -- git diff --check
```

Append a compact finding-to-implementation/proof table, exact commands/results,
RED/GREEN evidence, affected-caller tracing and non-goal confirmation here.
Unexpected failures follow wyrd-implement's trace/diagnosis rules; no test
weakening, timeout widening or undocumented assertion changes.

## Stop conditions

A correction requiring new approved behavior, public/persisted contract,
concurrency policy or architectural boundary returns SPEC_REVISION_REQUIRED.
Ordinary private lifecycle integration is implementation-owned. Do not replace
the explicit result/retry guarantee with eventual guard release or broaden it
into a new arbitrary-Drop/shutdown service requirement.

## Implementation evidence

Base: `f8b45a720` (uncommitted working-tree diff; git actions caller-owned).

Existing owner and gap: `ToolCalls` is the one engine-owned state that
outlives every attempt of a step (`Engine::attempt` creates it and settles it
into `Attempted`), but the session settlement lived only in the `ToolSession`
owned by the timed Agent future, so `timeout_at` dropped it unawaited. The
correction moves that same settlement into `ToolCalls` when the session opens
(`ToolSession::settle_with`), the Agent executor awaits `ToolCalls::settle`
on normal completion (replacing `ToolSession::close`), and `Engine::attempt`
awaits it after every attempt before timeout classification, retry or
`Attempted`. One settlement owner; no new type, runtime, setting, or guard.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-2; REQ-012/AC-009: a real engine attempt timeout cannot retry or produce a result until started host work settles (retry and no-retry) | `ToolCalls::settle`/`defer` and `ToolSession::settle_with` (`vala-verify/src/blocks/tools.rs`); `session.settle_with(calls)` at open and `calls.settle().await` before release (`blocks/agent.rs`); `tool_calls.settle().await` after each attempt (`blocks/engine.rs` `attempt`) | V1 `blocks_agent::timed_out_attempt_settles_before_retry_or_result`: with work held past `timeout_seconds: 1`, the engine polls the settlement before any second session or result; retry case reopens only after the work ended and succeeds in 2 attempts; no-retry case fails "timed out" with the model script spent | PASS |
| Old and new attempts never overlap started work; interrupted call cancelled with no digest/bytes; capacity reusable | same | V1 `reopened == [true]`, one `FsExtract` record cancelled with `(None, 0)`; V2 `interrupted_blocking_work_keeps_its_permit`, `extraction_settles_before_workspace_release` | PASS |
| Preserved callers and non-goals | engine timeout classification, retry count, `Busy` early return, check path and execution deadline unchanged; `HostedTools::open` settlement unchanged | V2 `admitted_waiters_survive_halt_without_dispatch`, `agent_tool_budget_spans_attempts_without_refund`, `agent_records_survive_failed_steps_and_inconclusive_checks`, `code_workspace_wait_obeys_operation_deadline`; owner lanes below | PASS |

RED (runtime source restored to `f8b45a720`, test unchanged): retry case
failed "retries 1: no new session before settlement" (the retry opened a
second session while the first extraction was held); with the cases
reordered, the no-retry case failed "retries 0: advanced while the work was
held" with a produced `Failed`/"timed out" outcome. GREEN with the fix.

Commands (all PASS):

- V1/V2 selectors above (`vala-verify` integration 5/5; wyrd-server lib 2/2
  under `scripts/postgres/with-test-postgres.sh`).
- `mise exec -- cargo nextest run --locked -p vala-verify --lib` (56) and
  `--test integration` (20).
- `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^verification::/)'` (65)
  and `--test integration -E 'test(/^pg_hosted_artifacts::/)'` (15): the
  hosted consumer of the changed Agent session lifecycle.
- `mise run fmt:check`, `mise run lints`, `mise run check:deps`,
  `mise exec -- git diff --check`.

Affected callers traced: `HostedTools::open` is the only producer of a
session settlement and is unchanged. Check executions have no per-attempt
timeout and settle on the Agent's normal path. The execution deadline and
the queued runner's abandonment/lease-loss select drop the whole engine
future as before, with no new drop-time or shutdown promise. A settlement
await is bounded by the host's bounded blocking work.

Non-goals stayed excluded: no Skald change, blocking `Drop`, runtime thread
wait, dependency, feature, public contract, late-result guard or TASK-002/003
work. Changed files: `vala-verify/src/blocks/{tools,agent,engine}.rs`,
`vala-verify/tests/integration/blocks_agent.rs` (V1, plus a `finished`
signal on the `HeldWork` double), and this evidence.
