# TASK-001 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `eb22b03f2bb766886d839bda23aafbd4ba130ab3`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 9
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Candidate identity was rechecked before writing this report and remained unchanged.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001–005, REQ-007–009, REQ-012–013A: one Agent-only declarative graph, exact bindings, pure validation, and resolved Prompt validation | `wyrd-spec/src/card/workflow.rs`; `skald-workflow/src/plan.rs` | `explicit_workflow_contract`; `resolved_bindings_reject_before_dispatch` exist and exercise the stated contract cases | PASS |
| REQ-006, REQ-010–012, REQ-020–023; AC-005/007/008: namespaced results, dependency visibility, deterministic scheduling/error selection, and complete terminal snapshots | `skald-workflow/src/{plan,workflow,run}.rs` | `explicit_namespaced_results` and lifecycle assertions in `bounded_attempt_lifecycle` | PASS |
| REQ-016–019, REQ-047–048; AC-020: bounded attempts, exact retry classification/backoff, cancellation/deadline precedence, peer drain, and owned task lifetime | `skald-workflow/src/{workflow,attempt}.rs` | `bounded_attempt_lifecycle` covers concurrency, retry, timeouts, cancellation, and drop; new observer callbacks remain able to change the result (INV-003) | FAIL |
| REQ-017/045, INV-023; AC-019: complete snapshot uses exact JCS accounting, including metadata/errors, and never exceeds `max_run_bytes` | `skald-workflow/src/run.rs`; `StepPayload::charged_bytes` | `terminal_budget_reserve` covers ordinary text but not JSON-escape expansion; source trace proves an admitted terminal snapshot can exceed the configured limit (INV-001) | FAIL |
| REQ-017/022 and the task's Scenario 6 constraint: oversized step data enters neither retained snapshots nor observations | Step payload is rejected in `AttemptOutcome::from_agent` after the Agent loop returns | `terminal_budget_reserve` proves snapshot discard only; the provider response is delivered to `Observer::on_model_result` before that check (INV-002) | FAIL |
| REQ-035–040, REQ-043, INV-009/011/020: per-step route precedence and immutable request/fallback/deadline/correlation | `skald-workflow/src/{plan,route,workflow}.rs`; `skald-providers/src/error.rs` | `isolated_route_calls` exercises route isolation and RemoteProblem projection | PASS |
| REQ-042/049, INV-010/010A/012/017; AC-016/023: one scoped ExtGateway transport with exact origin/header checks, screened DNS, no proxy/redirect, TLS, and redacted secrets | `wyrd-spec/src/card/workflow.rs`; `skald-workflow/src/route.rs`; `skald-providers/src/{endpoint,clients/external}.rs` | `bound_external_gateway_security` plus relocated endpoint-policy tests | PASS |
| REQ-024/051 and Scenario 7: Rust/Python explicit authoring and portable `outputs`/`steps` over one async engine | `skald-workflow/src/{workflow_surface,python}.rs`; public Python exports/stubs/tests | `explicit_builder_contract`; `test_explicit_workflow_bindings`; recorded Python/typecheck/codegen lanes | PASS |
| REQ-039: OpenAI Responses retains native input/function-call items through the Agent loop | `skald-spec/src/message.rs`; `skald-agent/src/{request_builder,session,loop_runtime}.rs` | `loop_responses` integration target and request/message round-trip tests exist; request assembly is native rather than cross-dialect | PASS |
| REQ-052, INV-008; AC-026: resolved Agent tools remain declared, caller-supplied local tools can run, and undeclared tools cannot | `Workflow::append_agent_step` retains `Agent`; execution calls the existing Agent loop | `explicit_builder_contract` asserts declared tool output and zero calls for undeclared tool | PASS |
| Non-goals: no remote Python/TypeScript/MCP workflow surface, compatibility engine, new crate, third-party dependency, or gateway credential administration | Complete base-to-candidate file/dependency diff | Static diff inspection | PASS |
| REQ-051/AC-024: direct consumers compile and scoped lanes are green | Complete cumulative diff across Skald, Wyrd consumers, schemas, SDK Python, examples, and TS error codes | Task records `test:skald`, `test:shared`, `test:wyrd`, Python/typecheck/codegen/boundary/format/lint lanes as exit 0; this reviewer inspected the named tests and sources but did not rerun the aggregate lanes | PASS WITH RECORDED EVIDENCE |

## Proposed findings

### INV-001 — INCORRECT — escaped text can make an admitted WorkflowRun exceed `max_run_bytes`

- Violated obligation: REQ-017, REQ-045, INV-023, AC-019, and Scenario 6 require exact complete-snapshot JCS accounting and guarantee that retained terminal snapshots never exceed `max_run_bytes`.
- Location: `crates/skald/skald-workflow/src/attempt.rs:34-41`; `crates/skald/skald-workflow/src/run.rs:84-99,128-152,318-340`.
- Evidence: `terminal_reserve` budgets the payload-free snapshot, where `text` is JSON `null`. `RunLedger::step_succeeded` then charges `StepPayload::charged_bytes`, which uses raw UTF-8 `String::len()` for text. Replacing `null` with a JSON string does not cost its raw byte length: quotes and JSON escaping are part of the canonical snapshot. For example, a text payload made of `N` quote characters costs `N` according to `charged_bytes` but serializes as a JCS string of `2N + 2` bytes. With an unrelated small declared output, a limit between the charged amount and the real canonical size is admitted as `succeeded` even though `WorkflowRun::canonical_len()` exceeds the limit. The current test uses ordinary unescaped text, so it does not falsify this path.
- Observable consequence: a server can retain and return a WorkflowRun larger than its configured hard snapshot ceiling; downstream retention/capacity assumptions based on INV-023 are false.
- Required testable correction: make the run ledger admit the exact canonical delta caused by replacing the reserved `null` payload fields (and the empty output object) with their candidate values, or equivalently verify the candidate snapshot's exact JCS size before committing it while preserving the terminal reserve. Add one focused case using quote/backslash/control-character text with the ceiling set exactly around the escape expansion, and assert either `WYRD_WORKFLOW_413_RUN_TOO_LARGE` with no payload or `canonical_len() <= max_run_bytes`.

### INV-002 — VIOLATION — oversized provider output reaches observers before Workflow size enforcement

- Violated obligation: Scenario 6's explicit constraint that no oversized payload enters snapshots or observations, together with REQ-017 and REQ-022's bounded result/observation behavior.
- Location: `crates/skald/skald-agent/src/loop_runtime.rs:720-746`; `crates/skald/skald-workflow/src/workflow.rs:472-480`; `crates/skald/skald-workflow/src/attempt.rs:74-123`.
- Evidence: the Agent loop calls `observer.on_model_result(..., response).await` with the full native `ProviderResponse`. Only after `Agent::run_prompt` completes does the Workflow call `AttemptOutcome::from_agent`, derive the normalized step payload, compare it with `max_step_result_bytes`, and discard it. Therefore a response whose normalized output is rejected as `WYRD_WORKFLOW_413_STEP_RESULT_TOO_LARGE` has already crossed the workflow's active observation boundary. The Workflow-specific step-result event is payload-free, but it does not undo the earlier model-result delivery.
- Observable consequence: a configured result ceiling prevents retention but not observation/export of the same oversized response, defeating the bound and potentially imposing unbounded observer/export work before the terminal error is produced.
- Required testable correction: enforce the Workflow's result bound at the owning observation boundary before forwarding response payload data, while retaining the existing Agent loop and observer system rather than adding a second event pipeline. Add a recording observer case where an oversized model answer produces `WYRD_WORKFLOW_413_STEP_RESULT_TOO_LARGE` and no payload-bearing observation receives that response.

### INV-003 — REGRESSION — new Workflow observer callbacks can fail or stall the run

- Violated obligation: REQ-016/019/047/048 and AC-020 require one bounded executor whose terminal semantics are determined by workflow/agent outcomes, cancellation, and deadlines; `skald-observer::Observer` additionally promises best-effort callbacks whose panics do not propagate into the Agent loop.
- Location: `crates/skald/skald-workflow/src/workflow.rs:180-187,230-243,286-297,300-303,379-425`; `crates/skald/skald-observer/src/observer.rs:7-23,100-128`; `crates/skald/skald-observer/src/composite.rs:133-174`; `crates/skald/skald-observer/src/scoped.rs:12-21`.
- Evidence: every new attempt/result/backoff callback is awaited directly without panic isolation. A panic in a step callback becomes a `JoinError`, which the executor converts into `WYRD_WORKFLOW_500_INTERNAL`, so observability changes the Workflow result. Binding-failure and workflow start/finish callbacks execute in the parent executor future; a panic escapes the run, and a callback that never resolves can prevent cancellation/deadline handling or prevent the terminal snapshot from returning. `with_observer` supplies task-local scope only and performs no isolation.
- Observable consequence: attaching an Observer can turn an otherwise successful workflow into a failed or panicked run, or keep `run_with_options` pending past cancellation/total deadline.
- Required testable correction: route the new Workflow callback invocations through one observer-owned best-effort panic/isolation mechanism and ensure parent-level callbacks cannot block cancellation, deadline terminalization, or return of an already complete snapshot. Add one focused observer fixture that panics (and one cancellation-aware pending callback if the mechanism is async) and assert the WorkflowRun remains determined solely by the underlying workflow outcome.

## Invariant trace notes

- Producer to sink for bindings is coherent: `WorkflowBinding` validates the exact source grammar; `WorkflowSpec::validate` checks visibility; `ExecutionPlan` preserves bindings; the parent ledger selects only succeeded dependency results; the Prompt binder receives converted string values immediately before dispatch.
- Route and secret state remain outside Cards/results: Card routes carry only non-secret headers and a binding name; runtime bindings own `SecretString`; merged secret header values are marked sensitive; adapters are immutable per step/attempt; `RemoteProblem` projects only the approved common fields.
- Task ownership is otherwise bounded: ready work lives in one `JoinSet`; ordinary failure stops later scheduling while draining peers; cancellation/deadline abort and drain; dropping the executor future drops the set.
- Primary ordinary errors are selected from plan order `(stage, step ID)`, independent of completion order. Cancellation and total deadline win through biased selection as required.

## Verification assessment

The task's evidence table at lines 455–465 names real source owners and real tests, and the focused test names are present. The complete recorded scoped verification is credible as compilation/regression evidence, but it does not close INV-001 because the budget test uses unescaped text, INV-002 because it asserts retained state rather than prior observer delivery, or INV-003 because no adversarial Observer is exercised. I did not rerun Cargo/Python aggregate lanes in this independent static pass.

## Overall result

**FAIL**

The candidate largely implements the explicit runtime model, but the three reachable invariant violations above prevent acceptance.
