---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-verifier-code-mode
spec_revision: 2
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6, FIND-TASK-001-8, FIND-TASK-001-9, FIND-TASK-001-10, FIND-TASK-001-11]
depends_on: []
---

# Restore bounded execution and honest code-mode evidence

## Outcome and authority

Complete TASK-001's already-approved behavior: failed or host-stopped Agent
Python cannot produce a later model judgment; cleanup settles its bounded
work; admitted calls accurately record delivery; valid JSON integers remain
lossless. Correct the rustdoc and recovery-proof gaps
in the same remediation. This is one bounded correction to existing owners,
not a new design plan or permission to implement TASK-002.

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode`.
- Approved authority: [specification revision 2](../../spec.md).
- Original task: [TASK-001](../../tasks/TASK-001-bounded-code-orchestration.md).
- Original base: `5234961d51798581a026834cfffd5efaadc226b4`.
- Reviewed candidate: `0e51af2a2332f362ab44b7c5b88b51c3cb4f8011`.
- Independent diagnosis/ledger: [findings-validation.md](findings-validation.md).
- Review verdict: [verdict.md](verdict.md).

The implementation agent can execute this task with the approved spec and
repository. The explanations below supply the diagnosis and selected
correction; the review reports provide supporting evidence, not hidden
implementation instructions. Revalidate private names against current source.
Keep the review's original subject unchanged; implementation occurs on the
caller's selected subsequent candidate. Git commits/pushes remain caller-owned.

## Diagnosis and selected correction

Paths below use `Vala` for `crates/vala/vala-verify`, `hosted` for
`crates/wyrd/wyrd-server/src/verification/hosted`, and `hosted tests` for
`crates/wyrd/wyrd-server/tests/integration/pg_hosted_artifacts.rs`.
Line numbers refer to the reviewed candidate, not a future remediation tree.

### Stop the affected invocation, preserving ordinary Python recovery

**FIND-TASK-001-1.** `hosted/tools.rs:302–334` preserves typed Busy but maps
other Sandbox execution failures to ordinary cause-less tool errors. Time
exhaustion from its Reservation takes the same ordinary path. `Sandbox::exec`
produces timeout/program failure at `sandbox.rs:389–395`; Vala
`blocks/tools.rs:409–420` does not raise Halt for those failures. Skald then
returns the error to the model and permits a final answer. This is observable
in the current public failure journey: it asserts a succeeded step whose
outer record contains a time-limit error. The hosted wait test also explicitly
expects the model to continue. An uncaught nested denial follows the same
path. Those assertions contradict the approved failed-step/inconclusive-check
and noncatchable-host-stop requirements, despite passing.

Preserve generated execution failure/time-stop identity through the existing
Invocation/Sandbox → session Halt → AgentExecutor/engine boundary. Include
operation timeout, cumulative Python exhaustion and uncaught generated program
failure. A domain error caught *inside* Python remains recoverable; an uncaught
program error must terminate this invocation. Leave general Skald repairable
tool behavior and unrelated invocations/services unchanged. Retain completed
and cancelled evidence; do not rewrite a successful result downstream to mask
the producer. Correct the misleading timeout-success assertions. [Proof P1](#p1--failed-or-stopped-execution).

**FIND-TASK-001-11.** Real Oracle `QueryQueueFull` is a retryable pod-local
pre-framing capacity refusal (`vala-bifrost-redux/src/oracle/admission.rs:913–925`;
`wyrd-spec/src/vala/error.rs:82–94`). BoundedQuery propagates it, but hosted
`tools.rs:241–253` projects it as StructuredInvocation. Halt only recognizes
an Invocation cause downcasting to BlockError::Busy. Consequently Python can
catch actual process overload and the model can judge; a submitted execution
never reaches existing engine Busy → Deferred → released/refunded claim.
Worker/workspace Busy proof exercises a different error variant and cannot
close this gap.

Extend the existing shared `Halt::observe` to classify the exact structured
catalog identity projected from `BifrostError::QueryQueueFull` as its existing
Busy control outcome, while leaving the structured problem and safe record
metadata intact. Use the concrete catalog variant's projected code, never
human prose or all 429/503 errors. Do not use `WyrdError::from_code` to recreate
delegated Vala errors; that mapper expressly excludes them. Preserve Oracle
fairness, permission errors, invalid queries and intrinsic admitted
`QueryResourcesExhausted` as their existing ordinary domain failures. No new
queue, public error contract or generalized cause mechanism is needed.
[Proof P6](#p6--capacity-stop-and-real-recovery).

### Settle existing workspace work and bound setup waits

**FIND-TASK-001-2.** Extraction acquires the writer in its cancellable async
caller and awaits a blocking closure that owns only Workspace/arguments
(`hosted/tools.rs:263–277`). Dropping that await detaches already-started work
and releases both writer and Dispatch active-operation permit. The closure
can still check room/persist into scratch (`workspace.rs:675–770`). Its Arc
keeps the directory/admission alive but neither joins work nor serializes
mutation. Invocation cleanup can finish while extraction continues, and a
sibling can then mutate against its unfinished room calculation.

Retain mutation and active-operation ownership until the real bounded work
settles, and integrate settlement into existing hosted attempt/workspace
cleanup before declaring cleanup complete or releasing the workspace. Reuse
Workspace/extractor/session ownership. A started extraction may finish and
publish ordinary partial scratch *during awaited cleanup*: no rollback is
promised. Require no mutation or late tool value after cleanup completes.
Moving only a guard or aborting a started blocking JoinHandle is insufficient;
do not block an async runtime thread or add a second extraction service.
[Proof P2](#p2--interrupted-blocking-extraction).

**FIND-TASK-001-3.** `Invocation::code` reserves time and computes its deadline,
then awaits writer/mount setup outside timeout enforcement (`tools.rs:304–327`).
A supported concurrent extraction can hold that writer longer than the code
operation allowance while the invocation deadline is still later. Accounting
elapsed time afterwards does not bound the wait; a late checkout can even
misclassify this elapsed code operation as worker-readiness Busy.

Apply the already-computed operation/invocation deadline to initial workspace
coordination/setup as well as Sandbox execution. An unstarted waiter expires
without a worker or child dispatch and propagates the time stop above. Started
blocking setup follows the same bounded settlement rule, not detachment.
Preserve existing reservation arithmetic/retry charging and genuine capacity
Busy; introduce no setting or scheduler. [Proof P3](#p3--workspace-wait-deadline).

### Account for admitted work and actual recipient delivery

**FIND-TASK-001-4.** Dispatch validates name/argument admission, then awaits its
semaphore before allocating the ToolCalls slot (`blocks/tools.rs:491–519`).
At cap 1, a second valid gathered child can be cancelled while waiting and
leave no record, even when the engine produces a failed result. It spent
budget and passed the spec's admission boundary; unfinished-record settlement
cannot recover a slot never created.

Start the existing bounded record after successful declaration/argument
validation and before the capacity wait. Reuse unfinished cancellation
settlement and engine result handoff. Preserve no leaf record for rejected
name/arguments, atomic spending without refunds, flat admission-start order,
and actual active-operation limits. [Proof P4](#p4--admitted-waiters-and-post-stop-dispatch).

**FIND-TASK-001-6.** Halt is checked at spending but not after the semaphore
suspension (`blocks/tools.rs:453–464,502–540`). With budget 3/cap 1, outer code
spends 1, A spends 2/holds capacity, B spends 3/waits, then a malformed fourth
request synchronously raises exhaustion while building the request future.
The inner Sandbox driver can complete A and invoke B in that same cooperative
poll before the outer Agent select regains control. Spending stays capped,
but protected work/audit starts after terminal refusal.

Enforce the existing Halt again at the shared actual execution boundary after
capacity acquisition and before capability invocation; stop suspended waiting
on Halt where needed. That boundary owns admission after a suspension, so
this guard belongs there. Preserve first stop identity, final-allowance success
and the admitted cancellation records above. No second spend, outer-select
bias workaround or per-capability guard. [Proof P4](#p4--admitted-waiters-and-post-stop-dispatch).

**FIND-TASK-001-5.** Dispatch finalizes successful digest/bytes before returning
JSON (`blocks/tools.rs:537–565`); only later do `answer` and `resume_futures`
convert/publish it (`sandbox.rs:489–510,361–385`). The current u64 conversion
failure leaves a false success record, but fixing numeric range alone is not
enough: an ordinary fs.list future can settle into the parent's `answered`
while Python performs independent local/mounted work before awaiting it.
A local failure/return before ResolveFutures drops that value without
publication. The already-finished record still says Python received it.
This supported pending-future path was independently validated; no unproved
deep-Card producer is needed.

Keep nested success pending until the existing driver publishes the bounded,
converted value at its resumption boundary. Use the existing record completion/
unfinished cancellation semantics for conversion failure or exit before
publication. An earlier successfully published child must retain its digest
when a later program error occurs; do not clear all successes on failure.
Preserve ordinary child failure records and the direct-call path. No worker
acknowledgement protocol, raw-result store, journal or requirement that Python
inspect a resumed value. [Proof P5](#p5--bridge-delivery).

**FIND-TASK-001-10.** The converter restricts integers to signed i64
(`sandbox.rs:722–724,799–803`), and its unit test incorrectly requires refusal
for u64::MAX (`:900–902`). The existing `ParameterValue::Json` contract uses
serde_json values and supports i64::MIN through u64::MAX, distinct from the
signed Int variant. Authorized cards.get of an Experiment with a structured
unsigned seed succeeds directly but fails inside Python. This is meaningful
supported data, not merely a huge pagination offset.

Extend only the existing two integer conversion arms using installed Monty
BigInt/object/IPC support for the unsigned portion, with exact checked
conversion back to i64 or u64. Preserve signed endpoints, finite floats,
collection/key/cycle/depth/byte limits and refusal below i64::MIN/above
u64::MAX. Do not stringify, round, add a dependency/codec, change Card types
or broaden arbitrary-precision JSON. Correct the test's rejected-u64 premise.
[Proof P7](#p7--lossless-existing-wire-integers), plus the real Card case in P5.

### Close bounded repository and recovery-proof gaps

**FIND-TASK-001-8.** Mandatory changed-item rustdoc is missing on fallible or
panicking production/private/test items: hosted `to_python` (`sandbox.rs:791`)
lacks Errors; `ToolCalls::finish` (`blocks/tools.rs:172`) lacks the slot panic
invariant; Vala test `run_engine`, `bound`, `query_call`, `code_call`;
hosted test `CodeAgent::call`; SDK test `leaves` and `tools_for` lack relevant
panic sections. Complete these and the same omission on other newly/materially
changed items in this bounded correction. State actual errors/named invariants;
add no defensive runtime logic, untouched-item cleanup or prose-only tests.
Batch this static correction with the runtime work.

**FIND-TASK-001-9.** The capacity test leaves `{}` queued after Busy, appends
its recovery program behind it, then invokes once (`hosted tests:1176–1185`).
MockProvider is FIFO; Skald accepts the front final answer without running any
tool. The assertion that the same program ran is therefore false, even though
the test is green. Align the existing script phases so recovery consumes the
program and final answer, then assert observed child execution/results or
record/provider consumption. Reuse the fixture/provider/server, without new
runtime code, sleeps, retry or timeout expansion.
[Proof P6](#p6--capacity-stop-and-real-recovery).

## Ordered correction and TDD

Use small RED/GREEN/REFACTOR increments within these user outcomes; do not
batch all RED tests or create a task per finding:

1. A user sees failed/inconclusive execution after uncaught program failure or
   a host time stop, and Busy/deferred work after actual process overload.
   Correct existing success-after-timeout assertions first; preserve ordinary
   Python-caught permission/domain error recovery.
2. An operator interrupts work and observes bounded settlement plus subsequent
   capacity recovery. Add deterministic extraction and held-writer deadline
   observations at existing owner seams; no host load or timing sleeps.
3. A user receives lossless valid data and honest start-ordered records even
   when work waits, is halted or never reaches Python. Test one missing
   admission/publication/integer observation at a time and rerun earlier
   affected cases after each change.
4. Keep the rustdoc and capacity fixture corrections green as the
   related runtime fixes land. Static/rustdoc changes need no manufactured RED.

Private signatures, lock representation, helper/module/fixture shape and
mechanical consumer repair remain implementation choices inside these owners.
Material stop: any need for new public/wire/persisted behavior, dependency,
feature, authorization policy, runtime/transport/storage owner or weaker proof
returns to planning/specification authority. This task selects none of those.

## Acceptance and focused evidence

| Finding | Required closing observation | Proof |
| --- | --- | --- |
| FIND-TASK-001-1 | Uncaught child error after a delivered result and held-query timeout fail action / make check inconclusive; no later model/tool work or false failed-subject judgment; records survive; fresh invocation succeeds. | P1, P5 |
| FIND-TASK-001-2 | Interrupt extraction after start: cleanup, writer and active-operation ownership remain until bounded settlement; no work/value after cleanup; workspace/capacity can be reused. Normal no-overwrite/shared-budget behavior preserved. | P2; affected workspace regression |
| FIND-TASK-001-3 | Held writer outlasts a shorter code-operation deadline: unstarted code expires on time without worker/child/model work; elapsed time is not replenished; started setup settles safely. | P3, P1 |
| FIND-TASK-001-4 | At cap 1, active and validated waiting calls appear once in admission order on produced failed/inconclusive results; waiter has bounded args/intended recipient/safe cancellation, zero bytes/no digest; rejected input/name still has no leaf record. | P4 |
| FIND-TASK-001-5 | A settled ordinary child not yet published when Python fails/returns has no delivered-value digest/bytes; successful await and delivered child preceding later failure retain correct digest. | P5 |
| FIND-TASK-001-6 | Raise exhaustion after B has spent but waits behind A; release A within inner driver polling. B never invokes, count remains capped, B cancellation remains, no model work follows. | P4 |
| FIND-TASK-001-8 | New/materially changed Rust items document actual intent, invariants, errors and panics, including touched test/private helpers; no runtime churn for docs. | Static changed-item audit; format/lint |
| FIND-TASK-001-9 | Capacity recovery actually executes the scripted program/child and consumes the intended response sequence, not only an empty final answer. | P6 |
| FIND-TASK-001-10 | Both conversion directions exactly preserve i64::MIN..u64::MAX, including nested values; reject outside the range without rounding/stringification; valid Card read works through Python. | P7, P5 |
| FIND-TASK-001-11 | Exact catalog QueueFull is uncatchable Busy for direct and nested origins, no later model/child work, fresh success; normal domain/resource error stays catchable; queued work defers/refunds rather than judges. | P6 and unchanged real Oracle producer proof |

Run from the repository root via `mise`, Cargo commands sequentially. Use
`WYRD_LOG=info,vala_verify=debug,wyrd_server=debug` for diagnosis. Existing
Postgres wrapper supplies the local setup. Build the actual server worker
with `mise run build:server` before public/real-worker proof; never infer an
executable path. Tests marked **planned** below use confirmed existing packages,
targets and setup; confirm their actual selector after addition and before RED.
Record any non-weaker selector correction. Extend existing coverage before
adding a new test. A passing zero-test filter or setup/compile failure is not
behavioral evidence.

### P1 — Failed or stopped execution

Existing deterministic hosted and public tests, correct and extend assertions:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::code_host_waits_obey_deadlines_and_cancellation)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::code_mode_failure_releases_session)'
```

### P2 — Interrupted blocking extraction

**Planned** original-task proof; deterministic owner start/settlement barriers:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::code_extraction_settles_before_workspace_release)'
```

### P3 — Workspace wait deadline

**Planned** held-writer operation-timeout proof, with later invocation deadline:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::code_workspace_wait_obeys_operation_deadline)'
```

### P4 — Admitted waiters and post-stop dispatch

**Planned** deterministic combined boundary case plus existing result/retry
regressions, with separate assertions for cancellation and post-halt dispatch:

```sh
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::admitted_waiters_survive_halt_without_dispatch)'
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::agent_records_survive_failed_steps_and_inconclusive_checks) | test(=blocks_agent::agent_tool_budget_spans_attempts_without_refund)'
```

### P5 — Bridge delivery

Extend these existing proofs for honest publication, uncaught failure and
valid unsigned Card data; keep ordinary caught domain failures repairable:

```sh
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::code_bridge_shares_the_recorded_path)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::code_bridge_preserves_declared_tool_boundaries)'
```

### P6 — Capacity stop and real recovery

Extend existing adapter/session and queued recovery proof for the exact
catalog QueueFull projection, direct/nested catch-all Python, sibling settlement
and correct FIFO recovery script. Keep an ordinary QueryResourcesExhausted or
permission-denial control case to prove no broad error reclassification.
Existing real Oracle producer tests establish the distinct capacity variant;
these focused cases prove it joins the existing Busy/defer/refund sink. No
synthetic host load or new harness is needed.

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::capacity_refusal_is_busy_at_every_hosted_consumer) | test(=pg_hosted_artifacts::busy_queued_execution_is_deferred_until_capacity_frees)'
```

### P7 — Lossless existing wire integers

Extend the existing pure test with signed endpoints, i64::MAX+1, u64::MAX,
nested objects, exact-byte boundaries and checked refusal outside the range:

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::sandbox::tests::code_bridge_json_conversion_is_lossless_and_bounded)'
```

After focused proof, run only affected owner/module checks required by the
actual write set and root rules, plus `mise run fmt:check`, `mise run lints`,
`mise run check:deps` and `mise exec -- git diff --check`. If generated sources
actually change, regenerate from their owner and run `mise run codegen:check`;
never hand-edit or bypass a red check. Record the source/trace diagnosis for
any failure-clearing test/assertion change. Do not run whole SDK journey suites,
platform matrix or `gate` for this remediation; integrated change review owns
the broad candidate checks.

## Preserved behavior and non-goals

Preserve the existing callable/declared catalog, accepted tenant/delegation,
exact tool plus downstream permissions, non-blocking audit, ordinary caught
catalog failures, request ceiling/final allowance, shared non-code cap, flat
records, pinned citations, additive historical/custom-data decoding, same pool
and portable interpreter/allocator/mount isolation. Keep already-delivered
values recorded through later failure and keep no-result cancellation without
a new journal/result guarantee.

Keep the existing `CodeTool` trait: its finding was withdrawn, and replacing
it with a callback is not part of this remediation. The remaining finding
IDs retain their numbering; ID 7 is reserved as withdrawn.

No migration, dependency/feature, new service/store/transport/runtime, worker
credentials/network/package access, raw result retention, rollback, generic
Oracle authorization overhaul, UI flow or TASK-002 language work. Do not add
a verification outcome-disable flag: the rejected proof assumption confuses
requirement `actual` ContextCapture with an outcome mode that does not exist.
Task-private proof wording can be corrected through `wyrd-plan`; it is not a
new implementation feature or a retained finding. Do not claim universal
elevated payload enforcement from sensitive metadata alone.

## Delivery evidence

Map each retained FIND ID to source and exact focused proof; record expected
behavioral RED/GREEN and affected earlier regressions, static rustdoc
closure, final commands and bounded corrections. Report remaining mandatory
failures honestly. Route implementation to `$wyrd-implement`; review performs
no implementation. A subsequent `$wyrd-task-review` checks these finding IDs,
the prior-candidate-to-new-candidate diff and affected paths while retaining
the original task/spec and full cumulative context. It does not restart the
whole original discovery without new material evidence.

## Implementation evidence

Candidate: `0e51af2a2..039374040` (production fix `b0362b781`; proofs
`f7fa4243d`, `e36e90139`, `523981842`, `aceae4b27`, `f6260d23c`,
`6053e4c11`; rustdoc `039374040`).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-1 uncaught/timed-out code fails the step, no later model/tool work, check inconclusive, records survive, fresh run succeeds | `Halt::observe` raises `Stop::Block` for any typed `BlockError` cause (`vala-verify/src/blocks/tools.rs`); hosted `host_stop` types every code stop (`hosted/tools.rs`) | P1 `code_host_waits_obey_deadlines_and_cancellation`, `verifier_building_blocks::code_mode_failure_releases_session` (step Failed, requirement Inconclusive, verdict not Failed, one model request); P5 `code_bridge_preserves_declared_tool_boundaries` (uncaught `ValueError` after a delivered read: Terminal, one model turn) | PASS |
| FIND-TASK-001-2 interrupted extraction keeps cleanup, writer and permit until settled; workspace reusable | `Invocation::blocking` on the attempt `TaskTracker`, holding writer and `hold_permit()` inside the blocking closure; `ToolSession::with_settlement`/`close` awaited by the Agent executor | P2 `verification::hosted::tools::pg_tests::extraction_settles_before_workspace_release` (writer locked, work unsettled, dir and admission held until release; fresh workspace extracts) and `blocks_agent::interrupted_blocking_work_keeps_its_permit` (cap 1: next read undispatched until release; dropped call cancelled); regression `code_calls_share_workspace_mutations` | PASS |
| FIND-TASK-001-3 held writer bounded by the code deadline, no worker, time not replenished | writer wait and mount setup under `timeout_at(min(attempt, reservation))` in `Invocation::code` | P3 `verification::hosted::tools::pg_tests::code_workspace_wait_obeys_operation_deadline` (stops at ≥300 ms, <5 s, idle sandbox never reached, second call gets the remainder, third budget spent); P1 retry "budget is spent" | PASS |
| FIND-TASK-001-4 admitted waiters recorded once, in order, cancelled with no digest | `Dispatch::call` records start before `permit()`; cancellation settles through `ToolCalls::settled` | P4 `admitted_waiters_survive_halt_without_dispatch`, `agent_records_survive_failed_steps_and_inconclusive_checks` | PASS |
| FIND-TASK-001-5 settled-but-unpublished child has no digest; delivered child keeps it | `Delivery` finishes the record only on `deliver`; sandbox `ResolveFutures` delivers just before `resume_futures`, `answer` refuses unconvertible values | P5 `code_bridge_shares_the_recorded_path` (dropped delivery: cancelled, no digest/bytes; received reads keep digests); hosted P5 (delivered read keeps digest, never-awaited list never recorded as returned) | PASS |
| FIND-TASK-001-6 exhaustion while B waits: B never invokes, count capped, no model work | `Dispatch::permit` selects on `halt.raised()` and rechecks after acquisition | P4 `admitted_waiters_survive_halt_without_dispatch` (RED confirmed before the fix), `agent_tool_budget_spans_attempts_without_refund` | PASS |
| FIND-TASK-001-8 rustdoc on new/changed items, including test helpers | `# Panics` on `run_engine`, `bound`, `query_call`, `code_call`, hosted `CodeAgent::call`, pg `invocation`, SDK `leaves`, `tools_for`; every new item documented | changed-item audit over `0e51af2a2..` (only false positive: `answer` returns `Option`); `mise run fmt:check`, `mise run lints` | PASS |
| FIND-TASK-001-9 capacity recovery runs the scripted program and child | test script order corrected | P6 `capacity_refusal_is_busy_at_every_hosted_consumer` (records `[CodeRun, FsRead]`, script fully consumed) | PASS |
| FIND-TASK-001-10 exact i64::MIN..u64::MAX both ways; refusal outside; real Card through Python | `json_from_python` BigInt arm (checked `i64`/`u64`), `to_python` via `python_u64` (`hosted/sandbox.rs`) | P7 `code_bridge_json_conversion_is_lossless_and_bounded`; P5 hosted Experiment `seed: u64::MAX` read by `cards.get` in Python equals `2 ** 64 - 1` | PASS |
| FIND-TASK-001-11 catalog QueueFull is uncatchable Busy at every origin; resource exhaustion stays catchable; queued work defers | `Halt::observe` maps `problem.code == BifrostError::QueryQueueFull.code()` to a Busy stop | P6 `blocks_agent::oracle_queue_refusal_is_busy_at_every_origin` (direct and caught nested → `EngineError::Busy`; `QueryResourcesExhausted` control catchable), `busy_queued_execution_is_deferred_until_capacity_frees` | PASS |

RED/GREEN: P4 and P6 Vala proofs failed before `b0362b781`; the P2 hosted
proof failed with untracked `spawn_blocking` that released the writer at once
("/work stays locked"), the P2 Vala proof failed with `hold_permit` returning
`None` ("the work holds the permit"), and P3 failed after 30 s with the wait
bounded by the attempt deadline; each restored source passed. P1/P5/P6 hosted
and SDK assertions were corrected to the fixed contract (failed step, no
answered stop) and pass.

Commands (all PASS): the P1–P7 selectors above; `cargo nextest run -p
vala-verify --lib` (56) and `--test integration` (19); wyrd-server lib
`test(/^verification::hosted::/)` (22) and integration
`test(/^pg_hosted_artifacts::/)` (15) under `with-test-postgres.sh`;
`mise run fmt:check`, `mise run lints`, `mise run check:deps`,
`mise exec -- git diff --check 0e51af2a2`. No generated source changed.

Selector corrections (not weaker): P2 and P3 are owner `pg_tests` in
`verification::hosted::tools` rather than `pg_hosted_artifacts`, because only
the owner can hold real extraction after it starts or hold `/work` without a
new hook; P2's permit half is `blocks_agent::interrupted_blocking_work_keeps_its_permit`,
since the permit hand-off belongs to Vala dispatch. The model-turn half of
P3 is covered by P1's hosted time-limit stop.

Limits: a real Oracle QueueFull is not produced in the hosted test; the exact
catalog projection is proven at the Vala session/engine seam and the
existing hosted Busy-deferral proof covers the sink. An engine timeout drops
the session without awaiting settlement; blocking work still owns the
workspace, writer and permit until it ends. Non-goals stayed excluded: no
dependency, feature, migration, Card type change, hook, or new setting;
`CodeTool` kept.
