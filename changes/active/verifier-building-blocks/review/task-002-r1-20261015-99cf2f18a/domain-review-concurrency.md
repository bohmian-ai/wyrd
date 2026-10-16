# Independent domain review: concurrency

Result: **PASS**. No material proposed findings in this domain.

## Immutable subject and review boundary

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-002`.
Base: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`.
Candidate: `99cf2f18a8cadfe3c500cf7157934047112d156a`.
Prior candidate: `d342c54749a871dcacf5b0c7ec29e3b458f2d9e2`.

This fresh review independently inspected the full cumulative execution change, using remediation changes to locate prior-finding closure. It covers the shared walker, both contract adapters and their callers, deterministic settlement, fanout, errors, cancellation, task joins, resource/permit ownership, retries and deadlines. It does not substitute for the other domain or repository reviews. No current-round discovery reports or intended verdict were read.

Inputs: `subject.md`, frozen approved `spec-snapshot.md` revision 6, `original-task.md`, `remediation-task.md`, prior verdict/validation ledger, complete base-to-candidate source diff, and recorded verification evidence. Governing authority includes AGENTS.md §§3–7/10–12, agent-rules, Wyrd design/doctrine, reference router, spec-driven-development, maintainer-style, Rust-core, testing-workflows and evaluation guidance. Relevant approved obligations are REQ-003/004/005/006 and INV-001/005/006/007; the approved spec replaces the old Verifier authored shape without waiving lifecycle ownership or bounds.

CodeGraph was used before locating/understanding source; focused source reads and caller searches expanded its navigation. HEAD was checked as the candidate. No source, test, packet, index or commit mutation was performed; only this assigned report was written. No Cargo process was started by this reviewer.

## Source coverage and execution paths

| Boundary | Inspected source | Assessment |
| --- | --- | --- |
| Existing shared execution owner | `executor.rs:151–268`, `RunLedger`, `Running`, `Prepared`, `walk`; complete removed stage/classify/bucket/order implementation in cumulative diff | One actual stage loop, fanout, join, ordered settlement and state ledger remains. Old intact schedulers were removed rather than wrapped. |
| Legacy adaptation and real callers | `executor.rs:313–465`, `execute_plan`, `prepare_task`, `first_skipped_dependency`, condition handling; `scenario.rs:195–230`; `orchestrator/scoring.rs:35–64,168–213`; `context.rs::ContextSnapshot::extend`; `store.rs::TaskRegistry` | Both scenario and record-scoring paths still reach the same established entry point, now using `walk`. Immutable snapshot and task registry remain contract adapters, not a second scheduler. |
| New facade and stage adaptation | `blocks/engine.rs:77–314`, constructor/run/drive/step/requirement; shared boundary `blocks.rs` | The facade retains executors and its shared semaphore. Declaration-index lookup and synthetic action/check stages adapt the new contract into `walk`; they do not own a second fanout or settled-state collection. |
| Action/check execution | `blocks/engine.rs:315–450`, `attempt`, `judge`, `trace`; result settlement and rollup later in the same module | Permits, retry budget, attempt accounting and per-attempt timeout remain explicit. An action error produces failed step evidence; check errors produce isolated inconclusive judgments. Execution deadline/join errors remain execution-wide errors. |
| State and cause propagation | Complete `blocks/scope.rs`, `Missing::strongest`, `Settled`, `Done`, `Scope::{step,verdict,resolve_all,resolve}`; engine `prepare`/`check_bindings` | The scope borrows the walk's ledger. Failed causes survive transitive blocked outputs; status/verdict reads remain values rather than missing output. |
| Prerequisite planning/validation | `wyrd-spec/src/card/blocks/validate.rs::VerifierSpec::{plan,dependencies,validate_with}`, step-budget and positional validation; `vala/eval/plan.rs::{plan_stages,Stage}` | Valid authored graphs supply unique IDs and topological stages. Per-step retries and positive timeout validation remain prerequisite contract ownership. No new planner is introduced. |
| Shared leaf regressions | Cumulative changes to `tasks/assertion.rs`, `tasks/trace.rs`, `results.rs`, manifest/lock; original and remediation engine tests | Comparison/capture/projection extraction adds no asynchronous owner or new runtime. New drift dependency introduces synchronous scoring, not another runtime or durable writer. |

No materially changed SQL, identity, permission, tenant, persistence, network client or hosted scheduling boundary was found in this cumulative write set. Executors still receive caller-provided authority through the unchanged injected seam; this library does not acquire it.

## Invariant and regression assessment

| Obligation / realistic path | Source evidence and consequence | Supporting proof | Result |
| --- | --- | --- | --- |
| One stage/state lifecycle for new and retained consumers | `walk` prepares from earlier settlements, spawns runnable work, joins and appends results; both `drive` and `execute_plan` invoke it. `Scope` reads `RunLedger<Done>`; legacy snapshot extends successful `RunLedger<EvalSettled>` outputs. | Both library consumer families are exercised in the complete library lane. | PASS |
| Dependency barrier includes upstream requirements | `drive` inserts an action stage and then its step-requirement stage for every original plan stage; `walk` does not enter the next stage before joining every task. Whole requirements are appended once. | `dependents_wait_for_upstream_step_requirements` asserts action/check order and once-only ordered results. | PASS |
| Completion order cannot reorder result output | Tasks carry their declaration index through the JoinSet; results are appended by `stage.tasks` order after the barrier. Requirement stages flatten requirements by plan-step order then declaration order. | Dependency-order test plus legacy executor/scenario library regressions. | PASS |
| Independent actions overlap while executed blocks remain bounded | Each action/check acquires from the engine's shared nonzero semaphore before invoking its work; clones share that semaphore. Permits cover action retry sequences and check invocation. Spawned tasks waiting for permits do not invoke executors. | `independent_actions_overlap_within_the_concurrency_bound` uses a real barrier: two permits complete, one permit reaches the execution deadline. | PASS |
| Preparation failures do not leak partially started stages | `prepare` returns the whole vector before the fanout loop starts. A preparation error returns before any task for that stage is spawned. | Direct control-flow evidence; no new hidden execution in binding resolution. | PASS |
| Legacy execution errors and panics still abort with no partial report | `execute_plan` propagates executor/condition errors unchanged; join errors map to its established `DagInvalid` error. Completion errors drop the stage's sole JoinSet. Previously separate kind buckets also aborted the whole run; the new shared owner preserves that boundary. | Existing executor/scenario tests pass; control-flow inspection covers error ownership beyond healthy-path tests. | PASS |
| New action/check failures stay isolated, orchestration errors remain distinct | `Attempted::settle` records failed steps; `judge` converts injected check errors to inconclusive requirements. Neither is returned as a walker task error. Join faults and capture errors propagate through the shared owner and return no verdict. | Retry/timeout/cause tests, no-step type-error/inaccessible-trace tests; direct source trace for join/capture propagation. | PASS |
| Per-step retry and timeout bounds stay visible | Attempts start at one; only retryable errors retry while `attempts <= retries`. Earlier local timeout drops that attempt and supplies retryable `timed out`; terminal errors stop immediately. Duration spans the acquired execution and all attempts. | `retries_spend_the_budget_and_keep_attempts_visible`; `step_timeouts_fail_the_step_and_let_unrelated_work_finish` proves two 5-second attempts, failed status and unrelated completed judgment. | PASS |
| Absolute deadline includes immediately ready work and final publication | `run` checks expiry before planning, wraps pending drive in `timeout_at`, and checks again after ledger projection and rollup. This closes the ready-future timeout caveat without claiming synchronous preemption. | New `elapsed_deadline_refuses_ready_work` covers empty and missing-evidence ready paths; existing future-budget empty path remains inconclusive. | PASS |
| Deadline / caller cancellation stops descendant work and releases resources | Dropping pending `drive` drops the active `walk` and its JoinSet, aborting every stage child including permit waiters. Held semaphore permits and executor futures are owned within those child futures and are dropped during task cancellation; later stages cannot start once the caller is dropped. No detached background task, persistent retry queue or lock owner is introduced. | `execution_deadline_cancels_running_work` observes the action future being dropped after deadline. Direct ownership trace covers caller-drop and siblings. | PASS |
| Failed evidence cannot become intentional skip | `requirement` obtains its owner gap but still resolves every explicit binding, then combines all causes with `Missing::strongest`; no blocked check is scheduled. The prepared action path similarly combines when/action/workspace gaps. | `skipped_owner_keeps_failed_check_evidence` asserts failed ancestor, missing slot, pure-skip control, no injected check calls and inconclusive aggregate. Existing transitive/mixed/status tests remain passing. | PASS |

## Prior finding closure in this domain

- **FIND-TASK-002-1 — closed for concurrency ownership.** `executor.rs::walk` is the sole stage iteration/fanout/join/ordered-ledger owner used by both caller families. The removed old bucket scheduler is not retained behind a wrapper. New `drive` constructs contract-specific stages and `Scope` borrows the same ledger; neither stores a competing mutable execution history. Existing scenario execution and retired legacy judge adaptation stay compiling without new-engine dependency on the judge.
- **FIND-TASK-002-2 — closed.** The common requirement-preparation boundary combines implicit owner and explicit evidence gaps before scheduling. The focused regression matches the legal failing-ancestor/false-condition path and checks the observable aggregate and suppressed executor calls.
- **FIND-TASK-002-3 — closed.** Public `run` rejects already elapsed deadlines and refuses to publish a verdict after synchronous completion exhausted the budget, while retaining native timeout/JoinSet cancellation. The focused regression checks both empty and missing-binding ready paths.
- FIND-TASK-002-4 concerns statistical completeness; FIND-TASK-002-5 is the separate full rustdoc audit. This report does not claim closure for those findings outside its domain.

## Verification and limits

Root's `review-verification.md` records exit 0 for the three exact remediation selectors, the full `vala-eval`/`vala-drift` library run (198 tests), all-features lib/tests Clippy, `fmt:check`, dependency checks and cumulative diff whitespace validation. The focused source bodies were inspected independently; a reported green check was not used in place of the source trace above.

No test was added or rerun by this reviewer. Task cancellation is Tokio task abortion: the existing timeout regression explicitly waits for the child drop signal, and this implementation does not promise synchronous completion of every abort before returning its error. Arbitrary blocking executor work cannot be preempted by cooperative async cancellation; the injected contract requires resources released on future drop and this change adds no blocking executor. These are existing runtime semantics, not identified regressions or a request for a new shutdown protocol.

The tests do not separately inject a panicking executor, caller-drop during semaphore wait, or cancellation during a check. Their paths share the inspected JoinSet/RAII owner with the exercised action deadline path; no distinct reachable defect was found that would require additional tests as remediation. The task explicitly defers integration/journeys and hosted admission/sandbox/workspace limits to later integration owners. No hosted safety or durability claim is inferred from these library checks.

Material proposed findings: **none**. Overall concurrency result: **PASS**.

Skipped Cargo duplication and integration/journeys as assigned. Remaining proof limit is hosted composition, outside this bounded library task.
