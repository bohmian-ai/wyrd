# TASK-001 invariant review — round 2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `28473e049705595306f2934cf4bc664168254086`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 10
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`

The candidate began this review at the stated commit. The only changes after the
round-1 production candidate `eb22b03f2bb766886d839bda23aafbd4ba130ab3`
are the round-1 review packet and specification/task metadata for approved
Revision 10. `git diff --quiet eb22b03f2..28473e049 -- crates sdks docs examples`
exits zero. Production source, tests, docs, examples, generated artifacts, and
recorded implementation evidence are therefore unchanged from round 1.

## Navigation and authority coverage

The review traced the contract producer in `wyrd-spec`, plan/lowering and the
run ledger in `skald-workflow`, Agent request/history and observer emission in
`skald-agent`, provider errors and external egress in `skald-providers`,
observer implementations in `skald-observer`, Python projection/aggregation,
and direct documentation/test consumers. Governing authority was `AGENTS.md`,
`architecture/agent-rules.md`, `architecture/wyrd-design.md`,
`architecture/wyrd-doctrine.mdx`, the applicable routed Rust, Python, PyO3,
errors, testing, agent, ownership, and architecture references, approved spec
Revision 10, and the original task. Prior round-1 findings were treated only as
hypotheses and checked against the cumulative candidate.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001–005, REQ-007–009, REQ-012–013A; AC-006: one Agent-only declarative graph, exact bindings, pure and resolved validation | `crates/wyrd-spec/src/card/workflow.rs`; `crates/skald/skald-workflow/src/plan.rs` | `explicit_workflow_contract`; `resolved_bindings_reject_before_dispatch` | PASS |
| REQ-006, REQ-010–012, REQ-018, REQ-020–023; AC-005/007/008: stable namespaced values, deterministic scheduling/projection, complete terminal snapshots | `skald-workflow/src/{plan,workflow,run}.rs` | `explicit_namespaced_results`; `bounded_attempt_lifecycle` | FAIL — escaped payload accounting can exceed the terminal ceiling and an active step can be cancelled with zero attempts (INVAR-R2-001, INVAR-R2-008). |
| REQ-016–019, REQ-047–048; INV-003/004/014/016; AC-020: exact bounded attempts, deadlines, cancellation, peer drain, callback isolation, and owned lifetime | `skald-workflow/src/{workflow,attempt}.rs`; `skald-observer` | `bounded_attempt_lifecycle` does not exercise adversarial callbacks, pre-poll cancellation, unrepresentable deadlines, or the maximum retry count | FAIL — INVAR-R2-004, INVAR-R2-008, INVAR-R2-009, INVAR-R2-010. |
| REQ-017/022/045, INV-023; AC-019: exact step/run ceilings and no oversized payload in snapshots or observations | `AttemptOutcome::from_agent`; `RunLedger`; Agent `on_model_result` | `terminal_budget_reserve` covers ordinary retained data only | FAIL — INVAR-R2-001 and INVAR-R2-003. |
| REQ-035–040, REQ-043, INV-009/011/020; AC-011/011A/015: isolated routes, remote errors, and provider-native Agent loops | `skald-workflow/src/{plan,route,workflow}.rs`; `skald-providers/src/error.rs`; `skald-agent/src/request_builder.rs` | `isolated_route_calls`; `loop_responses` | FAIL — Revision 10 closes the `RemoteProblem` seam, but the Responses continuation still removes native reasoning items (INVAR-R2-002). |
| REQ-042/049, INV-010/010A/012/017; AC-016/023: exact-origin bindings, reserved headers, bounded pinned transport, and secret containment | `wyrd-spec/src/card/workflow.rs`; `skald-workflow/src/route.rs`; `skald-providers/src/clients/{external,mod}.rs` | `bound_external_gateway_security` and endpoint-policy tests omit bound reserved names and reflected credentials | FAIL — INVAR-R2-011 and INVAR-R2-012. |
| REQ-024/051 and Scenario 7: exact Rust/Python authoring and portable run projection over one engine | `skald-workflow/src/{workflow_surface,python}.rs`; public Python declarations | Python tests, typecheck, and codegen recorded green | FAIL — the new wrapper remains outside the SDK owner and declarations erase exact nested DTOs (INVAR-R2-006). |
| Revision-10 packet seams for runtime secret headers and normalized remote problems | `route.rs:100-112`; `skald-providers/src/error.rs:59-88`; direct constructors/matches | Static type/source inspection; previously recorded all-feature lint | PASS — approved `HashMap<HeaderName, SecretString>` and `RemoteProblem(Box<RemoteProblem>)` match exactly; FIND-TASK-001-3/4 close. |
| REQ-051 and repository rules for imports, documentation, generated surfaces, and touched docs/examples | Cumulative changed files and completion evidence | Recorded scoped lanes omit docs/examples and four exact named-test selectors | FAIL — INVAR-R2-005, INVAR-R2-006, INVAR-R2-007. |
| Non-goals: no remote Python/TS/MCP Workflow API, compatibility engine, new crate/dependency, credential administration, or second executor | Complete base-to-candidate diff | Static cumulative-diff inspection | PASS |

## Prior-finding reconciliation

| Prior ID | Round-2 result | Evidence |
|---|---|---|
| FIND-TASK-001-1 | Remains open | `StepPayload::charged_bytes` still charges raw text length while `RunLedger` treats it as a serialized replacement charge. |
| FIND-TASK-001-2 | Remains open | `request_builder.rs:162-171` still filters every Responses `Reasoning` item. |
| FIND-TASK-001-3 | **CLOSED by Revision 10** | Spec/task and `route.rs:100-112` now all require/expose `HashMap<HeaderName, SecretString>`. |
| FIND-TASK-001-4 | **CLOSED by Revision 10** | Spec/task and `skald-providers/src/error.rs:59-88` now all require/expose `RemoteProblem(Box<RemoteProblem>)` over the public five-field payload. |
| FIND-TASK-001-5 | Remains open | Agent emits the full provider result before Workflow applies `max_step_result_bytes`. |
| FIND-TASK-001-6 | Remains open | Workflow callbacks are still directly awaited outside or before the relevant timeout/cancellation selects. |
| FIND-TASK-001-7 | Remains open | Five changed test bodies still contain function-scoped `use` declarations. |
| FIND-TASK-001-8 | Remains open | Completion evidence still omits `mise run docs:check` and `mise run check:examples`. |
| FIND-TASK-001-9 | Remains open | The four Responses/spec tests still have only package/target shorthand, not exact recorded selectors. |
| FIND-TASK-001-10 | Remains open | New PyO3 authoring/conversion/result behavior remains in `skald-workflow/src/python.rs`. |
| FIND-TASK-001-11 | Remains open | `WorkflowRun.steps`, `error`, and `to_dict` still erase exact nested DTOs to `Any`. |
| FIND-TASK-001-12 | Remains open | Concrete observer forwarding/bridge methods still lack required item docs and Python hooks lack aligned argument sections. |
| FIND-TASK-001-13 | Remains open | The guide still says step IDs are Agent names despite sanitization/prefix/collision behavior. |
| FIND-TASK-001-14 | Remains open | Parent marks running before spawn; aborted never-polled tasks are still written cancelled with attempt count zero. |
| FIND-TASK-001-15 | Remains open | Public durations still reach unchecked `Instant + Duration`. |
| FIND-TASK-001-16 | Remains open | `u32::MAX` retries is still valid input although the required first attempt cannot fit the public counter. |
| FIND-TASK-001-17 | Remains open | Secret-binding insertion still validates values but not reserved/routing header names. |
| FIND-TASK-001-18 | Remains open | External-gateway refusal text still becomes `ProviderError::Status.body`, whose derived `Debug` exposes it. |

## Proposed findings

### INVAR-R2-001 — INCORRECT — serialized run accounting undercharges text payloads

- Prior ID: FIND-TASK-001-1.
- Violated obligation: REQ-017/045, INV-023, AC-019/020, and Scenario 6 require the complete JCS-serialized terminal snapshot to fit `max_run_bytes`.
- Exact location: `crates/skald/skald-workflow/src/attempt.rs:34-41`; `crates/skald/skald-workflow/src/run.rs:84-99,128-152,318-340`.
- Evidence: the producer charges `String::len`; the reserve contains JSON `null`; the sink serializes a JSON string, whose quotes, backslashes, and controls expand. The ledger admits the raw length as if it were the exact replacement delta.
- Observable consequence: a succeeded returned `WorkflowRun` can have `canonical_len() > max_run_bytes`.
- Required testable correction: keep admission in `RunLedger`, reuse `jcs_len`, and charge the exact candidate replacement delta before committing it. Prove quote, backslash, and control-character text immediately around the ceiling.

### INVAR-R2-002 — INCORRECT — Responses reasoning continuation state is discarded

- Prior ID: FIND-TASK-001-2.
- Violated obligation: REQ-039 and Scenario 4 require the native OpenAI Responses dialect through the existing Agent tool loop.
- Exact location: `crates/skald/skald-agent/src/request_builder.rs:162-171`; `crates/skald/skald-spec/src/wire/openai_responses.rs:339-364`; `crates/skald/skald-agent/tests/loop_responses.rs:107-137`.
- Evidence: the producer emits `Reasoning`, but `assistant_message` filters that variant before the next stateless request; the focused test codifies the omission.
- Observable consequence: a reasoning-model tool loop does not replay the native continuation returned by the first response and may be rejected or lose context.
- Required testable correction: retain the response fields required to replay the existing reasoning item in native order; keep authored `previous_response_id` behavior and the single Agent loop unchanged. Prove reasoning, function call, and function-call output ordering in the current integration test.

### INVAR-R2-003 — VIOLATION — result-size enforcement occurs after payload observation

- Prior ID: FIND-TASK-001-5.
- Violated obligation: REQ-017/022 and Scenario 6 prohibit oversized step data in observations as well as retained snapshots.
- Exact location: `crates/skald/skald-agent/src/loop_runtime.rs:720-746`; `crates/skald/skald-workflow/src/workflow.rs:471-480`; `crates/skald/skald-workflow/src/attempt.rs:74-123`.
- Evidence: Agent passes the complete `ProviderResponse` to `on_model_result`; only after `run_prompt` returns does Workflow normalize and enforce `max_step_result_bytes`.
- Observable consequence: a payload rejected with `WYRD_WORKFLOW_413_STEP_RESULT_TOO_LARGE` has already crossed the configured observation boundary.
- Required testable correction: reuse the existing task-local observer scope and enforce the Workflow ceiling before forwarding payload-bearing model-result events. Prove an oversized response is not delivered while ordinary observations remain unchanged.

### INVAR-R2-004 — REGRESSION — observer callbacks can own Workflow failure and liveness

- Prior ID: FIND-TASK-001-6.
- Violated obligation: REQ-016/019/047/048 and AC-020 require bounded terminalization, while `Observer` promises best-effort callbacks whose panics do not enter Agent results.
- Exact location: `crates/skald/skald-workflow/src/workflow.rs:180-188,230-243,286-304,377-425`; `crates/skald/skald-observer/src/observer.rs:7-23`; `crates/skald/skald-observer/src/python.rs:248-339`.
- Evidence: start, binding-failure, attempt, result, backoff, and finish callbacks are directly awaited. Panics escape or become step failures; a pending callback sits outside/before the applicable select. The Python bridge provides a concrete indefinitely blocking `spawn_blocking` join.
- Observable consequence: adding an observer can panic, fail, or indefinitely stall an otherwise terminating Workflow, including past cancellation/deadline.
- Required testable correction: route all new Workflow hooks through one existing-observer-owned panic-isolated path, race nonterminal hooks with already-owned cancellation/deadlines, and do not delay return for terminal notification. Prove panic and pending hooks at every boundary.

### INVAR-R2-005 — VIOLATION — required repository proof remains incomplete

- Prior IDs: FIND-TASK-001-7, FIND-TASK-001-8, FIND-TASK-001-9, FIND-TASK-001-12, FIND-TASK-001-13.
- Violated obligation: repository import/documentation rules and the task's required exact verification evidence.
- Exact location: function-local imports at `workflow.rs:811,1246,1473,1692` and `workflow_surface.rs:848`; observer implementations at `skald-observer/src/composite.rs:132-174` and `python.rs:248-339`; Python hooks at `python/wyrd/observer.py:160-188`; guide at `docs/src/content/docs/how-to/build-a-workflow.svx:19`; evidence at task lines 454-466.
- Evidence: the prohibited imports and missing concrete-hook documentation remain; the guide still equates generated IDs with Agent names; docs/examples lanes and exact selectors for four named tests remain absent.
- Observable consequence: the candidate violates explicit repository acceptance rules, misdirects binding authors, and lacks required proof for changed docs/examples/tests.
- Required testable correction: move/deduplicate imports at the existing test-module scopes; document the existing concrete callbacks and Python arguments; correct the step-ID statement; run the two existing docs/example lanes and exact one-test selectors. Add no new harness.

### INVAR-R2-006 — VIOLATION — Python ownership and exact result typing remain broken

- Prior IDs: FIND-TASK-001-10 and FIND-TASK-001-11.
- Violated obligation: AGENTS.md §§7–8 and the task place new PyO3 behavior in `wyrd-sdk-python`; the public Python contract must preserve exact DTO structure.
- Exact location: `crates/skald/skald-workflow/src/python.rs:41-84,347-420,529-657`; `sdks/wyrd-sdk-python/python/wyrd/agent/__init__.pyi:401-439` and generated duplicate.
- Evidence: the SDK aggregator still delegates registration while new conversion, builder, run, and result projection live in the owner crate; `steps`, `error`, and `to_dict` still use nested `Any`.
- Observable consequence: the migration boundary grows new Python-only behavior and type checking cannot detect run/step/error field drift.
- Required testable correction: move only this task's new wrappers/projection into the existing SDK owner while calling the Rust-native engine; generate `TypedDict` shapes for exact DTOs while leaving JSON-valued fields broad. Prove public imports, Python tests, typecheck, codegen, and PyO3 scope.

### INVAR-R2-007 — INCORRECT — public guide names the wrong generated step IDs

- Prior ID: FIND-TASK-001-13 (also covered by INVAR-R2-005 as a repository violation).
- Violated obligation: public authoring documentation must match the binding contract.
- Exact location: `docs/src/content/docs/how-to/build-a-workflow.svx:19`; `crates/skald/skald-workflow/src/workflow_surface.rs:611-637`.
- Evidence: the guide says IDs equal Agent names, while the producer sanitizes, prefixes leading digits, synthesizes unnamed IDs, and suffixes collisions.
- Observable consequence: callers can bind/declare dependencies using the documented name and receive unknown-step errors.
- Required testable correction: document deterministic derivation and direct callers to the resulting `Workflow.steps` ID when the name is not already a unique identifier; prove with the existing builder test and docs lane.

### INVAR-R2-008 — INCORRECT — pre-poll cancellation records an active zero-attempt step

- Prior ID: FIND-TASK-001-14.
- Violated obligation: REQ-021/INV-003 and snapshot cases require begun active work to have at least one attempt; never-begun work is unstarted with no timestamps.
- Exact location: `crates/skald/skald-workflow/src/workflow.rs:205-228,249-285,365-381`; `crates/skald/skald-workflow/src/run.rs:120-125,166-172,211-220`.
- Evidence: the parent records `running` and time before spawn; the child increments only when first polled. Abort may win first, and the parent writes `cancelled` with zero attempts.
- Observable consequence: equivalent cancellation produces contract-distinct snapshots based only on Tokio scheduling.
- Required testable correction: at settlement, leave a cancelled task whose shared counter is zero for `RunLedger::finish` to make unstarted; use `step_cancelled` only after an attempt began. Prove both deterministic branches.

### INVAR-R2-009 — INCORRECT — public deadlines can panic instead of terminalizing

- Prior ID: FIND-TASK-001-15.
- Violated obligation: repository input/error rules and the task's pre-dispatch/terminal contract prohibit public timeout input from unwinding execution.
- Exact location: `crates/wyrd-spec/src/card/workflow.rs:349-368`; `crates/skald/skald-workflow/src/plan.rs:278-282`; `crates/skald/skald-workflow/src/workflow.rs:188,382,436-444`.
- Evidence: full `u64` seconds and arbitrary local `Duration` reach unchecked `Instant + Duration` in run, step, and Agent deadline construction.
- Observable consequence: authored YAML or local options can panic rather than return a stable error or run snapshot.
- Required testable correction: use checked construction at the owning validation/preparation boundary and reject unrepresentable values through existing field-specific errors; do not clamp or drop deadlines. Prove maximum values never dispatch or panic.

### INVAR-R2-010 — INCORRECT — accepted retry count cannot fit its result field

- Prior ID: FIND-TASK-001-16.
- Violated obligation: REQ-016 and the exact snapshot contract define attempts as `max_retries + 1` in `u32`.
- Exact location: `crates/wyrd-spec/src/card/workflow.rs:360-368`; `crates/skald/skald-workflow/src/workflow.rs:365-427`; `crates/skald/skald-workflow/src/run.rs:331-340`.
- Evidence: `u32::MAX` is accepted; the last required `attempt += 1` panics in debug or wraps in release. Reserve saturation does not repair execution semantics.
- Observable consequence: valid decoded input can panic/fail to terminate and cannot report the promised count.
- Required testable correction: pure validation rejects only `u32::MAX` through the existing retry-field error. Prove `u32::MAX - 1` remains valid and `u32::MAX` rejects before dispatch.

### INVAR-R2-011 — INCORRECT — secret bindings can override transport/routing headers

- Prior ID: FIND-TASK-001-17.
- Violated obligation: REQ-042/049, INV-010/017, and AC-016 require exact-origin binding and pre-dispatch reserved-header refusal.
- Exact location: `crates/skald/skald-workflow/src/route.rs:144-185,452-491`; authored classifier at `crates/wyrd-spec/src/card/workflow.rs:585-607`.
- Evidence: binding insertion checks values only; merge forwards any `HeaderName`, including `Host`, framing/hop-by-hop, forwarding/proxy, and Wyrd-internal names. Programmatic binding construction is a public approved path.
- Observable consequence: `Host` can route credentials to a virtual backend other than the approved origin; sibling names can alter framing/proxy/correlation behavior.
- Required testable correction: reuse the existing classifier's transport/routing/internal subset at binding insertion while continuing to allow bound credential names. Prove case-insensitive rejection plus valid Authorization/API-key bindings.

### INVAR-R2-012 — INCORRECT — external refusal errors retain reflected credentials

- Prior ID: FIND-TASK-001-18.
- Violated obligation: REQ-042, INV-012, and AC-016 prohibit bound secrets in errors or logs.
- Exact location: `crates/skald/skald-providers/src/clients/external.rs:130-149`; `crates/skald/skald-providers/src/clients/mod.rs:40-75`; `crates/skald/skald-providers/src/error.rs:9-30`.
- Evidence: the external client returns the shared status error containing the complete bounded refusal body. `ProviderError` derives `Debug`, so a gateway that reflects a credential exposes it through error inspection/logging.
- Observable consequence: external-client error debug output can disclose a bound credential even though Workflow projection is safe.
- Required testable correction: sanitize only external-gateway non-success bodies at `ExternalGatewayClient`, retaining status/retry metadata and native-provider behavior. Prove a reflected canary is absent from complete `Display`, `Debug`, and Workflow projection while retry classification is unchanged.

## Invariant trace assessment

The graph/binding producer-to-sink path is otherwise coherent: pure parsing and
visibility validation feed an immutable resolved plan; the ledger exposes only
succeeded dependencies; Prompt conversion occurs immediately before dispatch;
routes are immutable per attempt; secrets remain runtime-only; and ordinary
primary errors are chosen in plan order. Revision 10 removes the only two
public-seam conflicts without changing these paths. It does not amend any
obligation or implementation underlying the twelve source-local findings above.

## Verification assessment

The task records green Skald/shared/Wyrd/Python/typecheck/codegen/boundary/format
and lint lanes. This static repeat review did not rerun aggregates. The recorded
tests still do not exercise JSON-escape expansion, reasoning continuation,
pre-observation limits, adversarial callbacks, pre-poll cancellation,
unrepresentable deadlines, maximum retries, bound reserved headers, or reflected
secrets. Required docs/examples lanes and four exact selectors remain absent.

## Overall result

**FAIL**

Revision 10 closes FIND-TASK-001-3 and FIND-TASK-001-4 exactly. The other
round-1 findings remain reachable because their production sources and
governing obligations are unchanged; the candidate therefore does not yet
satisfy the original task.
