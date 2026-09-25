---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-skald-workflow-runtime
spec_revision: 5
requirements: [REQ-001, REQ-002, REQ-004, REQ-005, REQ-007, REQ-008, REQ-009, REQ-012, REQ-013, REQ-020, REQ-021, REQ-023, REQ-030, REQ-032, REQ-036, REQ-036A, REQ-037, REQ-040, REQ-042, INV-002, INV-003, INV-007, INV-009, INV-011, INV-012, INV-014, INV-015, AC-006, AC-011, AC-011A, AC-012]
depends_on: []
parent_task:
remediates: []
---

## Outcome and Value

Wyrd has one pure, schema-backed Workflow contract for exact bindings, Agent-only
steps, routes, run snapshots, public errors, and `workflows:run` authorization.
The repository architecture authority records Revision 5's approved narrow
exceptions for capability-scoped surfaces and ephemeral server run state.

## Owners, Scope, Consumers, and Prohibited Changes

`wyrd-spec` owns the IO-free Workflow and WorkflowRun wire contracts, structural
validation, schema derivation, and public errors. `wyrd-runtime` owns the typed
permission and built-in role grants. The Card registry, Skald runtime,
`wyrd-client`, server, CLI, and generators consume these contracts.

Do not add async, IO, PyO3, provider clients, lifecycle state, a second template
syntax, compatibility aliases, migrations, a persistent run model, Python,
TypeScript, or MCP Workflow invocation APIs. Generated files must come only from
their generators.

## Approach

1. Replace the ambiguous step-input/output shapes with the approved exact
   `WorkflowBinding` wire contract and remove unimplemented action/condition
   contracts.
2. Complete pure graph, identifier, input, output, route, fallback, URL/header,
   and secret-indirection validation without resolving other Cards.
3. Define the portable WorkflowRun, step-result, request/response, status, and
   error contracts required by local and HTTP consumers.
4. Add `workflows:run` to the typed permission model and approved built-in roles.
5. Synchronize `AGENTS.md`, Wyrd design, and doctrine with the approved authority
   changes, then regenerate contract artifacts from source.

## Ordered Implementation Scenarios

### Scenario 1 — Exact bindings and Agent-only graphs are structural contracts

**Behavior.** Workflow YAML round-trips exact source-path bindings, Agent-only
actions, explicit outputs, and deterministic route defaults. Invalid identifiers,
cycles, hidden step references, undeclared inputs, expression-like bindings,
missing outputs, removed action kinds, and removed conditions fail with stable
field-specific Wyrd errors. This proves REQ-001, REQ-002, REQ-004, REQ-005,
REQ-007 through REQ-009, REQ-012, REQ-013, INV-002, INV-003, and AC-006.

**RED.** Add
`card::workflow::tests::workflow_binding_contract_round_trips_and_rejects_invalid_sources`
and run:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib \
  -E 'test(=card::workflow::tests::workflow_binding_contract_round_trips_and_rejects_invalid_sources)'
```

It must fail because step inputs and outputs still use generic values and the
removed action/condition shapes are still accepted.

**GREEN.** Implement the minimum pure contracts and validation needed for the
new test, preserving Card reference and cascade behavior.

**REFACTOR.** Consolidate only genuinely shared parsing or validation logic into
small pure domain methods while the focused test remains green.

### Scenario 2 — Route, fallback, and secret declarations are closed and safe

**Behavior.** Workflow and step routes serialize with the approved precedence;
fallback is accepted only for `WyrdGateway`; external routes reject malformed
URLs, reserved/secret headers, and secret values while retaining only the named
credential binding. This proves REQ-036, REQ-036A, REQ-037, REQ-040, REQ-042,
INV-009, INV-011, INV-012, AC-011, and AC-011A.

**RED.** Add
`card::workflow::tests::workflow_route_contract_enforces_fallback_and_secret_rules`
and run:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib \
  -E 'test(=card::workflow::tests::workflow_route_contract_enforces_fallback_and_secret_rules)'
```

It must fail because the Workflow contract does not yet expose the closed route
and fallback model.

**GREEN.** Add only the approved route shapes and pure validation required to
pass the scenario.

**REFACTOR.** Reuse existing URL, identifier, non-secret, and error-catalog
types wherever they already express the invariant.

### Scenario 3 — Run snapshots are one portable contract

**Behavior.** WorkflowRun and step-result envelopes round-trip every approved
status, timestamps, registered identity, deterministic maps, outputs, attempts,
and stable errors. Invalid or internally inconsistent terminal snapshots are
rejected. This proves REQ-020, REQ-021, REQ-023, REQ-030, INV-007, and AC-012.

**RED.** Add
`card::workflow::tests::workflow_run_contract_round_trips_terminal_states` and
run:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib \
  -E 'test(=card::workflow::tests::workflow_run_contract_round_trips_terminal_states)'
```

It must fail because the current Skald-only result is not the approved public
wire envelope.

**GREEN.** Add the smallest pure run and request/response contract needed by
local, client, server, schema, and CLI consumers.

**REFACTOR.** Keep runtime-only provider responses and execution state outside
the portable contract; retain only normalized public values.

### Scenario 4 — Workflow authorization is a typed permission

**Behavior.** `workflows:run` parses, serializes, and participates in coverage
like existing permissions; admin covers it by wildcard, writer and agent receive
it explicitly, and reader/runtime_admin do not. This proves REQ-032.

**RED.** Add `permission::tests::workflow_run_permission_round_trips` and run:

```bash
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib \
  -E 'test(=permission::tests::workflow_run_permission_round_trips)'
```

It must fail because `Resource::Workflows` and its constructor/grants do not
exist.

**GREEN.** Extend the existing typed permission and built-in role sources only.

**REFACTOR.** Preserve the existing permission construction and coverage
patterns without adding a workflow-specific authorization subsystem.

## Acceptance Criteria

- Canonical YAML and JSON schemas expose only exact bindings, Agent actions,
  explicit outputs, and the approved route model.
- Pure validation never performs IO or requires a resolved Agent/Prompt.
- WorkflowRun and workflow-run HTTP bodies are typed and schema-generating.
- All public failures flow through the derive-backed Wyrd error catalog.
- `workflows:run` and built-in grants match Revision 5 exactly.
- Architecture authority distinguishes ephemeral server lifecycle state from a
  persisted run registry and permits this YAML/Rust/HTTP/CLI-only rollout.

## Expected Write Set and Consumer Closure

Likely owners and consumers include `crates/wyrd-spec/src/card/workflow.rs`,
`crates/wyrd-spec/src/error.rs`, exports/schema generation, `crates/shared/wyrd-runtime/src/permission.rs`,
built-in roles, `AGENTS.md`, `architecture/wyrd-design.md`,
`architecture/wyrd-doctrine.mdx`, and generator-produced schemas/OpenAPI/error
metadata. Paths are guidance, not an implementation allowlist.

## Verification and Evidence

Run the four focused tests above sequentially, then:

```bash
mise run test:wyrd
mise run test:shared
mise run codegen:check
mise run check:client-tier
mise run check:pyo3-scope
mise run fmt
mise run lints
git diff --check
```

TDD applies to the new contract validation and permission behavior. Authority
text and generated outputs use static review and generator-drift proof; no
manufactured RED is required for those portions.

## Material Stop Conditions

- A requirement needs IO, async, PyO3, or server ownership inside `wyrd-spec`.
- The approved wire shape, permission grants, route variants, or compatibility
  policy must change.
- Authority synchronization would require weakening tenant, audit, secret, or
  first-class-language guarantees beyond Revision 5's narrow exception.
- A new dependency, Cargo feature, migration, or persisted run record appears
  necessary.

## Authority Links

- `changes/active/skald-workflow-runtime/spec.md` Revision 5
- `AGENTS.md` §§2–9, 11, 14
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md` §§Client model, Workflow, Run identity
- `architecture/wyrd-doctrine.mdx` §§Runtime boundary, Public surfaces
- `architecture/references/architecture/patterns.md`
- `architecture/references/languages/errors.md`
- `architecture/references/languages/testing-workflows.md`
