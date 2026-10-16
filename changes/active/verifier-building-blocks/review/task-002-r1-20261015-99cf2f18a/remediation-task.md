---
id: TASK-002-R1
kind: remediation
status: ready
spec: SPEC-verifier-building-blocks
spec_revision: 6
parent_task: TASK-002
remediates: [FIND-TASK-002-1, FIND-TASK-002-2, FIND-TASK-002-3, FIND-TASK-002-4, FIND-TASK-002-5]
---

# Restore shared execution ownership and honest engine outcomes

Execute with `$wyrd-implement`. This is bounded remediation under the approved behavior, not a new design phase.

## Subject and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-002`.
Approved spec: `changes/active/verifier-building-blocks/spec.md`, revision 6.
Original task: `changes/active/verifier-building-blocks/tasks/TASK-002-verification-engine.md`.
Original base: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`.
Reviewed candidate: `d342c54749a871dcacf5b0c7ec29e3b458f2d9e2`.
Review: this directory's `verdict.md` and `findings-validation.md`; frozen authority/diff are `spec-snapshot.md`, `original-task.md`, `cumulative.diff`.
Read AGENTS.md, agent rules, applicable architecture and reference-router authorities. Preserve the original task's exclusions and shared TASK-001 boundary.

## Diagnosis and selected correction

### FIND-TASK-002-1: duplicate lifecycle and execution state

Approved INV-001 requires complete existing workflow extension. New `blocks/engine.rs::drive` (line 160), `run_steps` (207), `run_requirements` (308), and `blocks/scope.rs::Scope` (108) independently own stage walking, classification, fanout, result ordering and settled state. Existing `executor.rs::execute_plan/execute_stage`, `RunLedger`, `context.rs::ContextSnapshot` and `store.rs::TaskRegistry` retain the same workflow responsibilities and remain live through `scenario.rs:212` and `orchestrator/scoring.rs:200`. Sharing the planner, comparisons, projection and capture leaves does not satisfy the required owner reuse. Green new-engine tests do not exercise the existing owner with new semantics.

Extend the established Vala execution/context/registry ownership to support action outputs, step/final requirement positions, cause-preserving gaps and budgets. Both the meaningful new public facade and retained scenario entry points must use one stage/fanout/state lifecycle, with contract-specific adaptation preserving existing scenario results/errors. Remove the superseded parallel lifecycle/state machinery when those consumers share the owner. Moving or wrapping two intact schedulers does not close the finding. The old check-centric contracts lack the new semantics; adapt them without routing new blocks through the prohibited retired judge path. Do not remove unrelated scenario evaluation or defer its shared ownership to TASK-005.

This selects the existing domain owner; private helper structure remains implementation-owned. No new planner, framework or dependency is necessary.

### FIND-TASK-002-2: implicit owner skip suppresses stronger explicit evidence failure

At `blocks/engine.rs:320-326`, `run_requirements` returns an owning-step no-output gap before resolving explicit check bindings. A valid Verifier can contain a terminally failing `broken` step, then a `gated` step ordered after it whose action and false `when` read only ordinary input. A requirement on `gated` may legally read `steps.broken.output`: the validator permits ancestor references and explicit dependencies are ordering-only. The owner contributes an intentional skip, but the requirement also needs failed evidence. Current output is N/A; alongside a passing requirement, `all` can incorrectly pass.

At the common requirement preparation boundary, combine the containing-step gap and every explicit binding gap using existing `Missing::strongest`. This boundary first owns both implicit and explicit dependencies. Resolve causes even when the owner lacks output, and never invoke a blocked check. Preserve pure intentional skip as N/A, failed-owner requirements as inconclusive, status-only whole checks, existing final-check behavior and the rollup. An omitted required slot under an intentionally skipped owner is the same source defect. Do not compensate at each executor or in aggregation.

### FIND-TASK-002-3: immediately ready work bypasses an elapsed deadline

`VerificationEngine::run` (`blocks/engine.rs:139-152`) relies solely on `timeout_at`. Pinned Tokio 1.52.3 polls the inner future first. A valid empty/no-spawn graph can complete with an expired absolute deadline and return an Inconclusive outcome rather than `EngineError::DeadlineExceeded`. Synchronous planning precedes the timeout and final rollup follows it. Existing pending-action cancellation proof does not exercise this path.

Enforce deadline eligibility at the public run owner before work and refuse a completed verdict if the deadline expires during synchronous completion/planning. Retain native timeout cancellation for pending work, current retry policy, attempt accounting and JoinSet resource ownership. Reuse `Instant` and the existing deadline error. Do not introduce a timer abstraction, caller guard, public configuration or promise synchronous preemption.

### FIND-TASK-002-4: incomplete Custom population can pass

`DriftChecks::judge` (`blocks/drift.rs:57-76`) passes its selected raw batch straight to `score_drift`. Custom `score_custom` deliberately uses `collect_f64_non_null`. With metric `ms`, baseline 100, threshold 5 and selected values `[100, null]`, mean 100 becomes NoDrift and a Passed requirement/Verifier. Existing direct host (`verification/direct.rs:173-234`) rejects incomplete configured columns; scheduled `fold_custom` (`verification/drift.rs:628-644`) requires positive count, equal total/numeric counts and finite mean. PSI/SPC scorers already enforce their target completeness. The required standalone adapter has no upstream null-free guarantee; this is its selected-population boundary, independent of later hosted source admission.

Before unchanged Custom scoring, preserve completeness of the configured metric column by name. Null selected measurements produce the existing unscored Custom report through the existing adapter projection, retaining detail/window provenance. Preserve missing-column/type/scorer refusals and ignore unrelated nullable columns. Reuse Arrow's existing column/null APIs. Keep numerical math, fitted-state policy, hosts, scheduling and source-watermark policy unchanged; do not export private statistical target representations or add imputation/sampling behavior.

### FIND-TASK-002-5: complete only the validated rustdoc omissions

AGENTS §16 is a mandatory gate. The validator confirmed these touched-item omissions; document surviving or moved equivalents after lifecycle consolidation:

- `tasks/assertion.rs::execute_assertion` (74): missing rustdoc, including scoped selection, single/each comparison, attribution and propagated selection/type errors.
- `blocks/engine/tests.rs::run` (143): missing Errors, input-object/slot-key preconditions and forwarded deadline/cancellation behavior.
- `Script::calls` (55), `record` (60), both scripted executor `execute` methods (71/109): poisoned-lock panic conditions and applicable pending/barrier cancellation behavior. Existing executor Errors sections already exist.
- Test helpers `verifier` (123), `engine` (130), `requirement` (181), `step` (190), `verdict` (209), `drift_requirement` (973): actual caller preconditions (valid spec, concurrency bounds, present result IDs, valid citation JSON, object-valued method respectively).
- `blocks/scope.rs` test `path` (308): valid reference parsing precondition.
- `tasks/trace.rs::trace_executor` module (158): missing shared test-support module documentation.
- Public engine constructor (95): actual `Semaphore::MAX_PERMITS` panic bound.
- Private `drive` (160) and `requirement_at` (718): same-spec planner and valid index invariants/panics if those symbols survive. Production callers already establish these invariants; add no redundant runtime guards.

The `samples` batch fixture and shared trace `span` fixed timestamps have no caller-reachable construction failure; their proposed panic omissions were rejected. Do not add fictional preconditions or preserve dead items merely to document them. Inspect final touched-item documentation once; no new test/check script or lint suppression is needed.

## Preserved behavior and non-goals

Own only the Vala engine, established executor/context/registry adaptation, drift adapter, shared helpers and focused library tests. Preserve TASK-001 action/check contracts, deterministic operators, trace projection/capture semantics, statistical algorithms, requirement identities, original evidence/provenance, bounded retries/concurrency/cancellation, old scenario outputs/errors and unrelated consumers. Keep the existing public callable seam cohesive.

Do not touch server routes, SQL, SDKs, hosted Agent/code/extraction implementations, persistence, scheduling, reactions, provider configuration, public formats, or statistical policy. No new graph/runtime framework, dependency, compatibility layer or retired judge dependency in new execution. Do not rewrite historical TDD evidence or stage/commit the untracked parent packet as part of this remediation.

## Acceptance and focused proof

| Finding | Closure criterion and proof |
| --- | --- |
| FIND-TASK-002-1 | Source/callers demonstrate one shared stage/fanout/execution-state lifecycle for Verifier and retained scenario entry points; existing engine and executor/scenario library regressions retain behavior. A facade/relocation around independent drivers is insufficient. |
| FIND-TASK-002-2 | Valid failed-ancestor + false-own-when check is inconclusive, preserves failed-cause reason, invokes no check and cannot allow a passing `all` result. Cover an omitted required slot under a skipped owner and a pure intentional-skip control using existing scripted fixtures. |
| FIND-TASK-002-3 | Paused-time, elapsed-deadline empty execution returns DeadlineExceeded; include immediately missing-binding/no-spawn behavior in the same focused proof where useful. Future-deadline empty execution remains inconclusive and pending-work cancellation still drops work. No load or wall-clock sleeps. |
| FIND-TASK-002-4 | Nullable selected Custom metric yields inconclusive requirement/aggregate and retains supplied non-None window/detail. Complete matching/shifted controls still pass/fail, unrelated null columns are irrelevant, existing scorer refusal behavior stays intact. |
| FIND-TASK-002-5 | Final new/materially changed items meet substantive rustdoc, Errors and relevant actual panic/cancellation requirements; validate against bodies, without new prose-only tests. |

Use ordered RED/GREEN for the new behavioral regressions and record actual exact selectors after choosing test names. Existing successful tests cannot supply fabricated RED history. Reuse existing Script, paused Tokio time, drift fixtures and scenario tests; no new harness is required.

Run each newly named regression with its actual exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=<actual fully-qualified test name>)'
```

Rerun the existing focused drift composition proof (and record exact selectors for any specifically named existing regression in the implementation evidence):

```bash
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=blocks::engine::tests::drift_requirements_score_through_vala_drift)'
```

Final task-local lanes, as authorized by original TASK-002:

```bash
mise exec -- cargo nextest run --locked -p vala-eval --lib
mise exec -- cargo nextest run --locked -p vala-drift --lib
mise exec -- cargo clippy --locked -p vala-eval -p vala-drift --all-features --lib --tests -- -D warnings
mise run fmt
mise run check:deps
git diff --check
```

No Vala integration target, full user-journey suite or broad aggregate is required here. These run at integrated change review. Record write-set/caller closure and actual command results, then request a new immutable cumulative `$wyrd-task-review` from the original base with this verdict, ledger and remediation task. This task does not authorize implementation by the current review orchestrator.

