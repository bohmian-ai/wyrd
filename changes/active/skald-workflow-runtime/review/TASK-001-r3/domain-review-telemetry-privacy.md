# Telemetry and privacy domain review

## Subject and result

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 11 (`9a621a28a`)
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation inputs: `review/TASK-001-r2/TASK-001-R1-close-validated-runtime-gaps.md` and `review/TASK-001-r2/TASK-001-R1-addendum-revision-11.md`
- Domain result: **FAIL**

The Observer system and its public compatibility surfaces are deleted, `wyrd.observe` remains intact, payload-bearing Observer callbacks are gone, the engine-enforced step-result ceiling remains in the admission path, and the retained Agent timeout cases pass. Two telemetry correctness gaps remain in reachable runtime paths.

## Authority coverage

| Authority | Applied rule | Source coverage | Result |
|---|---|---|---|
| `spec.md` REQ-053 | Delete the complete Skald Observer system without aliases; emit payload-free synchronous Workflow/Agent spans with exact hierarchy, attempt/backoff/outcome/error-code coverage and OpenTelemetry GenAI conventions; telemetry must not own run outcomes. | Workspace/crate manifests and lockfile; Skald/Wyrd/Python exports; docs, examples, checks; `skald-agent/src/loop_runtime.rs`; `skald-workflow/src/workflow.rs`; capture tests. | **FAIL**: deletion and payload exclusion pass; panic outcome and GenAI convention values do not. |
| `spec.md` REQ-042 and INV-012; `AGENTS.md` current decisions | Credentials and secrets never enter logs, traces, observations, generated artifacts, or Workflow results. | All fields emitted by the new Workflow and Agent spans/events; relevant error projection and tests. | PASS. New span fields are an allowlist of identifiers/counts/status/codes. |
| `architecture/references/domain/telemetry-observations.md` | Follow OpenTelemetry conventions; record terminal status/error consistently; preserve async parentage; treat prompt/completion/tool arguments/results/headers as sensitive. | Span construction, recording, async instrumentation, capture exporter, privacy assertions. | **FAIL** on consistent terminal outcome and convention values; PASS on parentage and payload exclusion in ordinary paths. |
| `architecture/wyrd-security-posture.md` | Secrets never appear in traces or errors. | Agent/Workflow span attributes, backoff event fields, failure codes. | PASS for the changed telemetry. |
| OpenTelemetry GenAI registry linked by the repository telemetry authority | Use well-known operation/provider values and declared attribute types where applicable. | `provider_label`, `request_model`, `chat_span`, finish-reason recording, OpenAI-only capture proof. | **FAIL**; see TEL-002. |
| Revision-11 addendum | Reuse `wyrd_telemetry` capture, retain timeout coverage, enforce size at the engine, do not touch `wyrd.observe`. | Dev-dependency manifests, capture tests, `attempt.rs`, Python observe package and tests. | PASS. |

## Boundary and source coverage

- Deletion closure: confirmed no `skald-observer` crate/member/dependency/lock entry, no Agent or Workflow Observer hook/scope, no Wyrd re-export, and no Python `Observer`, `OtelObserver`, `observers=`, wrapper, stub, test, example, or boundary-check entry. Remaining `Observer` names are unrelated Bifrost/queue/UI concepts or the generic Rust-style example. No compatibility alias was added.
- Unrelated observation system: `sdks/wyrd-sdk-python/python/wyrd/observe/`, its stubs, and its unit/integration consumers remain present.
- Workflow telemetry: traced `WorkflowExecutor::execute` through scheduling, bind refusal, every attempt/retry/backoff, cancellation/deadline abort, join panic, ledger settlement, and terminal run projection. Ordinary hierarchy is `workflow.run` -> sibling `workflow.step` attempt spans -> `invoke_agent` -> `chat`/`execute_tool`.
- Agent telemetry: traced both Agent entry points, provider success/failure, callbacks, tool success/failure, timeout cancellation, terminal journal handling, and model/tool field producers. No prompt, input, request, response, tool argument/result, authored header, or credential value is directly recorded by the new span constructors.
- Bridge and capture: the production bridge uses the existing `wyrd-telemetry` tracing/OpenTelemetry path with a batch exporter; tests reuse `init_test_capture`, not a new harness or local initializer.
- Docs/examples/checks: obsolete Observer examples and docs are deleted; the Workflow guide documents the new span set; examples use ordinary `tracing` setup; boundary scripts no longer name the removed crate.
- Prior-finding closure in this domain: FIND-TASK-001-5, -6, and -12 are closed by deletion plus engine-side result admission; the timeout portion of the addendum is retained in `agent_timeout.rs`.

## Verification evidence and limits

Executed against the candidate, all exit 0:

```text
mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::run_tracing_spans)'
mise exec -- cargo nextest run --locked -p skald-agent --test agent_timeout -E 'test(=agent_run_timeout_terminates_cleanly) | test(=agent_run_no_timeout_runs_to_completion) | test(=agent_run_emits_genai_spans_without_payloads)'
git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721..afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f
```

Implementation evidence also records green `test:skald`, Python unit/type lanes, codegen, boundary checks, docs, examples, formatting, and lints. Those lanes do not cover a panicking step or non-OpenAI GenAI span values. The existing capture assertions exercise OpenAI only and check payload markers rather than the convention type of `gen_ai.response.finish_reasons`.

The candidate remained `afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f` through this audit.

## Proposed findings

### TEL-001 — A panicking attempt is exported as cancelled instead of failed

- Classification: `INCORRECT`
- Violated obligation: REQ-053 requires every `workflow.step` attempt span to carry its actual outcome and the stable Wyrd error code on failure; the telemetry authority requires terminal status/error consistency.
- Exact location: `crates/skald/skald-workflow/src/workflow.rs:319-357,421-473,583-633`; reachable user-code source at `crates/skald/skald-agent/src/loop_runtime.rs:454`.
- Evidence: `AttemptSpan` is owned inside the spawned step task. If the attempt panics, including a caller-registered `AgentTool::invoke` panic, unwinding drops `AttemptSpan` before `JoinSet` reports the panic. Its `Drop` unconditionally records `wyrd.workflow.step.outcome="cancelled"` with no `error.type`. The parent then converts the same `JoinError` into a failed step carrying `WYRD_WORKFLOW_500_INTERNAL`. The returned run and its required attempt span therefore disagree.
- Reachability: TASK-001 explicitly retains caller-registered local tools. `AgentTool::invoke` is awaited without a panic boundary, while `WorkflowExecutor::settle` explicitly handles task panics, so this is a supported reachable failure path rather than dormant defensive code.
- Observable consequence: operators and automated trace assertions see a cancelled attempt with no error code even though the portable run says the step failed internally. Failure counts, root-cause analysis, and retry/outcome correlation are wrong.
- Testable correction: at the existing Workflow attempt owner, ensure a panic is converted to the already-selected `WorkflowInternal` failure before the attempt span is closed, then record `failed`, `error.type=WYRD_WORKFLOW_500_INTERNAL`, and error status on that same span. Preserve the current returned run semantics, task ownership, cancellation behavior, and no-user-payload rule; do not add a second telemetry or error path.
- Focused closure proof: run a one-step Workflow whose registered local tool panics. Assert the run/step fails with `WYRD_WORKFLOW_500_INTERNAL` and its sole `workflow.step` span has outcome `failed`, matching `error.type`, rather than `cancelled`. Also retain the existing explicit-cancellation assertion that a genuinely interrupted attempt remains `cancelled` without a fabricated error code.

### TEL-002 — GenAI fields use non-convention values and a wrong finish-reason type outside the OpenAI-only proof

- Classification: `INCORRECT`
- Violated obligation: REQ-053 requires the Agent model/tool spans to use OpenTelemetry GenAI semantic conventions where defined; `architecture/references/domain/telemetry-observations.md` requires standard GenAI attributes to follow those conventions, not only reuse their keys.
- Exact location: `crates/skald/skald-agent/src/loop_runtime.rs:298-304,590-623,779-825`; incomplete proof at `crates/skald/skald-agent/tests/agent_timeout.rs:163-245`.
- Evidence:
  - `provider_label` exports `google` and `vertex`, while the GenAI registry's applicable well-known values are `gcp.gemini` (or `gcp.gen_ai` when the Google backend is not specific) and `gcp.vertex_ai`.
  - `chat_span` always exports `gen_ai.operation.name="chat"`; Gemini/Vertex GenerateContent requests have the applicable well-known operation `generate_content`.
  - `request_model` returns `None` for Gemini and Vertex even though the resolved Prompt retains the model, so both `invoke_agent` and model-call spans export an empty `gen_ai.request.model` rather than the selected model.
  - `gen_ai.response.finish_reasons` is recorded as one scalar string. The convention defines this attribute as `string[]`, corresponding to all generations received.
  - The only capture test constructs an OpenAI prompt and asserts only `openai`, `chat`, and `gpt-4o`; it does not inspect the finish-reason value/type.
- Observable consequence: Gemini and Vertex traces cannot be reliably grouped by the standard provider/operation/model dimensions, and finish-reason consumers receive a schema-incompatible scalar. Wyrd's canonical OTLP projection and downstream GenAI tooling can misclassify or discard these fields despite their standard-looking names.
- Testable correction: derive GenAI operation/provider/model metadata from the existing typed `ProviderRequest` plus the resolved Prompt model, reusing the existing provider/request owners; emit the registry's applicable well-known values and a real string-array finish-reasons value without recording response content. Do not add a telemetry abstraction, payload field, compatibility key, or dependency.
- Focused closure proof: extend the existing capture test with representative OpenAI, Gemini, and Vertex Agent runs. Assert the exact operation/provider/model values and exported array type/content for finish reasons, while retaining the payload/credential marker exclusion over every span and event.

## Security audit

### Critical

None.

### High

None.

### Medium

None beyond the telemetry correctness findings above; source inspection found no changed path that places prompt, response, tool argument/result, header, or credential material into the new span/event field allowlist.

### Low / Defense in depth

None proposed. Optional telemetry expansion is outside this acceptance audit.

### Positive controls

- The payload-bearing Observer callback surface is deleted instead of retained behind an alias.
- Workflow spans use an explicit metadata-only field allowlist and stable error codes.
- The existing production-shaped `wyrd-telemetry` capture is reused.
- Retry attempts are separate sibling spans and backoff contains only step identity, next-attempt count, and delay.
- `wyrd.observe` is preserved as an unrelated durable observation surface.

## Overall result

**FAIL** — TEL-001 and TEL-002 are bounded implementation findings within approved Revision 11 behavior.
