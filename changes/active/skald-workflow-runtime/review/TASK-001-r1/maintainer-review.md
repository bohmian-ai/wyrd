# Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `eb22b03f2bb766886d839bda23aafbd4ba130ab3`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 9
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Review lens: layout, ownership and method shape, naming, argument and return types, test clarity, documentation, generated declaration parity, and the smallest repository-native shape that remains safe to maintain.

## Authority Read

The review applied `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/references/languages/spec-driven-development.md`, `architecture/references/languages/maintainer-style.md`, and the applicable architecture, Rust, PyO3, Python/stub, testing, error, and external-network-security references. The candidate stayed at the requested commit throughout this review.

## Changed-Surface Coverage

| Surface | Symbols and consumers inspected | Maintainer assessment |
|---|---|---|
| Workflow wire contract | `WorkflowSpec`, `WorkflowStep`, `WorkflowBinding`, `LlmRoute`, external binding config, run/status/result/error DTOs, validation/errors, schemas and ref visitors in `wyrd-spec` | The contract types are cohesive, named in domain terms, and substantially documented. The Python projection does not preserve this precision (MNT-002). |
| Resolved plan and execution | `ResolvedGraph`, `ExecutionPlan`, `PlannedStep`, `WorkflowExecutor`, `StepTask`, `AttemptOutcome`, `RunLedger`, `OutputValidator`, binding selection, retry/deadline/settlement paths and their callers | Owners are discoverable and state is held on the struct that operates it. Pure validation remains synchronous. No material owner/method-shape finding. |
| Route and egress | `WorkflowExecutionDependencies`, `StepRoute`, gateway adapter, external binding collection, `ExternalGatewayClient`, relocated `EndpointPolicy`, gateway/server/test consumers | The route owner and narrow gateway trait are understandable and reuse the one endpoint policy. No maintainer finding beyond the public Python placement issue. |
| Agent request/session support | OpenAI Responses request extraction/rebuild, assistant/tool/session conversions, loop callers, native-response tests | The dialect-specific behavior remains in the request/session owners and is covered by focused integration tests. No material maintainer finding. |
| Observation | new Workflow attempt/result/backoff hooks across `Observer`, `CompositeObserver`, `PythonObserver`, Python `Observer`, generated stubs, executor call sites | Surface parity exists, but added implementations and Python methods do not meet the repository documentation contract (MNT-003). |
| Rust authoring surface | `Workflow`, `WorkflowBuilder`, `WorkflowInput`, `with_inputs`, `with_step_inputs`, `with_outputs`, validation/run variants, card/YAML consumers and examples | The existing owner remains the entry point and the methods are discoverable. Documentation incorrectly states the step-ID rule (MNT-005). |
| PyO3 and Python package | `skald-workflow/src/python.rs`, SDK module registration, `wyrd.agent` exports, `WorkflowRun`, `Observer`, package and duplicate generated stubs, Python tests/examples | New boundary behavior is implemented in the migration crate rather than the SDK owner (MNT-001), and run result declarations erase exact DTO structure (MNT-002). |
| Tests | all eight named scenario tests, shared test support, OpenAI Responses tests, observer tests, Python explicit-binding tests, deleted external tests and replacement inline tests | Coverage is broad, but several scenario functions bundle many independently failing behaviors and hide imports inside functions (MNT-004). |
| Docs/examples/generated artifacts | workflow README, docsite guide, Rust/Python examples, YAML examples, error docs, schema/error-code/stub updates | Generated declarations and examples moved with the API. One user-facing step-ID statement contradicts construction behavior (MNT-005). |

## Material Findings

### MNT-001 — New Python Workflow boundary behavior is owned by the migration crate instead of the Python SDK

- Changed locations: `crates/skald/skald-workflow/src/python.rs:41`, `crates/skald/skald-workflow/src/python.rs:347`, `crates/skald/skald-workflow/src/python.rs:558`; aggregation-only caller at `sdks/wyrd-sdk-python/src/lib.rs:48`.
- Governing rule: `AGENTS.md` §§7–8 and the PyO3/maintainer references require new or materially relocated PyO3 wrappers, conversion, aggregation, and the public Python package boundary to live under `sdks/wyrd-sdk-python/src`; retained owner-crate wrappers are migration state, not the home for new behavior. The task itself assigns new/materially relocated Python wrappers to that SDK.
- Evidence: the candidate adds Python input/default/binding conversion, four new Python Workflow methods, and the new portable `PyWorkflowRun` wrapper to `skald-workflow`. The SDK continues to do nothing more than call `skald_workflow::python_register`.
- Concrete maintenance cost: the Rust-native Workflow owner and the Python product boundary remain fused behind a core-crate feature. A maintainer looking in the declared SDK owner cannot find or change the new result and authoring contract, and future Python-only evolution continues widening a Skald crate that Rust and server maintainers must understand.
- Smallest testable correction: keep all Rust-native validation and execution on `Workflow`, but put the newly added Python conversion, `WorkflowRun` projection, Python methods/wrapper, and registration in `sdks/wyrd-sdk-python/src`, wrapping the Rust-native owner. Leave untouched legacy PyO3 in place only where this task did not materially add behavior. Prove public imports, unit behavior, Python-feature scope, stubs, and type checking from the SDK boundary.

### MNT-002 — The Python `WorkflowRun` declaration erases the exact portable result types

- Changed locations: `sdks/wyrd-sdk-python/python/wyrd/agent/__init__.pyi:401`, especially `steps` at line 424 and `error` at line 433; the same generated declarations appear in `python/wyrd/stubs/agent.pyi`. Runtime projection originates at `crates/skald/skald-workflow/src/python.rs:574`.
- Governing principle: the maintainer guide's Python contract rule requires public parameter and return types to describe the actual result and rejects broad `Any`/nested dictionaries when a domain type exists. `WorkflowStepResult` and `WorkflowRunError` are exact public DTOs in `wyrd-spec`.
- Evidence: `steps` is declared as `dict[str, dict[str, Any]]` and `error` as `dict[str, Any] | None`; `to_dict` is equally broad. The docstring lists expected keys but the type checker cannot enforce statuses, attempts, timestamps, payload optionality, or the bounded error shape.
- Concrete maintenance cost: additions or renames in the Rust wire contract can drift without a Python type error, while Python callers must rediscover key spelling and value optionality from prose. This is precisely the declaration-parity burden generated stubs are intended to remove.
- Smallest testable correction: expose generated SDK-owned typed projections (typed dictionaries are sufficient; separate runtime classes are unnecessary) for `WorkflowStepResult`, `WorkflowRunError`, and the complete run dictionary, and use them in the getters and `to_dict`. Keep only JSON-valued fields such as declared outputs/details broad. Regenerate rather than hand-edit stubs, then run codegen and Python type checking.

### MNT-003 — Added observer implementations and public Python hooks lack the required contract documentation

- Changed locations: `crates/skald/skald-observer/src/composite.rs:144`, `:150`, `:163`; `crates/skald/skald-observer/src/python.rs:280`, `:294`, `:318`; public Python methods at `sdks/wyrd-sdk-python/python/wyrd/observer.py:160`, `:168`, `:181`.
- Governing rule: `architecture/agent-rules.md` and `AGENTS.md` §16 require substantive rustdoc for every new or materially modified Rust item, including private and trait-implementation methods. The maintainer guide requires public Python docstrings to keep argument meanings and types aligned with signatures.
- Evidence: all six added Rust implementation methods have no rustdoc. The new public Python methods provide only one-line summaries (or an unstructured note) and omit the `Args` contract that the generated `.pyi` does contain.
- Concrete maintenance cost: the fan-out and interpreter bridge are the places where concurrency, payload omission, error-code meaning, and millisecond conversion matter, yet those implementation-local obligations are invisible where a maintainer changes the forwarding behavior. Runtime Python help also exposes less contract information than the generated stub.
- Smallest testable correction: document the new implementation methods at their concrete forwarding/bridge locations, including non-blocking callback-error handling and duration conversion where relevant, and give the Python methods signature-aligned `Args` sections matching the generated declarations. No behavior or abstraction change is needed.

### MNT-004 — Scenario proof is concentrated into very large, non-isolatable tests and hides dependencies inside test functions

- Changed locations: `crates/wyrd-spec/src/card/workflow.rs:1596`; `crates/skald/skald-workflow/src/workflow.rs:549`, `:810`, `:1245`, `:1472`, `:1691`; `crates/skald/skald-workflow/src/workflow_surface.rs:847`, `:968`. Function-local imports occur at `workflow.rs:811`, `:1246`, `:1473`, `:1692` and `workflow_surface.rs:848`.
- Governing rules/principles: the maintainer guide requires tests to prove a caller-visible outcome with clear setup and failure localization; the spec-driven reference requires one scenario at a time; `architecture/agent-rules.md` requires imports at the module top, including test-module dependencies.
- Evidence: `explicit_workflow_contract` spans about 420 lines; `bounded_attempt_lifecycle` about 430; `isolated_route_calls` about 220; `bound_external_gateway_security` about 210. Each sequentially proves many independent rejection, retry, cancellation, routing, security, and size cases. A first panic prevents later obligations from executing, and the exact named selector cannot isolate which behavior regressed. Several tests also conceal their dependency surface with function-scoped `use` blocks.
- Concrete maintenance cost: a maintainer changing one retry class or one endpoint rule must understand and rerun an umbrella test whose setup and assertions cover unrelated lifecycles. Failure output points into a long procedure rather than a named behavior, and early failures suppress evidence for later cases.
- Smallest testable correction: retain each task-required named test for its primary path, but split independent outcomes into focused, descriptively named tests or compact table-driven cases using the existing test-support module. Move all imports to the owning `mod tests` import block. Do not add a new harness or dependency.

### MNT-005 — The workflow guide states a step-ID rule the implementation does not provide

- Changed location: `docs/src/content/docs/how-to/build-a-workflow.svx:19`; governing implementation at `crates/skald/skald-workflow/src/workflow_surface.rs:611`.
- Governing principle: public documentation must match the typed contract and implementation; maintainers and users should not need to infer an identity transformation from source.
- Evidence: the guide says, "Step ids are the Agent names." `Workflow::next_step_id` actually replaces non-identifier characters with `_`, prefixes a leading digit, synthesizes IDs for unnamed Agents, and appends numeric suffixes on collisions.
- Concrete maintenance cost: callers can write bindings or `after` references using the documented Agent name and receive an unknown-step failure for names containing punctuation, leading digits, duplicates, or no name. Examples cover only the simple case and do not reveal the transformation.
- Smallest testable correction: describe step IDs as deterministic IDs derived from Agent names, list the sanitization/collision behavior, and tell callers to use the resulting `Workflow.steps` value when the name is not already a unique identifier. No API change is required.

## Verification Evidence and Limits

- The task records passing focused scenario selectors, `test:skald`, `test:shared`, `test:wyrd`, Python unit/type checks, codegen, boundary checks, formatting, lints, and Python formatting/lints.
- This maintainer pass inspected the complete base-to-candidate diff and the owning modules, direct callers, public declarations, examples, and relevant tests. It also ran `git diff --check` for the immutable range successfully.
- The findings above concern ownership, typed discoverability, documentation, and proof maintainability; the reported green lanes do not establish those structural requirements.

## Calibration Notes

- `WorkflowExecutor`, `RunLedger`, `ExecutionPlan`, and `WorkflowExecutionDependencies` are justified concrete owners rather than speculative service layers.
- The new external endpoint client reuses the relocated endpoint policy instead of creating a second network-policy implementation.
- The large wire-contract module remains cohesive enough that splitting it solely by line count would add navigation cost rather than remove it.

## Overall Result

**FAIL**

The runtime's principal Rust ownership shape is maintainable, but MNT-001 through MNT-005 are material changed-surface issues. The Python boundary placement and public type erasure are the most consequential; the documentation and test-shape findings have small, bounded corrections and require no redesign of the execution engine.
