# Structured Ponytail validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 11 at `9a621a28a40b67e82e4ba119f7df577dde893f1e`
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Prior review: `changes/active/skald-workflow-runtime/review/TASK-001-r2/`
- Remediation authority:
  `TASK-001-R1-close-validated-runtime-gaps.md` and
  `TASK-001-R1-addendum-revision-11.md`

The candidate and approved specification were byte-identical to the stated
commits before validation. No `.codegraph/` directory exists, so validation
used the complete cumulative diff, repository search, and direct source and
caller inspection. This pass read every discovery and follow-up report in this
directory and independently traced each proposed finding from producer to
consumer. It did not treat report agreement or green tests as proof.

## Proposal-by-proposal validation

| Discovery proposal | Validation | Final finding | Source-grounded disposition |
|---|---|---|---|
| `CONC-R3-001` | **REVISED, DEDUPLICATED** | `FIND-TASK-001-20` | A pending terminal journal append is a concrete manifestation of the Workflow owner's missing fixed Agent-deadline arm, not a separate standalone-Agent contract change. |
| `SYS-R3-001` | **REVISED, DEDUPLICATED** | `FIND-TASK-001-20` | Same missing `agent_deadline` select arm and correction owner as `CONC-R3-001`. |
| `TEL-001` | **CONFIRMED** | `FIND-TASK-001-21` | An attempt panic drops `AttemptSpan` as cancelled before the parent converts the join panic to a failed run. |
| `NET-R3-001` | **CONFIRMED** | `FIND-TASK-001-22` | Successful ExtGateway responses are decoded and retained without checking them against the bound secret values. |
| `TEL-002` | **REVISED** | `FIND-TASK-001-23` | Canonical provider values, available model values, and the scalar finish-reason field are defects. The mandatory `generate_content` subclaim is rejected. |
| `STD-R3-001` | **REVISED, DEDUPLICATED** | `FIND-TASK-001-23` | Same GenAI provider/model/type contract as the retained portions of `TEL-002`. |
| `STD-R3-002` | **CONFIRMED, COMBINED** | `FIND-TASK-001-24` | The wholly new timeout/telemetry fixtures and their fields/methods lack repository-required rustdoc. |
| `MNT-R3-002` | **CONFIRMED, COMBINED** | `FIND-TASK-001-24` | The materially changed Agent entry points and Responses helpers also lack required error/lifecycle contracts. |
| `MNT-R3-001` | **CONFIRMED** | `FIND-TASK-001-25` | The stateful Agent workflows remain free functions that thread `&Agent`, contrary to the hard struct-centered rule. |
| `MNT-R3-003` | **CONFIRMED** | `FIND-TASK-001-26` | The new library validator directly uses `anyhow` for one fixed refusal despite a standard error satisfying the dependency's alias. |
| `MNT-R3-004` | **CONFIRMED** | `FIND-TASK-001-27` | Three changed permanent documentation surfaces still advertise deleted APIs or an absent example. |
| Behavior-review empty proposal ledger | **REJECTED AS COMPLETE** | — | Its happy-path evidence does not exercise the retained deadline, panic, successful-reflection, telemetry-contract, or hard repository-rule paths. |
| Invariant-review empty proposal ledger | **REJECTED AS COMPLETE** | — | Same missing reachable paths; its prior-finding closure remains useful but does not establish an empty current ledger. |

## Conflict and correction-boundary resolution

### Agent deadline and journal liveness

`StepTask::run` fixes `agent_deadline` before the attempt, but its biased select
races only cancellation, total deadline, step deadline, and the Agent future
(`workflow.rs:421-463`). Native and ExtGateway execution start a new relative
Agent timeout only when `run_prompt` is polled, and both await terminal journal
settlement after that timer has fired (`loop_runtime.rs:138-203`). The same
Agent future also contains response normalization and output admission. The
single source defect is therefore the absent fixed Agent-deadline arm at
`StepTask`, which already owns the other precedence arms. Moving only the
terminal journal append under the standalone Agent timer would leave sibling
post-timer work and route-dependent Workflow timing unfixed.

### Panic outcome mismatch

The attempt guard lives inside the spawned `StepTask`; a panic in the awaited
Agent/tool future unwinds it and records `cancelled` before `JoinSet` can return
the panic to `WorkflowExecutor::settle`. Catching only `AgentTool::invoke`
would leave every sibling panic source with the same mismatch. The smallest
safe boundary is the complete attempt future while the existing guard is
still alive. The already-installed `futures` support is sufficient; no panic
service, observer, or telemetry layer is warranted.

### Successful ExtGateway credential reflection

REQ-042 prohibits a bound secret value from entering a returned result, not
only a refusal error. `ExternalGatewayClient` is the sole owner that has the
sensitive bound headers and the typed successful response before it becomes a
general `ProviderResponse`. The check belongs there. It must inspect decoded
content retained by the typed response so JSON escaping cannot hide a secret,
while ignored unknown members do not cause irrelevant refusal. Checking raw
bytes is insufficient; checking `AgentRun` or `WorkflowRun` is too late and
would duplicate the invariant across consumers.

### GenAI semantic conventions

The repository telemetry authority requires standard attributes to follow the
OpenTelemetry conventions, not merely reuse their keys. The current official
GenAI span contract makes provider and operation required, model conditionally
required when available, and finish reasons an optional `string[]`. The
candidate emits non-canonical `google`/`vertex`, omits the available Prompt
model for Gemini/Vertex, and emits finish reasons as a scalar. The pinned
`tracing-opentelemetry` visitor projects ordinary tracing fields only as scalar
values; adding a direct OTLP path for one optional field would be needless.
Delete the optional finish-reason attribute.

The proposed mandatory `generate_content` correction is rejected. The
normative registry names `generate_content` for multimodal generation, but the
official Google GenAI reference scenario records `chat` for a
`client.models.generate_content` call. Approved REQ-053 also names this as the
model-call `chat` span. That authority does not prove that transport method
alone requires changing the operation value. Retaining `chat` is therefore not
a validated defect in this task.

Primary grounding:

- [OpenTelemetry GenAI spans](https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/gen-ai-spans.md)
- [OpenTelemetry Google GenAI reference scenario](https://github.com/open-telemetry/semantic-conventions-genai/blob/main/reference/scenarios/google-genai/scenario.py)

### Documentation, owner shape, and dependency rules

`AGENTS.md` sections 5 and 16 and `architecture/agent-rules.md` make the
struct-centered and rustdoc rules hard acceptance criteria, including private
test items. The candidate materially rewrites the Agent orchestration yet
retains free functions that repeatedly read `Agent` state; its replacement
external test contains new undocumented fixture types, fields, helpers, and
trait methods. These are explicit rules, not style preferences. Separately,
`AGENTS.md` reserves `anyhow` for binaries, while `SchemaResolverError` accepts
an ordinary standard error. Each has a smaller in-place correction and needs no
new abstraction.

## Prior-finding closure

No prior `FIND-TASK-001-1` through `FIND-TASK-001-19` remains open, so none is
carried into the active ledger.

| Prior finding | Result | Independently validated closure evidence |
|---|---|---|
| `FIND-TASK-001-1` | **CLOSED** | `RunLedger` charges exact JCS replacement growth and `terminal_budget_reserve` covers escaped text. |
| `FIND-TASK-001-2` | **CLOSED** | Responses reasoning identity, summaries, encrypted state, function calls, and outputs are replayed in provider order. |
| `FIND-TASK-001-3` | **CLOSED BY REVISION 10** | The approved public seam remains `HashMap<HeaderName, SecretString>`. |
| `FIND-TASK-001-4` | **CLOSED BY REVISION 10** | The approved remote error seam remains the boxed five-field `RemoteProblem`. |
| `FIND-TASK-001-5` | **CLOSED BY REVISION 11** | The payload-bearing Observer path is deleted and step-result admission enforces the ceiling before retention. |
| `FIND-TASK-001-6` | **CLOSED BY REVISION 11** | The callback system and its user-code liveness owner are deleted; the new panic-span issue is distinct and receives ID 21. |
| `FIND-TASK-001-7` | **CLOSED** | Changed test imports are module-scoped. |
| `FIND-TASK-001-8` | **CLOSED** | Required docs and example lanes are recorded green. |
| `FIND-TASK-001-9` | **CLOSED** | Every named test has an exact recorded nextest selector. |
| `FIND-TASK-001-10` | **CLOSED** | New Workflow PyO3 ownership and registration live in `wyrd-sdk-python`. |
| `FIND-TASK-001-11` | **CLOSED** | Public Workflow run, step, and error declarations are precise `TypedDict`s. |
| `FIND-TASK-001-12` | **CLOSED BY REVISION 11** | Observer hooks no longer exist; current missing docs concern different new or materially changed items. |
| `FIND-TASK-001-13` | **CLOSED** | The guide accurately describes deterministic derived step IDs. |
| `FIND-TASK-001-14` | **CLOSED** | A never-polled aborted step becomes `unstarted`; begun work becomes `cancelled`. |
| `FIND-TASK-001-15` | **CLOSED** | Authored and local deadline construction is checked before dispatch; finding 20 concerns enforcement of a representable fixed deadline. |
| `FIND-TASK-001-16` | **CLOSED** | Pure validation rejects only the unrepresentable `u32::MAX` retry count. |
| `FIND-TASK-001-17` | **CLOSED** | Binding insertion reuses the transport/routing/internal header classifier. |
| `FIND-TASK-001-18` | **CLOSED** | External non-success bodies are replaced with a fixed diagnostic; finding 22 concerns the distinct successful-response path. |
| `FIND-TASK-001-19` | **CLOSED** | Explicit cumulative `git diff --check` exits zero. |

## Final deduplicated finding ledger

### FIND-TASK-001-20 — The Workflow does not enforce its fixed Agent deadline

- Discovery sources: `CONC-R3-001`, `SYS-R3-001`
- Status: **REVISED**
- Classification: `INCORRECT`
- Violated obligation: the Workflow retry/deadline contract, REQ-016,
  REQ-043, and Scenario 3 require the effective attempt deadline to include
  the fixed Agent deadline across provider/tool work, normalization, output
  validation, and settlement, with cancellation > total > step > Agent
  precedence and no surviving work.
- Exact location:
  `crates/skald/skald-workflow/src/workflow.rs:421-463,493-540`;
  `crates/skald/skald-agent/src/loop_runtime.rs:138-203`;
  `crates/skald/skald-workflow/src/route.rs:381-407,569-606`.
- Evidence and reachability: `agent_deadline` is computed at
  `workflow.rs:434-435`, but there is no arm for it in the select. Native and
  ExtGateway routes rely on the later relative timer inside `run_prompt`.
  After that timer expires, `append_terminal_journal_event` can remain pending
  forever. WyrdGateway alone consumes the earlier absolute deadline, creating
  route-dependent behavior. A Workflow Agent with a Journal that completes
  earlier appends and blocks the terminal append reaches this path.
- Observable consequence: a finite Agent timeout can overrun or never return,
  preventing retry and terminal `WorkflowRun` production while provider/tool
  or settlement work remains live.
- Decision-complete minimum correction: add the already-computed
  `agent_deadline` to `StepTask::run`'s existing biased select after
  cancellation, total deadline, and step deadline but before the attempt
  future. On expiry, construct the existing typed Agent timeout outcome and
  pass it through `AttemptOutcome::from_agent`, preserving its stable code and
  retryability. Do not alter the standalone Agent timeout contract, add a
  route-specific timer, detach journal work, or add a runtime.
- Preserved adjacent behavior: cancellation/total/step precedence, ordinary
  standalone Agent journaling, route-owned provider retry/fallback, Workflow
  backoff, and span outcome recording.
- Focused closure proof: a deterministic Workflow test supplies an Agent whose
  pre-terminal journal operations complete and terminal append remains
  pending. Advance paused time and prove the fixed Agent deadline returns the
  Agent timeout, performs the authored retry/exhaustion accounting, closes the
  attempt spans, and leaves no live attempt. Retain the existing precedence and
  Native/ExtGateway route-isolation assertions.

### FIND-TASK-001-21 — A panicking attempt is traced as cancelled but returned as failed

- Discovery source: `TEL-001`
- Status: **CONFIRMED**
- Classification: `INCORRECT`
- Violated obligation: REQ-053 requires every `workflow.step` attempt span to
  carry its actual outcome and stable Wyrd error code on failure; the telemetry
  authority requires terminal outcome and error consistency.
- Exact location:
  `crates/skald/skald-workflow/src/workflow.rs:319-357,421-473,583-633`;
  reachable caller-supplied panic at
  `crates/skald/skald-agent/src/loop_runtime.rs:379-520`.
- Evidence and reachability: a caller-supplied `AgentTool::invoke` is awaited
  without a panic boundary. Unwinding drops the live `AttemptSpan`, whose
  `Drop` records `cancelled`; only afterward does `settle` convert the
  `JoinError` to `WYRD_WORKFLOW_500_INTERNAL` and fail the run.
- Observable consequence: the authoritative run and its required attempt span
  disagree, so failure counts and trace/run correlation are wrong.
- Decision-complete minimum correction: catch unwinding around the complete
  attempt future inside `StepTask::run`, while `AttemptSpan` remains alive,
  using the already-installed `futures` support. Convert a panic to the
  existing non-retryable `WorkflowInternal` outcome and reuse the ordinary
  `span.failed` and settlement path. Do not catch only the named tool call or
  add a second telemetry/error owner.
- Preserved adjacent behavior: genuine cancellation remains `cancelled`, the
  returned panic projection remains `WYRD_WORKFLOW_500_INTERNAL`, no payload is
  recorded, and ordinary retry classification is unchanged.
- Focused closure proof: run one Workflow step whose registered local tool
  panics. Assert the run and step fail with
  `WYRD_WORKFLOW_500_INTERNAL`, the sole attempt span records `failed` with the
  same `error.type`, and the existing explicit-cancellation case still records
  `cancelled` without a fabricated error code.

### FIND-TASK-001-22 — Successful ExtGateway responses can return a bound secret

- Discovery source: `NET-R3-001`
- Status: **CONFIRMED**
- Classification: `INCORRECT`
- Violated obligation: REQ-042 and INV-012 require a bound secret value never
  to enter a returned result or error.
- Exact location:
  `crates/skald/skald-providers/src/clients/external.rs:28-43,96-173`;
  `crates/skald/skald-agent/src/loop_runtime.rs:329-376`;
  `crates/skald/skald-workflow/src/workflow.rs:493-540`.
- Evidence and reachability: non-success bodies are withheld, but a successful
  native response is decoded unchanged. Its retained strings flow through
  `AgentRun.output` or structured/tool content into the public Workflow result.
  A bound malicious gateway can return a valid 2xx response containing the
  credential it received.
- Observable consequence: a caller without binding-read access can receive and
  reuse the execution environment's credential.
- Decision-complete minimum correction: in `ExternalGatewayClient`, retain the
  values of headers already marked sensitive and inspect only the decoded JSON
  content retained by the typed successful response, recursively including
  retained member names and strings. If any nonempty bound value occurs, return
  one fixed safe non-retryable provider refusal without the response or match.
  Reuse the existing client, sensitive-header designation, bounded body, and
  error types; add no route-layer or result-layer guard.
- Preserved adjacent behavior: clean successful responses, non-success status
  and retry metadata, native-provider clients, Wyrd-gateway semantics, endpoint
  policy, and payload bounds remain unchanged.
- Focused closure proof: valid 2xx external responses reflect a canary in
  ordinary assistant text and retained structured/tool content, including a
  JSON-escaped form. Prove complete client errors and serialized Workflow
  results omit the canary and do not retry the containment refusal; retain a
  clean-success control and the existing 401 reflection case.

### FIND-TASK-001-23 — Agent spans emit non-conforming GenAI provider, model, and finish-reason fields

- Discovery sources: `TEL-002`, `STD-R3-001`
- Status: **REVISED**
- Classification: `INCORRECT`
- Violated obligation: REQ-053 and
  `architecture/references/domain/telemetry-observations.md` require standard
  GenAI attributes to follow OpenTelemetry semantic conventions.
- Exact location:
  `crates/skald/skald-agent/src/loop_runtime.rs:297-303,590-623,779-825`;
  incomplete proof at
  `crates/skald/skald-agent/tests/agent_timeout.rs:163-245`.
- Evidence and reachability: `provider_label` emits `google` and `vertex`
  instead of `gcp.gemini` and `gcp.vertex_ai`; `request_model` returns `None`
  for Gemini and Vertex although the resolved Prompt owns the selected model;
  and `gen_ai.response.finish_reasons` is recorded as one scalar string despite
  its registered `string[]` type. The only capture test covers OpenAI and does
  not inspect the finish-reason field.
- Observable consequence: downstream GenAI consumers split or discard standard
  dimensions and cannot reliably group Google/Vertex calls by provider/model.
- Decision-complete minimum correction: derive provider metadata from the
  existing typed request, supply the exact resolved Prompt model to the Agent
  and model-call spans, map Google/Vertex to the applicable well-known provider
  values, and delete the optional scalar finish-reason attribute. Keep the
  approved `chat` operation value; add no direct OTLP instrumentation,
  JSON-encoded pseudo-array, dependency, or telemetry abstraction.
- Preserved adjacent behavior: span hierarchy, stable error fields, journal
  finish reasons, provider dispatch, payload exclusion, and OpenAI/Anthropic
  mappings remain unchanged.
- Focused closure proof: extend the existing production-shaped capture test
  with representative OpenAI, Gemini, and Vertex runs. Assert exact provider
  and model values, absence of `gen_ai.response.finish_reasons`, retained
  `chat` operation, and payload-marker exclusion on every span/event.

### FIND-TASK-001-24 — New and materially changed Agent items lack required rustdoc

- Discovery sources: `STD-R3-002`, `MNT-R3-002`
- Status: **CONFIRMED**
- Classification: `VIOLATION`
- Violated obligation: `AGENTS.md` section 16 and
  `architecture/agent-rules.md` require substantive rustdoc for every new or
  materially modified Rust item, including private test items, fields, trait
  methods, errors, cancellation, and side effects.
- Exact location:
  `crates/skald/skald-agent/src/loop_runtime.rs:33-203`;
  `crates/skald/skald-agent/src/request_builder.rs:29-63,129-176`;
  `crates/skald/skald-agent/tests/agent_timeout.rs:248-379,417-443`.
- Evidence: `run` and `run_prompt` were materially changed but retain one-line
  docs with no `# Errors` or timeout/journal/cancellation contract;
  `validate_prompt_loop_request` and `assistant_message` gained fallible
  Responses behavior without `# Errors`; the new `RecordingJournal`,
  `RecordingProvider`, `FixedTool`, `ControlledTool`, and `SlowTool` fixtures,
  their fields, helpers, and trait methods are undocumented. The documented
  neighboring `FailingTool` confirms the intended local shape.
- Observable consequence: the repository's hard documentation gate is
  unsatisfied and maintainers must reconstruct lifecycle and fixture
  invariants from implementation details.
- Decision-complete minimum correction: document these existing items in
  place, including error conditions, terminal-journal best-effort behavior,
  timeout/cancellation effects, scripted response exhaustion, poison recovery,
  and controlled tool notification/wait behavior. Do not split the test,
  extract a documentation helper, or document untouched surrounding code.
- Preserved adjacent behavior: signatures, runtime behavior, fixture control
  flow, and test placement remain unchanged.
- Focused closure proof: source inspection of every cited item plus
  `mise run fmt`, `mise run lints`, and the existing Agent focused tests.

### FIND-TASK-001-25 — Stateful Agent orchestration remains outside `Agent`

- Discovery source: `MNT-R3-001`
- Status: **CONFIRMED**
- Classification: `VIOLATION`
- Violated obligation: `AGENTS.md` section 5 and
  `architecture/agent-rules.md` require stateful workflows and internal
  orchestration using an owner's state to be inherent methods on that concrete
  owner; passing the owner as a parameter is explicitly not stateless.
- Exact location:
  `crates/skald/skald-agent/src/loop_runtime.rs:33-203` with forwarders at
  `crates/skald/skald-agent/src/agent.rs:662-696`.
- Evidence and reachability: the two free orchestration functions accept
  `&Agent`, repeatedly consume its prompt, run configuration, provider
  override, journal, session, callbacks, and tools, and own the complete run
  lifecycle. The public inherent methods merely forward. The module was
  materially rewritten for REQ-053, so old functional drift is not precedent.
- Observable consequence: the natural state owner and lifecycle implementation
  can drift independently, and maintainers cannot discover the internal
  workflow through the required owner shape.
- Decision-complete minimum correction: make the input/session and explicit
  Prompt orchestration inherent `Agent` methods in the existing module and
  remove the forward-only free functions. Keep stateless request, span,
  journal, and conversion helpers free. Add no service, trait, wrapper, or new
  module.
- Preserved adjacent behavior: the public `Agent::run`, `run_with`, and
  `run_prompt` APIs, provider override precedence, Agent state, and all runtime
  semantics remain unchanged.
- Focused closure proof: source inspection plus the existing Agent timeout,
  Responses loop, structured-output, and Workflow tests; no new behavioral
  test is needed for the ownership-only move.

### FIND-TASK-001-26 — A library adds `anyhow` for one fixed resolver refusal

- Discovery source: `MNT-R3-003`
- Status: **CONFIRMED**
- Classification: `VIOLATION`
- Violated obligation: `AGENTS.md` section 4 reserves `anyhow` for binaries;
  the repository simplicity ladder requires the standard library before a
  general error dependency.
- Exact location:
  `crates/skald/skald-workflow/src/output.rs:17-31` and
  `crates/skald/skald-workflow/Cargo.toml:45`.
- Evidence: the crate's only source-level `anyhow` use constructs one fixed
  `NoRemoteResolver` error. `jsonschema::SchemaResolverError` is an
  `anyhow::Error` alias that accepts an ordinary standard error, while the
  public boundary already projects compilation failure into typed
  `WyrdError::WorkflowOutputSchema`.
- Observable consequence: a library violates the explicit error policy and
  retains a direct dependency for one untyped fixed message.
- Decision-complete minimum correction: construct a fixed `std::io::Error`
  and convert it through the existing `SchemaResolverError` boundary, then
  remove the direct `anyhow` dependency. Add no local error enum.
- Preserved adjacent behavior: remote-reference refusal, diagnostic text,
  typed public projection, and JSON-schema behavior remain unchanged.
- Focused closure proof: the existing output-schema tests plus
  `mise run test:skald`, formatting, and lints.

### FIND-TASK-001-27 — Changed permanent documentation names deleted surfaces

- Discovery source: `MNT-R3-004`
- Status: **CONFIRMED**
- Classification: `REGRESSION`
- Violated obligation: documentation must match the typed surface; the
  original task and REQ-053 delete obsolete Workflow machinery and Observer
  examples without compatibility aliases.
- Exact location: `docs/architecture/skald.md:9-30`, `CHANGELOG.md:5-13`, and
  `examples/rust/README.md:17-25`.
- Evidence: the changed architecture entry still attributes deleted
  `WorkflowDef`, `Task`/`TaskDef`, `Context`, `execute_task`, and
  `MessageConversion` handoff to `skald-workflow`; the changed Unreleased entry
  repeats `execute_task` and `MessageConversion`; and the examples README lists
  `tracing_stdout.rs` although the candidate deletes that file and Cargo target.
- Observable consequence: maintainers and users are directed to nonexistent
  APIs and an unrunnable example on the exact public surfaces this task changed.
- Decision-complete minimum correction: replace only the two obsolete Workflow
  descriptions with the implemented explicit Agent DAG, binding, route,
  bounded-execution, and portable-run ownership, and delete the absent example
  entry. Add no compatibility note, migration section, or replacement example.
- Preserved adjacent behavior: accurate crate-map entries, current tracing
  description, surviving examples, and generated docs remain unchanged.
- Focused closure proof: repository search for the deleted names in these
  changed permanent surfaces, then `mise run docs:check` and
  `mise run check:examples`.

## Rejected corrections and non-findings

- Do not move terminal journal settlement into the standalone Agent timer as a
  substitute for `FIND-TASK-001-20`; the Workflow fixed deadline owns the full
  attempt and all routes.
- Do not add direct OpenTelemetry instrumentation or encode a JSON string under
  the `string[]` finish-reason key; the optional field can be deleted.
- Do not require `generate_content` solely because the Google transport method
  has that name; current authority permits `chat` for the matching scenario.
- Do not scan ignored raw ExtGateway response members, add downstream secret
  guards, or weaken REQ-042 with an unstated entropy threshold.
- Do not add a panic service, Agent service wrapper, local error enum,
  replacement tracing example, or new dependency/check.

## Validation result

**FIX_REQUIRED** — eight bounded findings remain:
`FIND-TASK-001-20` through `FIND-TASK-001-27`. All corrections fit approved
Revision 11 and existing owners; none requires a new product, public API,
architecture, compatibility, cross-service, concurrency-semantics,
resource-ownership, or persistent-data decision.
