# Telemetry and privacy domain review

## Subject and result

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 11, including `REQ-053`
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation task:
  `changes/active/skald-workflow-runtime/review/TASK-001-r4/TASK-001-R3-close-round-four-review-gaps.md`
- Domain result: **PASS**

The candidate closes prior `FIND-TASK-001-29`. The model-call span now reads
the model from the effective post-`before_model` request and uses the resolved
Prompt model only for request variants whose wire shape omits it. Observer
deletion remains complete, Workflow and Agent spans retain the required
hierarchy and terminal outcomes, and no reviewed span or event carries prompt,
input, request, response, tool-argument/result, or credential payload.

## Authority and boundary coverage

| Authority or obligation | Source and consumer coverage | Result |
|---|---|---|
| Revision 11 `REQ-053` | Workspace manifests and lockfile; Skald/Wyrd Rust exports; Python package, native registration, stubs, tests, and examples; checks and docs; Workflow and Agent runtime spans. | PASS. The Skald Observer crate, hooks, aliases, exports, tests, examples, and boundary-check references are deleted. Telemetry is synchronous `tracing` instrumentation with no local initializer or second exporter. |
| `architecture/references/domain/telemetry-observations.md` | Span hierarchy, terminal status/error recording, retry/backoff visibility, GenAI semantic fields, payload sensitivity, and subscriber-independent control flow. | PASS. Standard names are used where applicable, payload fields are excluded, and tracing does not decide or await runtime work. |
| Prior `FIND-TASK-001-29` | `crates/skald/skald-agent/src/loop_runtime.rs:301-333,813-835,906-919`; callback contract; production-shaped capture test. | CLOSED. `chat_span` uses `request_model(request).unwrap_or(fallback_model)` after callback replacement and before dispatch. |
| Prior `FIND-TASK-001-27` and `FIND-TASK-001-28` | Documentation-only dependency correction and test-harness lifecycle correction, respectively. | No telemetry/privacy regression. Neither adds a telemetry surface, payload field, subscriber, exporter, retry, or Observer compatibility path. |

The OpenTelemetry GenAI registry defines `gen_ai.request.model` as the model a
request is made to, defines `gen_ai.response.finish_reasons` as `string[]`, and
lists `openai`, `anthropic`, `gcp.gemini`, and `gcp.vertex_ai` among the
well-known provider values. The candidate's model-call fields match that
authority: <https://opentelemetry.io/docs/specs/semconv/registry/attributes/gen-ai/>.

## Source coverage and evidence

- A candidate-tree search found no `skald-observer` dependency or crate,
  `OtelObserver`, `with_observers`, Agent/Workflow observer field, Python
  Observer export/stub, Observer test/example, or check allowlist outside the
  changelog's intentional removal notice. Remaining generic `Observer` names
  are unrelated Vala/test/UI mechanisms, not compatibility aliases.
- `WorkflowRunExecutor::execute` opens `workflow.run`; spawned step tasks inherit
  that span; every attempt opens a sibling `workflow.step`; Agent
  `invoke_agent`, `chat`, and `execute_tool` spans inherit the attempt context.
  `AttemptSpan` records `succeeded`, `failed` plus the stable code, or
  `cancelled` on interruption. The run span records terminal status and its
  primary stable code.
- Retryable failures close their attempt span before emitting one
  `workflow.step.backoff` event on the run span with only step id, next attempt,
  and bounded delay. Provider/gateway internal retries remain at their existing
  owners; this change introduces no duplicate attempt instrumentation.
- `Agent::run_with` and `run_prompt` create `invoke_agent` spans with agent id,
  semantic provider, resolved model, and empty error fields. Each effective
  request creates a `chat` span immediately before `dispatch`; provider failure
  records the stable provider code. Tool spans contain operation, tool name,
  and call id, while arguments and results remain in the journal rather than
  tracing fields.
- The round-five correction reuses the existing `request_model` owner. The new
  callback case proves that OpenAI dispatch and the corresponding `chat` span
  both carry `gpt-4o-mini`, while the existing Gemini/Vertex cases prove the
  Prompt-model fallback and the `gcp.gemini`/`gcp.vertex_ai` mappings.
- Span construction, field recording, and events are synchronous `tracing`
  calls. No telemetry branch invokes callbacks, performs IO, blocks, or changes
  a returned outcome. With no subscriber, the spans are no-ops and execution
  follows the same provider, tool, retry, and settlement paths.
- Payload-marker assertions cover Workflow input/output, Agent input/output,
  tool arguments/results, and the callback-replaced call. No marker appears in
  captured span or event attributes. The unsupported scalar
  `gen_ai.response.finish_reasons` field remains absent.

## Prior-finding closure

| Finding | Result | Domain evidence |
|---|---|---|
| `FIND-TASK-001-27` | CLOSED outside this domain | The architecture-only correction adds no runtime telemetry or privacy surface. |
| `FIND-TASK-001-28` | CLOSED outside this domain | The test-server lifecycle correction adds no application spans/events or exporter/subscriber behavior. |
| `FIND-TASK-001-29` | **CLOSED** | Effective callback-replaced OpenAI request model is the `chat` span model; model-less Google request shapes retain the resolved Prompt fallback; provider names, operation, payload exclusion, and finish-reason omission are preserved. |

## Verification evidence and limits

Executed against candidate `09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596`:

```text
mise exec -- cargo nextest run --locked -p skald-agent --test agent_timeout \
  -E 'test(=agent_run_chat_span_records_callback_replaced_model) | test(=agent_run_emits_genai_spans_without_payloads) | test(=agent_run_genai_google_provider_and_model)'
  3 selected, 3 passed

mise exec -- cargo nextest run --locked -p skald-workflow --lib \
  -E 'test(=workflow::tests::run_tracing_spans) | test(=workflow::tests::attempt_panic_matches_span)'
  2 selected, 2 passed

git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721 \
  09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596
  exit 0
```

The remediation record additionally reports green `mise run fmt`, `lints`,
`test:skald`, `test:wyrd`, `docs:check`, and `check:client-tier`. This domain
pass did not rerun those broader lanes. No dedicated no-subscriber test exists;
subscriber independence is established by the reviewed control flow and by the
absence of telemetry-dependent branches or awaited telemetry work, while the
ordinary non-capture test surface exercises the same runtime entry points.

## Findings

No material telemetry or privacy finding remains.

## Overall result

**PASS** — `FIND-TASK-001-29` is closed, the Revision 11 Observer deletion and
payload/privacy boundary remain intact, and the telemetry hierarchy, outcomes,
stable codes, semantic provider/model fields, retry visibility, and
subscriber-independent behavior satisfy the approved task.
