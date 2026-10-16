# Independent maintainer review — TASK-002 R1

Overall result: **PASS**. No material maintainer finding proposed.

## Subject and method

Reviewed original base `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0` through candidate `99cf2f18a8cadfe3c500cf7157934047112d156a`, with `d342c54749a871dcacf5b0c7ec29e3b458f2d9e2` used only to locate remediation changes. HEAD remained the candidate. Reviewed the frozen subject, approved revision 6, original task, prior verdict and independently validated finding ledger, remediation task, cumulative source changes and recorded verification. No current-round discovery report or implementation conclusion was used to establish this result.

Applied AGENTS.md, agent-rules, Wyrd design/doctrine, reference-router, spec-driven-development, maintainer-style, Rust-core, ownership patterns, evaluation and drift-monitoring guidance. The approved specification explicitly replaces the old authored implementation-kind split; unrelated architectural constraints still apply. CodeGraph preceded focused source and caller navigation.

## Changed-surface coverage

| Surface | Symbols, owner and consumer evidence | Maintainer assessment |
| --- | --- | --- |
| Public composition and dependency | `blocks.rs` exports `VerificationEngine`, `EngineError`, `DriftChecks`; `Cargo.toml` and lockfile add the existing `vala-drift` workspace dependency | Required reusable Vala facade; statistical implementation remains in its narrow owner. No SDK or binding declarations changed. |
| Shared execution lifecycle | Complete `executor.rs::RunLedger`, `Running`, `Prepared`, `walk`, `execute_plan`, `prepare_task`, `Executors::of`, `first_skipped_dependency`; retained `scenario.rs::execute_scenario` and `orchestrator/scoring.rs::score_record` callers | One stage/fanout/settlement owner serves both contracts. Errors, cancellation and deterministic append order are visible in the shared owner. Old kind buckets, their fanout, and separate merge workflow are removed. |
| Verifier orchestration | Complete `blocks/engine.rs`: constructor/run, drive, step/requirement preparation, attempt/judge/trace and supporting result adaptations, binding preparation, block keys and aggregation | Dependency-owning engine exposes the workflow. Contract adaptation is local and does not retain another scheduler or settled-state map. Pure preparation/selection stays synchronous. |
| Scope and attribution | Complete `blocks/scope.rs`: Cause/Missing, Settled/Done, Scope, resolution, selection and scope test | Scope is a borrowed read view of original declarations plus the common ledger. Cause precedence, null versus missing, status reads and original-slot propagation can be followed without reconstructing a second state authority. |
| Existing context and registry | `context.rs::ContextSnapshot`, `ExecutionContext`, `TaskOutput`, scope construction/extension; `store.rs::TaskRegistry` indexing/validation and callers | The old JSONPath/context contract and task definitions remain adapters for preserved scenarios. Their retained representation does not duplicate mutable lifecycle ownership. New block identity indexing cannot reuse `TaskRegistry<EvalTask>` unchanged without restoring prohibited retired-judge dispatch. |
| Statistical adapter | Complete `blocks/drift.rs::DriftChecks`, judge/scored/inconclusive/provenance; drift engine tests and original scorer boundary | Samples are keyed by slot, fitted state by requirement, window by execution. Adapter composes unchanged scorers and report projection. Custom completeness guard sits before the intentionally null-filtering scorer. |
| Shared deterministic leaves | `tasks/assertion.rs::execute_assertion`, `Compared`, `evaluate_single`, `evaluate_item_mode`; trace projection and shared fixture/module; `results.rs::capture_actual` and `apply_context_capture` | Existing owners supply comparison, span shape and capture semantics to both paths. The extra adapters replace task-specific argument coupling rather than copying algorithms. |
| Tests and documentation | Complete new engine/scope tests; changed executor test and surrounding error/skip tests; existing assertion, trace, scenario and scoring call paths; all surviving prior FIND-5 items | Scripted executors and shared span fixture make the behavioral purpose apparent. Barrier overlap, paused deadlines, attempts and cancellation have explicit observable assertions. Rustdoc now explains validated helper preconditions, constructor bounds, private index invariants and assertion error propagation. |

## Complete workflow comparison and FIND-TASK-002-1 closure

This is substantive consolidation, not relocation of two intact walkers. `VerificationEngine::drive` (`engine.rs:187–238`) builds a block index and adapts the approved step plan into action/step-requirement/final-requirement stages, then calls `executor::walk`. It has no stage execution loop, JoinSet, completion-order collection or ledger mutation of its own. Both it and `execute_plan` reach `walk` (`executor.rs:232–268`), which exclusively creates the run ledger, prepares each stage against prior settlements, spawns runnable futures, propagates the first error/join failure, restores declaration order and appends settlements.

`Scope` (`scope.rs:123–148`) borrows `RunLedger<Done>`; it no longer maintains step/verdict maps or advances state independently. `ContextSnapshot` remains the old contract's immutable projection of the same ledger into record/JSONPath executor inputs. `TaskRegistry` remains immutable Eval definition lookup. The Verifier block index is definition lookup into its immutable spec, not a graph store or execution-state authority. Reusing the old check-only task enum here would add coupling to the retired judge path expressly excluded by TASK-002.

Contract-specific preparation still differs appropriately: Eval preserves abort-on-executor-error and dependency-skip behavior; Verifier preserves action failures as evidence causes and check failures as inconclusive results. Forcing these distinct contracts into the same output enum or context document would add conversions without sharing another lifecycle. The meaningful common lifecycle has been extended in its established owner.

## Types, allocations and cumulative complexity

`RunLedger<O>` has two real result instantiations: `EvalSettled` and `Done`. Its ordered entries plus ID index support both deterministic reports and downstream lookup. The generic parameter is earned by existing consumers, rather than speculative extensibility. `Running<O,E>` allows the shared stage to carry heterogeneous step and requirement futures; no new public plugin/trait framework was introduced.

`RunLedger` was already publicly re-exported by `lib.rs:69`; its old public fields disappear and the type becomes generic. Repository caller tracing found no external-to-executor dependency on those old fields or concrete nongeneric type. All retained production entry points continue returning the unchanged `EvalReport`. This is an internal library adaptation permitted by the task, not evidence that an SDK/wire contract needs compatibility machinery. No alias or extra wrapper is warranted from the inspected consumers.

`Done` boxes large result payloads while keeping step state directly accessible. Result and state serve distinct consumers: the returned report and reference resolution. This follows the nearby existing `TaskRunOutcome::Ran(Box<AssertionResult>)` representation. Boxes do not introduce a second owner, arbitrary extension point, conversion layer or persistent format. No demonstrated maintenance or behavioral consequence justifies requiring a different equally clear representation.

Supporting `Bindings`, `Attempted` and `Judged` values each own a cohesive intermediate invariant. Block/Key identify locations in an immutable spec; they do not retain executors or spawn work. The cumulative implementation adds the required action/check semantics while removing the old four-bucket lifecycle. I found no deletable duplicate complete workflow, new dependency beyond the required existing statistical engine, compatibility path or unrelated product option.

## Prior findings and documentation

| Prior finding | Source-backed closure within this lens |
| --- | --- |
| FIND-TASK-002-1 | Shared `walk` and `RunLedger` reached by both live entry paths; old parallel fanout/order implementations removed, scope borrows shared settlements. |
| FIND-TASK-002-2 | `requirement` resolves explicit bindings even when its owner has no output and merges both gaps with `Missing::strongest`; the new skipped-owner test asserts failed-ancestor and omitted-slot outcomes plus pure-skip control and no executor call. |
| FIND-TASK-002-3 | `run` checks absolute expiry before planning and before returning the aggregated verdict, while retaining native timeout/drop ownership; elapsed ready-work test covers empty and missing-binding cases. |
| FIND-TASK-002-4 | `DriftChecks::judge` selects the configured Custom metric by name and projects the existing unscored report when it has nulls; test includes nullable unrelated-column and complete matching/shifted controls, missing metric and window/detail assertions. |
| FIND-TASK-002-5 | Surviving validated items now document selection/errors, actual panic preconditions, semaphore maximum and private same-spec/index invariants. Old removed helpers are not preserved solely for documentation. Fixed-fixture timestamp/batch claims remain excluded. |

No material documentation or generated-declaration mismatch was found in the changed public composition. Test imports/layout preferences are not a material maintainer finding and do not justify another remediation round.

## Verification limits

Inspected recorded exact regression commands, 123 vala-eval and 75 vala-drift library results, narrow all-features Clippy, formatting and dependency checks in the frozen verification evidence. Also inspected `review-verification.md`: root independently reran the three new exact regressions, the combined library lane, all-features library/test Clippy, format, dependency and cumulative whitespace checks with exit 0. These are recorded evidence, not checks run by this reviewer. Root owns independent Cargo verification; I ran no Cargo process, mutated no source and wrote only this report.

Skipped integration/journeys and hosted/provider/database execution as required by the isolated task. Residual proof risk is hosted composition, reserved for the integrated change review; no material task-local maintainer defect was identified.
