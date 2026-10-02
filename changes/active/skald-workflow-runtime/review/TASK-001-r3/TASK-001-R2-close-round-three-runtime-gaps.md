# TASK-001-R2 — Close round-three runtime gaps

## Authority and immutable subject

- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 11 at
  `9a621a28a40b67e82e4ba119f7df577dde893f1e`
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Reviewed candidate: `afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f`
- Review verdict:
  `changes/active/skald-workflow-runtime/review/TASK-001-r3/verdict.md`
- Validated evidence:
  `changes/active/skald-workflow-runtime/review/TASK-001-r3/findings-validation.md`

Implement this task through `$wyrd-implement`. This is the single remediation
task for the round-three verdict; do not create another plan.

## Outcome

Close `FIND-TASK-001-20` through `FIND-TASK-001-27` without changing the
approved public Workflow contract, adding a compatibility surface, restoring
the Observer system, or introducing another runtime, service, dependency, or
telemetry path.

## Diagnosis and required corrections

### FIND-TASK-001-20 — Enforce the fixed Agent deadline

- Obligation: REQ-016, REQ-043, and Scenario 3 require the effective attempt
  deadline to cover provider/tool work, normalization, output admission, and
  settlement with cancellation > total > step > Agent precedence.
- Current behavior: `StepTask::run` computes `agent_deadline` but does not race
  it. Native and ExtGateway routes start a later relative timer, after which a
  pending terminal journal append can keep the attempt alive indefinitely.
- Evidence: `crates/skald/skald-workflow/src/workflow.rs:421-463,493-540`,
  `crates/skald/skald-agent/src/loop_runtime.rs:138-203`, and
  `crates/skald/skald-workflow/src/route.rs:381-407,569-606`.
- Consequence and missing proof: finite Agent timeouts can overrun or never
  settle, and existing timeout tests do not hold terminal journal settlement
  pending across the fixed Workflow deadline.
- Required outcome: add the already-computed Agent deadline to the existing
  biased `StepTask` race after cancellation, total, and step deadlines but
  before the attempt. On expiry, use the existing typed Agent timeout outcome
  through `AttemptOutcome::from_agent` so retryability and stable codes remain
  authoritative.

### FIND-TASK-001-21 — Make panic tracing match run failure

- Obligation: REQ-053 requires every attempt span to record its actual terminal
  outcome and stable error code.
- Current behavior: a panic in caller-supplied attempt work drops `AttemptSpan`
  as cancelled; only afterward does `JoinSet` project the run as
  `WYRD_WORKFLOW_500_INTERNAL`.
- Evidence: `crates/skald/skald-workflow/src/workflow.rs:319-357,421-473,583-633`
  and the reachable tool call in
  `crates/skald/skald-agent/src/loop_runtime.rs:379-520`.
- Consequence and missing proof: the trace and durable returned result disagree;
  current tests cover explicit cancellation, not unwinding.
- Required outcome: catch unwinding around the complete attempt future inside
  `StepTask::run` while its existing span guard is alive, using installed
  futures support. Convert it to the existing non-retryable
  `WorkflowInternal` outcome and reuse ordinary failed-span settlement.

### FIND-TASK-001-22 — Contain successful ExtGateway secret reflection

- Obligation: REQ-042 and INV-012 prohibit a bound secret from entering any
  returned result or error.
- Current behavior: non-success bodies are sanitized, but successful decoded
  responses flow into Agent and Workflow results without comparison to the
  sensitive bound header values.
- Evidence: `crates/skald/skald-providers/src/clients/external.rs:28-43,96-173`,
  `crates/skald/skald-agent/src/loop_runtime.rs:329-376`, and
  `crates/skald/skald-workflow/src/workflow.rs:493-540`.
- Consequence and missing proof: a malicious bound endpoint can return the
  credential it received in valid assistant or structured/tool content;
  existing coverage proves only non-success containment.
- Required outcome: in `ExternalGatewayClient`, retain the values of headers
  already marked sensitive and recursively inspect only decoded JSON strings
  and member names retained by the typed successful response. Refuse any
  nonempty match with one fixed safe non-retryable provider error that includes
  neither response nor match.

### FIND-TASK-001-23 — Correct GenAI semantic fields

- Obligation: REQ-053 and the telemetry authority require standard GenAI
  attributes to use OpenTelemetry semantic values and types.
- Current behavior: Gemini and Vertex use non-canonical provider names, omit the
  available resolved Prompt model, and emit the `string[]` finish-reasons key
  as one scalar string.
- Evidence: `crates/skald/skald-agent/src/loop_runtime.rs:297-303,590-623,779-825`
  and the OpenAI-only proof at
  `crates/skald/skald-agent/tests/agent_timeout.rs:163-245`.
- Consequence and missing proof: downstream consumers split or discard standard
  dimensions and cannot group Gemini/Vertex calls reliably.
- Required outcome: derive metadata from the existing typed request, provide
  the exact resolved Prompt model to Agent/model-call spans, map Google and
  Vertex to `gcp.gemini` and `gcp.vertex_ai`, and delete the optional scalar
  finish-reason attribute. Retain the approved `chat` operation.

### FIND-TASK-001-24 — Complete required rustdoc

- Obligation: `AGENTS.md` and `architecture/agent-rules.md` require substantive
  rustdoc for new and materially changed Rust items, including private test
  fixtures, fields, trait methods, errors, cancellation, and side effects.
- Current behavior: changed Agent entry points and Responses helpers omit their
  error/lifecycle contracts; new timeout and telemetry fixtures and their
  fields, helpers, and trait methods are undocumented.
- Evidence: `crates/skald/skald-agent/src/loop_runtime.rs:33-203`,
  `crates/skald/skald-agent/src/request_builder.rs:29-63,129-176`, and
  `crates/skald/skald-agent/tests/agent_timeout.rs:248-379,417-443`.
- Consequence and missing proof: the hard documentation rule is unsatisfied;
  passing tests cannot prove source documentation quality.
- Required outcome: document the existing items in place, including errors,
  terminal-journal best effort, timeout/cancellation effects, scripted response
  exhaustion, poison recovery, and controlled tool notification/wait behavior.

### FIND-TASK-001-25 — Put Agent orchestration on its owner

- Obligation: the repository's struct-centered rule requires workflows that
  consume an owner's state to be inherent methods on that owner.
- Current behavior: two free functions accept `&Agent`, consume its runtime
  state and dependencies, and own the full lifecycle while inherent methods
  only forward.
- Evidence: `crates/skald/skald-agent/src/loop_runtime.rs:33-203` and
  `crates/skald/skald-agent/src/agent.rs:662-696`.
- Consequence and missing proof: ownership and lifecycle implementation can
  drift; behavior tests do not prove the required source shape.
- Required outcome: make the input/session and explicit-Prompt orchestration
  bodies inherent `Agent` methods in the existing module and remove the
  forward-only free functions. Leave genuinely stateless helpers free.

### FIND-TASK-001-26 — Remove direct library `anyhow`

- Obligation: repository Rust rules reserve `anyhow` for binaries and prefer
  the standard library for a fixed error.
- Current behavior: the Workflow library directly adds and uses `anyhow` only
  to construct the fixed no-remote-resolver refusal.
- Evidence: `crates/skald/skald-workflow/src/output.rs:17-31` and
  `crates/skald/skald-workflow/Cargo.toml:45`.
- Consequence and missing proof: the library violates the explicit error policy;
  behavior tests cannot justify the dependency choice.
- Required outcome: construct a fixed `std::io::Error`, convert it through the
  existing `SchemaResolverError` boundary, and remove the direct `anyhow`
  dependency. Do not add a local error enum.

### FIND-TASK-001-27 — Remove stale permanent documentation

- Obligation: changed permanent docs must describe the typed Revision 11
  surface and must not preserve deleted APIs or examples as compatibility.
- Current behavior: the architecture page and changelog name deleted Workflow
  machinery, while the Rust examples README lists deleted
  `tracing_stdout.rs`.
- Evidence: `docs/architecture/skald.md:9-30`, `CHANGELOG.md:5-13`, and
  `examples/rust/README.md:17-25`.
- Consequence and missing proof: maintainers and users are directed to APIs and
  an example that do not exist; a general docs lane did not catch the wording.
- Required outcome: replace only the obsolete Workflow descriptions with the
  implemented Agent DAG, bindings, routes, bounded execution, and portable run
  ownership; remove the absent example entry.

## Constraints and preserved behavior

- Preserve the Revision 11 public Rust/Python contracts, exact namespaced
  output, Responses replay, route selection, provider retry/fallback, Workflow
  backoff, stable error codes, and typed result projection.
- Preserve cancellation > total > step > Agent precedence, standalone Agent
  journaling, span hierarchy, payload exclusion, and genuine cancellation as
  `cancelled` without a fabricated error.
- Preserve endpoint/DNS/TLS/no-proxy/no-redirect/body-bound policy, clean 2xx
  responses, non-success retry metadata, native providers, and WyrdGateway.
- Preserve journal finish reasons and the approved `chat` operation; do not add
  direct OTLP instrumentation, a pseudo-array, or another telemetry owner.
- Do not scan ignored raw response members or move secret checks downstream.
- Do not add a runtime, service, trait, wrapper, module, error enum, dependency,
  check, compatibility note, migration section, or replacement tracing example.
- Do not restore any Skald Observer crate, hook, alias, or payload-bearing
  callback surface.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-001-20` | A pending terminal journal cannot outlive the fixed Workflow Agent deadline; retry/exhaustion, stable code, span closure, no-live-attempt behavior, and deadline precedence are proven. |
| `FIND-TASK-001-21` | A panicking local tool yields `WYRD_WORKFLOW_500_INTERNAL` in both the run and failed attempt span; explicit cancellation remains cancelled. |
| `FIND-TASK-001-22` | 2xx assistant and retained structured/tool reflections, including JSON escaping, are safely refused without leaking or retrying; clean 2xx and existing 401 controls still pass. |
| `FIND-TASK-001-23` | OpenAI, Gemini, and Vertex captures have exact provider/model values, retain `chat`, omit `gen_ai.response.finish_reasons`, and contain no payload marker. |
| `FIND-TASK-001-24` | Every cited new or materially changed item has substantive in-place rustdoc covering the applicable errors, state, lifecycle, and side effects. |
| `FIND-TASK-001-25` | The two stateful orchestration bodies are inherent `Agent` methods; public APIs and runtime behavior are unchanged. |
| `FIND-TASK-001-26` | The refusal uses a standard error through `SchemaResolverError`; `skald-workflow` no longer directly depends on `anyhow`; schema tests pass. |
| `FIND-TASK-001-27` | The three changed permanent surfaces contain no deleted Workflow names or absent example entry and accurately describe the implemented surface. |

## Required proof

Add or extend the smallest focused tests that prove:

1. paused-time fixed Agent-deadline expiry while terminal journal settlement is
   pending, including retry/exhaustion and no surviving attempt;
2. panic-to-failed-span/run consistency plus the existing cancellation control;
3. successful ExtGateway reflection containment for assistant and retained
   structured/tool content, an escaped canary, clean success, no containment
   retry, and the existing non-success case; and
4. OpenAI, Gemini, and Vertex provider/model attributes, absent scalar finish
   reasons, retained `chat`, and payload exclusion.

Run every specifically named test with its exact `mise exec -- cargo nextest
run --locked` selector, including the repository-managed environment wrapper
when required. Then run the narrowest existing repository lanes covering the
complete touched surface:

```bash
mise run fmt
mise run lints
mise run test:skald
mise run docs:check
mise run check:examples
```

Run any additional boundary or generation lane only if the implementation
actually changes that governed surface. Finish with:

```bash
git diff --check
```

Record commands, selected test counts, outcomes, and any genuine verification
limit in the implementation report. A red required lane blocks completion.
