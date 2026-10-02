# Task behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `28473e049705595306f2934cf4bc664168254086`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 10
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Review scope: complete cumulative base-to-candidate diff

The candidate was exactly `28473e049705595306f2934cf4bc664168254086` at review start. CodeGraph is absent. The review read the repository authorities, the complete diff, the original task, Revision 10, the r1 reports as hypotheses, and the changed caller-to-result paths. `git diff --quiet eb22b03f2bb766886d839bda23aafbd4ba130ab3..28473e049705595306f2934cf4bc664168254086 -- crates sdks docs examples Cargo.lock` exits 0: Revision 10 changes packet authority only; executable/product source is identical to the r1 candidate.

## Review findings

### Important

#### BEH-R2-001 — INCORRECT: complete-run size accounting undercharges JSON-escaped text

- Prior finding: `FIND-TASK-001-1` remains open.
- Violated obligation: REQ-017, REQ-045, INV-023, AC-019/020, and Scenario 6 require the complete JCS-serialized `WorkflowRun`, including metadata and errors, to fit `max_run_bytes`.
- Exact location: `crates/skald/skald-workflow/src/attempt.rs:34-41`; `crates/skald/skald-workflow/src/run.rs:84-99,128-152,318-340`.
- Evidence and reachable path: `AttemptOutcome::from_agent` builds a text `StepPayload`; `StepPayload::charged_bytes` charges `String::len`; every successful step reaches `RunLedger::step_succeeded`, which admits that charge as the retained payload increment. The terminal reserve serializes `text: null`, while the admitted run serializes the actual string. Quotes, backslashes, and control characters expand in JSON/JCS, so raw UTF-8 length is not the serialized replacement delta. The existing reserve test does not exercise escaping expansion.
- Observable consequence: the runtime can return `succeeded` with `jcs_len(run) > max_run_bytes`, defeating the configured complete-snapshot bound.
- Required testable correction: keep admission on `RunLedger`, but charge the exact JCS delta between the reserved step representation and the candidate retained result (or validate the exact candidate snapshot plus remaining reserve). Reuse `jcs_len`; do not add another serializer. Extend `terminal_budget_reserve` with quote, backslash, and control-character text at the ceiling and assert either bounded retention or `WYRD_WORKFLOW_413_RUN_TOO_LARGE` with the payload discarded.

#### BEH-R2-002 — INCORRECT: OpenAI Responses tool loops drop native reasoning continuation items

- Prior finding: `FIND-TASK-001-2` remains open.
- Violated obligation: REQ-039 and Scenario 4 require the OpenAI Responses dialect to retain its native shape through the Agent tool loop.
- Exact location: `crates/skald/skald-agent/src/request_builder.rs:162-171`; `crates/skald/skald-spec/src/wire/openai_responses.rs:336-358`; `crates/skald/skald-agent/tests/loop_responses.rs:107-137`.
- Evidence and reachable path: `assistant_message` explicitly filters every `OpenAiResponseItem::Reasoning` before rebuilding the next stateless request. `OpenAiResponseItem::Reasoning` omits the item identity needed for replay, and the focused integration test codifies omission. Any Responses reasoning model that emits reasoning before a function call reaches this path.
- Observable consequence: the second request is not the provider-native continuation returned by the first response and may be rejected or lose required reasoning context.
- Required testable correction: extend the existing Responses wire owner with only the response fields required for stateless replay, preserve reasoning items in provider order, and keep authored `previous_response_id` behavior unchanged. Update the existing tool-loop test to require the returned reasoning item, function call, and function-call output in native order.

### Critical

None.

### Suggestions

None. Optional improvements are outside this acceptance audit.

## Revision 10 and prior-finding reconciliation

| Prior finding | Present-round behavior assessment |
|---|---|
| `FIND-TASK-001-1` | **OPEN** — independently re-traced as BEH-R2-001; source and reachable path are unchanged. |
| `FIND-TASK-001-2` | **OPEN** — independently re-traced as BEH-R2-002; source and focused test still deliberately omit reasoning items. |
| `FIND-TASK-001-3` | **CLOSED** — Revision 10 fixes `HashMap<HeaderName, SecretString>` at `spec.md:1246-1264` and the task seam; the candidate exposes that exact type at `route.rs:100-112`. Header order is explicitly non-observable. |
| `FIND-TASK-001-4` | **CLOSED** — Revision 10 fixes `ProviderError::RemoteProblem(Box<RemoteProblem>)` plus the public five-field payload at `spec.md:1005-1018`; the candidate matches at `skald-providers/src/error.rs:59-88`. |
| `FIND-TASK-001-5` | **OPEN HYPOTHESIS, source unchanged** — the Agent still calls `on_model_result` with the full response at `loop_runtime.rs:720-746` before Workflow applies `max_step_result_bytes` in `AttemptOutcome::from_agent`. |
| `FIND-TASK-001-6` | **OPEN HYPOTHESIS, source unchanged** — Workflow callbacks remain directly awaited outside/before cancellation/deadline selection at `workflow.rs:180-188,230-243,377-425`. |
| `FIND-TASK-001-7` | **OPEN HYPOTHESIS, source unchanged** — no production or test source changed after r1. Repository-standards review owns final validation. |
| `FIND-TASK-001-8` | **OPEN HYPOTHESIS, evidence unchanged** — Revision 10 records no new docs/examples lane result. |
| `FIND-TASK-001-9` | **OPEN HYPOTHESIS, evidence unchanged** — Revision 10 records no new exact executions for the four r1 named tests. |
| `FIND-TASK-001-10` | **OPEN HYPOTHESIS, source unchanged** — task-added `PyWorkflowRun` and related behavior remain in `skald-workflow/src/python.rs`. |
| `FIND-TASK-001-11` | **OPEN HYPOTHESIS, source unchanged** — `WorkflowRun.steps`, `error`, and `to_dict` remain nested `Any` declarations at `python/wyrd/agent/__init__.pyi:401-439`. |
| `FIND-TASK-001-12` | **OPEN HYPOTHESIS, source unchanged** — no observer documentation changed after r1. Maintainer review owns final validation. |
| `FIND-TASK-001-13` | **OPEN HYPOTHESIS, source unchanged** — the guide still says step IDs equal Agent names at `build-a-workflow.svx:19`, while `next_step_id` sanitizes and uniquifies them at `workflow_surface.rs:611-631`. |
| `FIND-TASK-001-14` | **OPEN HYPOTHESIS, source unchanged** — the parent marks a step running before spawn at `workflow.rs:210-228`; the child records attempt 1 only when polled at `workflow.rs:365-381`. |
| `FIND-TASK-001-15` | **OPEN HYPOTHESIS, source unchanged** — unchecked `Instant + Duration` remains at `workflow.rs:188,382,436-444`. |
| `FIND-TASK-001-16` | **OPEN HYPOTHESIS, source unchanged** — `attempt += 1` still operates in `u32` at `workflow.rs:368-378`, while the public retry policy accepts `u32::MAX`. |
| `FIND-TASK-001-17` | **OPEN HYPOTHESIS, source unchanged** — `ExternalGatewayBindings::insert` still validates secret values but not reserved names at `route.rs:144-185`, and `merged_headers` forwards them at `route.rs:460-475`. |
| `FIND-TASK-001-18` | **OPEN HYPOTHESIS, source unchanged** — ExtGateway still uses shared status handling that retains the bounded refusal body in `ProviderError::Status` at `clients/external.rs:130-149` and `clients/mod.rs:61-74`. |

Revision 10 changes only the two public-seam decisions. It neither claims nor implements closure for the other r1 findings. The table records source-grounded status for claim comparison; the required specialist reports and fresh Ponytail validator remain responsible for the final ledger.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Scenario 1: exact Workflow contracts, Agent-only graph, binding grammar, graph/input/resolved-Prompt validation (REQ-001–005, REQ-007–009, REQ-012–013A, REQ-036–037, REQ-040, REQ-045; AC-006) | `wyrd-spec/src/card/workflow.rs`; `skald-workflow/src/plan.rs` and `workflow_surface.rs` | Recorded `explicit_workflow_contract` and `resolved_bindings_reject_before_dispatch`; r1 inspection found the declared pre-dispatch validation | PASS |
| Scenario 2: namespaced dependency data, exact binding conversion/output projection, deterministic complete results (REQ-006, REQ-010–012, REQ-020–023; AC-005, AC-007, AC-008) | `skald-workflow/src/workflow.rs`, `run.rs`, `plan.rs` | Recorded `explicit_namespaced_results`; namespaced maps and plan-order primary errors remain unchanged | PASS |
| Scenario 3: bounded concurrency, retry/deadline classification, ordinary drain, cancellation, and owned task lifetime (REQ-016, REQ-018–019, REQ-043, REQ-047–048; AC-020) | `workflow.rs` owned `JoinSet`; `attempt.rs`; `run.rs` | Recorded `bounded_attempt_lifecycle`, but it does not close unchanged callback, pre-poll cancellation, maximum-duration, or maximum-retry paths represented by prior findings 6 and 14–16 | FAIL |
| Scenario 4: exact route dependencies, isolated call context, all five native dialects, safe remote projection (REQ-035–040, REQ-043, INV-020; AC-011A) | `route.rs`; `skald-providers/src/error.rs`; `request_builder.rs` | Revision 10 and candidate now agree on boxed `RemoteProblem`; recorded `isolated_route_calls`; BEH-R2-002 remains | FAIL |
| Scenario 5: scoped ExtGateway transport, exact binding contract, header/origin/protocol checks, screened and pinned egress (REQ-042, REQ-049, INV-010/010A/017; AC-016, AC-023) | `endpoint.rs`, `clients/external.rs`, `route.rs` | Revision 10 and candidate now agree on `HashMap`; recorded security test does not close unchanged reserved secret-header or reflected-secret paths (prior 17/18) | FAIL |
| Scenario 6: exact input/step/full-snapshot JCS budgets and terminal reserve (REQ-017, REQ-045, INV-023; AC-019/020) | `plan.rs::resolve_input`; `attempt.rs::StepPayload`; `run.rs::RunLedger` | Recorded `terminal_budget_reserve` does not cover escaping expansion (BEH-R2-001) or pre-observation rejection (prior 5) | FAIL |
| Scenario 7: Rust/Python explicit authoring, shared engine, portable outputs/steps, declared local tools (REQ-024, REQ-047, REQ-051; AC-024, AC-026) | `workflow_surface.rs`; `skald-workflow/src/python.rs`; generated Python declarations | Recorded builder/Python tests prove ordinary use, but unchanged Python ownership and erased nested DTO typing hypotheses remain (prior 10/11) | FAIL |
| Public run/error schema, UUIDv7 IDs, enum wire names, default maps, deny-unknown DTOs, derive-backed codes | `wyrd-spec` contracts, schemas, generated TypeScript codes | Recorded `codegen:check`, `test:shared`, and `test:wyrd` | PASS |
| Exact Revision 10 external-binding collection seam | `spec.md:1246-1264`; task packet seam; `route.rs:100-112` | Direct source comparison | PASS — prior 3 closed |
| Exact Revision 10 `RemoteProblem` seam | `spec.md:1005-1018`; task packet seam; `skald-providers/src/error.rs:59-88` | Direct source comparison and unchanged direct consumers | PASS — prior 4 closed |
| Existing consumers compile in one cohesive change; no contract-only handoff (REQ-051, AC-024) | Cumulative diff includes Agent/provider/gateway/client/Python consumers | Recorded Skald/shared/Wyrd/Python/all-feature lanes; the two former seam mismatches are resolved, but remaining source defects prevent task acceptance | FAIL |
| Repository documentation and required verification evidence | Changed docs/examples/tests and task evidence | No new docs/examples or exact-test evidence after r1; unchanged guide contradicts step-ID generation | FAIL |
| Non-goals: no remote Python/TS/MCP Workflow runtime, compatibility alias, migration layer, new crate/dependency, Vault/Operator migration, or second engine | Complete diff introduces none of these surfaces | Diff/manifests and caller inspection | PASS |

## Open questions

None. Revision 10 resolves the only two material public-seam questions from r1.

## Verification notes

- The executable source and recorded verification evidence are unchanged from r1. This review used source/diff evidence and did not rerun Cargo-backed lanes, avoiding contention with parallel required reviewers.
- The r1 task review independently ran `terminal_budget_reserve`, `bounded_attempt_lifecycle`, and the Responses tool-loop test successfully. Those green tests do not exercise the reported escaped-text or reasoning-continuation failures.
- Revision 10 records no remediation implementation or additional verification for prior findings 1/2 or 5–18.
- Candidate remained `28473e049705595306f2934cf4bc664168254086` at report completion.

## Overall result

**FAIL**

Revision 10 correctly closes prior `FIND-TASK-001-3` and `FIND-TASK-001-4`, but the cumulative candidate still fails the original task: complete-run accounting can retain a snapshot beyond its JCS byte ceiling, and OpenAI Responses tool-loop continuation still discards native reasoning items. The unchanged source also leaves the remaining r1 findings available for the required specialist and Ponytail validation passes.
