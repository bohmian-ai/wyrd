---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-skald-workflow-runtime
spec_revision: 5
requirements: [REQ-001, REQ-002, REQ-003, REQ-014, REQ-015, REQ-024, REQ-025, REQ-028, REQ-034, REQ-035, REQ-036, REQ-036A, REQ-037, REQ-038, REQ-039, REQ-040, REQ-041, REQ-042, REQ-043, REQ-044, INV-003, INV-004, INV-005, INV-009, INV-010, INV-010A, INV-011, INV-012, AC-001, AC-002, AC-003, AC-011, AC-011A, AC-014, AC-015, AC-016, AC-017]
depends_on: [TASK-001, TASK-002]
parent_task:
remediates: []
---

## Outcome and Value

Rust callers can load one Workflow YAML bundle, resolve its Agent/Prompt graph,
register it with exact relationships, fetch and hydrate the locked graph, and
execute it locally through Native, WyrdGateway, or direct ExtGateway routes.
All routes retain the same Skald workflow semantics and require no live
credentials in unit or journey tests.

## Owners, Scope, Consumers, and Prohibited Changes

Skald owns route-neutral workflow execution and provider abstractions;
provider-specific wire behavior remains in provider owners. `wyrd-loader` owns
`path`/`inline`/`ref` bundle resolution. `wyrd-client::Cards` remains the shared
registry/hydration handle. Registration remains server-owned and declarative.

Do not add a route registry, gateway Card, invocation-time route override,
provider branches to the DAG executor, secret-bearing Card fields, automatic
registration during local execution, server lifecycle state, or Python,
TypeScript, MCP, and UI surfaces. ExtGateway must never recurse through the Wyrd
gateway.

## Approach

1. Complete Workflow bundle loading and resolved-graph validation using existing
   `path`, `inline`, and `ref` semantics.
2. Preserve exact Workflow/Agent/Prompt identities and derived relationships
   through registration, fetch, and hydration.
3. Select route-specific provider implementations through the existing provider
   registry/Agent override seam while leaving the DAG executor route-neutral.
4. Reuse the public Wyrd gateway client for local WyrdGateway calls and perform
   ExtGateway calls directly from the local runtime.
5. Apply protocol compatibility, credential binding, URL/header, TLS, redirect,
   response-size, deadline, and local-profile network controls before dispatch.

## Ordered Implementation Scenarios

### Scenario 1 — One YAML bundle loads, validates, registers, and hydrates

**Behavior.** A code-review bundle with path, inline, and ref dependencies loads
into one Workflow, rejects unresolved refs without a registry client, derives
exact Agent/Prompt relationships on registration, and rehydrates the same locked
versions after fetch. This proves REQ-001 through REQ-003, REQ-014, REQ-024,
REQ-025, REQ-028, INV-003 through INV-005, AC-001 through AC-003.

**RED.** Add `workflow_yaml_bundle_hydrates_explicit_bindings` to the existing
`workflow_surface` target and run:

```bash
mise exec -- cargo nextest run --locked -p skald-workflow \
  --test workflow_surface \
  -E 'test(=workflow_yaml_bundle_hydrates_explicit_bindings)'
```

It must fail because the current lowering/hydration path discards required
Workflow semantics or cannot resolve the complete graph.

**GREEN.** Complete loading and hydration through existing loader and Card
handles without adding another bundle format or registry implementation.

**REFACTOR.** Consolidate duplicate Card-to-runtime conversion only where an
existing owner naturally holds it.

### Scenario 2 — Route precedence selects existing provider boundaries

**Behavior.** Step route overrides workflow route, workflow route overrides
Native, and registered execution cannot override the stored result. Native uses
the Prompt provider, local WyrdGateway uses the public server ingress, and
ExtGateway directly reaches its bound endpoint. Fallback is forwarded only for
WyrdGateway. This proves REQ-035 through REQ-041, INV-009, INV-010A, INV-011,
AC-011, AC-011A, AC-014, and AC-017.

**RED.** Add a `workflow_routing` integration target with
`local_route_precedence_reaches_each_declared_boundary` and run:

```bash
mise exec -- cargo nextest run --locked -p skald-workflow \
  --test workflow_routing \
  -E 'test(=local_route_precedence_reaches_each_declared_boundary)'
```

It must fail because Workflow routes are not yet projected into runtime provider
selection.

**GREEN.** Adapt each route to the existing provider registry/client seam and
prove the target boundary with deterministic local upstreams.

**REFACTOR.** Keep route resolution in one typed boundary and remove any
provider-string branching exposed by the passing scenario.

### Scenario 3 — Protocol compatibility is typed and translation stays gateway-owned

**Behavior.** OpenAI Chat, OpenAI Responses, Anthropic Messages, Gemini, and
Vertex retain their Prompt request shapes; ExtGateway rejects dialect mismatch;
WyrdGateway permits only its implemented capability matrix, including local
Vertex refusal. This proves REQ-038 through REQ-040, REQ-043, REQ-044, INV-011,
and AC-015.

**RED.** Add `route_protocol_matrix_rejects_unsupported_combinations_before_io`
to `workflow_routing` and run:

```bash
mise exec -- cargo nextest run --locked -p skald-workflow \
  --test workflow_routing \
  -E 'test(=route_protocol_matrix_rejects_unsupported_combinations_before_io)'
```

It must fail until route/request compatibility and gateway capability checks are
connected to Workflow execution.

**GREEN.** Reuse provider request enums and the gateway's typed capability
contract; do not translate inside Workflow code.

**REFACTOR.** Delete redundant dialect checks if the existing provider or
gateway owner already supplies the authoritative typed check.

### Scenario 4 — External gateway egress fails closed without leaking secrets

**Behavior.** Local ExtGateway requires the named binding and exact origin,
rejects secret/reserved headers, unsafe redirects, oversized/timed-out responses,
and unapproved endpoint changes, while permitting explicitly configured private
local fixtures. Errors, logs, results, and observations remain redacted. This
proves REQ-034, REQ-038, REQ-042, INV-010, INV-012, AC-016, and AC-017.

**RED.** Add `external_gateway_binding_and_network_policy_fail_closed` to
`workflow_routing` and run:

```bash
mise exec -- cargo nextest run --locked -p skald-workflow \
  --test workflow_routing \
  -E 'test(=external_gateway_binding_and_network_policy_fail_closed)'
```

It must fail because no Workflow-owned external route currently applies the
approved binding and bounded-IO contract.

**GREEN.** Reuse existing TLS, URL, secret, and provider HTTP machinery in the
narrowest owner and expose only redacted stable errors.

**REFACTOR.** Share existing network-policy primitives; do not create a second
SSRF or credential framework.

## Acceptance Criteria

- One canonical bundle supports unregistered and registered local Rust
  execution without semantic drift.
- Registration stores exact Workflow, Agent, Prompt, route, and fallback
  relationships without executing anything.
- Route selection uses the provider abstraction and the exact approved
  precedence.
- ExtGateway is direct from the executing process and WyrdGateway alone enters
  Wyrd gateway governance.
- All route/protocol/security journeys use deterministic local upstreams and no
  real credentials.

## Expected Write Set and Consumer Closure

Likely surfaces include `crates/skald/skald-workflow`, narrow provider/runtime
owners, `crates/shared/wyrd-loader`, `crates/shared/wyrd-client/src/cards`, Card
relationship extraction/registration consumers, and credential-free test
fixtures. Existing installed dependencies and network controls must be reused.

## Verification and Evidence

Run each focused scenario sequentially, then:

```bash
mise run test:skald
mise run test:shared
mise run test:cards:integration
mise run test:gateway:native
mise run check:client-tier
mise run check:pyo3-scope
mise run codegen:check
mise run fmt
mise run lints
git diff --check
```

## Material Stop Conditions

- Route portability requires a new gateway protocol, route variant, credential
  shape, third-party dependency, or Cargo feature.
- Registered local execution cannot preserve exact Card versions through the
  existing loader/client boundaries.
- ExtGateway cannot satisfy DNS/redirect/TLS/secret controls through existing
  approved network owners.
- A Python, TypeScript, MCP, UI, durable-job, or server-side tool surface becomes
  necessary.

## Authority Links

- `changes/active/skald-workflow-runtime/spec.md` Revision 5
- `changes/active/wyrd-gateway-v1/spec.md` Revision 21
- `AGENTS.md` §§3, 6, 9–12
- `architecture/wyrd-security-posture.md` §§Source credentials and SSRF defense,
  Cryptography and secret handling
- `architecture/references/architecture/patterns.md` §§Provider Runtime Pattern,
  External Network Pattern
- `architecture/references/languages/testing-workflows.md`
