# Repository standards review — TASK-002

Overall result: **FAIL**. One consolidated documentation finding requires closure. This is a standards audit, not an independent task-acceptance verdict.

## Immutable subject and method

Repository and subject are those in `subject.md`: base `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`, candidate `d342c54749a871dcacf5b0c7ec29e3b458f2d9e2`. HEAD remained the candidate during inspection. Reviewed the complete cumulative diff, including the manifest, lockfile, task evidence, new engine/scope/drift modules, all new tests, and changed comparison, trace, and capture helpers. No source was edited.

Used CodeGraph first for the changed owners and for `execute_plan`, `execute_stage`, `Executors`, `RunLedger`, `ContextSnapshot`, and `TaskRegistry`; expanded into the changed source, callers, tests, manifests, and `mise.toml`. No other discovery review was consulted.

## Authority coverage

The reference router was `architecture/references/README.md`. Global authorities were `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md` (especially Doctrine, client/owner boundaries and Verifier), and `architecture/wyrd-doctrine.mdx`. Selected focused references were `doctrine/architecture-constraints.md`, `architecture/patterns.md`, `languages/spec-driven-development.md`, `languages/implementation-execution.md`, `languages/maintainer-style.md`, `languages/rust-core.md`, `languages/errors.md`, `languages/testing-workflows.md`, and `domain/evaluation.md`, `domain/drift-monitoring.md`, and `domain/vala-architecture.md`.

| Changed surface | Applicable authority and inspected evidence |
|---|---|
| `Cargo.lock`, `crates/vala/vala-eval/Cargo.toml` | AGENTS §§2–4, 11; architecture-constraints, patterns, rust-core and Vala ownership. Inspected workspace dependency declarations (`vala-drift`, Arrow, Tokio, thiserror), owner manifest and lockfile delta. |
| `src/blocks.rs` exports and new `blocks/engine.rs` | AGENTS §§2–6, 9–12, 15–16; agent rules; Wyrd Verifier/service boundaries; patterns, rust-core, errors, evaluation, Vala architecture, maintainer-style. Inspected injected capabilities, methods, stage lifecycle, scope integration, outcomes and local errors. |
| New `blocks/scope.rs` | Same Rust/ownership/documentation authorities; evaluation evidence semantics; existing context/selector owners. Inspected all reference roots, cause/state fields, selector recursion, and tests. |
| New `blocks/drift.rs` | Same Rust/ownership/documentation authorities; drift-monitoring, evaluation, Vala architecture. Inspected fitted-state/sample ownership, `score_drift` call and wire-detail projection. |
| `src/tasks/assertion.rs` | AGENTS §§4–6, 10–12, 16; rust-core, evaluation, testing-workflows, maintainer-style. Inspected the complete shared comparison bodies and the existing assertion caller. |
| `src/tasks/trace.rs` | Same Rust and test authorities; evaluation, trace identity and evidence boundaries. Inspected existing TraceTaskExecutor/execute_trace caller, shared projection and exported test fixture/module. |
| `src/results.rs` | AGENTS §§4–6, 10–12, 16; evaluation capture policy, errors, rust-core, testing-workflows. Inspected capture extraction and all sibling capture entry points. |
| `blocks/engine/tests.rs` and scope tests | AGENTS §§6, 11–12, 16; agent rules; testing-workflows, implementation-execution, spec-driven-development and maintainer-style. Inspected every helper, scripted executor, all 14 engine tests and scope test. These are cfg(test) in-module tests, despite a separate module file. |
| Task packet evidence | AGENTS §§11–15; spec-driven-development, implementation-execution; original task and frozen spec. Inspected exact selector list, recorded commands, TDD caveat and reuse record. |

SQL, RLS, migrations, public HTTP/MCP handlers, SDK/PyO3/TypeScript, generated artifacts, docs-site and external URL adapters are absent from the cumulative diff. No production signature or field in the changed modules accepts a SQL pool, connection or transaction. Bifrost publication/ingest/query/maintenance and durable data behavior are unchanged; deeper Bifrost, security, operations, binding-language and analytical-storage references are therefore not activated by this write set.

## Rule results

| Applicable rule | Result | Source evidence |
|---|---|---|
| Vala owns reusable evaluation/drift; Skald remains independent; specs remain declarative and foundational | PASS | New runtime is confined to vala-eval; no wyrd-spec, Skald, server or SDK edits. Drift dependency is at the consuming Vala owner. |
| Dependencies inherit workspace versions; no unearned feature/profile or alternate runtime | PASS | Manifest adds only `vala-drift = { workspace = true }`; lockfile adds that existing workspace dependency. No feature/profile or dependency version changes. |
| Domain identities, narrow concrete owners, closed states | PASS | `TaskId`, `SlotName`, `StepStatus`, `RequirementOutcome`, `Cause` and `Settled` are used; VerificationEngine retains executor dependencies/semaphore; Scope retains per-execution state; DriftChecks retains supplied evidence. Pure transforms remain synchronous free helpers. This does not establish task compliance of the separate orchestration owner. |
| Existing owner comparison before new machinery | PASS for documented inspection; no standards-only acceptance conclusion | Existing execute_plan/execute_stage operate on EvalTask/TaskOutput and abort on execution errors; ContextSnapshot/TaskRegistry retain the Eval model. New engine instead consumes the prerequisite block contracts and cause-aware state. Shared comparisons, projection, trace source and capture are reused. Whether extension rather than coexistence satisfies the original explicit reuse instruction belongs in the implementation/maintainer acceptance audits; field ownership alone cannot excuse duplication. |
| No unsafe, production input unwrap, ad hoc runtime, new lint suppression, ignored tests or credentials | PASS | Full changed Rust source has no unsafe, `.unwrap()`, `#[allow]`, `#[ignore]`, runtime construction or credential access. The planner lookup `expect` names its declared-step invariant. Script fixtures remain Rust-only. |
| Async is at injected runtime/check boundaries; pure selectors/comparators/capture stay synchronous | PASS for boundary shape | drive/run_steps/run_requirements compose executor work; attempt/trace await injected capabilities. Pure selection, decide, preparation and capture helpers are synchronous. Cancellation/deadline correctness is a separate behavioral/domain review. |
| Crate-local errors use thiserror; wire errors stay at public boundary | PASS | EngineError uses `thiserror::Error`; this change adds an internal engine seam and no HTTP/MCP/SDK mapping. No parallel status/code/problem-json logic is added. |
| Assertion determinism, statistical owner and capture reuse | PASS for standards ownership | Existing operators and comparison helpers are reused; DriftChecks calls unchanged `vala_drift::score_drift`; apply_context_capture delegates to shared capture_actual. No model/provider invocation is embedded in deterministic assertions. |
| Tenant/auth/audit/server durable behavior stays outside engine | PASS | No permission evaluation, IO client construction, SQL, persistence, outbox, schedule or dispatch in changed engine paths. Caller-injected implementations are retained. |
| Test placement/runtime ownership and deterministic synchronization | PASS | `#[cfg(test)] mod tests`, scripted Rust executors, paused Tokio time, Barrier/Notify; no interpreter lifetime, server fixture, live provider or sleeps. |
| Exact focused test commands and task-scoped final proof | PASS with limits below | 15 exact selectors recorded; root reran 195 library tests and supplied successful format/dependency/Clippy logs. Task explicitly excludes integration/journey suites here; new server/client workflow is deferred to integrated change proof. |
| Required rustdoc for all touched Rust items, Errors/Panics/cancellation conditions | **FAIL — REPO-001** | Fallible test run helper lacks Errors, helper panics are undocumented, and changed trace test module has no module rustdoc; details below. |
| Top-of-module imports / bare names in signatures | FAIL, non-blocking note | New test local `use` statements at engine/tests.rs:721–722 and :964–965; qualified RecordBatch return type at :963. No behavioral consequence established. |
| Test-first execution and tracked packet lifecycle | FAIL as historical/process note, not a new implementation finding | Task evidence explicitly says engine draft preceded tests; approved packet is mostly untracked in this worktree. These facts are disclosed, not fabricated RED evidence. See limits. |

## Material finding

### REPO-001 — Incomplete rustdoc on changed tests/helpers and module

Classification: **VIOLATION**. Governing rules: AGENTS §16 and agent-rules Rust documentation bullet explicitly cover private items, helpers and tests, require Errors for fallible functions and Panics where a panic remains, and call missing/incomplete rustdoc BLOCK_BEFORE_MERGE. The review skill expressly retains the rustdoc blocker even when other style-only findings are non-blocking.

Exact evidence and affected items:

- `crates/vala/vala-eval/src/blocks/engine/tests.rs:143–168`: `run` returns `Result<VerificationOutcome, EngineError>` but documents only a 60-second deadline. It lacks `# Errors`, its fixture-input panics, and cancellation/deadline behavior relevant to how this shared helper drives tests.
- In the same file, `Script::calls` (:55) and `record` (:60) panic on poisoned locks; scripted action/check `execute` (:71/:109) call these helpers (the action also locks attempts); `verifier` (:123) deserializes/validates with expect; `engine` (:130) requires positive concurrency; `requirement` (:181) and `step` (:190) require the result ID to exist; `verdict` (:209) requires valid citation JSON; `samples` (:960) requires a valid batch; `drift_requirement` (:973) requires object-valued method fields. These new helper docs omit their actual panic preconditions.
- `blocks/scope.rs:308`: new test `path` requires a syntactically valid reference but has no Panics documentation.
- `src/tasks/trace.rs:158`: `trace_executor` is materially changed from private to `pub(crate)` to share its fixture, yet it has no module rustdoc. The changed `span` fixture (:231) documents sharing but not the timestamp-construction invariant its expect enforces.
- `blocks/engine.rs:95`: the public new constructor forwards concurrency to `Semaphore::new`, which panics above Tokio's supported permit maximum; its docs only state the concurrency purpose. `drive` (:159) and `requirement_at` (:718) retain planner/key indexing invariants without describing their panic conditions. These should be covered in the same bounded rustdoc correction, not left for another round.

Consequence: the mandatory repository documentation gate is unmet, and maintainers extending fixtures or using the constructor do not see the relevant input/invariant and failure contracts. Green Clippy does not cover private/test documentation completeness.

Smallest testable correction: complete rustdoc only on the new/materially changed items above: explain fixture preconditions, returned errors and cancellation where relevant; document the trace test module's role; accurately document retained constructor/planner invariants. Do not change production behavior, replace the fixture, add a lint allowance, or weaken tests. Inspect the complete touched-item rustdoc once, then run the narrow vala-eval Clippy check (including tests) and formatting check. Source review is the credible closure proof for private docs; a new test/check is unnecessary.

## Non-blocking notes and verification limits

The local-import/qualified-signature issues above are style violations without demonstrated behavioral consequence, so they do not independently produce FIX_REQUIRED. The serde conversion in DriftChecks::scored is a projection to the wire RequirementDetail at the result boundary; no standards finding is asserted merely from its presence. Full fidelity and exceptional-score behavior belong in the drift review.

The implementation evidence candidly records tests after the draft, so the historical ordered TDD requirement was not followed. Rewriting evidence to imply otherwise would be wrong; that history cannot be repaired by another ceremonial RED run. The original first-run defects were fixed in engine lookup/fixture identifiers rather than by disabling or weakening assertions. No separate blocking implementation correction follows solely from test order.

The task branch contains the TASK-002 evidence but most of the parent packet is untracked. The orchestrator has frozen the available approved packet as review inputs; this review does not stage or commit someone else's packet. Integrated packet durability remains caller/change-owner responsibility.

Root reports independent successful `library-check.log` (120 vala-eval + 75 vala-drift), `format-check.log`, `dependency-check.log`, and `clippy-check.log` for `--locked -p vala-eval -p vala-drift --all-features --lib --tests -- -D warnings`. These supplement recorded implementation checks. I ran static inspection only and no Cargo commands, avoiding overlapping builds. Workspace-wide lints, integration and journey suites were not independently run by this reviewer; this report does not claim they passed.
