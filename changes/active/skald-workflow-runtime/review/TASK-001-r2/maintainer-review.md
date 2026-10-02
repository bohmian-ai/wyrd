# Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `28473e049705595306f2934cf4bc664168254086`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 10
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Review lens: layout, ownership and method shape, naming, argument and return
  types, test clarity, documentation, generated declaration parity, and the
  smallest repository-native shape that remains safe to maintain.

The candidate was `28473e049705595306f2934cf4bc664168254086` before and after
this review. Production source is unchanged from round 1 candidate
`eb22b03f2bb766886d839bda23aafbd4ba130ab3`; the intervening range changes
only the active packet and adds the prior review artifacts.

## Authority Coverage

This review applied `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`,
`architecture/references/languages/spec-driven-development.md`,
`architecture/references/languages/maintainer-style.md`, and the applicable
Rust, error, Python/PyO3, generated-contract, documentation, and testing rules.
No `.codegraph/` index exists, so source navigation used repository search and
the complete base-to-candidate diff.

Revision 10 expressly adopts
`HashMap<HeaderName, SecretString>` for external secret headers and
`RemoteProblem(Box<RemoteProblem>)`. The candidate implements those exact
shapes at `crates/skald/skald-workflow/src/route.rs:111` and
`crates/skald/skald-providers/src/error.rs:70`; the former also documents why
ordering is inapplicable. The prior FIND-TASK-001-3/4 hypotheses therefore do
not produce maintainer findings in this pass.

## Changed-Surface Coverage

| Surface | Symbols, callers, and declarations inspected | Assessment |
|---|---|---|
| Workflow wire contract | `WorkflowSpec`, step/action/binding/route types, run/status/result/error DTOs, validation, error conversion, reference visitors, schema generator, checked-in schemas | Cohesive contract ownership and domain naming. Revision 10 matches the implemented external-header and boxed-remote-problem shapes. Python result declarations do not preserve the DTO precision (MNT-R2-002). |
| Resolved plan and execution | `ExecutionPlan`, `ResolvedGraph`, `PlannedStep`, `WorkflowExecutor`, `StepTask`, `AttemptOutcome`, `RunLedger`, `OutputValidator`, retry/deadline/settlement paths, `Workflow::run*` callers | Concrete owners hold their shared state; pure validation stays synchronous. No owner/method-shape finding. |
| Routes and provider egress | `WorkflowExecutionDependencies`, `StepRoute`, `ExternalGatewayBindings`, `ExternalGatewayClient`, relocated `EndpointPolicy`, gateway adapter/server/test consumers | The route owner and narrow gateway capability are discoverable, and endpoint policy is reused rather than duplicated. No maintainer finding. |
| Agent loop and session | OpenAI Responses request reconstruction, native messages, session seeding, loop runtime, integration tests | Dialect-specific behavior remains with request/session owners. No material maintainer finding. |
| Observation | Workflow attempt/result/backoff hooks in `Observer`, `CompositeObserver`, `PythonObserver`, Python `Observer`, generated stubs, executor callers | Surface parity exists, but the concrete forwarding/bridge implementations and public Python methods omit required contract documentation (MNT-R2-003). |
| Rust authoring | `Workflow`, `WorkflowBuilder`, `WorkflowInput`, `with_inputs`, `with_step_inputs`, `with_outputs`, validation/run variants, examples and YAML | The existing owner remains the discoverable entry point. The public guide misstates generated step IDs (MNT-R2-005). |
| PyO3 and Python package | `skald-workflow/src/python.rs`, SDK aggregation, `wyrd.agent` exports, `WorkflowRun`, observer surface, duplicate generated declarations, Python tests/examples | New Python-boundary behavior is implemented outside its required SDK owner (MNT-R2-001), and declarations erase exact result shapes (MNT-R2-002). |
| Tests | Contract/runtime/builder scenario tests, `test_support`, Agent Responses tests, observer tests, Python explicit-binding tests, replacement of old external tests | The shared support module removes real duplication. Five function-local import groups violate the repository dependency-layout rule (MNT-R2-004). The prior suggestion to split the broad scenario tests is not retained as a material issue. |
| Consumers and generated/docs surfaces | shared state tests, gateway/server/test fixture consumers, CLI re-exports, Rust/Python examples, docs, schemas, Python stubs, TypeScript error codes | Consumers compile against the relocated/types surfaces in the recorded evidence. Generated artifacts moved with the API; the step-ID guide remains inaccurate (MNT-R2-005). |

## Material Findings

### MNT-R2-001 — New Python Workflow behavior is owned by the migration crate instead of the Python SDK

- Changed location: `crates/skald/skald-workflow/src/python.rs:41-86,350-433,529-657`; aggregation-only consumer at `sdks/wyrd-sdk-python/src/lib.rs:46-49`.
- Governing rule: `AGENTS.md` §§7–8, `architecture/wyrd-doctrine.mdx`, and the maintainer guide require new or materially relocated Python logic, wrappers, boundary conversion, and aggregation to live in `sdks/wyrd-sdk-python`; retained owner-crate wrappers are migration state only.
- Evidence: the candidate adds Workflow input/default/binding conversion, new Python authoring/validation/run methods, the `PyWorkflowRun` wrapper, its JSON projection, and class registration to `skald-workflow`. The SDK still delegates the whole surface through `skald_workflow::python_register`.
- Concrete maintenance cost: the declared Python owner does not contain the new public Python contract, so maintainers must change a Rust runtime crate for Python-only evolution and cannot discover the boundary from the SDK module tree.
- Smallest testable correction: leave untouched legacy wrappers in their current migration location, but move this task's new conversions, Workflow Python methods, `PyWorkflowRun`, and their registration into `sdks/wyrd-sdk-python/src`, wrapping the Rust-native `Workflow`. Reuse the existing SDK module entry point and duplicate no native validation or execution logic. Prove the public imports and Python tests, retained-feature compilation, `check:pyo3-scope`, codegen, and type checking.
- Nearby pattern: SDK-owned PyO3 modules such as `sdks/wyrd-sdk-python/src/state.rs` and registration from `sdks/wyrd-sdk-python/src/lib.rs`.

### MNT-R2-002 — Python declarations erase exact Workflow result DTOs

- Changed location: `sdks/wyrd-sdk-python/python/wyrd/agent/__init__.pyi:401-439` and generated duplicate `python/wyrd/stubs/agent.pyi`; projection source at `crates/skald/skald-workflow/src/python.rs:574-645`.
- Governing principle: the maintainer guide requires public Python types to describe the actual domain result rather than nested `Any` when an exact contract exists. `WorkflowStepResult`, `WorkflowRunError`, and `WorkflowRun` are exact public DTOs.
- Evidence: `steps` is declared as `dict[str, dict[str, Any]]`, `error` as `dict[str, Any] | None`, and `to_dict` as `dict[str, Any]`. Prose lists some keys but the type checker cannot enforce status values, attempts, timestamps, optional payloads, or error fields.
- Concrete maintenance cost: Rust wire-field drift does not cause Python typing failures, and callers must rediscover the portable shape from prose.
- Smallest testable correction: from the SDK-owned annotation/generator source, emit `TypedDict` projections for the step result, run error, and complete run dictionary, then use them in the getters and `to_dict`. Keep genuinely JSON-valued outputs/details broad and add no runtime DTO classes. Regenerate; do not hand-edit generated stubs.
- Nearby pattern: the exact `Literal` return already used by `WorkflowRun.status` in the same declaration.

### MNT-R2-003 — New observer implementations and Python hooks lack required contract documentation

- Changed location: `crates/skald/skald-observer/src/composite.rs:144-174`, `crates/skald/skald-observer/src/python.rs:280-339`, and `sdks/wyrd-sdk-python/python/wyrd/observer.py:160-188`.
- Governing rule: `architecture/agent-rules.md` requires substantive rustdoc on every new or materially modified Rust item, including implementation methods; the maintainer guide requires public Python docstrings to align argument names, meanings, and types with signatures.
- Evidence: the six concrete Rust forwarding/bridge methods have no item documentation. The three public Python hooks have only summary prose and no signature-aligned `Args` contract.
- Concrete maintenance cost: callback fan-out, swallowed callback errors, payload omission, stable error-code meaning, and duration conversion are not documented at the concrete owners maintainers must modify.
- Smallest testable correction: document those existing methods in place, including fan-out/bridge behavior and duration units where applicable, and add matching Python `Args` sections. Add no helper or abstraction.
- Nearby pattern: the documented observer trait methods in `crates/skald/skald-observer/src/observer.rs:107-128` and the fuller Python workflow start/finish hook documentation immediately above the affected methods.

### MNT-R2-004 — Test dependencies are hidden inside five scenario functions

- Changed location: `crates/skald/skald-workflow/src/workflow.rs:811-816,1246-1258,1473-1483,1692-1694` and `crates/skald/skald-workflow/src/workflow_surface.rs:848-850`.
- Governing rule: `architecture/agent-rules.md` requires all imports at the top of their module; a `#[cfg(test)] mod tests` is its own permitted import scope, not permission for function-scoped imports.
- Evidence: `bounded_attempt_lifecycle`, `isolated_route_calls`, `bound_external_gateway_security`, `terminal_budget_reserve`, and `explicit_builder_contract` introduce their dependencies with function-local `use` statements even though each already lives inside an owning test module.
- Concrete maintenance cost: the test module's dependency manifest is incomplete and repeated names can drift between scenario bodies.
- Smallest testable correction: move the existing imports to the two owning test-module import blocks and deduplicate them. Do not split tests or introduce a test harness to close this finding.
- Nearby pattern: the top-of-test-module import blocks at `workflow.rs:501-516` and `workflow_surface.rs:784-800`.

### MNT-R2-005 — The workflow guide misstates generated step IDs

- Changed location: `docs/src/content/docs/how-to/build-a-workflow.svx:19`; implementation at `crates/skald/skald-workflow/src/workflow_surface.rs:611-632`.
- Governing principle: public documentation must match the authoring contract and the implementation that generates caller-visible identifiers.
- Evidence: the guide says step IDs are Agent names. `Workflow::next_step_id` replaces invalid characters with `_`, prefixes leading digits, synthesizes unnamed IDs, and suffixes collisions.
- Concrete maintenance cost: users can author bindings or `after` references with the documented name and receive an unknown-step failure for punctuation, leading digits, duplicates, or unnamed Agents.
- Smallest testable correction: describe IDs as deterministic values derived from Agent names, state the sanitization/prefix/collision behavior, and direct callers to the resulting `Workflow.steps` value when a name is not already a unique identifier. No API or helper change is needed.
- Nearby pattern: the exact rule is already documented on `Workflow::next_step_id` at `workflow_surface.rs:611-615`.

## Verification Evidence and Limits

- The task records green Skald/shared/Wyrd/Python/codegen/boundary/format/lint lanes; this review used those as available evidence rather than rerunning the implementation suite.
- This pass inspected the complete base-to-candidate range, the revision-10-only delta, owning modules, direct callers, declarations, tests, examples, and documentation named above.
- `git diff --check base..candidate` reports a trailing blank line in the prior immutable `TASK-001-r1/findings-validation.md`; it is review-artifact formatting and not a source-maintainability finding for TASK-001.
- The findings concern source ownership, contract discoverability, documentation, and import placement. Existing green behavior tests do not establish those structural rules.

## Calibration Notes

- `WorkflowExecutor`, `RunLedger`, `ExecutionPlan`, and
  `WorkflowExecutionDependencies` are justified concrete owners, not
  speculative service layers.
- The external client reuses the relocated endpoint policy; a second network
  policy abstraction is neither present nor needed.
- Revision 10 makes the implemented unordered header map and boxed remote
  problem the single approved shapes, so no adapter or compatibility layer is
  warranted.
- The long scenario tests cover task-defined scenario groups and reuse existing
  test support. Splitting them may improve failure localization, but that is an
  uncertain preference rather than a task-blocking maintenance defect; only
  the explicit import-placement violation is retained.
- The large Workflow wire-contract module remains cohesive enough that a
  line-count-only split would add navigation rather than remove it.

## Overall Result

**FAIL**

Revision 10 closes the prior collection/error-shape conflict, and the main
Rust runtime ownership remains understandable. MNT-R2-001 through MNT-R2-005
remain material changed-surface issues with bounded corrections; none requires
redesign of the execution engine or a new abstraction.
