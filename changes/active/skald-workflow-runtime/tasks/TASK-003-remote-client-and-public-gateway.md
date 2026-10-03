---
id: TASK-003
kind: implementation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 12
requirements: [REQ-058, AC-029, AC-031, REQ-024, REQ-031, REQ-036A, REQ-038, REQ-039, REQ-043, REQ-045, REQ-046, INV-004, INV-007, INV-011, INV-012, INV-020, AC-011A, AC-012, AC-013, AC-019]
depends_on: [TASK-001, TASK-002-cleanup]
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

## Source-backed reuse map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Remote lifecycle | `WyrdClient::submit_idempotent`, shared HTTP transport | Cards registration transport; existing client integration layout | Workflow-run HTTP capability | Narrow Workflows facade with native DTOs | New capability has no existing lifecycle facade; no new transport |
| Public gateway | `HttpTransport::request_json_with_headers`, `GatewayCallRequest`, existing provider codecs | gateway invocation/ingresses and PG tests | Immutable per-step fallback/deadline projection | Existing auth/transport/codec owners plus WyrdGatewayCaller adapter | Adapter earns the Skald/shared-client boundary; no new reqwest client or public header API |
| Local execution setup | `wyrd-client/src/global_config.rs::GlobalConfig`, `skald-workflow::WorkflowExecutionDependencies`, `ExternalGatewayBindings` | existing client config, SDK facade from cleanup, CLI config consumers | One local selected-dependency path for all SDKs/CLI | Shared facade composes current config/dependencies/secret resolvers | Narrow LocalWorkflowConfig field is needed for approved bindings; no per-language configuration owner |
| Run projections | existing `ProviderError::Status`, RemoteProblem and WyrdError metadata | runtime per-step adapter and native gateway error envelopes | Safe protocol error projection | Existing error owners | No second error catalog |

## Approach

1. Compose shared local Workflow execution dependencies from current configuration,
   then implement the exact shared Workflows facade over current idempotent transport.
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

### Shared local execution configuration

This task, not TASK-005, owns `wyrd_client::GlobalConfig.workflow` and its
SDK/CLI consumer closure. Preserve serde defaults and deny-unknown behavior:

```rust
pub struct LocalWorkflowConfig {
    pub external_gateway_bindings:
        BTreeMap<CredentialBindingName, ExternalGatewayBindingConfig>,
}
// ExternalGatewayBindingConfig is the existing pure native DTO:
// { protocol, origin: Url, secret_headers: BTreeMap<String, SecretRef> }
```

Default bindings are empty. All three SDKs and CLI use the shared Workflow
facade's execution composition; they do not each parse bindings or resolve
secrets. At run time, select stored routes/bindings first, then resolve only
selected SecretRefs with current secret helpers and construct existing
WorkflowExecutionDependencies. Native retains the process provider registry;
explicit native runtime injection remains supported. Public WyrdGateway uses
existing Wyrd authentication, not provider credential administration.

A Cards-loaded Workflow retains the existing Cards connection context for
public WyrdGateway calls; a file-loaded Workflow lazily uses ambient client
configuration when external refs or a selected WyrdGateway call require it.
No bearer is copied into the Workflow. Purely Native runs need no Wyrd client.
ExtGateway has the existing explicitly configured local private-address policy,
origin/protocol/header/TLS/redirect/bounded IO checks. Config contains secret
refs only. Loading and apply read no execution secrets and dispatch nothing.
Per-call metadata stays immutable and out of shared provider/header state.
Python uses the existing shared runtime bridge; TypeScript native async methods
and Rust await the same engine. No Python/TS server-run lifecycle methods ship.

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

### Scenario 4 — SDKs and CLI share selected local execution dependencies

**Behavior.** REQ-058, AC-029/031: Native, ExtGateway and public WyrdGateway
execution uses shared config; only selected secrets resolve at run time; file
loading/apply do neither. Cards connection overrides remain in effect for
registered-local public gateway calls. Caller tool registries stay unchanged.

**RED.** Planned lib test `workflow::tests::selected_local_dependencies_use_shared_config`:
`mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=workflow::tests::selected_local_dependencies_use_shared_config)'`.
Use existing deterministic transport/upstreams and controlled secret resolver;
assert no calls during loading, no lookup for unused bindings, same declared
route/fallback/deadline projection, correct configured client, and pre-dispatch
refusal for absent/wrong binding. Expect missing common configuration path.
Native language lifetime projection is tested through the cleanup SDK journeys;
TASK-005 adds real gateway/ExtGateway SDK route journeys after this task.

**GREEN.** Existing GlobalConfig and dependency owners compose one native path;
SDK methods and CLI consume it, while explicit native injection still works.

**REFACTOR.** Remove CLI/private per-language duplicate configuration assembly;
no global per-call state, runtime feature, or new credential API.

## Acceptance Criteria

Shared local execution/configuration serves all three SDKs and CLI with
selected-secret-only preparation and existing client context. Exact shared
remote APIs and Rust exports compile Python-free; per-call metadata
isolation and native error shape are proved; every affected ingress consumes
the header safely; ordinary clients behave identically. Served OpenAPI records
encoding/limits and existing gateway error codes. TASK-004 owns in-process
projection and TASK-005 owns the real local Workflow→public gateway journey.

## Expected Write Set and Consumer Closure

`crates/shared/wyrd-client/src` facade/public caller/transport-private extension
and exports/GlobalConfig/local dependency composition; Rust/Python/TypeScript
local projections and declaration/consumer closure; Rust SDK projection proof;
`crates/wyrd/wyrd-server/src/components/gateway/{ingress,routes,...}`;
owning tests above and served OpenAPI tests/docs. No provider credentials in
client adapter, no TS/Python server-run lifecycle surface; their local execution projections
are required under Revision 12. REQ-049 authorizes
the existing-workspace client→Skald edge; no reverse edge or python activation.

## Verification and Evidence

Focused commands above; `mise run test:shared`, `mise run test:wyrd-sdk`,
`mise run test:gateway:native`, `mise run test:principals:integration`,
`mise run codegen:check`, `mise run check:client-tier`,
`mise run check:sdk-client-tier`, `mise run check:pyo3-scope`,
`mise run py:test:unit`, `mise run py:typecheck`, `mise run py:format`,
`mise run py:lints`, `mise run ts:test:unit`, `mise run ts:typecheck`,
`mise run ts:napi:check`, `mise run fmt`, `mise run lints`, `git diff --check`.
Served documentation/export changes have static/regression proof, no invented
RED for generated text. `codegen:check` does not prove OpenAPI; inspect the
served document with the composed-server lane. Local deterministic upstreams
require no live model credentials. Record selected counts for planned tests.

## Material Stop Conditions

Stop for new ingress/protocol/credential APIs, provider-body context changes,
public arbitrary-header transport surface, weakened idempotency or native
error normalization, Python-enabled Rust/TS dependency cone or polling knobs.

## Authority Links

- [Approved Revision 12](../spec.md); [TASK-001](TASK-001-explicit-local-runtime.md)
- `AGENTS.md`; `architecture/agent-rules.md`
- `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`
- `architecture/references/languages/{agent-harness,errors,spec-driven-development,implementation-execution,testing-workflows}.md`

## Implementation Evidence

Commits: `af10a6090` (Workflows), `415dc2904` (fallback header ingress),
`80abe8483` (shared local Workflow config/dependencies), `f075a5999`
(`WyrdGatewayCall.model`), `1eb76d81e` (public caller, local WyrdGateway
routing, shared secret reader), plus the evidence commit (served OpenAPI
header proof).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Exact shared remote create/get/cancel/wait, one idempotency key across retries, 1 s wait, terminal statuses returned | `crates/shared/wyrd-client/src/workflow/remote.rs` `Workflows` over `submit_idempotent`/shared transport | `wyrd-client::workflow_transport shared_workflow_client_contract` PASS | PASS |
| Rust SDK projects the same handles Python-free | `sdks/wyrd-sdk-rust/src/lib.rs` re-exports `Workflows`, `PublicWyrdGatewayCaller` | `mise run test:wyrd-sdk` (1/1); `check:sdk-client-tier`, `check:pyo3-scope` PASS | PASS |
| Concurrent public calls keep isolated fallback/deadline; cancellation drops IO; correlation trace-only | `crates/shared/wyrd-client/src/workflow/gateway.rs` `PublicWyrdGatewayCaller::call` (per-call header, `tokio::select!` on cancel/timeout, `workflow.gateway.call` span) | `public_gateway_call_context_and_errors` PASS | PASS |
| OpenAI Chat/Responses, Anthropic, Gemini use existing ingress; Vertex refused before IO; model from `WyrdGatewayCall.model` only | `NativeCall::project` in `workflow/gateway.rs`; `HttpTransport::post_native` (crate-private) | same test: four dialect projections, Vertex refusal, no request on cancelled run | PASS |
| Native error envelopes and uncoded statuses project only safe status/code/message/field/remediation | `Ingress::problem`, `wyrd_problem`, `provider_remediation` in `workflow/gateway.rs` (typed `OpenAiErrorEnvelope`/`AnthropicErrorEnvelope`/`GoogleErrorEnvelope`, `WyrdError::codes()`, `ProviderError::from_status`) | same test: three envelopes, uncoded 429/503/408/400, no `sk-canary` leak | PASS |
| All affected public ingresses consume `wyrd-gateway-fallback` after auth; bad values dispatch nothing; absent header unchanged; header never forwarded | `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs` `requested_fallback`; `routes.rs` chat/responses; `wyrd-spec/src/gateway/policy.rs` decoder | `components::gateway::pg_invocation_tests::public_ingress_workflow_fallback` PASS; whole module 23/23 PASS | PASS |
| Served OpenAPI records header encoding/limits/error code | `FALLBACK_HEADER_DOC` on four `utoipa::path` params | `pg_openapi_contract gateway_inference_ingress_publishes_the_fallback_header` PASS within `mise run test:principals:integration` (20/20) | PASS |
| SDKs/CLI share selected local execution dependencies; only selected secrets resolve at run; load/apply read none; Cards context retained for WyrdGateway | `crates/shared/wyrd-client/src/workflow/local.rs` `SelectedRoutes`; `global_config.rs` `LocalWorkflowConfig`; `Workflow { client }` set by Cards loads, else `WyrdClient::from_global()` | `workflow::tests::selected_local_dependencies_use_shared_config` PASS (Native, ExtGateway, wiremock WyrdGateway with bearer and `openai/gpt-5-5`) | PASS |
| Python and TypeScript use the shared run path; no server-run lifecycle surface | Python `run` calls `wyrd_client::Workflow::run`; TS native already calls the shared `run` | `py:test:unit` 526 passed; `py:typecheck`; `ts:test:unit` 37 passed; `ts:typecheck`; `ts:napi:check` PASS | PASS |
| `WyrdGatewayCall.model` carried from the Prompt; refused pre-dispatch when not a gateway model | `crates/skald/skald-workflow/src/route.rs` `gateway_model`, `resolve_route`; `plan.rs` | `nextest -p skald-workflow --lib` 12/12 PASS (`isolated_route_calls` asserts `openai/gpt-test`) | PASS |
| One SecretRef reader shared by gateway, server, and client | `crates/shared/wyrd-utils/src/secret.rs` `read_secret_ref`; callers `wyrd-gateway/src/credential.rs::read_binding`, `wyrd-server/src/config.rs` managed keys, `wyrd-client/src/workflow/local.rs` | `nextest -p wyrd-gateway --lib` 58/58; `nextest -p wyrd-server --lib -E 'test(/^config::/)'` 76/76; `test:shared` 708/708 | PASS |

### Approvals and owner decisions

- **`WyrdGatewayCall.model: ModelRef`**: the human approved this on
  2026-10-03 as a minimal spec amendment to Revision 12. It is filled from
  `AttemptRouteContext` at plan time, and both gateway callers take the model
  only from this field. The `spec.md` `WyrdGatewayCall` block was updated.
- **Secret reader owner: `wyrd-utils`.** It already depends on `wyrd-spec`.
  It is a client-tier, server-free shared utility crate with an `fs` module,
  and the move adds only `secrecy`. `wyrd-vault` was rejected because its
  scope is Vault network reads. The reader is synchronous; async callers use
  `spawn_blocking`.

### Diagnosis

- **Symptom:** `public_ingress_workflow_fallback` took about 61 s.
- **Evidence:** the time is spent after the assertions, when the PG fixture
  is dropped. In the module run, 22 of 23 sibling tests show the same
  ~63 s, including tests that predate this task. The one exception, the
  replica-concurrency test, takes 124 s.
- **Cause:** the delay is in the shared PG test fixture teardown and
  affects the whole module, not this test. Adding
  `drain_gateway(&state).await` at the end of the test, following the
  file's precedent, makes its tracked audit and accounting tasks settle,
  but it does not remove the fixture-wide wait.
- **Fix site:** the shared fixture in the module. All tests pass and the
  delay is not a failure, so it was left outside this task's write set.

### Verification commands

All commands ran with `CARGO_TARGET_DIR=/home/thorrester/Documents/GitHub/wyrd/target`
and exited 0:

- `mise run fmt` and `mise run lints`.
- `cargo clippy -p wyrd-server --all-features --test pg_openapi_contract -- -D warnings`.
- `mise exec -- cargo nextest run --locked -p skald-workflow --lib` (12).
- `-p wyrd-gateway --lib` (58).
- `-p wyrd-server --lib -E 'test(/^config::/)'` (76).
- `mise run test:shared` (708, which includes the three focused wyrd-client
  tests above).
- `mise run test:wyrd-sdk` (1).
- `mise run test:gateway:native`: Rust 8/8, Python 2/2.
- `mise run test:principals:integration` (20/20 OpenAPI).
- The PG module under `scripts/postgres/with-test-postgres.sh` with
  `-E 'test(/^components::gateway::pg_invocation_tests::/)'` (23/23).
- `mise run codegen:check`, `check:client-tier`, `check:sdk-client-tier` and
  `check:pyo3-scope`.
- `mise run py:test:unit`, `py:typecheck`, `py:format` and `py:lints`.
- `mise run ts:test:unit`, `ts:typecheck` and `ts:napi:check`.
- `git diff --check`.

### Non-goals

These remain excluded:

- no new model ingress, Vertex public endpoint, or credential mutation API;
- no provider-body Workflow fields;
- no polling knobs;
- no public arbitrary-header transport (`post_native` is crate-private);
- no Python or TypeScript server-run lifecycle methods;
- no Python-enabled Rust or TypeScript dependency cone.

The in-process server caller belongs to TASK-004, and it must take its model
from `WyrdGatewayCall.model`. The real local Workflow-to-public-gateway SDK
journeys belong to TASK-005.
