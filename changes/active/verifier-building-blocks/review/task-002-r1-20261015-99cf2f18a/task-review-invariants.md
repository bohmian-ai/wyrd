# TASK-002 invariant review — PASS

## Immutable subject and coverage

Base: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`. Candidate: `99cf2f18a8cadfe3c500cf7157934047112d156a`; HEAD matched during review. This audit covers the complete cumulative implementation against original TASK-002 and approved specification revision 6, not merely remediation changes. Inputs were the frozen subject, original task, spec, cumulative/remediation diffs, prior verdict/validated ledger, remediation task and verification evidence. No current-round discovery report or implementation summary supplied the conclusions.

Authorities consulted: AGENTS.md, architecture/agent-rules.md, wyrd-design.md, wyrd-doctrine.mdx, reference router, spec-driven-development, maintainer-style, architecture patterns, Rust core, evaluation, drift-monitoring and testing-workflows. Approved r6 supersedes the old authored Verifier split; unrelated boundaries remain binding. CodeGraph was the initial code-navigation mechanism.

Source coverage includes the full engine/scope/drift adapter, established executor/ledger and snapshot integration, changed assertion/trace/result adapters, contract planning/validation, retained scenario/scoring callers, injected execution seam, scorer dispatch/Custom scoring, manifests/lock change, new engine regression bodies and relevant existing executor/assertion/scenario regression paths. No source was modified and no Cargo process was started by this reviewer.

## Producer-to-sink invariant traces

**Shared lifecycle.** `VerifierSpec::plan` (`wyrd-spec/src/card/blocks/validate.rs:106`) produces the existing ExecutionPlan. `VerificationEngine::drive` (`blocks/engine.rs:188`) adapts action stages into actions followed by their requirement stages, then appends final requirements. Both this entry point and `executor.rs::execute_plan:328` delegate to `executor.rs::walk:232`. Only that owner creates stage JoinSets, joins futures, advances stage barriers, appends ordered settlements and owns cancellation. `RunLedger<O>:157` owns the settled-entry/index representation. Scope borrows the ledger; it no longer owns independent settled-step/verdict maps. Legacy ContextSnapshot is an adapter populated from the same ledger through `base.extend(outputs)`, retaining initial context/media/identity. Different preparation semantics are justified by different old/new contracts, not duplicate schedulers.

Retained siblings: `scenario.rs::execute_scenario:195` calls execute_plan at line 212; `orchestrator/scoring.rs` calls it at line 200. Their old kind executors remain callable. New block execution imports no retired JudgeInvoker/JudgeTaskExecutor/Executors::judge/EvalTask::LlmJudge. No new graph planner or store exists. The generic public RunLedger re-export has no existing external source consumers requiring migration.

**Cause conservation.** Scope::resolve distinguishes missing required evidence, optional null, absent children and upstream no-output cause. Status/verdict reads remain independent of output. Scope::resolve_all merges every explicit reference gap with Missing::strongest. prepare combines when/action/workspace gaps before deciding intentional skip. VerificationEngine::requirement resolves explicit bindings even under a skipped/failed owner and combines both sources of gaps before invoking any check. Failure therefore wins over intentional skip; pure intentional gaps remain N/A. Judged::report records one result and decide excludes only N/A.

**Budgets and failures.** run checks absolute expiry before planning and before publishing its verdict, while timeout_at bounds pending drive work. attempt acquires the common semaphore, retries only retryable errors within the step budget, races earlier attempt deadlines, and retains attempts/duration/final reason. judge uses the same semaphore. Check failures are inconclusive, step failures carry failed no-output causes, and overall timeout/join failure is an execution error. Shared walk drops its JoinSet on cancellation/error, aborting remaining work and preventing subsequent stages. This does not promise synchronous preemption or rollback of already performed executor work.

**Evidence/statistical ownership.** Original SlotName sets travel with inputs/workspace, are combined by Bindings::sources, survive step settlement and derived-output selection, and bound accepted Agent/code citations. A pass without any reachable citation becomes inconclusive. Assertions reuse existing comparisons, traces reuse source/project_spans, and result families share capture_actual. DriftChecks selects samples by slot and fitted state by requirement ID, then delegates unchanged statistical math to vala-drift. Its Custom metric null guard runs before non-null averaging loses population completeness; unrelated nullable columns do not block scoring. Scored reports retain typed detail/window provenance. Scheduling, source authorization, persistence, citation locator validation and hosted execution remain their separately assigned owners.

## Complete task acceptance matrix

Test names below identify source proof; exact invocations/results are in verification-evidence.md and review-verification.md.

| Requirement, criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: same engine handles no-step/multi-step; empty requirements cannot pass | run/drive and decide; no special test engine | no_step_verifier_judges_inputs_and_traces; empty_and_all_skipped_verifiers_are_inconclusive | PASS |
| REQ-002: named resolved evidence, required omissions vs optional null, missing child vs explicit null | Scope::resolve/select and requirement preparation; registration retains type/schema admission | missing_and_optional_evidence_resolve_honestly; skipped_owner_keeps_failed_check_evidence | PASS |
| REQ-003: existing planner, independent overlap, action then step checks, dependent barrier, final checks once | VerifierSpec::plan; drive stage adaptation; shared walk/ledger | independent_actions_overlap_within_the_concurrency_bound; dependents_wait_for_upstream_step_requirements | PASS |
| REQ-003: original inputs/output/status/verdict and explicit requirement id/statement; dotted/index/each selection | Scope and check_bindings | scope selection test; each_assertion_compares_every_wildcard_match; ordering/citation tests | PASS |
| REQ-004 task slice: injected actions, bounded retries/timeouts/concurrency, visible final attempt state | attempt, Attempted::settle, BlockCall deadline and common permits | retries_spend_the_budget_and_keep_attempts_visible; step_timeouts_fail_the_step_and_let_unrelated_work_finish; overlap test | PASS |
| REQ-005: deterministic assertions/trace reuse; failed comparison is data; isolated errors inconclusive | shared comparison/projection helpers; trace; Judged adaptation | no-step/each/citation tests; retained assertion and trace regressions | PASS |
| REQ-005 / INV-007: expired absolute deadline and cancellation remain execution errors | run pre/post checks, timeout, shared JoinSet lifetime | elapsed_deadline_refuses_ready_work; execution_deadline_cancels_running_work | PASS |
| REQ-006 / INV-006: direct/transitive skip causes; strongest failure; never call check on missing evidence; status-only checks remain usable | Missing::strongest; Scope; prepare; requirement | skip_causes_propagate_and_failure_wins; skipped_owner_keeps_failed_check_evidence | PASS |
| REQ-006: all/pass-rate semantics, inclusive threshold, inconclusive denominator, all-skipped/empty, once-only aggregation | decide; ledger and requirement-stage ordering | decide_rules_count_inconclusive_requirements; empty/all-skipped and ordering tests | PASS |
| REQ-014 task slice: unchanged PSI/SPC/Custom math, requirement-specific fitted state, slot-specific samples, readiness/insufficient data and detail/window | DriftChecks::judge/scored; unchanged vala-drift | drift_requirements_score_through_vala_drift; custom_drift_needs_complete_measurements; 75 scorer library tests | PASS |
| REQ-015 task slice: complete attributable breakdown, check config, original evidence, actual capture, executor provenance | run, Judged::report, Attempted::settle, capture_actual | no-step/order/capture/citation/drift regressions; shared seam tool-record test | PASS |
| INV-001: extend complete existing lifecycle/state owner; reusable offline Vala engine | walk and generic RunLedger used by both paths; Scope is read projection | retained executor/scenario/assertion regressions; caller tracing above | PASS |
| INV-005: no application runtime/external writes/host sandbox implementation introduced | injected closed Action/Check seams; bounded Vala write set | cumulative diff; check:deps | PASS |
| INV-007 task slice: bounded concurrency/deadline; no interpreter or tenant scratch state introduced | semaphore and JoinSet ownership; per-execution injected executors | overlap/cancellation/timeout tests; source inspection | PASS |
| INV-008: unrelated scenario/offline behavior preserved; TASK-001 contracts untouched; no premature removal of last old consumers | execute_plan adapter preserves snapshot/output/error/skip semantics; callers retained | full eval library lane includes scenario and executor regression coverage | PASS |
| AC-002: ordering, mixed/transitive causes, missing denominator | mapped engine owners above | source-inspected focused tests and 123 eval library tests | PASS |
| AC-003 task-local boundary: task-3 implementations compose through same production seam | existing ActionExecutor/CheckExecutor/BlockCall, public VerificationEngine | scripted execution, position, provenance tests | PASS |
| AC-007 task-local drift slice: selection/readiness/detail retained; hosted schedule/reactions excluded here | existing scorers and adapter | drift/scorer library proof | PASS |
| AC-010 bounded proof | review-verification.md records independent commands/logs | 198 library tests; three exact new regressions; all-features lib/tests Clippy; fmt:check; check:deps; cumulative diff --check all exit 0 | PASS |
| Required touched-item rustdoc remediation | assertion helper; constructor/run/drive/requirement lookup; scope path; shared trace module; Script/test helper docs | source/body comparison and narrow checks | PASS |
| No new graph/store/runtime, statistical policy, SDK/server/SQL/persistence/policy/reactions, public wire format or language-runtime tests | complete cumulative source and manifest/lock diff | static inspection; check:deps | PASS |

This is TASK-002 acceptance. Full extraction/host execution, hosted schedule/reactions, persisted identity and SDK journeys remain other tasks' obligations; their absence here is not missing task scope.

## Prior stable finding closure

| Stable ID | Current evidence | Status |
|---|---|---|
| FIND-TASK-002-1 | executor::walk/RunLedger own shared lifecycle/state; both entry paths use them. Scope borrows settled state; old parallel run_steps/run_requirements/JoinSets/maps are removed. Contract adapters preserve distinct semantics. | CLOSED |
| FIND-TASK-002-2 | requirement resolves bindings before combining owner gap with Missing::strongest. New regression covers failed ancestor, omitted required input, pure skip, no executor call and inconclusive rollup. | CLOSED |
| FIND-TASK-002-3 | run checks absolute deadline before work and after verdict computation. New paused test covers empty and immediately missing-evidence paths; future-deadline empty and pending cancellation controls remain. | CLOSED |
| FIND-TASK-002-4 | selected configured metric is looked up by name and null population returns existing unscored detail. New regression covers unrelated null columns, complete pass/fail, missing column and non-None window/detail. | CLOSED |
| FIND-TASK-002-5 | Surviving validated items now document actual Errors/preconditions/panics/cancellation. Deleted items were not retained for docs. Rejected fixed-fixture claims remain excluded. | CLOSED |

## Proposed findings and result

No proposed blocking findings. The generic walker is earned by two real consumers and removes duplicate ownership; its preparation adapters preserve contract-specific semantics. No reachable regression or missing task outcome was established after tracing producers, state owners and retained siblings.

**Overall result: PASS.** Independent root checks are recorded in review-verification.md; this reviewer ran no overlapping Cargo commands and wrote only this report.

Skipped integration/journeys, provider/database/host-runtime execution and mutation-based RED reproduction under bounded review instructions. Hosted integration remains for integrated change review; this report makes no end-to-end delivery claim.
