# Independent maintainer review — TASK-002

Overall: **FAIL**

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-002`.
- Base: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`.
- Candidate: `d342c54749a871dcacf5b0c7ec29e3b458f2d9e2`; HEAD still equals this candidate at the end of source inspection.
- Reviewed the complete cumulative diff, frozen `original-task.md`, `spec-snapshot.md` revision 6, and `verification-evidence.md`. No other reviewer's report or conclusions were read.
- Governing rules: `AGENTS.md` §§3–6, 10–12, 14–16; `architecture/agent-rules.md`; `architecture/references/languages/maintainer-style.md`; spec-driven-development; applicable Rust-core, evaluation and drift references; Wyrd design doctrine/client boundaries and Verifier/scenario sections; Wyrd doctrine. The approved specification explicitly supersedes the old authored Verifier kinds, while retaining ownership and reuse constraints.
- Followed CodeGraph before locating source. Expanded its map by reading complete changed production functions, new scope/drift modules, engine/test modules, and neighboring execution/context/registry/scenario owners.

## Changed-surface coverage

| Surface | Symbols and consumers inspected | Assessment |
|---|---|---|
| `blocks.rs` | New `drift`, `engine`, `scope` modules and re-exports; unchanged `BlockCall`, `ActionExecutor`, `CheckExecutor`, `Position`, output/error contracts | Public engine seam is discoverable and uses Task 1's injected contracts. No private seam changes. Module split itself is coherent. |
| `blocks/engine.rs` | `EngineError` and variants; `VerificationEngine` fields, `new`, `with_drift`, `run`, `drive`, `run_steps`, `attempt`, `run_requirements`, `judge`, `trace`; `Bindings::sources`; `prepare`, `check_bindings`, `skipped`; `Attempted::settle`; all `Judged` constructors/conversions; `requirement_at`, `json`, `reason`, `decide` | Meaningful dependency-owning public handle and mostly clear pure helpers. Material duplicate workflow finding M-001 below. |
| `blocks/scope.rs` | `Cause::outcome`; `Missing::failed/strongest`; `Settled::status`; all `Scope` fields and `new/settle/step/record_verdict/resolve_all/resolve`; `json_input`, `wire`, `select`; selector test | Typed references and explicit gap causes are readable. Scope retains a second execution-state authority rather than evolving the existing context owner (M-001). New typed selector semantics are required; old JSONPath is not a substitute for the approved reference vocabulary. |
| `blocks/drift.rs` | `DriftChecks` fields; `new`, `judge`, `scored`, `inconclusive`, `provenance`; `score_drift`, `FittedBaseline`, engine check dispatch and new drift test | Narrow adapter retains resolved inputs and delegates statistical math. Distinct fitted state per requirement and sample slot map are intentional. No algorithm copy or server IO added. |
| `tasks/assertion.rs` | `execute_assertion`, `Compared`, `evaluate_single`, `evaluate_item_mode`; `AssertionTaskExecutor::assert/execute`, operators and assertion tests; new engine assertion/each callers | Comparator sharing preserves legacy outcomes and errors. `execute_assertion` is materially modified without required rustdoc (M-002). |
| `tasks/trace.rs` | `project_spans`, `trace_executor` visibility, shared `span` fixture; `execute_trace`, `project_one`; new engine `trace`, existing trace tests | Projection and fixture are actually reused; projection behavior unchanged. Materially exposed test module has no module rustdoc (M-002). |
| `results.rs` | `apply_context_capture`, new `capture_actual`, `hash_context_value`; engine requirement capture; existing full/hash/redact tests | Single capture implementation retained, legacy adapter delegates, semantics unchanged. |
| `blocks/engine/tests.rs` | `Behavior`, `DropSignal::drop`, `Script` state/methods and both executor implementations; all helpers; all fourteen engine tests, including matrix, barriers, paused-time deadlines, citation outcomes, capture and drift fixtures | Deterministic synchronization is clear and avoids wall-clock sleeps. It proves new engine behavior but does not exercise the pre-existing scheduler with the new lifecycle. New fallible `run` helper omits `# Errors` (M-002). |
| Cargo manifest/lock | Added `vala-drift`; existing Arrow/Tokio dependencies | Dependency is earned by direct scoring composition and remains in the Vala owner. No new dependency family or feature flag. |
| Committed TASK-002 packet | Task/evidence changes and actual recorded verification limits | Evidence names exact selectors and admits post-draft tests. No production behavior is inferred from the completion prose. |
| Sibling owners/callers | `executor.rs` stage walk, classification, fanout, ordering, `RunLedger`, `TaskExecutor`, `Executors`; `context.rs` `ContextSnapshot/ExecutionContext`, outputs and selection; `store.rs` `TaskRegistry`; `ScenarioScoring::new/score_record/score_scenario/score_record_batch`; `execute_scenario`; existing executor/assertion/trace/context tests | Existing scenario owner remains live; candidate does not extend it. New engine only shares the plan and leaves, not execution lifecycle. |

No SDK, language binding, generated declaration, SQL, server route, persistent data, or external-write surface is changed in the range. Those require no declaration-parity correction here. Full journey execution is explicitly deferred by the task/spec and is not a maintainer finding.

## Material findings

### M-001 — VIOLATION / duplicated orchestration and execution scope

**Location:** `crates/vala/vala-eval/src/blocks/engine.rs:159` (`drive`), `:207` (`run_steps`), `:308` (`run_requirements`), and `crates/vala/vala-eval/src/blocks/scope.rs:103` (`Scope`).

**Obligation:** Spec INV-001 says “Extend complete existing workflows; leaf-call reuse does not justify a duplicate orchestrator.” Original TASK-002 expressly requires reusing/generalizing `TaskExecutor`/`Executors`, stage driver, scope/context and registry, and forbids a second graph runtime/store. `AGENTS.md` §15 requires extending an existing owner before adding machinery; maintainer style requires owning retained state once.

**Evidence and existing alternative:** `executor.rs:201–239` already owns the topological stage loop, stage barrier, concurrent work dispatch, ordered result merge and snapshot extension; `:248–294` classifies conditions and dependency skips; `:303–446` owns task fanout/collection/order; `RunLedger` at `:155` owns skip/outcome state. `ContextSnapshot` at `context.rs:82` owns per-run original input and settled outputs; `TaskRegistry` at `store.rs:52` owns plan/task lookup. These files are unchanged by the candidate. Instead new `VerificationEngine::drive` walks the same plan independently, creates a private ID-position lookup, and threads its own `Scope`; `run_steps` and `run_requirements` each spawn and collect a separate `JoinSet` and rebuild output order; `Scope` owns new step/verdict maps and gap state. The new code never composes the existing driver, registry or context. Sharing `VerifierSpec::plan`, comparators, `project_spans` and capture leaves does not remove the independent stage/state owner.

**Reachable sibling consumers:** `scenario.rs:212` calls `execute_plan`; `orchestrator/scoring.rs:51–57` builds `TaskRegistry`/`Executors`; `:200` uses the old driver for record scoring; `score_scenario` and `score_record_batch` retain that path. The public task-required `blocks::VerificationEngine::run` uses the new independent driver and is exercised by fourteen new tests. Its lack of a hosted caller in this intermediate task does not make the duplicate dormant: the task explicitly delivers this reusable public engine.

**Missing behavior in existing owner:** The old contracts return check-centric `TaskOutput`, abort execution on executor errors, and have a boolean skip set; they cannot represent action outputs, failure-caused skips, step-plus-final requirement positions, or the new budgets without extension. That demonstrates required extension, not that two full schedulers are needed. Existing stage/barrier/fanout/context responsibilities remain the same. The task's reuse map explicitly selected evolution at those owners, with scenario adapters as needed.

**Concrete maintenance consequence:** Two live execution lifecycles now own skip classification, stage transitions, cancellation/fanout and result ordering. A correction to these shared execution invariants cannot be made at one owner; it must be considered in both old scenario and new Verifier paths. The new tests can stay green while the existing owner is never exercised with the approved lifecycle. This is unearned retained machinery, not a preference about file placement or struct naming.

**Smallest correction boundary:** Evolve the existing Vala execution/context/registry owners to carry the new action/check lifecycle and cause-preserving state, retaining task-specific adapters for unrelated scenario contracts and preserving their behavior. Keep the injected Task-001 boundary and public callable engine as needed; a public handle must delegate to the shared lifecycle rather than own a second scheduler. Do not delete scenario evaluation or import the forbidden old judge executor into new Verifier execution. Do not merely move the independent scheduler to `executor.rs`, rename it, or add a wrapper while both independent lifecycle implementations remain. No new product or public-format decision is required. This is a bounded implementation correction under INV-001: the original task's extension instruction agrees with the approved specification and stronger ownership rules, so it is not an invalid private task mechanic requiring task correction. TASK-005 may remove obsolete hosted consumers when they move; that later removal does not authorize a parallel retained scenario scheduler here.

**Focused closure proof:** Existing engine ordering/overlap/mixed-cause/timeout tests must execute the evolved shared lifecycle, and focused existing scenario/executor regression tests must prove preserved legacy semantics through that same owner. Source evidence should identify one stage/fanout/state lifecycle, with mode-specific adaptation confined to the real contract differences. Preserve existing isolated `vala-eval` and `vala-drift` verification scope; no integrated journey run is required for this correction.

### M-002 — VIOLATION / required rustdoc missing on changed items

**Locations:** `crates/vala/vala-eval/src/tasks/assertion.rs:74` (`execute_assertion`); `crates/vala/vala-eval/src/blocks/engine/tests.rs:144` (new fallible `run` helper); `crates/vala/vala-eval/src/tasks/trace.rs:158` (materially changed `trace_executor` module).

**Obligation:** `AGENTS.md` §16 and `architecture/agent-rules.md` explicitly require substantive rustdoc for all new/materially modified Rust items, including private functions, modules and test helpers; every fallible function needs `# Errors`. Missing rustdoc is an explicit hard blocker under the task-review skill.

**Evidence:** `execute_assertion` now passes decomposed operator/ID/expected arguments to the shared comparison routines but still has no rustdoc at all. `run` has only “Run `spec` with JSON inputs and a 60-second deadline” above a `Result<VerificationOutcome, EngineError>` signature, with no `# Errors` naming forwarded failures. `trace_executor` changed from private test module to `pub(crate)` for cross-module fixture access but has neither outer nor inner module rustdoc. The newly shared `span` function itself does have rustdoc; that does not document its module.

**Concrete maintenance consequence:** The changed assertion boundary leaves its selection/type-error behavior undocumented, and the new test-call wrapper hides its failure contract. The shared test module's expanded role is not documented where a maintainer discovers it. This is the repository's explicit documentation acceptance gate, even though it introduces no separate runtime defect.

**Smallest correction:** Add intent/workflow/error documentation directly to these touched items; include relevant panic/cancellation conditions where applicable. Cover the changed/new items in the same bounded documentation pass as implementation remediation, without editing unrelated old helpers or adding a new check/harness. Existing `AssertionTaskExecutor::assert` nearby provides the precise error pattern; `VerificationEngine::run` provides deadline/cancellation semantics the test helper forwards.

**Focused closure proof:** Inspect these item docs against their full bodies and call paths, then run formatting and the narrow owning verification lane already required for implementation changes. No extra runtime test is needed solely for prose.

## Non-blocking notes

The injected engine struct owns meaningful dependencies, and `Bindings`, `Attempted`, `Judged`, `Missing`, and `Settled` each explain a concrete intermediate invariant. The issue is duplicate lifecycle ownership, not that these names or structs exist. Pure comparison and selection helpers appropriately remain synchronous. No speculative redesign, naming-only finding, or additional abstraction is requested.

## Verification and limits

This was a static independent maintainer audit. Recorded evidence reports 120 `vala-eval` and 75 `vala-drift` tests, fifteen exact engine/scope selectors, Clippy, format, dependency checks and whitespace checks passing; these commands were not independently rerun in this role. Their success does not establish required workflow reuse or rustdoc completeness. No source was changed; only this assigned report was written. Candidate identity remained unchanged.
