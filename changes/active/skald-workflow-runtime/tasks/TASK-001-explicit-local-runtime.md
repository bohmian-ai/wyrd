---
id: TASK-001
kind: implementation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 10
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-013A, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-020, REQ-021, REQ-022, REQ-023, REQ-024, REQ-035, REQ-036, REQ-036A, REQ-037, REQ-038, REQ-039, REQ-040, REQ-042, REQ-043, REQ-045, REQ-047, REQ-048, REQ-049, REQ-051, REQ-052, INV-001, INV-002, INV-003, INV-004, INV-007, INV-008, INV-009, INV-010, INV-010A, INV-011, INV-012, INV-014, INV-016, INV-017, INV-020, INV-021, INV-023, AC-005, AC-006, AC-007, AC-008, AC-011, AC-011A, AC-016, AC-019, AC-020, AC-023, AC-024, AC-026]
depends_on: []
---

# Explicit local Workflow execution

Implementation skill: `$wyrd-implement`.

## Outcome and Value

An explicitly bound Agent-only Workflow runs through one async Skald engine,
returns the native portable WorkflowRun even after execution failure, and has
bounded ownership/cancellation. Rust and existing local Python surfaces move
with the contract. This is one buildable outcome: REQ-051/AC-024 prohibit a
contract-only task followed by consumer repair.

## Owners, Scope, Consumers, and Prohibited Changes

`wyrd-spec` owns Workflow bindings/routes/run DTOs/public error metadata;
`skald-workflow` owns pure resolved-plan validation, execution and local APIs;
`skald-providers` owns provider errors and shared provider/ExtGateway egress;
`sdks/wyrd-sdk-python` owns new/materially relocated Python wrappers.
Existing `Workflow`, `DagExecutor`, `WorkflowDef`, `Agent::run_prompt`, Prompt
binding and `AgentTool` are the starting points, not examples for a second engine.

Direct closure includes all constructors/exhaustive matches for the removed
actions/condition and old result shape, Agent/provider error consumers, local
builders, SDK aggregation/exports/stub generation, and endpoint-policy consumers
in gateway dispatch, admin validation, boot and tests. Vala's direct Workflow
consumers compile without introducing a reverse Skald dependency.
Keep Native transport and shared `wyrd-vault::VaultKv2`, gateway Vault and
Operator key consumers unchanged. Do not add remote Python/TS/MCP Workflow
surfaces, implicit compatibility translation, migration docs, new crates,
unapproved third-party dependencies, or gateway credential administration.

## Approach

1. Implement the spec's exact pure contracts and derive-backed errors, removing
   unsupported actions/condition and moving every direct consumer together.
2. Complete pure DAG/binding/resolved-Prompt validation and lowering without
   IO or a second renderer; preserve resolved Agent tools and run configuration.
3. Complete the existing executor's immutable namespaced context, exact output
   projection, retry/deadline composition and bounded owned task lifetime.
4. Implement the exact route dependency/call/binding interfaces; move only
   provider/ExtGateway endpoint policy to `skald-providers` with full consumers.
5. Align Rust/Python authoring/results and generated/export surfaces. Finish
   focused proof, Python-feature compilation, codegen and all-feature lint
   before any dependent task starts.

### Packet-local public seams

The following packet-local stubs fix the cross-task/public seams. All pure wire
types live in `wyrd-spec`; runtime-only types live in their Skald owner. These
are contract shapes, not private-module instructions.

```rust
// Pure/schema contracts; maps deterministic, IDs UUIDv7 canonical strings.
struct WorkflowRunId(Uuid);
enum WorkflowRunStatus { Queued, Running, Succeeded, Failed, Cancelled, TimedOut }
enum WorkflowStepStatus { Pending, Running, Succeeded, Failed, Cancelled, Unstarted }
struct WorkflowRunError { code: String, message: String, details: JsonValue, remediation: String }
struct WorkflowStepResult {
    status: WorkflowStepStatus, text: Option<String>, structured_output: Option<JsonValue>,
    attempts: u32, started_at: Option<DateTime<Utc>>, ended_at: Option<DateTime<Utc>>,
    error: Option<WorkflowRunError>,
}
struct WorkflowRun {
    run_id: WorkflowRunId, workflow: Option<CardRef>, status: WorkflowRunStatus,
    outputs: BTreeMap<String, JsonValue>, steps: BTreeMap<String, WorkflowStepResult>,
    created_at: DateTime<Utc>, started_at: Option<DateTime<Utc>>, ended_at: Option<DateTime<Utc>>,
    error: Option<WorkflowRunError>,
}
struct CreateWorkflowRunRequest {
    workflow: CardRef, input: BTreeMap<String, JsonValue>, timeout_seconds: Option<u64>,
}
enum LlmRoute {
    Native, WyrdGateway,
    ExtGateway { protocol: ExternalGatewayProtocol, base_url: AbsoluteUrl,
                 headers: BTreeMap<String, String>, credential_binding: CredentialBindingName },
}
enum ExternalGatewayProtocol {
    OpenAiChat, OpenAiResponses, AnthropicMessages, GeminiGenerateContent, VertexGenerateContent,
}
struct ExternalGatewayBindingConfig {
    protocol: ExternalGatewayProtocol, origin: Url, secret_headers: BTreeMap<String, SecretRef>,
}
// Runtime-only; secrets never become schema/Card/snapshot values.
enum ExternalEndpointProfile { Local, Production }
struct WyrdGatewayCall {
    request: ProviderRequest, fallback: Option<GatewayFallbackOverride>, timeout: Duration,
    correlation: WorkflowGatewayCorrelation,
}
struct WorkflowGatewayCorrelation { run_id: WorkflowRunId, step_id: String, attempt: u32 }
trait WyrdGatewayCaller: Send + Sync {
    async fn call(&self, call: WyrdGatewayCall, cancellation: &CancellationToken)
        -> Result<ProviderResponse, ProviderError>;
}
struct WorkflowExecutionDependencies { /* private native, gateway, bindings, profile */ }
impl WorkflowExecutionDependencies {
    fn new(native: ProviderRegistry) -> Self;
    fn with_wyrd_gateway(self, gateway: Arc<dyn WyrdGatewayCaller>) -> Self;
    fn with_external_gateways(self, bindings: ExternalGatewayBindings) -> Self;
    fn with_endpoint_profile(self, profile: ExternalEndpointProfile) -> Self;
}
struct ExternalGatewayBinding {
    name: CredentialBindingName, protocol: ExternalGatewayProtocol, origin: Url,
    secret_headers: HashMap<HeaderName, SecretString>,
}
struct ExternalGatewayBindings { /* keyed by CredentialBindingName */ }
impl ExternalGatewayBindings {
    fn new() -> Self;
    fn insert(&mut self, binding: ExternalGatewayBinding) -> Result<(), WyrdError>;
}
struct WorkflowExecutionLimits {
    max_concurrency: NonZeroUsize, deadline: Option<Duration>,
    max_input_bytes: Option<usize>, max_step_result_bytes: Option<usize>, max_run_bytes: Option<usize>,
}
struct WorkflowRunOptions { limits: WorkflowExecutionLimits, cancellation: CancellationToken }
impl Workflow {
    async fn run(&self, input: impl Into<WorkflowInput>) -> WorkflowResult<WorkflowRun>;
    async fn run_with(&self, providers: &ProviderRegistry, input: impl Into<WorkflowInput>)
        -> WorkflowResult<WorkflowRun>;
    async fn run_with_options(&self, dependencies: &WorkflowExecutionDependencies,
        input: impl Into<WorkflowInput>, options: WorkflowRunOptions) -> WorkflowResult<WorkflowRun>;
}
// Added to existing ProviderError, propagated through all direct consumers.
RemoteProblem(Box<RemoteProblem>)
pub struct RemoteProblem { code: String, status: u16, message: String, field: Option<String>, remediation: String }
```

Run/step enums and route/protocol variants serialize snake_case; run ID is
transparent; output/step maps and request input default empty; request and
config DTOs deny unknown fields. `WorkflowBinding` serializes one validated
source string; Workflow inputs retain native `ParameterValue` defaults; Workflow
and step routes are optional and step fallback remains optional. Remove direct
Prompt/Mcp actions and condition. Register these public codes through derive
metadata, never a parallel hand-written catalog:

| Suffix after `WYRD_WORKFLOW_` | HTTP/terminal use |
|---|---|
| `404_RUN_NOT_FOUND` | 404 unknown/foreign/expired/evicted/lost run |
| `409_IDEMPOTENCY_CONFLICT` | 409 changed scoped-key request |
| `422_RUN_REQUEST` | 422 invalid shape/input/timeout/CardRef |
| `422_ROUTE_UNSUPPORTED` | 422 incompatible environment/dialect |
| `422_SERVER_NATIVE_UNSUPPORTED` | 422 server graph Native |
| `422_TOOL_UNAVAILABLE` | 422 unavailable/duplicate declared tool |
| `429_RUN_CAPACITY` | 429 tenant/global active capacity |
| `503_BINDING_UNAVAILABLE` | 503 missing/unavailable runtime binding |
| `503_RUN_UNAVAILABLE` | 503 shutdown/unavailable tracked admission |
| `413_INPUT_TOO_LARGE` | 413 preacceptance decoded input |
| `413_GRAPH_TOO_LARGE` | 413 counts/resolved bytes/terminal reserve |
| `413_STEP_RESULT_TOO_LARGE` | terminal step error |
| `413_RUN_TOO_LARGE` | terminal run error |
| `504_STEP_TIMEOUT` | terminal exhausted outer attempt timeout |
| `504_RUN_TIMEOUT` | terminal total-deadline expiry |

Existing registry/permission/audit/provider errors remain authoritative.
RemoteProblem code() returns borrowed `&str`; known code remediation comes
from derive catalog, otherwise existing provider category. Project only
`{field}` details or `{}`, never raw upstream body/arbitrary details.

Packet-local binding contract: `WorkflowBinding` is a validated string newtype
with exactly these wire roots: `input.<name>`,
`steps.<step_id>.output.text`, `steps.<step_id>.output.structured`, or
`steps.<step_id>.output.structured.<field>...`. Components use the existing
parameter identifier grammar `[A-Za-z_][A-Za-z0-9_]*`; no whitespace, template
delimiters, surrounding prose, expressions, multiple-source composition, or
array indexing. Input names must be declared by WorkflowSpec.inputs. A step
binding may see only its declared transitive dependencies, never itself or an
unrelated step; a reference does not add an edge. Final Workflow output
selectors may see any declared step or input and retain the selected JSON type.
Every unresolved declared Prompt variable has exactly one input binding and
extra bindings are rejected. Defaults are native ParameterValue values;
invocation overrides must match their variant, with unknown keys rejected.
Selected strings pass unchanged, null becomes empty text, and other values
become compact JSON at the Prompt boundary only. Missing selected runtime
fields fail the dependent attempt before its provider call.

Packet-local ExtGateway trust contract: protocol must match the native request
dialect; the named binding must match protocol and exact origin and cannot be
replaced by invocation input. Base URL must be absolute, without userinfo,
fragment, or query, and HTTPS outside the explicit local/test profile. Authored
header names are case-insensitively unique and reject `host`, `content-length`,
`connection`, `transfer-encoding`, `te`, `trailer`, `upgrade`, `forwarded`,
`x-forwarded-*`, `proxy-*`, `x-wyrd-access-token`, `wyrd-request-id`,
`authorization`, and names containing `api-key` or `token`. Cards contain only
non-secret headers and the binding alias, never secret values or secret-store
coordinates; authored headers cannot overwrite bound secret headers. Resolve,
screen every resolved address, and pin that exact result before transmission;
no re-resolution, proxies, or redirects. Block metadata/link-local in every
profile; Production rejects private/loopback/CGNAT/ULA, while Local permits
explicitly configured private targets. Preserve TLS and connection/request/
response bounds. Native transports and shared Vault/Operator policies are not
part of this migration.

Required operation order: validate pure graph/input types then resolved Prompt
variables/dialect before dispatch; take only completed visible dependency
values, convert strings/null/other JSON to existing Prompt text conversion,
then invoke the existing binder. One attempt covers Agent loop→normalization→
output validation, not provider-internal retry. Count the begun attempt even
on binding failure. Eligible retries use `min(initial_ms * 2^(r-1), 30_000)`
saturating, no jitter; None/zero is immediate. Auth/permission/binding/route/
tool/callback/session/journal/max-iteration/cancel/invariant failures never
retry; eligible provider connection/timeout/decode/408/429/5xx, Agent timeout/
decode/validation, and outer step timeout do. Gateway outcomes are retryable
only for exact `WYRD_GATEWAY_429_LIMIT_EXCEEDED`,
`WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE`, and
`WYRD_GATEWAY_504_DEADLINE_EXCEEDED`; other gateway auth, authorization,
budget, shape, binding, route, or invariant codes are terminal regardless of
HTTP status. Retry an Agent's final structured-output decode/validation
failure, not a tool, callback, session, journal, or max-iteration failure.
Race cancellation first, then total deadline, then attempt timeout, then Agent
timeout when ready together; never begin another attempt after cancellation/
total deadline. A stage failure schedules no later work but drains current peers;
explicit cancel/deadline abort-and-drain; parent drop aborts owned JoinSet.
Only then terminalize, preserving succeeded data and marking never-started
steps unstarted. Primary ordinary error is earliest stage then smallest step ID.
This task's abort guarantee owns Skald futures only. TASK-004 owns server read-
tool collection/settlement outside those abortable futures and joins it before
the server commits the terminal snapshot/releases Workflow capacity; do not
embed an authoritative query drain inside a future that step abort can destroy.
Succeeded has declared outputs/no run error/all succeeded steps; failed has
empty outputs/one primary run error; cancelled has empty outputs/no run error;
timed_out has empty outputs/exact run-timeout error. Terminal steps have no
pending/running; failed steps alone carry error, succeeded steps alone carry
payload. Reserve complete metadata and capped 2-KiB safe errors before allowing
payload to consume full JCS snapshot budget; no oversized data enters snapshots
or observations. Local defaults: eight concurrent steps, no total/input/result
cap; only run_with_options supplies non-Native dependencies.

For explicit authoring, add consuming Rust methods on `Workflow` and matching
builder methods: `with_inputs(BTreeMap<String, ParameterValue>)`,
`with_step_inputs(&str, BTreeMap<String, WorkflowBinding>)`, and
`with_outputs(BTreeMap<String, WorkflowBinding>)`, each returning
`WorkflowResult<Self>`. Step-ID lookup and binding syntax are checked when
set; completeness is checked at final build/load/run, not while an incomplete
builder is being populated. Existing sequential/parallel/add/add_after only
construct graph edges. The local Python wrapper exposes the same `with_*`
names with ordinary dicts and returns an updated Workflow value; native
conversion and validation stay in Rust. It exposes portable `outputs`/`steps`,
not `parameters`/`final_output` aliases. This does not remove standalone
PromptDraft shorthand: inline Agent Prompts use native requests while existing
standalone Prompt authoring helpers continue to compile to native Prompts.

## Ordered Implementation Scenarios

All Rust selectors below are **planned**, not existing passing tests. Place
them in the indicated owning inline test module; record exact selected counts
after adding them. Default features unless explicitly stated. Every GREEN
reruns all earlier scenario tests; every REFACTOR keeps them green.

### Scenario 1 — The declared graph is the contract

**Behavior.** REQ-001–005/007–009/012–013A/036–037/040/045: exact serde/schema
shapes; Agent-only actions; valid identifiers; nonempty outputs; defaults/type
overrides; declared dependency visibility; route/fallback constraints; stack-safe
deep validation. Invalid hidden references, cycles, duplicate edges, expression
bindings and missing/extra unresolved Prompt variables fail before dispatch.

**RED.** Add `card::workflow::tests::explicit_workflow_contract` in `wyrd-spec`;
`mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::workflow::tests::explicit_workflow_contract)'`.
Add `workflow_surface::tests::resolved_bindings_reject_before_dispatch`;
`mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow_surface::tests::resolved_bindings_reject_before_dispatch)'`.
Expect obsolete accepted shapes or incomplete validation, not an unrelated
fixture failure. Table-driven cases assert safe field-specific stable errors.

**GREEN.** Implement pure contract and resolved validation; construction/lowering
does not bind live IO into Cards or add async to pure computation.

**REFACTOR.** Reuse the native parameter grammar and Prompt request/binder;
remove duplicated graph or expression validation instead of adding a framework.

### Scenario 2 — Explicit data injection and deterministic results

**Behavior.** REQ-006–012/020–023: parallel outputs with the same `summary` key
remain independent; downstream original-input/text/whole-object/nested-field
bindings select only visible completed results. String/null/compact-JSON Prompt
conversion matches existing behavior, and final output preserves JSON type.
Dependencies alone inject nothing; missing runtime fields fail before the
downstream call. Complete statuses, attempt counts and primary peer error are
deterministic under reversed completion order.

**RED.** Add `workflow::tests::explicit_namespaced_results`;
`mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::explicit_namespaced_results)'`.
Expect current flat context, implicit message handoff, inferred final output or
error-only failure to violate assertions.

**GREEN.** Execute the validated plan through the existing Agent loop, preserve
partial succeeded peers and terminalize every remaining step appropriately.

**REFACTOR.** Delete obsolete implicit handoff/result paths when no consumer
remains; retain observation integration rather than a separate event subsystem.

### Scenario 3 — Retries obey one deadline and one ownership boundary

**Behavior.** REQ-016–019/043/047–048: concurrency ceiling; ordinary peer drain;
cancel/deadline abort-and-drain; parent-drop cleanup; exact eligible/terminal
error classification, attempts, saturating exponential backoff/cap and timeout
precedence. Provider retries/fallback are inside one Workflow attempt; local
tools can repeat under authored retries. No task/Native/ExtGateway IO survives.

**RED.** Add `workflow::tests::bounded_attempt_lifecycle`;
`mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::bounded_attempt_lifecycle)'`.
Use deterministic synchronization/clock seams already available in the owning
runtime, not sleeps or raised host load; expect present detached work, unbounded
concurrency or incorrect retry classification to fail.

**GREEN.** Use spec-approved owned JoinSet, effective deadlines and cancellation;
observe every attempt/backoff and return complete terminal snapshots.

**REFACTOR.** Keep Agent, provider and Workflow retry owners distinct; no
parallel synchronous engine or configurable polling/scheduling abstraction.

### Scenario 4 — Route context cannot leak between steps

**Behavior.** REQ-035–040/043/047 and AC-011A: exact concrete dependency owner;
step-over-workflow precedence; immutable fallback/deadline/cancellation/
correlation per model call; all five request dialects retain their native shape
for Native/ExtGateway and use the narrow WyrdGateway trait. RemoteProblem
retains only the spec's safe common metadata and existing category mapping.

**RED.** Add `workflow::tests::isolated_route_calls`;
`mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::isolated_route_calls)'`.
Expect unsupported route lowering, discarded fallback/tool declarations or
wrong remote error projection. A fake caller proves the neutral seam here;
TASK-003/004 prove real public/in-process gateway implementations.

**GREEN.** Use private immutable per-step Provider adapters and existing Agent
registry; include all ProviderError match consumers in this change.

**REFACTOR.** Reuse typed request codecs and existing providers; no request-body
Workflow metadata, global route state or second tool loop.

### Scenario 5 — ExtGateway has one scoped secure transport

**Behavior.** REQ-042/049: exact binding origin/protocol/header constraints;
local private allowance versus production DNS/address screening/pinning;
no-proxy/redirect refusal/TLS/bounded response/timeouts; forbidden headers and
credential collisions refuse before dispatch. Native and Vault/Operator
transport behavior remains unchanged across the endpoint-policy move.

**RED.** Add `workflow::tests::bound_external_gateway_security`;
`mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::bound_external_gateway_security)'`.
Expect missing direct route/binding enforcement. Existing endpoint tests move
with their owner and remain regression proof, not a manufactured RED for relocation.

**GREEN.** Move `EndpointPolicy` from the gateway to existing provider transport
and close every enumerated consumer. Resolve/screen/pin effective addresses
before secret transmission. No Vault migration or new native-client profile.

**REFACTOR.** Remove the old policy implementation; keep one provider-egress
owner and redacted binding Debug. Do not consolidate unrelated network owners.

### Scenario 6 — Full snapshot budgets always permit terminalization

**Behavior.** REQ-017/045: JCS input/full-snapshot size and step text/structured
size; metadata/errors charged; bounded 2-KiB diagnostics; payload-free terminal
reserve; exact 413 projections and no oversized payload in retained results or
observations. Local uncapped defaults and eight-ready-step limit remain exact.

**RED.** Add `workflow::tests::terminal_budget_reserve`;
`mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::terminal_budget_reserve)'`.
Expect metadata exemption, retained oversize data or inability to cancel/fail
near the output ceiling. Server graph admission proof belongs to TASK-004.

**GREEN.** Apply the specified canonical accounting and reserved terminal
space at candidate transitions/output projection without copying raw payloads
into diagnostics or observers.

**REFACTOR.** Share synchronous size/error projection on its natural owner;
no allocator-size heuristic or second error catalog.

### Scenario 7 — Local authoring and Python use the explicit model

**Behavior.** REQ-024/047/051: Rust/Python builders can declare inputs/bindings/
outputs; edges infer no data; text shorthand requires declared string `input`;
outputs/steps replace old fields; sync Python calls the shared engine and local
callable tools still execute only when declared and supplied.

**RED.** Add `workflow_surface::tests::explicit_builder_contract`;
`mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow_surface::tests::explicit_builder_contract)'`.
Add Python `test_explicit_workflow_bindings` in the existing
`tests/unit/runtime/workflow/test_workflow_parameter_injection.py` owner;
after `mise run py:setup`, from repository root run
`mise exec -- uv run --project sdks/wyrd-sdk-python python -m pytest -q sdks/wyrd-sdk-python/tests/unit/runtime/workflow/test_workflow_parameter_injection.py::test_explicit_workflow_bindings`.
Expect missing authoring/result APIs or implicit behavior. Rewrite old implicit
tests to assert the approved replacement, not delete their coverage.

**GREEN.** Thin language conversion/wrappers, exports and generator annotations
move together; no interpreter lifetime in Rust tests, no hand-edited stubs.

**REFACTOR.** Remove obsolete result/implicit-binding code; no aliases or
compatibility engine. Keep untouched legacy Python migration state untouched.

## Acceptance Criteria

All scenario cases pass; portable snapshots satisfy every status/field/attempt
invariant; deep graph validation is stack-safe; local custom tools reach outputs
but undeclared/unregistered tools never execute. Workspace remains buildable
with retained Python features and no subsequent task repairs compilation.
Real registered route journeys are TASK-004/005 closure, not claimed by mocks.

## Expected Write Set and Consumer Closure

Likely `crates/wyrd-spec/src/card/workflow.rs`, public errors/schema generator;
`crates/skald/skald-workflow/src/{workflow_surface,workflow,def,context,run,error,...}`;
`crates/skald/skald-providers/src/{error,transport,...}`; all direct native
Agent/Vala result/error consumers; `crates/wyrd/wyrd-gateway/src/{endpoint,adapter,...}`;
server gateway admin/boot/test composition references; Python SDK wrappers,
`src/lib.rs`, public `python/wyrd/agent` exports and generated annotations/tests.
Existing-workspace dependency edges and exact runtime features authorized by
REQ-049 may be wired; Tokio `net` is the only added resolver feature.

## Verification and Evidence

Run focused selectors above, then `mise run test:skald`, `mise run test:shared`,
`mise run test:wyrd`, `mise run py:test:unit`, `mise run py:typecheck`,
`mise run codegen:check`, `mise run check:client-tier`,
`mise run check:pyo3-scope`, `mise run check:unwrap-audit`, `mise run fmt`,
`mise run lints`, `mise run py:format`, `mise run py:lints`, and
`git diff --check`. These are predependent scoped checks, not the final plan
aggregate. Use `mise exec -- cargo check --locked -p skald-workflow --features python`
for retained feature compile proof; Python lifetime proof stays Python-owned.
Relocation/default schema/export/docs work has static/regression proof, not
manufactured RED. Audit endpoint/Vault/Operator write-set and dependency closure.

## Material Stop Conditions

Stop for any new public route/action/binding root/renderer, changed retry or
terminal semantics, unapproved feature/dependency, broadened endpoint/Vault
policy, remote language surface or inability to close direct consumer compilation.

## Authority Links

- [Approved Revision 10](../spec.md)
- `AGENTS.md` §§2–12, 14–16; `architecture/agent-rules.md`
- `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`
- `architecture/references/{doctrine/architecture-constraints,architecture/patterns}.md`
- `architecture/references/languages/{rust-core,errors,spec-driven-development,implementation-execution,testing-workflows}.md`

## Implementation Evidence

Commits `dbf640c26..286218a68` on `wyrd/skald-workflow-runtime/TASK-001`.
RED: each scenario selector was written against the old implicit engine
(flat context, edge message handoff, `parameters`/`final_output`, Prompt/Mcp
actions, condition) and failed to compile or assert before GREEN.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| S1 declared graph is the contract | `wyrd-spec/src/card/workflow.rs` (bindings, routes, run DTOs, derive codes); `skald-workflow/src/{plan,workflow_surface}.rs` | `card::workflow::tests::explicit_workflow_contract` (1/1); `workflow_surface::tests::resolved_bindings_reject_before_dispatch` | PASS |
| S2 explicit injection, deterministic results | `skald-workflow/src/workflow.rs` executor, namespaced context, output projection | `workflow::tests::explicit_namespaced_results` | PASS |
| S3 retries, one deadline, owned lifetime | `workflow.rs`/`attempt.rs` JoinSet, backoff, precedence, classification | `workflow::tests::bounded_attempt_lifecycle` | PASS |
| S4 isolated route calls, RemoteProblem | `skald-workflow/src/route.rs`; `skald-providers/src/error.rs` `RemoteProblem` | `workflow::tests::isolated_route_calls` | PASS |
| S5 ExtGateway secure transport | `skald-providers` endpoint policy (moved from `wyrd-gateway`), binding validation, pinning | `workflow::tests::bound_external_gateway_security`; gateway endpoint tests in `test:wyrd` | PASS |
| S6 terminal budget reserve | `workflow.rs` canonical accounting and terminal reserve | `workflow::tests::terminal_budget_reserve` | PASS |
| S7 Rust/Python explicit authoring | `workflow_surface.rs` `with_*`; `skald-workflow/src/python.rs` `PyWorkflowRun`; stubs and exports | `workflow_surface::tests::explicit_builder_contract`; `test_workflow_parameter_injection.py::test_explicit_workflow_bindings` | PASS |
| OpenAI Responses Agents run their tool loop (REQ-039) | `skald-spec` `MessageNum::OpenAiResponses`; `skald-agent/src/request_builder.rs` (`PromptLoopSupport::OpenAiResponses`, native extract/assistant/rebuild/tool-result); `session.rs` dialect-aware seeding | RED: the loop refused with "not yet supported". `-p skald-agent --test loop_responses` (`agent_run_executes_openai_responses_tool_loop`, `responses_session_turns_seed_native_items`); `-p skald-spec --lib` `request::round_trip::messages_roundtrip`, `request::untagged_dispatch::message_num_untagged_dispatch_per_provider`; the `isolated_route_calls` `responses` step over WyrdGateway | PASS |
| Local tools reach outputs; undeclared tools never run | `workflow_surface.rs` tests | `explicit_builder_contract` (tool result reaches `outputs["found"]`; undeclared gives `WYRD_AGENT_404_TOOL_NOT_IN_AGENT`, zero calls) | PASS |
| Workspace builds with retained Python features | all consumers in commit range | `cargo check --locked -p skald-workflow --features python`; `mise run lints` (all features) | PASS |
| Scoped lanes | — | `test:skald`, `test:shared` (702 passed), `test:wyrd` (2285 passed), `py:test:unit`, `py:typecheck`, `codegen:check`, `check:client-tier`, `check:pyo3-scope`, `check:unwrap-audit`, `fmt`, `lints`, `py:format`, `py:lints`, `git diff --check`: all exit 0 | PASS |

Deviations and limits:
- `ExternalGatewayBinding.secret_headers` is a `HashMap<HeaderName, SecretString>`,
  because `HeaderName` is not `Ord`. Its iteration order is never observable.
- `DEFAULT_MAX_RETRIES = 3` for the local executor.
- The OpenAI Responses Agent loop was added at the lead's direction. The
  write set now includes `skald-spec/src/message.rs` (new native
  `MessageNum::OpenAiResponses(Vec<OpenAiResponseItem>)`, the only
  array-shaped variant) and `skald-agent/src/{request_builder,session,loop_runtime}.rs`.
  Loop history uses native Responses `input` items (message, function_call,
  function_call_output) with no cross-dialect translation. Session-seeded
  assistant turns use Responses output messages. History is replayed
  statelessly, the same as every other dialect. Reasoning items are not
  replayed, because the wire type drops their item id. `previous_response_id`
  is left as authored.
- `ProviderError::RemoteProblem` boxes its payload as `Box<RemoteProblem>`
  because of clippy `result_large_err`. The fields match the spec.
- Six catalog codes that no remaining path emits were removed: `404_TASK`,
  `500_AGENT_RESPONSE_MISSING`, `500_LOCK`, `500_MAX_RETRIES`, `500_STALLED`,
  and `501_UNSUPPORTED_HANDOFF`, all prefixed `WYRD_WORKFLOW_`.
  `MISSING_PARAMETER` was retitled.
- `tests/parameter_injection.rs` was superseded by the inline scenario tests.
- Added the `workflow_run` and `create_workflow_run_request` schemas.
- Real registered route journeys remain TASK-004/005 closure.

Non-goals stayed excluded:
- No remote Python, TypeScript, or MCP Workflow surface.
- No compatibility aliases or migration docs.
- No new crates or third-party dependencies.
- Native transport and Vault/Operator policies are unchanged.
