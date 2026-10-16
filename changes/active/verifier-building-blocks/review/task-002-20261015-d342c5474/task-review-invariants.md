# TASK-002 independent invariant review

Result: **FAIL**.

## Immutable subject and coverage

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-002`.
Base: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`.
Candidate: `d342c54749a871dcacf5b0c7ec29e3b458f2d9e2`; HEAD remained this candidate at inspection.
Authority: frozen approved specification revision 6, original TASK-002, AGENTS.md, architecture/agent-rules.md, relevant design/doctrine sections, reference router, spec-driven-development, maintainer-style, Rust core, evaluation, and drift-monitoring references. The approved specification explicitly supersedes the old implementation-kind design for this change; that stale design is not independently a defect in this bounded task.

Inspected the entire executable change: engine, scope, drift adapter, seam exports, comparison/projection/capture extraction, manifest and lockfile. Expanded the supplied navigation map through CodeGraph into the unchanged planner/validator, executor, context, registry, TraceSource, scenario, orchestrator/scoring, and task executors. Inspected all new engine/scope tests and recorded owner-level verification. No other review reports or conclusions were consulted. This review does not substitute implementation claims for source evidence.

## Producer-to-consumer map

- `VerifierSpec::dependencies/plan` produces shared `ExecutionPlan` stages from action/when references and explicit ordering dependencies. `VerificationEngine::drive` consumes those stages; step requirement references are checked against transitive ancestors by `Validator::validate_reference`.
- Evidence declarations enter `Scope`; reference resolution produces `CallInput` values plus original slot sets. Action bindings consume them; successful action output inherits those source sets. Status/verdict constants intentionally have no evidence sources.
- Action errors become `Attempted`, then `Settled::NoOutput` with `Missing::Failed`; an intentional gate skip becomes `Missing::Skipped`. `Scope::resolve_all` merges gaps using `Missing::strongest`. Requirement execution also produces a gap from its owning step, but currently bypasses that merge (INV-REV-002).
- Check results pass through `Judged`, capture handling, `RequirementResult`, and exactly one final `decide` call. Agent/code results filter citation slot reachability and downgrade citation-less passes; locator validation is a server/host obligation explicitly assigned outside this task.
- Drift requirements select fitted state by requirement ID and batches by input slot, call unchanged `score_drift`, serialize full report detail and window provenance, and use the common rollup.
- Existing scenario and scoring consumers retain `execute_plan`, `ContextSnapshot`, `TaskRegistry`, and `Executors`. They do not call the new driver/scope; these complete workflow owners were left unchanged (INV-REV-001).

## Acceptance matrix

Verification names below refer to recorded exact-selector results in `verification-evidence.md`, inspected against their source. No tests were independently rerun in this static review.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001/AC-002: same in-memory engine supports zero and multiple preparation steps | `engine.rs::run/drive` | no-step and dependent-order engine tests | PASS |
| REQ-002: required omission and missing child selectors do not call checks | `scope.rs::resolve/resolve_all`, `check_bindings` | missing-and-optional evidence and selector tests | PASS |
| REQ-002: omitted optional input resolves to null, allowing explicit null comparison | optional-input branch in `Scope::resolve` | missing-and-optional evidence test | PASS |
| REQ-003: action/when references use the established graph planner | `VerifierSpec::plan`, `drive` | dependent-order test | PASS |
| REQ-003: step action precedes its requirements; dependents wait; final checks last | `drive` stage barriers | dependent-order test and source call order | PASS |
| REQ-003: current requirement fields, upstream outputs/statuses/verdicts are exposed in allowed positions | `Scope::resolve`; existing reference validator | dependent-order test, reference validator tests, source | PASS |
| REQ-004/INV-007: bounded active blocks, retries and attempt timeouts | shared semaphore; `attempt` retry loop and timeout race | overlap/concurrency, retries, step-timeout tests | PASS |
| REQ-004/015: attempts, final reason, duration and tool provenance survive success/failure | `Attempted::settle` | retries and timeout tests; seam tool-record test | PASS |
| REQ-005: assertions and trace checks reuse deterministic comparisons/projection | `judge/trace`, changed shared assertion and trace helpers | no-step, each, existing library regression results | PASS |
| REQ-005: isolated executor errors are inconclusive, completed false assertions fail normally | `Judged::compared`, hosted-check error branch | no-step and hosted-check tests | PASS |
| REQ-006/INV-006: strongest failure cause wins across all requirement evidence and owning-step gaps | `run_requirements` short-circuits at line 326 | current mixed-cause test covers final checks only; omitted reachable step-check case below | FAIL |
| REQ-006: intentional/transitive output skips retain cause; status-only reads are usable | `Settled`, `Scope::resolve` output/status branches | skip-causes test | PASS |
| REQ-006: empty/all-skipped inconclusive; denominator includes inconclusive; inclusive threshold; one result per requirement | `decide`, ordered result reconstruction | empty/all-skipped, table-driven decide, dependent-order tests | PASS |
| REQ-014: drift math and baseline policies unchanged, per-requirement state and per-slot samples | `DriftChecks::judge`, unchanged vala-drift | drift engine test and recorded 75 drift library tests; SPC source shares unchanged scorer path | PASS |
| REQ-014/015: full report and window attribution retained | `DriftChecks::scored/provenance` | drift test plus serialization source | PASS |
| REQ-015: exact check, containing step, statement, reason, actual capture and executor provenance retained | `run_requirements` result construction; `capture_actual` | capture and dependent-order tests; source | PASS |
| INV-001/task reuse boundary: extend existing driver/context/registry rather than duplicate orchestration | new complete driver and scope alongside unchanged originals | static comparison of owners and consumers below | FAIL |
| INV-005: no server, durable storage, reactions, provider, interpreter or policy logic introduced | complete diff and dependency inspection | recorded check:deps; source | PASS |
| INV-007: execution deadline/cancellation abort descendant work without fabricated verdict | outer `timeout_at` and owned `JoinSet`s | execution-deadline cancellation test | PASS |
| INV-008/task non-goals: existing scenario/offline callers and old judge remain compiling and unchanged; no new scenario action | preserved `scenario.rs`, `orchestrator/scoring.rs`, old task executors | recorded library/clippy checks; integration/journeys explicitly deferred | PASS |
| AC-003/007/010: drift composition, in-memory proof and shared injected executor handoff available | public engine constructor, with_drift and shared traits | scripted tests use production engine and seams | PASS |

## Proposed material findings

### INV-REV-001 — VIOLATION: second complete orchestration and scope owner

**Obligation.** Approved INV-001 says “Extend complete existing workflows; leaf-call reuse does not justify a duplicate orchestrator.” TASK-002 explicitly requires reuse/generalization of `TaskExecutor`/`Executors`, stage driver, scope/context and registry; its reuse map says “No new graph store or orchestration owner” and “Replace the internal representation in its owner.” AGENTS.md §15 and spec-driven-development also require existing-owner reuse.

**Location.** `crates/vala/vala-eval/src/blocks/engine.rs:78` (`VerificationEngine`), especially `drive` at 158 and `run_steps` at 218; `blocks/scope.rs:111` (`Scope`).

**Evidence and sibling owners.** The range does not change `executor.rs`, `context.rs`, or `store.rs`. New `drive/run_steps/run_requirements` implements a second topological-stage loop, fanout, ordering, execution ledger and snapshot extension. New `Scope` separately owns evidence, step output/status and requirement verdict maps. Existing `execute_plan/execute_stage/classify_stage_tasks`, `RunLedger`, `ContextSnapshot` and `TaskRegistry` continue to own the sibling workflow consumed by `scenario.rs:212` and `orchestrator/scoring.rs:200`. Sharing `VerifierSpec::plan`, comparison helpers and Tokio primitives shares leaves, not this workflow.

**Existing alternative and missing behavior.** Existing owners are EvalTask/check-specific, abort on errors, and have kind-specific snapshot/dispatch. The task selected extension of those owners to action plus requirement positions, cause-preserving gaps, and injected block capabilities while keeping scenario entry points. Nothing in the source establishes a lifecycle or dependency boundary that prevents that extension; preserving old entry points does not require a second stage driver/state owner. A direct call from the new driver into the unmodified legacy judge dispatch would be wrong; generalizing the owner need not introduce such a call.

**Consequence.** Two independently maintained execution/state pipelines encode the same staged evaluation capability, contrary to the explicit acceptance constraint; scenario regressions and new engine tests no longer exercise a shared execution owner. This finding removes unearned runtime machinery rather than requesting stylistic relocation.

**Smallest correction outcome.** Extend the established execution/context/registry owners to express the approved block contracts and cause-preserving results; retain old scenario entry-point behavior through that existing owner, and remove the duplicate workflow/state implementation. Keep the public engine facade if it remains a meaningful injected capability handle. Do not merely move the new functions or add a generic wrapper around two intact drivers. No new planner, framework, dependency, public format or semantics is needed.

**Focused closure proof.** Source demonstrates one owner of stage driving and execution state, called by both new Verifier execution and retained scenario evaluation; rerun new owner-level tests plus existing scenario/unit consumers. Retain the task prohibition on full integration/journey suites here.

### INV-REV-002 — INCORRECT: owning-step skip hides failed required evidence

**Obligation.** REQ-006 says failure wins when both causes affect evidence needed by one requirement; INV-006 forbids missing evidence/failed dependencies becoming intentional non-applicability.

**Location.** `crates/vala/vala-eval/src/blocks/engine.rs:320-326`, especially `blocked.map_or_else(|| check_bindings(scope, requirement), Err)`.

**Reachable producer-to-consumer path.** Declare step `broken` whose action returns a terminal failure. Declare later step `gated` with `depends_on: [broken]`, a false `when` over an ordinary input, and an action binding only that input. Attach a requirement to `gated` whose check needs `steps.broken.output`. This is valid: `depends_on` adds ordering without data (REQ-003), and `Validator::validate_reference` at `validate.rs:725` admits a step requirement reading any transitive ancestor's output. After `broken` settles, its scope entry carries `Cause::Failed`; `gated` intentionally skips and carries `Cause::Skipped`. `run_requirements` obtains that own-step skip, then never calls `check_bindings`. `Judged::missing` consequently emits `not_applicable`, although resolving the explicitly required ancestor output would yield `Failed` and require `inconclusive`. Adding an otherwise passing final requirement can now turn `all` into passed by excluding this result.

**Sibling tracing.** `prepare` already resolves when/action/workspace bindings together and merges via `Missing::strongest`; `Scope::resolve_all` also merges all bound gaps. Whole-Verifier checks go through that path and are covered by the existing mixed-cause test. Only the step-check owner-gap branch bypasses it. Step checks whose own owner failed already get inconclusive; the correction must preserve that and the non-invocation of every check with missing evidence.

**Smallest correction outcome.** At the requirement preparation boundary, combine the owning-step gap with gaps from all of that requirement's bindings using the existing strongest-cause rule. Resolve bindings even when the owner has no output, but never invoke a blocked check. Keep all-intentional skip cases not applicable and keep status-only final checks usable. This consumer owns the invariant because it is the first place both the containing-step context and requirement bindings coexist.

**Focused closure proof.** Add an in-memory engine regression for the valid false-when/failed-ancestor case above, asserting inconclusive, no check invocation, failure reason retained, and an overall `all` verdict that cannot become passed. Cover the analogous missing-required-input gap under an intentional owner skip and an all-intentional control case; reuse the existing script seam and owner-level test lane.

## Verification and limits

Recorded 120 vala-eval and 75 vala-drift tests, focused engine/scope selectors, Clippy, formatting, dependency check and diff check are credible for covered paths but do not prove either omitted condition above. The new drift test has PSI and Custom cases; SPC adapter execution is supported by source and underlying drift tests but lacks a dedicated new engine case. Integration and full journeys are intentionally deferred by the original task, not requested as remediation here. The evidence admits tests were drafted after the engine rather than ordered TDD; that history cannot be repaired by pretending a later rerun was RED. No safety check or old assertion was weakened in the executable diff inspected.

No implementation changes, commits, merges, pushes or deployments were made. No optional improvements or unrelated pre-existing debt are proposed.
