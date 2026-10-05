# Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 11
  (`9a621a28a`), including REQ-053
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation authority:
  `review/TASK-001-r2/TASK-001-R1-close-validated-runtime-gaps.md` and
  `review/TASK-001-r2/TASK-001-R1-addendum-revision-11.md`

The candidate was exactly `afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f`
when this review began and after source inspection. No `.codegraph/` directory
exists, so navigation used the complete Git diff and repository search.

## Authority Coverage

This pass applied `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-design.md`,
`architecture/references/languages/spec-driven-development.md`,
`architecture/references/languages/maintainer-style.md`, and the applicable
Rust, Python/PyO3, generated-declaration, documentation, and testing
references. It reviewed the complete base-to-candidate range and treated the
round-two findings as closure hypotheses rather than conclusions.

## Changed-Surface Coverage

| Surface | Symbols, callers, tests, and declarations inspected | Assessment |
|---|---|---|
| Workflow wire contract | `WorkflowSpec`, steps/actions/bindings/routes, run/status/result/error DTOs, validation, references, generated schemas and error declarations | Contract types remain cohesive in `wyrd-spec`; names and generated shapes align with the revised task. |
| Resolved planning and execution | `ResolvedGraph`, `ExecutionPlan`, `WorkflowExecutor`, `StepTask`, `AttemptOutcome`, `RunLedger`, result budgeting, retry/deadline/cancellation settlement, tracing guards, focused Workflow tests | Stateful scheduling and settlement have concrete owners; pure planning stays synchronous. No new owner or layout issue in `skald-workflow`. |
| Agent loop and Responses replay | `Agent::run*` callers, `loop_runtime::run`, `run_prompt`, `run_loop`, request reconstruction, native Responses reasoning items, session/journal helpers, `loop_responses` and `agent_timeout` | Responses state and tracing remain with the Agent runtime, but the materially changed orchestration is still implemented as free functions that thread the `Agent` owner (MNT-R3-001), and changed fallible entry/helper docs omit required error contracts (MNT-R3-002). |
| Routes and external egress | `WorkflowExecutionDependencies`, gateway callers/bindings, reserved-header validation, `ExternalGatewayClient`, endpoint policy, refusal-body withholding, gateway consumers and security tests | Existing endpoint policy and provider transport are reused. Secret-bearing state has redacted debug shape and the external-only refusal rule stays localized. |
| Observer deletion and tracing replacement | workspace/manifests, deleted `skald-observer`, Agent/Workflow hooks, `AttemptSpan`, GenAI span helpers, package exports, tests, examples, checks, docs | The Observer implementation and public surfaces are removed without a compatibility layer. Payload-free tracing uses existing owners and test capture. Permanent docs still name removed Workflow APIs and a deleted example (MNT-R3-004). |
| Rust authoring and consumers | `Workflow`, `WorkflowBuilder`, inputs/bindings/outputs, Wyrd/Vala direct consumers, Rust examples and crate README | The Workflow owner is discoverable and consuming methods preserve the builder contract. No additional abstraction is warranted. |
| Python/PyO3 boundary | SDK-owned `workflow.rs`, owner-crate orphan conversion, native registration, public package exports, exact `TypedDict` result declarations, duplicate generated stubs, Python tests/examples | Round-two ownership and result-typing findings are closed: task-added wrappers now live in the SDK and declarations preserve the run/step/error shapes. |
| Tests and verification artifacts | Workflow/Agent focused tests, inline Workflow scenarios, external cross-crate Agent tests, recorded exact selectors and scoped lanes, cumulative diff check | Test names and setup make their scenarios followable. The recorded proof covers the changed language and documentation surfaces; this review did not rerun the implementation suite. |

## Prior-Finding Closure

| Prior maintainer finding | Closure evidence | Result |
|---|---|---|
| MNT-R2-001 — Python Workflow ownership | `sdks/wyrd-sdk-python/src/workflow.rs` now owns conversion, wrappers, execution bridge, and registration; `skald-workflow/src/python.rs` retains only the orphan-rule error conversion. | CLOSED |
| MNT-R2-002 — erased Python result DTOs | `WorkflowRunError`, `WorkflowStepResult`, and `WorkflowRunDict` are exact `TypedDict` declarations used by `WorkflowRun` accessors and `to_dict`. | CLOSED |
| MNT-R2-003 — Observer documentation | REQ-053 deletes the Observer system and every affected hook; there is no surviving callback surface to document. | CLOSED BY DELETION |
| MNT-R2-004 — function-local test imports | Workflow test dependencies now live in the owning test-module import blocks. | CLOSED |
| MNT-R2-005 — generated step-ID guide | `build-a-workflow.svx` now describes sanitization, leading-prefix, unnamed, and collision behavior and points callers to `Workflow.steps`. | CLOSED |

## Material Findings

### MNT-R3-001 — Materially changed Agent orchestration still lives outside its owner

- Changed location: `crates/skald/skald-agent/src/loop_runtime.rs:34-203`;
  forwarding callers at `crates/skald/skald-agent/src/agent.rs:668,681,695`.
- Governing rule: `AGENTS.md` section 5 and
  `architecture/agent-rules.md` require internal orchestration that uses an
  owner's state or dependencies to be inherent methods on that concrete owner;
  passing the owner as a parameter does not make the workflow stateless.
- Evidence: `loop_runtime::run` and `loop_runtime::run_prompt` accept
  `this: &Agent`, read its provider override, prompt, run configuration,
  journal, session, callbacks and tools, and coordinate the complete run
  lifecycle. They were materially changed to remove Observer behavior and own
  the new `invoke_agent` tracing lifecycle. The public `Agent` methods only
  forward their arguments to these free functions.
- Concrete maintenance cost: the public owner exposes the operation but its
  lifecycle implementation is undiscoverable from the owner and can be
  changed independently of the invariants on `Agent`; the current `this`
  parameter is dependency threading around the repository's required method
  shape.
- Smallest testable correction: make these two orchestration bodies focused
  private inherent methods on `Agent` in the existing `loop_runtime` module
  and have the public methods call them through `self`. Keep stateless request,
  span, journal, and conversion helpers as free functions; add no service,
  trait, or module. Existing Agent timeout, Responses loop, and Workflow tests
  are the proof.
- Nearby Wyrd pattern: `WorkflowExecutor::execute`/`drive` and
  `StepTask::run` in `skald-workflow/src/workflow.rs` keep lifecycle behavior
  on the state-owning concrete type.

### MNT-R3-002 — Changed fallible Agent entry points do not document their error contracts

- Changed location: `crates/skald/skald-agent/src/loop_runtime.rs:33-203` and
  `crates/skald/skald-agent/src/request_builder.rs:29-63,129-176`.
- Governing rule: `architecture/agent-rules.md` requires substantive rustdoc
  for every materially modified Rust item and a `# Errors` section for every
  fallible function; the maintainer guide requires docs to describe the actual
  failure boundary rather than repeat a name.
- Evidence: `run` and `run_prompt` now own new tracing and changed terminal
  behavior but are documented only as “Runs the bounded loop.”
  `validate_prompt_loop_request` now admits Responses requests and
  `assistant_message` now retains native Responses continuation items, yet
  neither public fallible helper has a `# Errors` contract. The adjacent
  changed `extract_messages`, `rebuild_request_messages`, and `run_loop`
  functions demonstrate the required local documentation shape.
- Concrete maintenance cost: a maintainer changing Responses admission or the
  Agent lifecycle cannot determine from these items which prompt, provider,
  callback, journal, session, tool, timeout, or message-shape failures cross
  each boundary; callers must reconstruct that contract from the bodies.
- Smallest testable correction: document the existing error boundaries in
  place, including the stable categories returned by the two run entry points
  and the unsupported/empty response cases of the two request helpers. Do not
  extract helpers or change behavior. `mise run docs:check`, formatting, and
  lints are sufficient proof in addition to the existing focused tests.

### MNT-R3-003 — The new validator uses `anyhow` in a library where the standard error boundary suffices

- Changed location: `crates/skald/skald-workflow/src/output.rs:17-31` and
  `crates/skald/skald-workflow/Cargo.toml:45`.
- Governing rule: `AGENTS.md` section 4 reserves `anyhow` for binaries, and the
  Ponytail/maintainer rules prefer the standard library before a general error
  dependency in a focused library boundary.
- Evidence: the new `NoRemoteResolver` creates its sole resolver error with
  `anyhow::anyhow!`. `jsonschema::SchemaResolverError` is the dependency's
  error return type and accepts an ordinary standard error; the Workflow
  module already projects compilation failure into typed
  `WyrdError::WorkflowOutputSchema` at its public boundary.
- Concrete maintenance cost: the library now directly depends on and authors
  an untyped application-error value for one fixed refusal, creating a second
  error vocabulary inside an otherwise typed module and retaining a direct
  dependency that has no other source use.
- Smallest testable correction: construct a fixed `std::io::Error` (or another
  existing standard error) and convert it to `SchemaResolverError`, then remove
  the direct `anyhow` dependency if no source use remains. Preserve the current
  no-network resolver behavior and public `WorkflowOutputSchema` projection;
  the existing output-schema tests plus the Skald lane prove the change.

### MNT-R3-004 — Permanent documentation still advertises APIs and an example deleted by this task

- Changed location: `docs/architecture/skald.md:28-30`, `CHANGELOG.md:5-8`, and
  `examples/rust/README.md:17-25`.
- Governing principle: the maintainer guide requires documentation to match
  the typed surface; REQ-053 and the original task remove obsolete Workflow
  machinery and the Observer examples without compatibility aliases.
- Evidence: the architecture crate map still says `skald-workflow` owns
  `WorkflowDef`, `Task`/`TaskDef`, `Context`, `execute_task`, and
  `MessageConversion` handoff, although those modules and exports are deleted
  in the cumulative candidate. The Unreleased changelog repeats
  `execute_task` and `MessageConversion`. The Rust examples README still lists
  `tracing_stdout.rs`, while the candidate deletes both that file and its Cargo
  bin target.
- Concrete maintenance cost: maintainers and users are directed to nonexistent
  types and an unrunnable example precisely where they look for the current
  crate boundary and examples.
- Smallest testable correction: replace the two legacy Workflow descriptions
  with the current explicit-binding Agent DAG, route, bounded-execution, and
  portable-run ownership already documented by the crate root; remove the
  deleted `tracing_stdout.rs` list entry. Add no compatibility note or
  replacement example. Prove with `mise run docs:check` and
  `mise run check:examples`.

## Verification Evidence and Limits

- The remediation record reports green exact selectors for the Workflow
  budgeting/lifecycle/tracing/binding tests, Responses tool loop/session
  tests, Agent timeout/tracing tests, and `skald-spec` message tests.
- It also records green `test:skald`, Python unit/typecheck/format/lint,
  codegen, client-tier, PyO3-scope, unwrap audit, docs, examples, Rust format
  and lints, plus an explicit clean cumulative `git diff --check`.
- This review inspected the cumulative diff, changed owners and modules,
  direct callers, focused tests, manifests, generated declarations, package
  exports, examples, and relevant permanent documentation. It did not rerun
  the recorded implementation suite.
- Green behavioral and generation lanes do not establish the owner-shape,
  required rustdoc, library error-dependency, or semantic documentation
  obligations in MNT-R3-001 through MNT-R3-004.

## Calibration Notes

- `WorkflowExecutor`, `RunLedger`, `ExecutionPlan`, `StepTask`,
  `WorkflowExecutionDependencies`, and `AttemptSpan` are cohesive concrete
  owners or guards required by their state/lifecycle; no deletion or generic
  abstraction is recommended.
- The SDK Workflow module is intentionally a thin PyO3 projection over the
  Rust-native owner. Its conversion helpers are boundary-local and do not
  duplicate validation or execution behavior.
- `skald-workflow` retains a `python` feature only for the orphan-rule error
  conversion. Removing that feature or migrating unrelated legacy Agent PyO3
  code is outside this review; MNT-R3-003 concerns only the directly used
  general-purpose error dependency in the new validator.
- The long Workflow scenario tests share the existing support module and
  prove task-defined lifecycle groups. Splitting them remains a preference,
  not a material finding.

## Overall Result

**FAIL**

The round-two maintainer findings are closed, and the revised Observer
deletion/tracing and SDK ownership are coherent. MNT-R3-001 through
MNT-R3-004 are bounded changed-surface maintenance violations with small,
testable corrections; none requires a product or architecture decision.
