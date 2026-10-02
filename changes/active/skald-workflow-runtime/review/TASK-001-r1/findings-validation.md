# Structured Ponytail validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `eb22b03f2bb766886d839bda23aafbd4ba130ab3`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 9
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Discovery inputs: both task reviews, standards, maintainer, system, concurrency,
  network-security, and focused follow-up reports in this directory

The candidate stayed at the stated commit throughout validation. This pass
read the applicable authorities, cumulative diff, cited bodies, callers,
sibling consumers, and focused tests. No source was changed.

## Claim validation

| Discovery claim | Validation | Final finding | Source-grounded resolution |
|---|---|---|---|
| BEH-001 | **CONFIRMED** | FIND-TASK-001-1 | `StepPayload::charged_bytes` charges raw text bytes while `RunLedger` treats the charge as the serialized delta; JSON escaping makes the retained snapshot larger. |
| BEH-002 | **CONFIRMED** | FIND-TASK-001-2 | The Responses loop deliberately removes every reasoning item, and the focused test codifies that omission. This is reachable for reasoning-model tool calls and fails the required native Responses continuation. |
| BEH-003 | **CONFIRMED** | FIND-TASK-001-3 | The public field is `HashMap`, not the fixed `BTreeMap<HeaderName, _>`. `HeaderName` has no `Ord`, so the approved shape cannot be implemented directly; authority must choose the implementable public shape. |
| BEH-004 | **CONFIRMED** | FIND-TASK-001-4 | The public tuple variant plus separate boxed struct changes construction and exhaustive matching from the fixed struct variant. The lower-authority lint accommodation cannot amend the seam. |
| INV-001 | **CONFIRMED, DUPLICATE** | FIND-TASK-001-1 | Same producer, budget owner, and consequence as BEH-001. |
| INV-002 | **CONFIRMED** | FIND-TASK-001-5 | `on_model_result` receives the native response before `AttemptOutcome::from_agent` applies `max_step_result_bytes`; the later discard cannot retract the observation. |
| INV-003 | **REVISED** | FIND-TASK-001-6 | The panic and pending-callback paths are real, but they share the same missing best-effort callback boundary as SYS-001 and are one finding. |
| STD-001 | **CONFIRMED** | FIND-TASK-001-7 | Five added function-local imports violate the repository's explicit top-of-module import rule. |
| STD-002 | **CONFIRMED** | FIND-TASK-001-8 | Changed docs and examples require `docs:check` and `check:examples` (or every touched example task); neither result is recorded. |
| STD-003 | **CONFIRMED** | FIND-TASK-001-9 | The four newly named Responses/spec tests have only package/target shorthand in completion evidence, not recorded exact selectors. Earlier scenario commands do not cover these four names. |
| MNT-001 | **CONFIRMED** | FIND-TASK-001-10 | New conversion, authoring, result-wrapper, and registration behavior was added to the retained owner-crate migration boundary while the declared SDK owner remains an aggregator. |
| MNT-002 | **CONFIRMED** | FIND-TASK-001-11 | `WorkflowStepResult` and `WorkflowRunError` are exact domain DTOs, but the public declarations erase them into nested `Any` dictionaries. |
| MNT-003 | **CONFIRMED** | FIND-TASK-001-12 | Six new concrete Rust observer implementation methods lack the mandatory item documentation; the three public Python hooks also omit the required signature-aligned argument contract. |
| MNT-004 | **REVISED IN PART, OTHERWISE REJECTED** | FIND-TASK-001-7 | Its import-placement portion is STD-001. Splitting the intentionally task-defined umbrella scenario tests is not independently required by the approved task and is omitted. No new harness is warranted. |
| MNT-005 | **CONFIRMED** | FIND-TASK-001-13 | The guide says step IDs equal Agent names, while `next_step_id` sanitizes, prefixes, synthesizes, and suffixes them. |
| SYS-001 | **CONFIRMED, DEDUPLICATED** | FIND-TASK-001-6 | Parent and step callbacks are directly awaited outside or before the relevant cancellation/deadline selects; Python supplies a concrete blocking implementation. |
| CONC-001 | **CONFIRMED** | FIND-TASK-001-14 | The parent marks a step running before spawn, while the child increments attempts only when polled. Abort in between records `cancelled` with zero attempts. |
| CONC-002 | **CONFIRMED** | FIND-TASK-001-15 | Public `u64`/`Duration` values reach unchecked `Instant + Duration` in three paths and can panic instead of returning a stable validation/run result. |
| CONC-003 | **CONFIRMED** | FIND-TASK-001-16 | `u32::MAX` retries is accepted although `max_retries + 1` cannot fit the public `u32` attempt counter; execution eventually panics or wraps. |
| NET-001 | **CONFIRMED** | FIND-TASK-001-17 | Binding insertion validates values but not names; `merged_headers` forwards a bound `Host` or other routing/framing/internal header unchanged. |
| NET-002 | **REVISED** | FIND-TASK-001-18 | The reflected secret is retained in `ProviderError::Status` and exposed by derived `Debug`. Current Workflow/journal projections use safe display/code data, so claims of propagation into those sinks are omitted. |

## Final validated finding ledger

### FIND-TASK-001-1 — Complete-run accounting undercharges escaped text

- Discovery sources: BEH-001, INV-001
- Status: **CONFIRMED**
- Classification: `INCORRECT`
- Violated obligation: REQ-017, REQ-045, INV-023, AC-019/020, and task Scenario 6 require the complete terminal `WorkflowRun` JCS serialization to fit `max_run_bytes`.
- Exact location: `crates/skald/skald-workflow/src/attempt.rs:34-41`; `crates/skald/skald-workflow/src/run.rs:84-99,128-152,324-340`.
- Evidence and reachability: text results are produced at `AttemptOutcome::from_agent`, charged with `String::len`, admitted by `RunLedger::step_succeeded`, then serialized as JSON. Quotes, backslashes, and control characters expand during JCS serialization; the reserve counted `null`, so raw UTF-8 length is not the replacement delta. `WorkflowExecutor` routes every successful step through this ledger path.
- Observable consequence: the runtime can return `succeeded` with `run.canonical_len() > max_run_bytes`.
- Decision-complete minimum correction: keep accounting on `RunLedger`; before committing a candidate payload, charge the exact JCS delta between the reserved step representation and that candidate (or equivalently validate the exact candidate snapshot plus untouched reserve). Reuse `jcs_len`; do not add a second serializer or allocator-size estimate.
- Focused closure proof: extend `terminal_budget_reserve` with quote, backslash, and control-character text around the exact ceiling; assert the payload is discarded with `WYRD_WORKFLOW_413_RUN_TOO_LARGE` or the returned run's canonical length is within the limit.

### FIND-TASK-001-2 — Responses tool loops drop native reasoning continuation items

- Discovery source: BEH-002
- Status: **CONFIRMED**
- Classification: `INCORRECT`
- Violated obligation: REQ-039 and task Scenario 4 require OpenAI Responses to work in its native request dialect through the Agent tool loop.
- Exact location: `crates/skald/skald-agent/src/request_builder.rs:162-171`; `crates/skald/skald-spec/src/wire/openai_responses.rs:339-364`; `crates/skald/skald-agent/tests/loop_responses.rs:107-137`.
- Evidence and reachability: `assistant_message` filters every `Reasoning` output item; the test expects the omission. A Responses reasoning model that returns reasoning before a function call reaches this path on the next stateless request.
- Observable consequence: the second request is not the native continuation returned by the first response and can be rejected or lose reasoning context.
- Decision-complete minimum correction: extend the existing `OpenAiResponseItem::Reasoning` wire owner with only the response fields required for stateless replay and retain reasoning items in order. Keep the authored `previous_response_id` behavior unchanged and add no alternate loop.
- Focused closure proof: update the existing Responses loop test so the second request contains the returned reasoning item, function call, and function-call output in native order.

### FIND-TASK-001-3 — The fixed external-binding map type is not implementable

- Discovery source: BEH-003
- Status: **CONFIRMED — SPEC_REVISION_REQUIRED**
- Classification: `VIOLATION`
- Violated obligation: the approved spec and packet-local public seam fix `BTreeMap<HeaderName, SecretString>`; REQ-051 requires that exact contract and its consumers to move together.
- Exact location: `changes/active/skald-workflow-runtime/spec.md:1244-1263`; `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md:107-119`; `crates/skald/skald-workflow/src/route.rs:100-112`.
- Evidence and reachability: the candidate publicly exposes `HashMap`; Rust's `HeaderName` does not implement `Ord`, so the approved `BTreeMap` key cannot be used without changing the public key type or adding a wrapper, both different contracts.
- Observable consequence: callers written to the approved field type do not compile against the candidate.
- Required authority decision: revise the approved public seam to an implementable existing collection. The smallest choice is the candidate's `HashMap<HeaderName, SecretString>` because header order is not observable and no wrapper invariant is needed. Do not add an ordered header-name newtype solely to preserve an impossible spelling.
- Closure proof after approval: compile a direct public constructor using exactly the revised type and keep binding collision/security tests green.

### FIND-TASK-001-4 — `ProviderError::RemoteProblem` changes the fixed public variant shape

- Discovery source: BEH-004
- Status: **CONFIRMED — SPEC_REVISION_REQUIRED**
- Classification: `VIOLATION`
- Violated obligation: the approved spec and task fix a direct struct variant with `code`, `status`, `message`, `field`, and `remediation`; REQ-051 requires exact consumer closure.
- Exact location: `changes/active/skald-workflow-runtime/spec.md:1004-1029`; `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md:129-164`; `crates/skald/skald-providers/src/error.rs:59-88`.
- Evidence and reachability: the candidate exports `RemoteProblem(Box<RemoteProblem>)` and a separate public struct. Constructors and exhaustive matches in Workflow and gateway consumers use the tuple shape.
- Observable consequence: downstream construction and matching differ from the approved API even though the runtime fields are equivalent.
- Required authority decision: choose between the fixed direct struct variant and the current boxed representation under the repository's error-size lint. Ponytail recommends approving the current boxed payload rather than adding widespread lint exceptions or another indirection layer.
- Closure proof after approval: compile constructors and matches for exactly the approved shape and rerun provider error plus all direct Agent/Workflow/gateway consumers under all-feature lints.

### FIND-TASK-001-5 — Oversized provider output is observed before Workflow enforcement

- Discovery source: INV-002
- Status: **CONFIRMED**
- Classification: `VIOLATION`
- Violated obligation: REQ-017/022 and task Scenario 6 prohibit an oversized provider payload from entering observations.
- Exact location: `crates/skald/skald-agent/src/loop_runtime.rs:720-746`; `crates/skald/skald-workflow/src/workflow.rs:471-480`; `crates/skald/skald-workflow/src/attempt.rs:74-123`.
- Evidence and reachability: the Agent emits the full `ProviderResponse`; only after `run_prompt` returns does Workflow normalize and reject the step payload. Every Workflow Agent attempt uses this order.
- Observable consequence: a payload rejected as `WYRD_WORKFLOW_413_STEP_RESULT_TOO_LARGE` has already crossed the active observer boundary.
- Decision-complete minimum correction: reuse the existing task-local observer scope and enforce the Workflow result ceiling before delegating payload-bearing `on_model_result` events. Keep Agent's loop and observer API as the single pipeline; do not create parallel Workflow events or copy the rejected payload into diagnostics.
- Focused closure proof: a recording observer receives no payload-bearing model-result event for an over-limit final answer, while the step returns the exact 413 code and ordinary in-limit Agent observation remains unchanged.

### FIND-TASK-001-6 — Workflow callbacks own failure and liveness instead of remaining best-effort

- Discovery sources: INV-003, SYS-001
- Status: **REVISED**
- Classification: `REGRESSION`
- Violated obligation: REQ-016/019/047/048 and AC-020 require cancellation/deadline-bounded terminalization; `Observer` promises best-effort callbacks whose panics do not enter Agent results.
- Exact location: `crates/skald/skald-workflow/src/workflow.rs:180-188,230-243,286-304,377-425`; `crates/skald/skald-observer/src/observer.rs:7-23`; `crates/skald/skald-observer/src/python.rs:248-339`.
- Evidence and reachability: start, binding-failure, result, backoff, and finish callbacks are directly awaited. Parent callback panics escape; step callback panics become internal step failures; pending callbacks sit outside or before relevant timeout selects. `PythonObserver` supplies a concrete indefinitely blocking `spawn_blocking` join.
- Observable consequence: adding an observer can panic, fail, or indefinitely stall an otherwise terminating Workflow, including beyond cancellation or deadline.
- Decision-complete minimum correction: add one observer-owned safe invocation path and route every new Workflow callback through it. Isolate callback panics; race nonterminal callback completion against the already-owned cancellation, total deadline, and (for attempt callbacks) the already-computed attempt deadline. A terminal notification must not delay return of the completed snapshot. Reuse Tokio and the existing cancellation/deadline state; add no observer scheduler or timeout knob.
- Focused closure proof: one deterministic observer fixture panics and blocks at start, attempt-start, result/backoff, and finish boundaries; underlying success/failure remains authoritative, cancellation/deadline returns a complete snapshot, and no further attempt starts.

### FIND-TASK-001-7 — Added tests hide imports inside functions

- Discovery sources: STD-001; MNT-004 (import portion only)
- Status: **CONFIRMED**
- Classification: `VIOLATION`
- Violated obligation: `architecture/agent-rules.md` requires all imports at module scope, including test-module dependencies.
- Exact location: `crates/skald/skald-workflow/src/workflow.rs:811,1246,1473,1692`; `crates/skald/skald-workflow/src/workflow_surface.rs:848`.
- Evidence: the candidate adds function-scoped `use` statements at all five locations.
- Observable consequence: the test module's dependency surface is concealed and repeatedly reintroduced.
- Decision-complete minimum correction: move and deduplicate these imports in the existing `#[cfg(test)] mod tests` import blocks. Do not split tests or add modules solely for this correction.
- Focused closure proof: source inspection plus `mise run fmt` and `mise run lints`.

### FIND-TASK-001-8 — Required docs and example lanes are absent

- Discovery source: STD-002
- Status: **CONFIRMED**
- Classification: `VIOLATION`
- Violated obligation: `AGENTS.md` requires `mise run docs:check` for `docs/` changes and `mise run check:examples` (or each touched example task) for example changes.
- Exact location: changed docs/examples in the cumulative diff; completion evidence at `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md:455-465`.
- Evidence: three docs files and twenty example files changed; neither required result is recorded.
- Observable consequence: the accepted evidence does not prove the changed documentation or examples build and remain valid.
- Decision-complete minimum correction: run the two existing repository lanes and record their successful results; fix any red result rather than adding a new check.
- Focused closure proof: `mise run docs:check` and `mise run check:examples`, both exit zero.

### FIND-TASK-001-9 — Four named Rust tests lack exact recorded execution

- Discovery source: STD-003
- Status: **CONFIRMED**
- Classification: `VIOLATION`
- Violated obligation: `AGENTS.md` and the spec-driven testing reference require an exact `mise exec -- cargo nextest run --locked` selector for every specifically named Rust test.
- Exact location: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md:462`.
- Evidence: `agent_run_executes_openai_responses_tool_loop`, `responses_session_turns_seed_native_items`, `request::round_trip::messages_roundtrip`, and `request::untagged_dispatch::message_num_untagged_dispatch_per_provider` are named with package/target shorthand only.
- Observable consequence: the completion record can pass without proving that each claimed test was selected.
- Decision-complete minimum correction: use the repository-pinned toolchain and record one exact selector for each named test, with its explicit package and `--test` or `--lib` target. No new test is required.
- Focused closure proof: each exact command selects one test and exits zero.

### FIND-TASK-001-10 — New PyO3 Workflow behavior is outside the Python SDK owner

- Discovery source: MNT-001
- Status: **CONFIRMED**
- Classification: `VIOLATION`
- Violated obligation: `AGENTS.md` §§7-8 and the task's ownership boundary place new or materially relocated PyO3 wrappers in `sdks/wyrd-sdk-python/src`; owner-crate wrappers may remain only as migration state.
- Exact location: `crates/skald/skald-workflow/src/python.rs:41-84,347-405,519-656`; aggregator at `sdks/wyrd-sdk-python/src/lib.rs:40-50`.
- Evidence and reachability: the candidate adds conversion, three authoring methods, validation/run conversion, and `PyWorkflowRun` to `skald-workflow`; the SDK only delegates registration.
- Observable consequence: the declared Python boundary owner does not own the new public Python contract, and the migration crate grows new Python-only behavior.
- Decision-complete minimum correction: leave untouched legacy wrappers where they are, but move this task's new conversion, `with_*`/validation/result projection, `PyWorkflowRun`, and their registration into the existing Python SDK crate. Continue calling Rust-native `Workflow`; duplicate no validation or execution logic.
- Focused closure proof: public Python imports and unit tests pass from `wyrd`, retained-feature compilation stays green, and `check:pyo3-scope`, codegen, and typecheck pass.

### FIND-TASK-001-11 — Python declarations erase exact Workflow result DTOs

- Discovery source: MNT-002
- Status: **CONFIRMED**
- Classification: `INCORRECT`
- Violated obligation: the maintainer Python-contract rule requires public result types to describe the actual domain shape; `WorkflowStepResult` and `WorkflowRunError` are exact public DTOs.
- Exact location: `sdks/wyrd-sdk-python/python/wyrd/agent/__init__.pyi:401-439` and generated duplicate `python/wyrd/stubs/agent.pyi`; projection source at `crates/skald/skald-workflow/src/python.rs:574-646`.
- Evidence: `steps`, `error`, and `to_dict` expose nested `Any`, so the checker cannot enforce statuses, counters, timestamps, payload optionality, or error fields.
- Observable consequence: Rust wire-field drift does not produce a Python type failure and callers must infer the contract from prose.
- Decision-complete minimum correction: from the SDK-owned annotation/generator source, emit `TypedDict` projections for the step result, run error, and complete run dictionary; keep genuinely JSON-valued outputs/details broad. Add no runtime DTO classes. Never hand-edit generated stubs.
- Focused closure proof: regenerate cleanly and run `mise run codegen:check` plus `mise run py:typecheck` with a small typing fixture that accesses the exact fields.

### FIND-TASK-001-12 — New observer hooks lack required contract documentation

- Discovery source: MNT-003
- Status: **CONFIRMED**
- Classification: `VIOLATION`
- Violated obligation: `architecture/agent-rules.md` requires substantive rustdoc on every new or materially modified Rust item; the maintainer guide requires public Python docstrings aligned with signatures.
- Exact location: `crates/skald/skald-observer/src/composite.rs:144-174`; `crates/skald/skald-observer/src/python.rs:280-339`; `sdks/wyrd-sdk-python/python/wyrd/observer.py:160-183`.
- Evidence: six concrete Rust implementation methods have no item documentation; three public Python methods omit their arguments' meanings/types.
- Observable consequence: callback ordering, payload omission, error-code meaning, and millisecond conversion are not documented at the concrete forwarding/bridge owners.
- Decision-complete minimum correction: document those existing methods in place, including fan-out/bridge behavior and duration units; add signature-aligned Python `Args` sections. Add no abstraction or behavior.
- Focused closure proof: documentation source inspection, `mise run fmt`, `mise run lints`, and `mise run docs:check`.

### FIND-TASK-001-13 — Workflow guide misstates generated step IDs

- Discovery source: MNT-005
- Status: **CONFIRMED**
- Classification: `INCORRECT`
- Violated obligation: public documentation must match the authoring contract.
- Exact location: `docs/src/content/docs/how-to/build-a-workflow.svx:19`; implementation at `crates/skald/skald-workflow/src/workflow_surface.rs:611-637`.
- Evidence: the guide says IDs are Agent names; `next_step_id` sanitizes invalid characters, prefixes leading digits, synthesizes unnamed IDs, and suffixes collisions.
- Observable consequence: users can author bindings/`after` references with the documented name and receive unknown-step errors.
- Decision-complete minimum correction: describe IDs as deterministic values derived from names, state sanitization/prefix/collision behavior, and direct callers to the resulting `Workflow.steps` value when the name is not already a unique identifier.
- Focused closure proof: `mise run docs:check` plus the existing builder contract test.

### FIND-TASK-001-14 — Cancellation can produce an active step with zero attempts

- Discovery source: CONC-001
- Status: **CONFIRMED**
- Classification: `INCORRECT`
- Violated obligation: the snapshot invariants require cancelled active steps to retain timestamps and at least one attempt; only pending/unstarted steps have zero.
- Exact location: `crates/skald/skald-workflow/src/workflow.rs:205-228,249-285,365-381`; `crates/skald/skald-workflow/src/run.rs:120-125,166-172,211-220`.
- Evidence and reachability: the parent records `running` and a start time before spawning; the child increments the atomic counter only when first polled. Cancellation can abort the spawned task first, after which the parent writes `cancelled` with zero.
- Observable consequence: equivalent cancellation requests return contract-distinct snapshots based only on Tokio polling order.
- Decision-complete minimum correction: at the executor settlement boundary, distinguish a spawned task whose attempt counter is still zero: leave it for `RunLedger::finish` to terminalize as `unstarted` with cleared timestamps; only call `step_cancelled` after an attempt has begun. This consumer owns the join outcome plus counter, so no duplicated downstream guard is needed.
- Focused closure proof: deterministically cancel after spawn but before first poll and assert zero-attempt work is `unstarted` with no timestamps, while an already-begun attempt is `cancelled` with attempts at least one.

### FIND-TASK-001-15 — Unrepresentable deadlines panic on public input

- Discovery source: CONC-002
- Status: **CONFIRMED**
- Classification: `INCORRECT`
- Violated obligation: repository input/error rules and the task's terminal behavior prohibit a public timeout from unwinding execution.
- Exact location: `crates/wyrd-spec/src/card/workflow.rs:349-368`; `crates/skald/skald-workflow/src/plan.rs:278-282`; `crates/skald/skald-workflow/src/workflow.rs:188,382,436-444`.
- Evidence and reachability: the full `u64` timeout and arbitrary local `Duration` are accepted, then added to `Instant` with unchecked `+`; unrepresentable sums panic.
- Observable consequence: authored YAML or local options can unwind the runtime instead of returning a stable pre-dispatch error/run result.
- Decision-complete minimum correction: replace every `Instant + Duration` with checked construction at the owning boundary. Reject an unrepresentable authored step timeout through the existing field-specific Workflow validation path and an unrepresentable local run deadline through the existing pre-dispatch `WorkflowResult`; do not clamp, saturate, or silently remove the deadline.
- Focused closure proof: maximum-value step timeout and local deadline cases return the selected existing stable validation/request error and never dispatch or panic; normal deadline precedence stays green.

### FIND-TASK-001-16 — Accepted retry count cannot fit the public attempt counter

- Discovery source: CONC-003
- Status: **CONFIRMED**
- Classification: `INCORRECT`
- Violated obligation: REQ-016 and the exact snapshot contract define attempts as `max_retries + 1` in a `u32` field.
- Exact location: `crates/wyrd-spec/src/card/workflow.rs:360-368`; `crates/skald/skald-workflow/src/workflow.rs:368-378,409-427`; `crates/skald/skald-workflow/src/run.rs:331-340`.
- Evidence and reachability: `u32::MAX` is accepted; incrementing the final required attempt panics in debug or wraps in release. The reserve's saturation does not repair execution semantics.
- Observable consequence: a validly decoded retry policy can panic or fail to terminate and cannot report the declared exact attempt count.
- Decision-complete minimum correction: at pure `WorkflowSpec` validation, reject only `max_retries == u32::MAX` because its required first attempt is unrepresentable. Reuse the existing field-specific validation error; do not add downstream saturation or a wider internal counter that still cannot project to the public field.
- Focused closure proof: contract tests accept `u32::MAX - 1`, reject `u32::MAX` before dispatch with the retry field named, and retain ordinary retry accounting tests.

### FIND-TASK-001-17 — Secret bindings may override routing and transport headers

- Discovery source: NET-001
- Status: **CONFIRMED**
- Classification: `INCORRECT`
- Violated obligation: REQ-042/049, INV-010/017, AC-016, and Scenario 5 require exact-origin binding and pre-dispatch reserved-header refusal.
- Exact location: `crates/skald/skald-workflow/src/route.rs:144-185,460-475`; authored classifier at `crates/wyrd-spec/src/card/workflow.rs:590-607`.
- Evidence and reachability: insertion checks secret values only. `merged_headers` forwards any `HeaderName`, including `Host`, framing/hop-by-hop, proxy/forwarding, and Wyrd-internal headers. Direct programmatic binding construction is an approved public path.
- Observable consequence: on a shared reverse proxy, a bound `Host` can route credentials to a virtual backend other than the approved origin; other reserved names can alter framing/proxy/correlation behavior.
- Decision-complete minimum correction: split the existing route-header classification into the reusable transport/routing/internal subset plus the Card-only credential subset. Apply the former in `ExternalGatewayBindings::insert`; continue allowing credential names such as `authorization` and API-key/token headers only in secret bindings. Do not create a second unrelated denylist.
- Focused closure proof: insertion rejects case-insensitive Host, hop-by-hop/framing, forwarding/proxy, and Wyrd-internal names, accepts ordinary bound Authorization/API-key headers, and preserves authored-header/collision tests.

### FIND-TASK-001-18 — External-gateway refusal bodies retain reflected credentials

- Discovery source: NET-002
- Status: **REVISED**
- Classification: `INCORRECT`
- Violated obligation: REQ-042, INV-012, and AC-016 prohibit bound secret values in errors or logs.
- Exact location: `crates/skald/skald-providers/src/clients/external.rs:130-149`; `crates/skald/skald-providers/src/clients/mod.rs:42-75`; `crates/skald/skald-providers/src/error.rs:14-30`.
- Evidence and reachability: the external client uses shared status handling, which stores the complete bounded refusal body in `ProviderError::Status`; derived `Debug` prints it. A gateway that received the credential can reflect it. Current Agent/Workflow/journal projections use safe display/code values, so they are not part of this finding.
- Observable consequence: public external-client error inspection or debug logging can reveal the bound credential.
- Decision-complete minimum correction: sanitize only external-gateway non-success errors at `ExternalGatewayClient` before returning them: retain status and retry metadata, replace the refusal body with a fixed safe diagnostic, and leave native-provider behavior unchanged.
- Focused closure proof: an external gateway reflects a canary credential in a refusal; the complete returned error's `Display` and `Debug` and its Workflow projection omit the canary, while status-based retry classification is unchanged.

## Validation result

**SPEC_REVISION_REQUIRED**

The ledger contains bounded implementation and evidence corrections, but
FIND-TASK-001-3 and FIND-TASK-001-4 cannot be closed without human-approved
public-seam decisions. All other retained findings have task-local minimum
corrections using existing owners, mechanisms, dependencies, and checks.
