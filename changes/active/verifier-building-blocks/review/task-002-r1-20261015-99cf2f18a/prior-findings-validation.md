# Independent structured Ponytail validation — TASK-002

Recommendation: **FIX_REQUIRED**. Five deduplicated findings remain. None requires a new product, wire, statistical, tenancy, persistence, or concurrency-semantics decision.

## Immutable subject and validation method

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-002`.
- Base: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`.
- Candidate: `d342c54749a871dcacf5b0c7ec29e3b458f2d9e2`.
- Approved authority: frozen `spec-snapshot.md`, revision 6; `original-task.md`; AGENTS.md; agent rules; applicable Wyrd design/doctrine; reference router; spec-driven-development, maintainer-style, Rust-core, evaluation, drift-monitoring and Vala ownership guidance.
- Inputs: complete cumulative change, both task reviews, standards, maintainer, system, concurrency and drift discovery reports, `claims-comparison.md`, `followup-review.md`, recorded verification and `review-verification.md`.

This is a fresh validation context, independent of implementation and discovery. CodeGraph preceded source navigation; subsequent focused source and caller searches expanded its results. I inspected the complete new engine/scope/adapter, changed comparison/projection/capture bodies, new tests, manifest/lock changes and task evidence. I traced the existing executor/context/registry and scenario/scoring consumers, prerequisite planner/reference validator, all production drift scorer entry points, and the pinned Tokio timeout implementation. Full bodies of proposed correction owners were inspected. There is no prior review/remediation finding ledger to preserve.

The approved specification explicitly supersedes the old implementation-kind split in architecture references. It does not waive existing-owner reuse, incomplete-evidence semantics, statistical completeness or documentation. Private task mechanics cannot strengthen product behavior; retained corrections below already follow approved INV-001/006/007, REQ-005/006/014 and the explicit repository rustdoc gate.

HEAD was checked unchanged at completion. Tracked source/index are unchanged; the untracked parent packet and review artifacts remain uncommitted. No implementation, test mutation, commit or Cargo execution occurred in this validation role.

## Adjudication of every discovery finding

| Discovery IDs | Status | Source-based disposition |
| --- | --- | --- |
| BEH-002, INV-REV-001, M-001 | CONFIRMED; consolidated as FIND-TASK-002-1 | `drive` plus two fanout/ordering routines and `Scope` implement a complete independent execution/state workflow. Existing `execute_plan`/`execute_stage`, `RunLedger`, `ContextSnapshot` and `TaskRegistry` remain live through scenario/scoring callers. Their missing new block semantics require extension; they do not establish that extension is impossible. INV-001 explicitly forbids leaf-only reuse inside a duplicate orchestrator. |
| BEH-001, INV-REV-002 | CONFIRMED; consolidated as FIND-TASK-002-2 | `run_requirements` chooses an owning-step gap before resolving explicit requirement bindings. A failed ancestor's output can legally be required by an intentionally skipped step's check. The stronger cause never reaches `Missing::strongest`, producing N/A rather than inconclusive. |
| CONC-001 | CONFIRMED as FIND-TASK-002-3 | `run` relies solely on `timeout_at`; pinned Tokio polls the inner future before the timer. Empty/no-spawn paths complete immediately even with an elapsed absolute deadline. Pending-work cancellation tests cover a different path. |
| DRIFT-D1, FOLLOWUP-DRIFT-001 | CONFIRMED; consolidated as FIND-TASK-002-4 | Custom scorer intentionally drops nulls; new adapter does not preserve the selected-population completeness check present in both existing direct/scheduled hosts. The standalone adapter is an explicit task deliverable, so its path is required despite hosted composition being deferred. |
| REPO-001, M-002 | REVISED; consolidated as FIND-TASK-002-5 | Actual omissions remain, including the complementary missing assertion rustdoc. Some claimed panic conditions are impossible for fixed fixture construction and are excluded individually below. This is one bounded rustdoc correction, not separate style rounds. |

System discovery proposed no findings. Its deployed-path PASS is consistent with the new engine having no production integration yet; it does not negate required standalone behavior. General drift/deadline PASS cells in other discovery reports prove the covered math/pending-work paths and are qualified by the concrete negative paths above. Follow-up resolved the Custom boundary uncertainty; no unresolved contradiction or missing caller path requires another discovery agent.

## FIND-TASK-002-1 — duplicate stage and execution-state ownership

**Status:** CONFIRMED. **Classification:** VIOLATION / unearned duplicate workflow. **Sources:** BEH-002, INV-REV-001, M-001.

**Obligation:** approved INV-001 requires complete existing workflow extension and explicitly rejects leaf-call reuse as justification for a duplicate orchestrator. Original TASK-002 selects existing driver/context/registry extension, preserving scenario consumers. AGENTS §15 and spec-driven-development require comparison and reuse of the existing complete owner before new machinery.

**Exact changed locations:** `crates/vala/vala-eval/src/blocks/engine.rs:160` (`drive`), `:207` (`run_steps`), `:308` (`run_requirements`); `blocks/scope.rs:108` (`Scope`). Existing alternatives: `executor.rs:201-239`, `:248-294`, `:303-446`, `RunLedger` at `:155`; `context.rs:82-181`; `store.rs:52-126`.

**Producer-to-consumer evidence:** `VerifierSpec::plan()` supplies shared topological stages. The candidate independently walks them, classifies missing/gated actions, spawns a JoinSet, collects/restores stage order, advances separate step/verdict state, spawns another requirement JoinSet, and aggregates. Existing `execute_plan` independently walks the same stage representation through classification/fanout/order/context extension. `scenario.rs:212` and `orchestrator/scoring.rs:200` continue calling that owner; the new public task-required engine uses none of it. The cumulative diff leaves executor/context/registry unchanged. Comparator, trace projection and capture sharing is real, but occurs beneath both independent lifecycles.

Existing owners lack action output, cause-preserving no-output states, step/final requirement positions and new budgets. They are extensible internal Rust owners, with no separate deployment/runtime boundary forbidding adaptation. Their retained old contracts therefore justify adaptation, rather than a second workflow. Conversely, blindly calling old kind dispatch would violate the prohibition on new-engine dependency on the retired judge. A correction must share lifecycle ownership while retaining explicit contract adaptation, not force new execution through unchanged old judge mechanics.

**Consequence:** staged dispatch, state advancement, skip propagation, ordering and cancellation now have two independent implementations. New engine tests cannot prove that the existing stage owner handles the approved lifecycle. This is an explicitly forbidden retained outcome, not a naming/placement preference.

**Minimum decision-complete correction:** extend the established Vala execution and context/registry ownership to support the approved action/check positions and cause-aware settled state; make both the new Verifier facade and retained scenario entry points use that common lifecycle. Preserve scenario output/error semantics through the appropriate existing contract adapters. Remove superseded parallel driver/state machinery once consumers share the owner. A wrapper or relocation around two intact implementations does not close it. Do not create a new planner/framework, remove scenario evaluation, or route new blocks through the retired judge. TASK-005's later removal of obsolete hosted consumers does not waive this task's existing scenario boundary.

**Ladder:** deletion alone would remove required new execution or scenarios; existing execution/context owners are the next sufficient option. No standard-library primitive replaces the cohesive domain lifecycle, and no dependency is needed. This is an implementation correction within approved ownership, not a specification revision.

**Focused closure:** source demonstrates one stage/fanout/state lifecycle reached by both required entry paths, and the existing engine library tests plus existing executor/scenario library regressions pass through it. Keep the isolated vala-eval/vala-drift library, narrow Clippy, format and dependency lanes; no full journey/integration run is required here.

## FIND-TASK-002-2 — owner skip hides failed requirement evidence

**Status:** CONFIRMED. **Classification:** INCORRECT. **Sources:** BEH-001, INV-REV-002.

**Obligation:** REQ-006 says failure wins when both causes affect the evidence required by one check; INV-006 prohibits missing/failed evidence becoming intentional non-applicability. Scenario 3 explicitly requires this negative flow.

**Location:** `crates/vala/vala-eval/src/blocks/engine.rs:320-326`, `run_requirements`.

**Reachability and trace:** a validated spec can contain failing code step `broken`, then `gated` ordered with `depends_on: [broken]` but binding its action/false `when` only to ordinary input, and a requirement on `gated` that reads `steps.broken.output`. Ordering without data is permitted by REQ-003; `validate.rs:724-735` allows a step check to read an ancestor. `Attempted::settle` records `broken` as a failed gap. `prepare` records `gated`'s false condition as skipped. `run_requirements` obtains only the latter gap and bypasses `check_bindings` through `blocked.map_or_else(..., Err)`. `Judged::missing` emits N/A; `decide` excludes it. Alongside an independent passing requirement, `all` can report Passed instead of Inconclusive.

**Sibling comparison:** `prepare` combines all when/action/workspace gaps; `Scope::resolve_all` combines every reference gap using `Missing::strongest`. Successful-owner and whole-Verifier checks already use that path. No change to each check implementation or aggregation is needed. An otherwise missing required input under an intentional owner skip has the same source defect, not a separate finding.

**Minimum correction:** at requirement preparation, combine the containing-step gap with the strongest gap from all explicit requirement bindings using the existing strongest-cause rule. Preserve the implicit owner dependency, resolve binding causes even for skipped owners, and never invoke a check when any required cause blocks it. Keep pure intentional skips N/A, failed owners inconclusive, and status-only whole checks usable. This consumer boundary is appropriate because it is the first owner that has both implicit containing-step state and explicit requirement bindings. Do not add repeated guards to judge/executors or compensate in `decide`.

**Ladder:** required semantics cannot be deleted; `Missing::strongest` already provides exactly the combination rule. Reuse it in the common requirement preparation boundary resulting from finding 1.

**Focused closure:** reuse the script fixture for failed ancestor + false own `when`, with a requirement reading ancestor output and a passing independent requirement. Assert Inconclusive, failed-cause reason retained, and no check invocation. Add a missing-required-slot variant and pure intentional-skip control within the same focused case/table where practical. Existing mixed whole-Verifier/status/aggregation tests remain passing. Run the exact new selector and isolated vala-eval library lane.

## FIND-TASK-002-3 — ready execution bypasses an elapsed deadline

**Status:** CONFIRMED. **Classification:** INCORRECT. **Source:** CONC-001.

**Obligation:** REQ-005 preserves overall deadlines as execution errors, INV-007 bounds execution deadlines, and TASK-002 assigns deadline ownership to the engine. `run` documents execution before its supplied absolute deadline.

**Location:** `crates/vala/vala-eval/src/blocks/engine.rs:139-152`, `VerificationEngine::run`.

**Evidence:** valid empty spec → `plan` → `drive` with no stages → empty `run_requirements` → Ready. No timer or JoinSet suspends this path. Tokio 1.52.3 `src/time/timeout.rs:211-221` polls `me.value` before `delay`; Ready is returned as Ok without checking expiry. Thus an already elapsed deadline, or a future first polled after expiry, returns a completed Inconclusive outcome rather than `DeadlineExceeded`. Missing-binding/no-spawn paths have the same property. Synchronous planning precedes the timeout and synchronous final rollup follows it; neither is otherwise checked. There is only one public engine run owner; pending action/check paths own JoinSets whose cancellation behavior should stay intact.

**Consequence:** callers cannot reliably distinguish completed execution from timeout, including the simplest valid Verifier. The pending-action cancellation regression is credible but does not cover this reachable edge.

**Minimum correction:** enforce absolute deadline eligibility at `run` before work, retain the existing timeout for pending execution, and refuse to publish a verdict when the overall deadline has elapsed during synchronous completion/planning. Use existing `Instant` and `EngineError::DeadlineExceeded`. Do not add a timer abstraction, downstream caller guard, changed retry policy or new deadline semantics. This does not promise preemption of arbitrary synchronous computation; it prevents reporting such work as completed within an exhausted budget.

**Ladder:** cannot delete deadlines; existing engine owner plus native `Instant`/timeout suffices. No new mechanism is required.

**Focused closure:** paused-time test with an already elapsed deadline and validated empty spec asserts `DeadlineExceeded`; include an immediately missing-binding case in the same focused test if useful. Preserve future-deadline empty Inconclusive and pending-work cancellation tests. Run exact selector and isolated vala-eval library lane. No synthetic load or wall-clock timing test is needed.

## FIND-TASK-002-4 — Custom null samples can manufacture a healthy result

**Status:** CONFIRMED. **Classification:** INCORRECT. **Sources:** DRIFT-D1, FOLLOWUP-DRIFT-001.

**Obligation:** REQ-014 retains insufficient-data behavior alongside unchanged algorithms; INV-006 forbids insufficient drift data becoming success; TASK-002 scenario 4 and drift-monitoring reference require honest incomplete-evidence results.

**Location:** `crates/vala/vala-eval/src/blocks/drift.rs:57-76`, `DriftChecks::judge`.

**Evidence and complete sibling tracing:** `DriftChecks::new` accepts valid nullable RecordBatches without a completeness precondition. `judge` selects a slot batch and directly calls `score_drift`; engine `check_bindings` intentionally has no ordinary drift binding to inspect. `score_drift` (`baseline/mod.rs:129-170`) delegates Custom to `score_custom` (`custom/mod.rs:29-68`). Its `collect_f64_non_null` (`feature.rs:28-40`) removes null rows, then `score_custom_mean` compares the surviving mean. Configured metric `ms`, baseline 100, threshold 5, selected `[Some(100), None]` becomes NoDrift → Passed check via `scored` → Passed requirement via `Judged::checked` → Passed one-requirement `all`.

All production scoring entry points were traced. Existing direct host `DirectExecutor::drift` (`verification/direct.rs:173-234`) requires every configured selected feature value present and uses `DriftReport::unscored` when incomplete; Custom selects its configured metric name. Existing scheduled host `fold_custom` (`verification/drift.rs:628-644`) requires positive count, `n == numeric_n` and finite valid mean before `try_verify:727` calls the aggregate scorer. PSI/SPC raw scorers independently apply target completeness. Remaining scorer callers are tests. No upstream standalone producer enforces null-free samples. Hosted scheduling/source-completeness admission remains outside this task, but nulls in the adapter's supplied selected population are already observable here.

**Consequence:** a missing selected measurement is discarded and the requirement/Verifier can appear healthy. Unchanged numerical math is insufficient evidence that the adapter preserves existing incomplete-window behavior.

**Minimum correction:** preserve the existing configured-metric completeness rule at `DriftChecks::judge` before invoking the unchanged Custom scorer. A null in the selected metric yields the existing unscored Custom report through `scored`, retaining detail/window provenance. Resolve the metric by name, preserve missing-column/type/scorer refusals, ignore unrelated columns, and keep math/host/scheduling/baseline policy unchanged. The guard belongs at this selected raw-evidence boundary because it owns the complete supplied population before a deliberately null-filtering numerical helper loses row-count information. Do not export the drift crate's private target representation, change public low-level scoring semantics, impute samples or claim upstream watermark completeness.

**Ladder:** completeness cannot be removed; existing host behavior defines the sufficient rule; existing Arrow column/null APIs and unscored projection implement it without a dependency or harness.

**Focused closure:** nullable selected Custom metric `[100, null]` must produce Inconclusive requirement and aggregate with supplied non-None window/detail retained. Complete matching and shifted numeric controls remain Passed/Failed. Use the existing IO-free drift/engine fixture, exact new selector, existing drift selector and isolated vala-eval/vala-drift library checks. No statistical algorithm or integrated journey change is prescribed.

## FIND-TASK-002-5 — required touched-item rustdoc is incomplete

**Status:** REVISED. **Classification:** VIOLATION. **Sources:** REPO-001, M-002.

**Obligation:** AGENTS §16 and agent-rules require substantive rustdoc for all new/materially changed items, `# Errors` for fallible functions, relevant panic/invariant and async cancellation contracts. Missing rustdoc is explicitly blocking in the task-review skill. No additional style-only finding is retained.

Each complementary claim was checked against the full body and callers:

| Claimed item | Adjudication |
| --- | --- |
| `tasks/assertion.rs:74` `execute_assertion` (M-002) | CONFIRMED: changed comparison calls materially modify this function; no rustdoc exists. Document scoped selection, single/each comparison, result attribution and propagated selection/type errors. Nearby `AssertionTaskExecutor::assert` accurately identifies those errors. Do not broaden this into documentation of untouched `resolve_observed`/`extract`. |
| `blocks/engine/tests.rs:143` `run` | CONFIRMED: `Result` requires `# Errors`; helper also requires object input and valid slot keys, and forwards engine deadline/cancellation behavior. Document these actual preconditions/contracts. |
| `Script::calls` `:55`, `record` `:60`, ActionExecutor `execute` `:71`, CheckExecutor `execute` `:109` | CONFIRMED: lock `expect` paths or forwarding calls retain the poisoned-lock panic precondition. Action execution additionally waits on barrier/pending scripts and cancellation ends those futures; describe that behavior. Existing Errors sections on both executor methods already exist; do not claim they are missing. |
| `verifier` `:123` | CONFIRMED: supplied JSON must deserialize and validate; docs currently omit these panic preconditions. |
| test `engine` `:130` | CONFIRMED: positive concurrency is required; it also inherits the public semaphore upper bound. |
| `requirement` `:181`, `step` `:190` | CONFIRMED: missing requested result ID panics, currently undocumented. |
| `verdict` `:209` | CONFIRMED: arbitrary supplied citation JSON must deserialize as citations; currently undocumented. |
| `samples` `:960` | REJECTED as a reachable panic omission: one Float64 field and exactly one constructed Float64 array necessarily satisfy `try_new`; input cannot produce type/count/length mismatch. No new test, Result, or fabricated caller panic requirement is warranted. |
| `drift_requirement` `:973` | REVISED: `method.as_object()` requires object-valued supplied method; document it. The constructed check is unconditionally an object, so its `as_object_mut()` invariant is not another caller panic condition. |
| `blocks/scope.rs:308` test `path` | CONFIRMED: supplied suffix must form a valid Reference; document that parsing precondition. |
| `tasks/trace.rs:158` `trace_executor` module | CONFIRMED: changed visibility exposes the module as shared test support; neither inner nor outer module rustdoc exists. Add its role, without rewriting untouched helpers/tests in that module. |
| shared trace `span` `:231` | REJECTED as a reachable panic omission: both UTC timestamps are fixed valid constants; arbitrary typed trace/span/name/scenario inputs cannot invalidate them. Existing docs explain sharing and fixture role. Do not invent a timestamp input precondition or rewrite the fixture. |
| public engine `new` `engine.rs:95` | CONFIRMED: `NonZeroUsize` does not exclude counts above Tokio's `Semaphore::MAX_PERMITS`; document that actual constructor panic bound. No new validation API or public error is requested. |
| `drive` `engine.rs:160` | REVISED: document its invariant that the plan is derived from the same immutable spec and its invalid-plan lookup panic. Production `run` guarantees this; do not report an independently reachable production panic or add redundant validation. |
| `requirement_at` `engine.rs:718` | REVISED: document in-bounds step/requirement key preconditions and indexing panic. All current callers generate valid ranges; this is private invariant documentation, not a demonstrated runtime defect requiring guards. |

**Consequence:** changed assertion selection/error behavior and shared helper contracts remain undocumented in violation of the explicit mandatory gate; Clippy does not prove private/test docs complete. Constructor bounds also matter to legitimate Rust callers.

**Minimum correction:** complete only retained touched-item rustdoc above, alongside implementation remediation. Document actual existing behavior; remove obsolete docs/items if finding 1 eliminates them rather than preserving dead machinery to document it. Keep fixed fixture invariants honest; no new runtime validation, lint suppression, check script or test harness. Inspect the final touched write set once so a moved/new symbol is not left undocumented.

**Focused closure:** source review against function bodies and callers, formatting and the same narrow vala-eval Clippy including tests. No additional runtime test is needed for prose. This is batched with material implementation corrections, not a style-only remediation round.

## Verification, excluded claims and final recommendation

`review-verification.md` records root's independent 195 passing library tests, check-only formatting, dependency checks and all-features lib/tests Clippy against this candidate. Recorded implementation evidence includes 15 exact selectors. I inspected their test bodies and limits; neither green source compilation nor discovery agreement proves the omitted mixed-cause, ready-deadline or incomplete-Custom paths. No new failing test was inserted because reviewed source must remain immutable; these findings are source-proven and each names the smallest targeted closure proof.

Historical tests-after-draft ordering is truthfully recorded. It cannot be repaired by manufacturing RED history and is not a separate runtime finding. Current `requirement_at` uses a lazy match rather than eager fallback indexing, matching the reported original lookup-panic correction; globally distinct fixture requirement IDs match the declared validation rule. No assertion disabling, timeout inflation, skip or suppression was found in those fixes. Local import layout and packet tracking are non-blocking process/style notes; no packet commit or formatting-only follow-on task is required by this ledger. No optional SPC/provenance test expansion is demanded absent a demonstrated defect.

Issue one bounded self-contained remediation task for **FIND-TASK-002-1 through FIND-TASK-002-5**, routed to `$wyrd-implement`. Findings 2/3 attach to the shared lifecycle correction; finding 4 remains the narrow drift selected-evidence boundary; finding 5 is batched prose. All outcomes are fixed by approved authority and existing owners, so neither task correction through wyrd-plan nor SPEC_REVISION_REQUIRED is warranted. Subsequent review must cover the complete original base-to-remediated candidate and prove every prior ID closed.

Skipped integration/journeys and additional Cargo runs per bounded task scope and source immutability. Residual acceptance risks are precisely the five retained findings; hosted integration remains unproven until the integrated change review.
