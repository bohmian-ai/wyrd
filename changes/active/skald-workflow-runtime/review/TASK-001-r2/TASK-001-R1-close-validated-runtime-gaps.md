---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 10
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-5, FIND-TASK-001-6, FIND-TASK-001-7, FIND-TASK-001-8, FIND-TASK-001-9, FIND-TASK-001-10, FIND-TASK-001-11, FIND-TASK-001-12, FIND-TASK-001-13, FIND-TASK-001-14, FIND-TASK-001-15, FIND-TASK-001-16, FIND-TASK-001-17, FIND-TASK-001-18, FIND-TASK-001-19]
---

# Close validated TASK-001 runtime and proof gaps

## Authority and immutable inputs

- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  revision 10.
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`.
- Review verdict:
  `changes/active/skald-workflow-runtime/review/TASK-001-r2/verdict.md`.
- Validated ledger:
  `changes/active/skald-workflow-runtime/review/TASK-001-r2/findings-validation.md`.
- Original base: `a51af030b6039eea4b2914f3ebf2c31925d08721`.
- Reviewed candidate: `28473e049705595306f2934cf4bc664168254086`.

Revision 10 closes prior FIND-TASK-001-3 and FIND-TASK-001-4. Do not reopen or
remediate those public shapes: `ExternalGatewayBinding.secret_headers` remains
`HashMap<HeaderName, SecretString>`, and `ProviderError::RemoteProblem` remains
`RemoteProblem(Box<RemoteProblem>)` over the public five-field payload.

Route this task directly to `$wyrd-implement`. Reassess the complete cumulative
candidate against the original task after implementation.

## Diagnosis and required correction

### FIND-TASK-001-1 — exact run-size accounting

`StepPayload::charged_bytes` uses raw text length, but `RunLedger` treats the
charge as the serialized replacement cost. JSON escaping expands quotes,
backslashes, and control characters, so a successful returned run can exceed
`max_run_bytes`.

Keep admission with `RunLedger` and reuse the existing `jcs_len` mechanism to
charge the exact JCS delta between the reserved step representation and the
candidate payload before committing it. Preserve payload-free terminal reserve,
the step-result ceiling, structured-output accounting, and the existing 413
projection. Do not introduce another serializer or allocator-size estimate.

### FIND-TASK-001-2 — native Responses continuation

The OpenAI Responses Agent loop filters every `Reasoning` output item before
the next stateless request, and the existing wire variant omits the identity
needed for replay. A reasoning-model tool call therefore loses provider-native
continuation state.

Extend the existing Responses wire owner with only the fields needed for
stateless replay and retain reasoning items in provider order with the function
call and function-call output. Preserve authored `previous_response_id`, the
single Agent loop, and native shapes for all other dialects. Do not add a second
loop or cross-dialect translation.

### FIND-TASK-001-5 — enforce size before observation

The Agent emits the full `ProviderResponse` to `on_model_result` before
Workflow applies `max_step_result_bytes`. A result later rejected as oversized
has already crossed the configured observation boundary.

Reuse the existing task-local observer scope to enforce the Workflow ceiling
before forwarding payload-bearing model-result events. Keep the Agent observer
API and loop as the sole pipeline. Preserve ordinary in-limit observations,
journals, provider-native execution, and the terminal 413 result; add no
parallel Workflow event surface.

### FIND-TASK-001-6 — callback failure and liveness isolation

Workflow directly awaits start, binding-failure, attempt, result, backoff, and
finish callbacks. Parent callback panics escape; step callback panics can become
internal failures; pending callbacks can prevent cancellation, deadline, or
terminal return. This contradicts the observer's best-effort contract.

Route every Workflow callback through one observer-owned best-effort invocation
boundary. Isolate panics, race nonterminal callbacks against the already-owned
cancellation and absolute deadlines, compute the attempt deadline before
attempt-start observation, and ensure terminal notification does not retain the
completed return. Preserve callback order/data, authoritative run outcomes,
task ownership, and deadline precedence. Reuse Tokio and existing state; add no
scheduler, queue, or timeout configuration.

### FIND-TASK-001-7 — test import ownership

Five changed tests declare imports inside function bodies, contrary to the
repository rule that a test module's import block exposes its dependencies.

Move and deduplicate only those imports into the two existing `#[cfg(test)] mod
tests` import blocks in `workflow.rs` and `workflow_surface.rs`. Preserve the
scenario grouping and tests; do not split files or add a harness.

### FIND-TASK-001-8 — docs and examples evidence

The candidate changes documentation and Rust/Python/YAML examples but records
neither `mise run docs:check` nor `mise run check:examples` (or every touched
example task).

Run the two existing lanes and record successful results in the remediation
evidence. Fix any red result at its source. Do not add a check or substitute the
broad aggregate merely to produce evidence.

### FIND-TASK-001-9 — exact named-test evidence

The task record names four Rust tests but supplies only package/target shorthand,
which can pass without selecting each named test.

Run and record exact `mise exec -- cargo nextest run --locked` commands with
explicit package, target, and exact test expressions for:

- `agent_run_executes_openai_responses_tool_loop`;
- `responses_session_turns_seed_native_items`;
- `request::round_trip::messages_roundtrip`; and
- `request::untagged_dispatch::message_num_untagged_dispatch_per_provider`.

Confirm the exact fully qualified names with `cargo nextest list` if necessary.
Do not add or rename tests merely to satisfy the evidence rule.

### FIND-TASK-001-10 — Python SDK ownership

This task added Workflow conversions, authoring/validation methods, result
projection, `PyWorkflowRun`, and registration to the retained
`skald-workflow` migration boundary while the Python SDK remains an aggregator.
Repository authority permits existing wrappers to remain temporarily but places
new behavior in `sdks/wyrd-sdk-python/src`.

Leave untouched legacy wrappers where they are. Move only this task's new
Python conversions, authoring/validation/result projection, `PyWorkflowRun`,
and registration into the existing SDK owner, calling the Rust-native
`Workflow` APIs. Preserve public `wyrd.agent` imports, Rust-native engine
ownership, and the shared runtime bridge. Duplicate no validation, execution,
transport, or lifecycle behavior.

### FIND-TASK-001-11 — exact Python result declarations

Generated public declarations reduce fixed `WorkflowStepResult`,
`WorkflowRunError`, and complete run dictionaries to nested `Any`, so field
drift cannot be caught by Python typing.

Update the SDK-owned annotation/generator source to emit `TypedDict`
projections for the step result, run error, and complete run dictionary. Keep
only genuinely JSON-valued outputs/details broad and keep status as a literal
union. Runtime values remain ordinary wire-shaped dictionaries; add no runtime
DTO classes and do not hand-edit generated stubs.

### FIND-TASK-001-12 — observer contract documentation

Six concrete Rust observer forwarding/bridge methods lack required item docs,
and three public Python hooks omit signature-aligned argument contracts.

Document the existing methods in place, including fan-out or bridge behavior,
swallowed callback errors, payload omission, stable error-code meaning, and
duration units as applicable. Add aligned Python `Args` sections. Change no
signature or behavior and add no documentation helper.

### FIND-TASK-001-13 — generated step-ID guide

The public guide says step IDs equal Agent names, while `next_step_id`
sanitizes invalid characters, prefixes leading digits, synthesizes unnamed
values, and suffixes collisions. Users can therefore author bindings or
dependencies to the documented but nonexistent ID.

Correct the guide to describe deterministic IDs derived from names, state the
transformations, and direct callers to the resulting `Workflow.steps` value
when the name is not already a unique valid identifier. Keep the algorithm and
authoring API unchanged.

### FIND-TASK-001-14 — pre-poll cancellation state

The parent records a step as running before spawn, while the child increments
the attempt counter only on first poll. Cancellation may abort first, after
which settlement records `cancelled` with zero attempts—an impossible active
snapshot.

At the existing executor settlement owner, leave an aborted task whose attempt
counter is zero for `RunLedger::finish` to terminalize as `unstarted` with no
timestamps. Call `step_cancelled` only after an attempt began. Preserve
already-begun cancellation, pending work, ordinary drain, and all timestamp and
attempt invariants. Do not add downstream normalization.

### FIND-TASK-001-15 — checked deadline construction

Full-range authored timeouts and public local `Duration` values reach unchecked
`Instant + Duration` at run, step, and Agent deadline construction and can
panic.

Use checked construction at the existing validation/preparation owners. Reject
an unrepresentable authored timeout through the field-specific Workflow
validation path and an unrepresentable local run deadline through the existing
pre-dispatch `WorkflowResult`. Preserve representable semantics, cancellation,
and deadline precedence. Do not clamp, saturate, or silently drop a bound.

### FIND-TASK-001-16 — representable retry count

`max_retries == u32::MAX` is accepted even though its required first attempt
beyond those retries cannot fit the public `u32` attempts field. Execution can
panic or wrap and fail to terminate.

At pure `WorkflowSpec` validation, reject only `u32::MAX` using the existing
field-specific validation error. Preserve every other value, defaults,
backoff, and exact accounting. Do not add downstream saturation or widen an
internal counter that still cannot project to the public field.

### FIND-TASK-001-17 — reserved binding headers

`ExternalGatewayBindings::insert` validates secret values but not header names,
and merge forwards any name. A bound `Host` can route credentials to another
virtual backend behind the screened TLS origin; framing, forwarding, proxy, and
internal names can alter transport behavior.

Reuse the existing route-header classification by separating its
transport/routing/internal subset from the Card-only credential subset. Apply
the former during binding insertion while continuing to permit binding-owned
`authorization`, API-key, and token headers. Preserve authored credential
refusal, origin/protocol validation, collision handling, and screened pinned
egress. Do not create a second unrelated denylist.

### FIND-TASK-001-18 — reflected credentials in refusal errors

ExternalGateway uses shared status handling that retains a bounded refusal body
in `ProviderError::Status`; derived `Debug` exposes it. A gateway that received
a credential can reflect it into that error before the later safe Workflow
projection.

Sanitize only external-gateway non-success errors at `ExternalGatewayClient`.
Preserve status and retry metadata, replace the refusal body with a fixed safe
diagnostic, and leave native-provider error behavior unchanged. Preserve body
bounds, retry classification, success decoding, and the existing safe Workflow
projection.

### FIND-TASK-001-19 — cumulative clean-diff evidence

The immutable base-to-candidate `git diff --check` fails on one extra terminal
blank line in the committed round-one `findings-validation.md`. No-argument
working-tree checks do not inspect that range.

Remove only that extra terminal blank line. Preserve the report's content and
all production source. Prove the cumulative base-to-remediation-candidate range
with explicit `git diff --check`; do not run a formatter sweep or add another
check.

## Constraints and non-goals

- Preserve every Revision 10 public shape, error code, serialization name,
  retry/deadline precedence rule, and non-goal not explicitly corrected above.
- Keep durable and provider behavior in its current Rust owners. The Python
  move is boundary ownership only and must not duplicate core behavior.
- Keep native-provider transport/error behavior outside the external-gateway
  sanitization unchanged.
- Preserve SSRF resolve-screen-pin behavior, TLS verification, disabled proxies
  and redirects, response bounds, and credential-header collision checks.
- Add no crate, third-party dependency, public route, remote Python/TypeScript/
  MCP Workflow surface, compatibility alias, scheduler, queue, serializer,
  runtime DTO class, or repository check.
- Do not weaken, ignore, allowlist around, or delete a failing test or check.
- Do not broaden this remediation into unrelated wrapper migration, test
  splitting, documentation redesign, or provider refactoring.

## Acceptance criteria

| Finding | Required observable proof |
|---|---|
| FIND-TASK-001-1 | Escaped text at the ceiling is discarded with the exact run-too-large error or yields a canonical run within `max_run_bytes`. |
| FIND-TASK-001-2 | The second Responses request preserves returned reasoning, function call, and function-call output in native order. |
| FIND-TASK-001-5 | Over-limit provider output never reaches a payload-bearing observation; the step returns the exact 413 result and in-limit output remains observable. |
| FIND-TASK-001-6 | Panicking or pending callbacks cannot change the authoritative result or prevent cancellation/deadline/return; no later attempt begins after terminalization. |
| FIND-TASK-001-7 | No changed test function contains a `use`; formatting and lints pass. |
| FIND-TASK-001-8 | `mise run docs:check` and `mise run check:examples` exit zero and are recorded. |
| FIND-TASK-001-9 | Each of the four named tests is selected by an exact nextest command and passes. |
| FIND-TASK-001-10 | New Workflow PyO3 behavior and registration are owned by the SDK; public imports and retained-feature compilation pass without duplicated core behavior. |
| FIND-TASK-001-11 | Generated declarations expose precise step/run-error/run dictionaries and codegen/typecheck pass. |
| FIND-TASK-001-12 | Concrete Rust and public Python observer hooks document their actual arguments, units, forwarding, and error behavior. |
| FIND-TASK-001-13 | The guide accurately describes generated IDs and the builder contract test remains green. |
| FIND-TASK-001-14 | Never-polled aborted work is unstarted with zero attempts/no timestamps; begun work is cancelled with at least one attempt. |
| FIND-TASK-001-15 | Maximum authored/local deadlines return stable pre-dispatch errors without panic or dispatch; ordinary precedence remains green. |
| FIND-TASK-001-16 | `u32::MAX - 1` retries validates, `u32::MAX` is rejected before dispatch, and ordinary exhaustion accounting remains exact. |
| FIND-TASK-001-17 | Binding insertion rejects routing/framing/forwarding/proxy/internal names and accepts ordinary credential names before dispatch. |
| FIND-TASK-001-18 | A reflected credential is absent from complete external error `Display`/`Debug` and Workflow projection while retry classification remains unchanged. |
| FIND-TASK-001-19 | Explicit base-to-remediation-candidate `git diff --check` exits zero. |

## Focused and broader verification

Add or extend only the nearest existing tests named in the validated ledger:

- `terminal_budget_reserve` for escaped text and exact run accounting;
- the existing Responses tool-loop test for reasoning replay;
- existing Workflow observer/runtime tests for pre-observation rejection,
  callback panic/pending behavior, pre-poll cancellation, deadline rejection,
  and retry-bound validation;
- existing bound external-gateway security/provider tests for reserved names and
  reflected-secret refusal errors;
- public Python Workflow tests/type fixtures for SDK ownership and exact result
  declarations; and
- the existing builder contract test for step-ID behavior.

Every specifically named Rust test must be run through an exact
`mise exec -- cargo nextest run --locked` package/target/test expression. Then
run the smallest existing scoped lanes that cover the touched surfaces:

- `mise run test:skald`;
- `mise run py:test:unit` and `mise run py:typecheck`;
- `mise run codegen:check`;
- `mise run check:pyo3-scope` and `mise run check:client-tier`;
- `mise run fmt`, `mise run lints`, `mise run py:format`, and
  `mise run py:lints` as applicable;
- `mise run docs:check`;
- `mise run check:examples`; and
- explicit `git diff --check
  a51af030b6039eea4b2914f3ebf2c31925d08721..<remediation-candidate>`.

If any selected lane is red, diagnose and fix the failure rather than recording
it as pre-existing or weakening the gate. A later `$wyrd-task-review` must audit
the complete cumulative candidate against the original TASK-001 and this
remediation task.

## Implementation evidence

This evidence was recorded under spec Revision 11 (REQ-053) and
`TASK-001-R1-addendum-revision-11.md`.

- Observer deletion and tracing: `47a231ab3`
- Docs generator: `757bf6cbc`
- Python `_init` removal: `0935c4d63`

Every exact command below was prefixed with
`CARGO_TARGET_DIR=/home/thorrester/Documents/GitHub/wyrd/target` and exited 0.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-1 | `4f61a70fa`: exact JCS growth charged for retained step payloads | `workflow::tests::terminal_budget_reserve` | PASS |
| FIND-TASK-001-2 | `30519fa38`, `1f9905085`: native Responses reasoning items replayed in order | `loop_responses` `agent_run_executes_openai_responses_tool_loop`, `responses_session_turns_seed_native_items`; `request::round_trip::messages_roundtrip`; `request::untagged_dispatch::message_num_untagged_dispatch_per_provider` | PASS |
| FIND-TASK-001-5 | Closed by deletion: no payload-bearing observation exists. The engine enforces the ceiling at admission in `skald-workflow/src/attempt.rs` (`AttemptOutcome::from_agent` with `max_step_result_bytes` returns `WYRD_WORKFLOW_413_STEP_RESULT_TOO_LARGE`). Spans carry no payloads. | `workflow::tests::terminal_budget_reserve` (exact 413, one attempt, no text); `workflow::tests::run_tracing_spans` (no payload in any span attribute) | PASS |
| FIND-TASK-001-6 | Closed by deletion: there is no callback boundary, and spans are synchronous `tracing` that cannot change outcomes. The `AttemptSpan` drop guard records `cancelled` without affecting control flow. | `workflow::tests::run_tracing_spans` (cancelled run: status `cancelled`, attempts `[1 cancelled]`); `workflow::tests::bounded_attempt_lifecycle` | PASS |
| FIND-TASK-001-7 | `4f61a70fa`: test imports moved into module import blocks | `mise run fmt`, `mise run lints` | PASS |
| FIND-TASK-001-8 | `757bf6cbc`: `generate_api_docs.py` aligned with the `error.rs` catalog and regenerated. `check:examples` now builds `wyrd-rust-examples` (the removed `otel` feature is gone). | `mise run docs:check`, `mise run check:examples` | PASS |
| FIND-TASK-001-9 | Exact selectors are listed under "Exact named tests". | 15 exact nextest commands, each running 1 test that passed | PASS |
| FIND-TASK-001-10 | `b2f0a7b70`: Workflow PyO3 lives in `sdks/wyrd-sdk-python/src/workflow.rs`. The surviving part after deletion: `extract_observers` and the observer parameters are removed, and `skald_observer::python` registration is gone. | `mise run check:pyo3-scope`, `mise run py:test:unit`, `mise run py:typecheck` | PASS |
| FIND-TASK-001-11 | `b2f0a7b70`: precise step, run-error and run TypedDict declarations | `mise run codegen:check`, `mise run py:typecheck` | PASS |
| FIND-TASK-001-12 | Closed by deletion: no observer hooks remain to document. The REQ-053 spans are documented in `docs/src/content/docs/how-to/build-a-workflow.svx` (Observe). | `mise run docs:check` | PASS |
| FIND-TASK-001-13 | `5c474e747`: the step-ID paragraph in `build-a-workflow.svx` is kept. Its observer-hook part was removed with the system. | `workflow_surface::tests::explicit_builder_contract` | PASS |
| FIND-TASK-001-14 | Pre-poll abort reports `unstarted` (kept from `51c8189f7`/`2f909d860`) | `workflow::tests::bounded_attempt_lifecycle` | PASS |
| FIND-TASK-001-15 | Checked deadline construction (kept from `51c8189f7`) | `workflow::tests::bounded_attempt_lifecycle`; `workflow_surface::tests::resolved_bindings_reject_before_dispatch` | PASS |
| FIND-TASK-001-16 | Retry bound (kept from `51c8189f7`) | `workflow::tests::bounded_attempt_lifecycle` | PASS |
| FIND-TASK-001-17 | `128c0a93f`: reserved binding header names are refused | `workflow::tests::bound_external_gateway_security` | PASS |
| FIND-TASK-001-18 | `1affd4be0`: reflected refusal bodies are withheld | `workflow::tests::bound_external_gateway_security` | PASS |
| FIND-TASK-001-19 | `415a28175`: extra blank line removed | `git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721..HEAD` exit 0 | PASS |
| REQ-053 deletion (addendum 1) | Removed: the `skald-observer` crate and workspace member, the Agent and Workflow hooks, `observe.rs`/`StepResultCeiling`, `observers`/`with_observers`, the `wyrd` re-exports and tests, Python `Observer`/`OtelObserver`/`observers=` and `_init`, stubs, tests, examples, docs, architecture references, and the `client-tier`/`pyo3-scope`/`error-coverage`/`test-families` references. No alias was added, and `wyrd.observe` is untouched. | `mise run check:client-tier`, `check:pyo3-scope`, `codegen:check`, `py:test:unit`, `scripts/checks/error-coverage.sh` | PASS |
| REQ-053 tracing (addendum 2) | `workflow.rs`: `workflow.run` (id, run id, step count, status), a child `workflow.step` per attempt (step id, attempt, outcome, `error.type`), and the `workflow.step.backoff` event (next attempt, delay). `loop_runtime.rs`: `invoke_agent`, `chat` and `execute_tool` with GenAI attribute names. All are payload-free and use the existing `wyrd_telemetry::init_test_capture`. | `workflow::tests::run_tracing_spans`; `agent_timeout` `agent_run_emits_genai_spans_without_payloads` | PASS |
| Engine-enforced ceiling (addendum 3) | `attempt.rs` `AttemptOutcome::from_agent` | `workflow::tests::terminal_budget_reserve` | PASS |
| Agent timeout coverage (addendum 4) | `observer_timeout.rs` rewritten as `agent_timeout.rs`, asserting results, journal and spans | `agent_timeout` `agent_run_timeout_terminates_cleanly`, `agent_run_no_timeout_runs_to_completion` | PASS |

### Diagnosis

- **Symptom:** `py:test:unit` failed at collection with `ImportError: cannot import name '_init' from 'wyrd._wyrd'`.
- **Cause:** `_init` was registered by the deleted `skald_observer::python` and only installed the observer bridge.
- **Fix site:** removed `_init` from `python/wyrd/__init__.py` and `scripts/assemble_stubs.py`, then regenerated the stubs. There are no other callers.

### Exact named tests

```bash
mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::terminal_budget_reserve)'
mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::bounded_attempt_lifecycle)'
mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::bound_external_gateway_security)'
mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::run_tracing_spans)'
mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::explicit_namespaced_results)'
mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::isolated_route_calls)'
mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow_surface::tests::resolved_bindings_reject_before_dispatch)'
mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow_surface::tests::explicit_builder_contract)'
mise exec -- cargo nextest run --locked -p skald-agent --test loop_responses -E 'test(=agent_run_executes_openai_responses_tool_loop)'
mise exec -- cargo nextest run --locked -p skald-agent --test loop_responses -E 'test(=responses_session_turns_seed_native_items)'
mise exec -- cargo nextest run --locked -p skald-agent --test agent_timeout -E 'test(=agent_run_timeout_terminates_cleanly)'
mise exec -- cargo nextest run --locked -p skald-agent --test agent_timeout -E 'test(=agent_run_no_timeout_runs_to_completion)'
mise exec -- cargo nextest run --locked -p skald-agent --test agent_timeout -E 'test(=agent_run_emits_genai_spans_without_payloads)'
mise exec -- cargo nextest run --locked -p skald-spec --lib -E 'test(=request::round_trip::messages_roundtrip)'
mise exec -- cargo nextest run --locked -p skald-spec --lib -E 'test(=request::untagged_dispatch::message_num_untagged_dispatch_per_provider)'
```

### Lanes (all exit 0)

`mise run fmt`, `lints`, `py:format`, `py:lints`, `codegen:check`,
`check:client-tier`, `check:pyo3-scope`, `check:unwrap-audit`, `test:skald`,
`py:test:unit` (490 passed), `py:typecheck`, `docs:check` and `check:examples`.
Also: `scripts/checks/error-coverage.sh`, `cargo nextest run --locked -p wyrd`
(3 passed), `-p vala-eval` (126 passed), and
`git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721..HEAD`.

The non-goals stayed excluded: no new crate, dependency, route, scheduler,
queue, serializer, runtime DTO class, repository check or telemetry-init API.
`wyrd-telemetry` is a dev-dependency only, using its existing `test-support`
feature.
