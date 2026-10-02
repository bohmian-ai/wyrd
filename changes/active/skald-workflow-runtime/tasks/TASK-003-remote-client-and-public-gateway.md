---
id: TASK-003
kind: implementation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 10
requirements: [REQ-024, REQ-031, REQ-036A, REQ-038, REQ-039, REQ-043, REQ-045, REQ-046, INV-004, INV-007, INV-011, INV-012, INV-020, AC-011A, AC-012, AC-013, AC-019]
depends_on: [TASK-001]
---

# Shared remote client and public governed calls

Implementation skill: `$wyrd-implement`.

## Outcome and Value

Rust/CLI can create/get/cancel/wait through one shared handle and locally
executed WyrdGateway steps carry their own immutable fallback/context through
existing authenticated model ingress. No recursive server call or competing
transport is introduced. The client builds before the new Workflow routes
exist; real Workflow lifecycle proof is TASK-004/005.

## Owners, Scope, Consumers, and Prohibited Changes

`wyrd-client` owns `Workflows` and `PublicWyrdGatewayCaller` with exact spec
methods; `wyrd-server` owns authenticated gateway header validation at existing
OpenAI/Anthropic/Gemini ingress. The first-class Rust package projects the
shared exports without enabling Python features. Add no new model ingress,
Vertex public endpoint, credential mutation, mutable per-adapter context,
provider-body Workflow fields, polling settings or general public header API.

## Approach

1. Implement the exact shared Workflows facade over current idempotent transport.
2. Implement the public gateway caller using shared auth/pool/tracing with
   per-call headers, bounded remaining timeout and cancellable IO.
3. Decode/validate the exact fallback header after ordinary authentication on
   all affected public ingresses; forward only the typed fallback into the
   current governed request, never the header to providers.
4. Normalize native error envelopes to RemoteProblem, document served operations
   and prove transport/header concurrency, cancellation and backward behavior.

### Existing seams and required ordering

```rust
pub struct Workflows { /* owned shared WyrdClient */ }
impl Workflows {
    pub fn new(client: WyrdClient) -> Self;
    pub async fn create(&self, request: &CreateWorkflowRunRequest) -> Result<WorkflowRun, WyrdError>;
    pub async fn get(&self, run_id: &WorkflowRunId) -> Result<WorkflowRun, WyrdError>;
    pub async fn cancel(&self, run_id: &WorkflowRunId) -> Result<WorkflowRun, WyrdError>;
    pub async fn wait(&self, run_id: &WorkflowRunId) -> Result<WorkflowRun, WyrdError>;
}
pub struct PublicWyrdGatewayCaller { /* owned shared WyrdClient */ }
impl PublicWyrdGatewayCaller { pub fn new(client: WyrdClient) -> Self; }
// Implements the packet's WyrdGatewayCaller::call immutable call contract.
```

Create serializes `{workflow: CardRef, input: object, timeout_seconds: optional}`
to `POST /v1/workflow-runs`; first/replay responses are direct WorkflowRun
202/200. GET and cancel use `/v1/workflow-runs/{run_id}` and
`/v1/workflow-runs/{run_id}/cancel`, direct 200 snapshots. Exact pure Run DTOs
are printed in TASK-001's packet-local seam; use those native types unchanged.
Wait polls once per second; cancelled/failed/timed_out are returned terminal
values, not converted into HTTP failures. Dropping wait drops polling only.

`WyrdClient::submit_idempotent` in `src/client.rs` is the create precedent:
mint the key once before retry. `HttpTransport::request_json_with_headers` in
`src/transport/http.rs` is the crate-private request-scoped-header precedent;
adapt inside that owner for native-envelope errors and remaining timeout rather
than creating a new reqwest client or publicly exposing arbitrary headers.
Existing auth refresh/retry stays request-local; accepted-job authority is a
server host rule, not an auth exemption for this local adapter.

Consume `WyrdGatewayCall` exactly as TASK-001/spec define. Public ingress uses
`GatewayCallRequest` in `components/gateway/invocation.rs`; preserve existing
operation/dialect conversion and separate cancellation, set only fallback and
remaining timeout. Authentication precedes header interpretation; bad header
refuses before gateway dispatch; no override retains tenant fallback policy.
Header: `wyrd-gateway-fallback`, unpadded base64url over JCS,
8-KiB encoded/4-KiB decoded; reject malformed/empty/duplicate/self-reference
with `WYRD_GATEWAY_400_INVALID_REQUEST`, `details.field = "fallback"`.
Remaining timeout and correlation never enter the native provider request.

Packet-local native-error contract: parse the existing OpenAI, Anthropic, or
Google protocol error envelope. Keep HTTP status, a redacted safe message, and
its stable Wyrd code when present; OpenAI's safe `param` becomes optional
`field`. No other native-envelope member becomes portable error details.
The in-process WyrdError projection retains only `details.field` and the same
safe status/code/message/remediation fields. If no Wyrd code is present, map
status through the existing `ProviderError::Status::code()` behavior in
`skald-providers/src/error.rs`: 401/403 → `SKALD_PROVIDERS_401_AUTH`,
408 → `SKALD_PROVIDERS_408_TIMEOUT`, 429 →
`SKALD_PROVIDERS_429_RATE_LIMIT`, 500–599 →
`SKALD_PROVIDERS_5XX_UPSTREAM`, and other refusal statuses →
`SKALD_PROVIDERS_400_BAD_REQUEST`. A client transport timeout instead uses the
existing `ProviderError::Timeout` category; an uncoded HTTP 504 remains in
the existing 5xx status category. Reuse category remediation, not a second
HTTP error catalog. Known Wyrd codes get their
remediation from the derive-backed catalog; unknown codes use the existing
provider-category remediation. The per-step adapter passes RemoteProblem
unchanged. WorkflowRunError.details is exactly `{"field": value}` when present
or `{}` otherwise; no arbitrary body, upstream text, prompts, credentials,
protocol-only members, or diagnostic details survive projection. Task-001's
bounded snapshot diagnostic projection still applies after normalization.

## Ordered Implementation Scenarios

Selectors are **planned**, default features. Every GREEN reruns prior tests;
every REFACTOR keeps them green. Mock transport proof supports, not replaces,
the real accepted-run journey in TASK-004.

### Scenario 1 — One stable remote run contract

**Behavior.** REQ-031/045–046: exact create/get/cancel URL/body/status projection;
create retries retain one key; wait polls every second, terminates on every
terminal status, propagates canonical errors and dropping it sends no cancel
or resubmission. Rust SDK export is the same handle.

**RED.** Add `shared_workflow_client_contract` to new
`crates/shared/wyrd-client/tests/workflow_transport.rs`;
`mise exec -- cargo nextest run --locked -p wyrd-client --test workflow_transport -E 'test(=shared_workflow_client_contract)'`.
Expect absent facade/transport projection. Table-driven wire cases include
both first acceptance and replay without route-specific wrappers.

**GREEN.** Exact methods on one shared handle; existing transport owns auth,
idempotency and retry. No local execution in the remote method.

**REFACTOR.** Reuse transport primitives and existing error mapping; no second
client implementation in Rust SDK or CLI.

### Scenario 2 — Concurrent public calls retain isolated context

**Behavior.** REQ-036A/038–039/043: concurrent calls with different fallback and
deadlines remain isolated; cancellation drops caller IO; correlation is trace
only. OpenAI Chat/Responses, Anthropic and Gemini use their existing ingress;
Vertex refuses locally before upstream dispatch. All three native error
envelopes and uncoded categories project only safe common fields.

**RED.** Add `public_gateway_call_context_and_errors` to `workflow_transport`;
`mise exec -- cargo nextest run --locked -p wyrd-client --test workflow_transport -E 'test(=public_gateway_call_context_and_errors)'`.
Expect missing adapter/header or native errors incorrectly treated as canonical
problem JSON. Assert stable code/status/message/field/remediation only, no raw
upstream body or arbitrary details.

**GREEN.** Immutable per-call projection through shared authenticated transport,
with spec timeout/cancellation behavior and normalized RemoteProblem.

**REFACTOR.** Reuse existing typed provider codecs and category metadata;
never store call fields in shared registry/header state.

### Scenario 3 — Authenticated ingresses consume the fallback header

**Behavior.** AC-011A: all three public protocols validate exact encoding/
limits/model candidates, set `GatewayCallRequest.fallback`, and never forward
the header. Malformed/oversized/empty/duplicate/self-refused values dispatch
nothing; omitted header preserves unmodified client behavior.

**RED.** Add `components::gateway::pg_invocation_tests::public_ingress_workflow_fallback`
to the existing server inline PG owner;
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib -E "test(=components::gateway::pg_invocation_tests::public_ingress_workflow_fallback)"'`.
Drive real authenticated ingress against local deterministic upstreams;
expect today’s `fallback: None` lowering to ignore the header.

**GREEN.** Extend only affected ingress assembly/route docs and reuse current
gateway validation/authorization/candidate selection.

**REFACTOR.** One narrow header decoder at the owning ingress boundary;
do not create a gateway feature route or bypass authorization for Workflow callers.

## Acceptance Criteria

Exact shared APIs and Rust exports compile Python-free; per-call metadata
isolation and native error shape are proved; every affected ingress consumes
the header safely; ordinary clients behave identically. Served OpenAPI records
encoding/limits and existing gateway error codes. TASK-004 owns in-process
projection and TASK-005 owns the real local Workflow→public gateway journey.

## Expected Write Set and Consumer Closure

`crates/shared/wyrd-client/src` facade/public caller/transport-private extension
and exports; `sdks/wyrd-sdk-rust` projection proof;
`crates/wyrd/wyrd-server/src/components/gateway/{ingress,routes,...}`;
owning tests above and served OpenAPI tests/docs. No provider credentials in
client adapter, no TS/Python remote invocation surface. REQ-049 authorizes
the existing-workspace client→Skald edge; no reverse edge or python activation.

## Verification and Evidence

Focused commands above; `mise run test:shared`, `mise run test:wyrd-sdk`,
`mise run test:gateway:native`, `mise run test:principals:integration`,
`mise run codegen:check`, `mise run check:client-tier`,
`mise run check:sdk-client-tier`, `mise run check:pyo3-scope`,
`mise run fmt`, `mise run lints`, `git diff --check`.
Served documentation/export changes have static/regression proof, no invented
RED for generated text. `codegen:check` does not prove OpenAPI; inspect the
served document with the composed-server lane. Local deterministic upstreams
require no live model credentials. Record selected counts for planned tests.

## Material Stop Conditions

Stop for new ingress/protocol/credential APIs, provider-body context changes,
public arbitrary-header transport surface, weakened idempotency or native
error normalization, Python-enabled Rust/TS dependency cone or polling knobs.

## Authority Links

- [Approved Revision 10](../spec.md); [TASK-001](TASK-001-explicit-local-runtime.md)
- `AGENTS.md`; `architecture/agent-rules.md`
- `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`
- `architecture/references/languages/{agent-harness,errors,spec-driven-development,implementation-execution,testing-workflows}.md`
