# Telemetry and privacy domain review

## Subject and result

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `a704a8890ef20efe65fee1e116f7d02288f8ec5c`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation task: `changes/active/skald-workflow-runtime/review/TASK-001-r3/TASK-001-R2-close-round-three-runtime-gaps.md`
- Prior review: `changes/active/skald-workflow-runtime/review/TASK-001-r3/`
- Domain result: **FAIL**

The candidate closes the two prior telemetry findings: panicking attempts now
settle as failed while their attempt span is alive, and Agent spans use the
approved provider/model values without the unsupported scalar finish-reason
field. The Observer system remains fully deleted, span hierarchy and payload
exclusion are otherwise intact, and the test-harness lifecycle fix has no
telemetry or privacy regression. One reachable model-call path still exports an
incorrect standard model attribute.

## Authority and boundary coverage

| Authority or obligation | Source and consumer coverage | Result |
|---|---|---|
| Revision 11 `REQ-053` | Workspace manifests and lockfile; Rust and Python exports; docs, examples, checks; `skald-agent/src/loop_runtime.rs`; `skald-workflow/src/workflow.rs`; capture tests. | **FAIL** only for the effective model on a callback-replaced request; deletion, hierarchy, outcomes, error codes, and payload exclusion pass. |
| `REQ-042`, `INV-012`, and telemetry payload safety | Every new Workflow/Agent span and event field; successful ExtGateway reflection containment; capture assertions. | PASS. The tracing field allowlist contains identifiers, counts, statuses, timing, and stable codes, not prompt, request, response, tool argument/result, header, or credential values. |
| `architecture/references/domain/telemetry-observations.md` | Parentage, terminal outcome consistency, GenAI semantic attributes, sensitive payload exclusion, and existing `wyrd-telemetry` bridge. | **FAIL** for `gen_ai.request.model` accuracy after a supported request replacement; other reviewed rules pass. |
| Prior `FIND-TASK-001-21` | `StepTask::run`, `AttemptSpan`, caller tool panic, cancellation control, and `attempt_panic_matches_span`. | CLOSED. The complete attempt future is caught before the span guard settles, producing `failed` plus `WYRD_WORKFLOW_500_INTERNAL`; genuine interruption remains `cancelled`. |
| Prior `FIND-TASK-001-23` | `invoke_agent_span`, `chat_span`, provider mapping, Google/Vertex Prompt models, finish-reason omission, and Agent capture tests. | CLOSED as specified. `gcp.gemini` and `gcp.vertex_ai` are emitted, the resolved model is present for Google/Vertex, `chat` is retained per the approved remediation, and the scalar finish-reasons field is absent. |
| Test-harness fix `516d0fbcc` | `WyrdTestServerInner` drop order, `shutdown`, `Drop`, in-process and bound modes, and recorded `test:wyrd` rerun. | PASS for this domain. It cancels the shared state token before runtime owners and the fixture drop; it adds no telemetry fields, payload path, subscriber, or exporter. |

The OpenTelemetry registry defines `gen_ai.request.model` as the model a request
is made to and `gen_ai.response.finish_reasons` as `string[]`; it also lists the
provider values used by the candidate. See the official
[GenAI attribute registry](https://opentelemetry.io/docs/specs/semconv/registry/attributes/gen-ai/).

## Source coverage

- Confirmed the candidate tree has no `skald-observer` crate, Agent/Workflow
  observer hook, `observers`/`with_observers` surface, Python
  `Observer`/`OtelObserver` export or stub, Observer test/example, or boundary
  check reference. Remaining `Observer` names belong to unrelated Vala and
  generic reference examples. No compatibility alias exists.
- Traced `workflow.run` through scheduling, retry/backoff, attempt execution,
  panic conversion, cancellation/deadline interruption, and terminal status.
  Each attempt span is a child of the run span; Agent/model/tool spans inherit
  the attempt context; retries are sibling attempt spans.
- Traced both Agent entry points through Prompt rendering, `before_model`
  replacement, provider dispatch, tool execution, timeout, terminal journaling,
  and span settlement. Span fields do not record the runtime payloads handled by
  those paths.
- Reviewed the production-shaped `wyrd_telemetry::init_test_capture` use and
  confirmed no local telemetry initializer or second exporter was added.
- Reviewed docs, examples, Rust/Python exports, manifests, lockfile, and
  repository checks for deletion closure. The separate `wyrd.observe` package
  remains intact.
- Reviewed the `wyrd-testing` fixture change because it entered the candidate.
  Its cancellation and field-order changes are lifecycle-only and do not
  weaken telemetry redaction or add observable application behavior.

## Verification evidence and limits

Executed against candidate `a704a8890ef20efe65fee1e116f7d02288f8ec5c`:

```text
mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::run_tracing_spans) | test(=workflow::tests::attempt_panic_matches_span)'
  2 passed, 10 skipped

mise exec -- cargo nextest run --locked -p skald-agent --test agent_timeout -E 'test(=agent_run_timeout_terminates_cleanly) | test(=agent_run_no_timeout_runs_to_completion) | test(=agent_run_emits_genai_spans_without_payloads) | test(=agent_run_genai_google_provider_and_model)'
  4 passed

git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721..a704a8890ef20efe65fee1e116f7d02288f8ec5c
  exit 0
```

The remediation evidence also records green `mise run test:skald`,
`mise run test:wyrd` after `516d0fbcc`, `mise run lints`, `docs:check`, and
`check:examples`. Existing callback coverage proves that a `before_model`
callback may replace the native request's model, but no trace-capture test joins
that supported behavior to the emitted `chat` model attribute. The candidate
remained unchanged during this domain audit.

## Proposed finding

### TEL-R4-001 — A callback-replaced model call is traced as the original Prompt model

- Classification: `INCORRECT`
- Violated obligation: REQ-053 requires Agent model-call spans to use
  OpenTelemetry GenAI semantic-convention attributes. The telemetry authority
  requires those attributes to describe the operation consistently;
  `gen_ai.request.model` denotes the model the dispatched request targets.
- Exact location:
  `crates/skald/skald-agent/src/loop_runtime.rs:301-333,817-833`;
  reachable callback contract and proof at
  `crates/skald/skald-agent/src/callbacks.rs:54-56,81-102` and
  `crates/skald/skald-agent/tests/callbacks.rs:133-160`.
- Evidence: `before_model` runs before dispatch and may return a replacement
  `ProviderRequest`. The existing test changes an OpenAI request model from the
  Prompt model to `replacement-model` and proves that exact replacement reaches
  the provider. After the callback, `run_loop` calls `chat_span(&request,
  model)`, but `chat_span` records the separately retained Prompt `model`
  argument unconditionally. The span therefore says the call targeted the
  original model while the provider receives the replacement model. The new
  telemetry tests cover ordinary OpenAI and Google/Vertex requests only, so
  they do not expose this disagreement.
- Observable consequence: model-call traces and any downstream grouping,
  latency/error analysis, or policy evidence keyed by
  `gen_ai.request.model` attribute a real provider call to the wrong model.
- Testable correction: at the existing `chat_span` owner, use the model encoded
  by the effective post-callback `ProviderRequest` whenever
  `request_model(request)` supplies one, and fall back to the resolved Prompt
  model only for request variants such as Gemini/Vertex whose wire request does
  not carry the model. Preserve the approved provider values, `chat` operation,
  payload-free field set, and absent finish-reasons field; add no telemetry
  abstraction or exporter.
- Focused closure proof: extend production-shaped capture coverage with an
  Agent whose `before_model` callback replaces an OpenAI request model. Assert
  the provider receives `replacement-model` and the corresponding `chat` span
  records that same value. Retain Google/Vertex cases to prove their Prompt
  model fallback and the current payload-marker exclusion.

## Security and privacy assessment

No separate security finding is proposed. The changed span/event constructors
do not place prompt text, input, provider requests/responses, tool arguments or
results, headers, secret values, or reflection matches into telemetry. The
Observer callback path that previously carried payloads is deleted rather than
hidden behind an alias.

## Overall result

**FAIL** — prior telemetry findings are closed, but `TEL-R4-001` is one bounded,
reachable REQ-053 telemetry correctness defect within the approved behavior.
