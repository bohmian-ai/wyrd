---
id: SPEC-skald-workflow-runtime
revision: 6
status: draft
---

# Skald workflow runtime

## Objective and user value

Finish the existing Skald workflow capability so one declarative Workflow Card
can be authored in YAML, executed locally before registration, registered with
`wyrd apply`, fetched and executed locally, or invoked on `wyrd-server`.

The result gives users one workflow definition and one Skald execution model
for single-agent and multi-agent DAGs. A user can test a code-review workflow
locally, register the same card and its referenced Agents and Prompts, then
choose local execution or tenant-bound server execution without rewriting the
workflow.

## Research baseline

The change completes existing behavior rather than introducing another
workflow engine:

- `wyrd-spec::card::workflow::WorkflowSpec` already declares workflow inputs,
  steps, dependencies, step inputs, conditions, timeouts, retries, outputs,
  governance, and observation hooks.
- `skald-workflow::Workflow` already loads Workflow Card YAML and executes
  locally through `DagExecutor`.
- `DagExecutor` already validates and executes dependency-ready steps in
  concurrent topological stages, invokes Skald Agents through a supplied
  `ProviderRegistry`, retries calls, emits observer events, and captures task
  responses.
- Skald Prompt binding already owns `${name}` and `{{name}}` discovery,
  JSON-safe substitution through provider-request string leaves, missing-value
  detection, and preservation of the native provider request shape. Workflow
  execution must supply resolved variable values to that existing boundary; it
  must not create a second prompt renderer.
- Skald already has native request and client coverage for OpenAI Chat,
  OpenAI Responses, Anthropic Messages, Google GenerateContent, and Vertex
  GenerateContent.
- Wyrd gateway Revision 21 is approved and implemented. `wyrd-server` exposes
  OpenAI-compatible, Anthropic Messages, and Gemini GenerateContent ingress;
  every governed call converges on one in-process gateway invocation pipeline.
  The gateway supports OpenAI, Anthropic, Gemini, Vertex, and tenant-defined
  OpenAI-compatible deployments across its declared operation matrix.
- Gateway provider deployments, provider/model RBAC, fallback, limits,
  accounting, capture, and provider credentials are server-owned. Environment,
  Vault, and tenant-submitted managed credentials resolve only inside the
  authorized gateway call. Workflow Cards and invocation input never carry a
  provider credential.
- Gateway credential mutation is intentionally absent from the first-class
  Rust, Python, and TypeScript SDKs. Tenant administrators use the CLI or the
  explicitly scoped MCP write tool; workflows consume configured deployments
  and never administer credentials.
- Gateway invocation authorization is synchronous, while its canonical audit
  append is tracked and non-blocking. Workflow-invocation authorization is a
  separate server decision and retains the normal transactional, fail-closed
  audit rule.
- `wyrd apply` and the shared loader already resolve Card-level `path`,
  `inline`, and `ref` forms and register Cards in dependency order.
- `wyrd-server` already owns process-local, tenant-qualified run state for Eval
  and tracked background work for MCP and gateway calls. Those lifecycle
  patterns are the bounded precedent for asynchronous Workflow execution; this
  change does not add a durable queue or scheduler.
- `wyrd-client` already owns stable `Idempotency-Key` submission and safe
  transport retry. Workflow creation reuses it rather than adding a second
  retry protocol.

The remaining behavior is contract-to-runtime completeness: current lowering
discards declared step inputs, timeouts, retry backoff, workflow outputs, and
non-Agent actions; referenced Agents are not hydrated by the ordinary YAML
execution path; concurrent structured outputs share a collision-prone flat
map; dependency completion implicitly prepends provider messages; final output
is inferred from the last topological step; terminal failures discard the
partial run; Workflow DAG validation is not enforced during loading or
registration; and no CLI or server invocation surface exists. The gateway is
therefore an implemented upstream dependency; this change owns only the
workflow contracts, the existing Skald runtime's missing semantics, and a thin
server lifecycle around that runtime.

## Implemented gateway boundary consumed by this change

The workflow runtime does not recreate gateway behavior:

- A locally executed `WyrdGateway` step calls the public `wyrd-server`
  gateway ingress using the execution environment's server address and Wyrd
  access credential. It can use only the OpenAI-compatible, Anthropic Messages,
  and Gemini GenerateContent protocols exposed at that public edge; Vertex's
  native protocol has no public gateway ingress in V1.
- This change extends those authenticated model ingresses with only the
  optional `wyrd-gateway-fallback` header defined below so a Workflow's stored
  per-step override reaches the existing `GatewayCallRequest.fallback`. It adds
  no new gateway route or provider request field.
- A workflow executing on `wyrd-server` enters the same governed invocation
  pipeline through the existing in-process gateway boundary and verified
  caller context; it does not call the public API recursively.
- The gateway owns authorization, provider/model resolution, fallback
  eligibility, translation, provider credentials, limits, accounting, capture,
  and invocation audit. Skald supplies the Prompt's typed provider request,
  the stored per-step fallback override, deadline, cancellation, and workflow
  correlation.
- `Native` is a caller-local route. Server workflow invocation accepts only
  `WyrdGateway` and `ExtGateway`; it rejects a workflow containing a resolved
  `Native` step before the first step starts.
- `ExtGateway` is execution-local: the process currently executing the step
  calls the configured external gateway directly. A local process and
  `wyrd-server` therefore use the same route meaning, and neither sends an
  `ExtGateway` request through the Wyrd gateway.
- Gateway failures retain stable redacted Wyrd errors. Workflow results never
  expose gateway deployment or credential identity.

## Definitions

- **Workflow definition:** the `spec` of a `kind: Workflow` Card.
- **Local file execution:** execution from a local Workflow Card YAML document
  or bundle without requiring registration.
- **Local registered execution:** fetching a registered Workflow Card and its
  locked dependencies, then executing them in the caller's process.
- **Server execution:** asking `wyrd-server` to resolve and execute a registered
  Workflow Card under the authenticated tenant and server runtime.
- **Step output:** the normalized text and/or structured JSON produced by one
  Agent step, retained under that step's stable ID.
- **Workflow binding:** one validated source path selecting a workflow input or
  step output. In a step-input map it assigns that value to one existing Prompt
  variable and may reference only a declared dependency. It performs no string
  interpolation. Its wire form is the source-path string shown below, not a
  tagged value or template expression.
- **Workflow output:** a named JSON value selected by one validated binding
  after all required steps complete.
- **Execution environment:** the local process or `wyrd-server`; it supplies
  route-appropriate clients, tools, observers, deadlines, and resource bounds
  but does not change workflow semantics. Gateway provider credentials remain
  owned by the gateway and are never supplied to Skald.
- **LLM route:** the registered declaration that selects how an Agent step's
  native provider request reaches an LLM. The public Rust contract is the
  closed `LlmRoute` enum with `Native`, `WyrdGateway`, and `ExtGateway { ... }`
  variants, serialized with `native`, `wyrd_gateway`, and `ext_gateway` wire
  names.
- **Native route:** Skald calls the native provider selected by the Agent's
  Prompt without an intervening gateway.
- **Wyrd gateway route:** local Skald execution sends the Prompt's request to
  the public gateway ingress on `wyrd-server`; server Skald execution enters
  the same governed gateway pipeline in process.
- **External gateway route:** the current execution process sends the Prompt's
  native provider protocol directly to the user-provided gateway endpoint and
  headers declared on the route. "Execution-local" does not mean
  "developer-machine only"; it includes `wyrd-server` when the server owns the
  run.
- **Server Workflow run:** process-local, tenant- and principal-qualified
  lifecycle state for one asynchronously executing registered Workflow. It is
  available after the creating HTTP request disconnects but is not durable
  across process restart and is not a persisted Wyrd run registry.

## User ergonomics

### Define a multi-agent workflow in YAML

A workflow bundle is ordinary Wyrd YAML. Agents and reusable Prompts remain
separate Cards, and the Workflow connects the Agents into a DAG:

```text
code-review/
├── workflow.yaml
├── agents/
│   ├── security.yaml
│   ├── correctness.yaml
│   └── final-reviewer.yaml
└── input.json
```

`workflow.yaml` declares two independent reviewers followed by one final
reviewer. The final step names both dependencies and explicitly binds their
namespaced outputs:

```yaml
apiVersion: wyrd/v1
kind: Workflow
metadata:
  space: engineering
  name: code-review
  version: "1.0.0"
spec:
  description: Review a change concurrently, then validate and consolidate findings.
  llm_route:
    kind: wyrd_gateway
  inputs:
    code:
      type: str
      value: ""

  steps:
    - id: security
      action:
        type: agent
        target:
          path: ./agents/security.yaml
      inputs:
        code: input.code
      timeout_seconds: 60
      retry:
        max_retries: 1

    - id: correctness
      action:
        type: agent
        target:
          path: ./agents/correctness.yaml
      inputs:
        code: input.code
      timeout_seconds: 60
      retry:
        max_retries: 1

    - id: final_review
      action:
        type: agent
        target:
          path: ./agents/final-reviewer.yaml
      depends_on: [security, correctness]
      inputs:
        code: input.code
        security_review: steps.security.output.text
        correctness_review: steps.correctness.output.text
      timeout_seconds: 60
      retry:
        max_retries: 1

  outputs:
    review: steps.final_review.output.text
```

`WorkflowStep.inputs` does not template the Prompt. Each map key names one
variable already declared by the Agent's Prompt, and each value is one exact
workflow binding. At execution time the workflow resolves the binding to a
value, converts that value through the existing Skald workflow-to-Prompt value
conversion, and passes the resulting `(name, value)` pairs to the existing
Prompt binding API. Prompt binding remains the only `${name}` / `{{name}}`
renderer.

Workflow bindings are plain source paths, not expressions. V1 permits
`input.<name>`, `steps.<step_id>.output.text`, and
`steps.<step_id>.output.structured.<field>...`; path components use the
existing identifier grammar and arrays are not addressable. A Workflow binding
cannot contain surrounding text or compose multiple sources. Constants belong
in the Prompt or in `WorkflowSpec.inputs` defaults. Workflow `outputs` use the
same exact source-path shape and preserve the selected JSON value's type.

Each Agent owns its instruction Prompt. `agents/security.yaml` is representative:

```yaml
apiVersion: wyrd/v1
kind: Agent
metadata:
  space: engineering
  name: security-reviewer
  version: "1.0.0"
spec:
  prompt:
    inline:
      provider: openai
      model: gpt-5-5
      system: |
        You are a security reviewer. Report only exploitable security findings.
        For every finding, identify the affected code and a concrete failure path.
      messages:
        - |
          Review this change:
          {{code}}
      variables: [code]
      response_type: text
  tool_names: []
  run_config:
    max_iterations: 1
    timeout_ms: 60000
```

`agents/correctness.yaml` changes the instruction while preserving the same
input contract:

```yaml
apiVersion: wyrd/v1
kind: Agent
metadata:
  space: engineering
  name: correctness-reviewer
  version: "1.0.0"
spec:
  prompt:
    inline:
      provider: openai
      model: gpt-5-5
      system: |
        You are a correctness reviewer. Find reachable bugs, data loss, races,
        and contract violations. Ignore style-only concerns.
      messages:
        - |
          Review this change:
          {{code}}
      variables: [code]
      response_type: text
  tool_names: []
  run_config:
    max_iterations: 1
    timeout_ms: 60000
```

`agents/final-reviewer.yaml` receives both prior outputs through ordinary Prompt
variables and validates them against the original input:

```yaml
apiVersion: wyrd/v1
kind: Agent
metadata:
  space: engineering
  name: final-reviewer
  version: "1.0.0"
spec:
  prompt:
    inline:
      provider: openai
      model: gpt-5-5
      system: |
        You are the final reviewer. Validate each proposed finding against the
        supplied code, remove duplicates and unsupported claims, and return one
        prioritized review.
      messages:
        - |
          Code:
          {{code}}

          Security review:
          {{security_review}}

          Correctness review:
          {{correctness_review}}
      variables: [code, security_review, correctness_review]
      response_type: text
  tool_names: []
  run_config:
    max_iterations: 1
    timeout_ms: 60000
```

`input.json` is the invocation payload:

```json
{
  "code": "diff --git a/src/auth.rs b/src/auth.rs\n..."
}
```

The same bundle supports all requested journeys:

```bash
# Test the unregistered bundle in the local process.
wyrd workflow run \
  --file ./code-review/workflow.yaml \
  --input-file ./code-review/input.json \
  --execution local

# Register the Workflow, Agents, and Prompts as one resolved Card graph.
wyrd apply ./code-review

# Fetch the registered graph, but execute it in the local process.
wyrd workflow run \
  --space engineering \
  --name code-review \
  --version 1.0.0 \
  --input-file ./code-review/input.json \
  --execution local

# Ask wyrd-server to resolve and execute that same registered graph.
# This prints the run ID, then waits for the terminal result by default.
wyrd workflow run \
  --space engineering \
  --name code-review \
  --version 1.0.0 \
  --input-file ./code-review/input.json \
  --execution server

# Detach from a long-running review, then inspect or cancel it later.
wyrd workflow run \
  --space engineering \
  --name code-review \
  --version 1.0.0 \
  --input-file ./code-review/input.json \
  --execution server \
  --detach
wyrd workflow status <run-id>
wyrd workflow cancel <run-id>
```

### Declare provider routing

Provider routing is part of the Workflow Card because a registered Workflow
version must say whether its model calls are direct, Wyrd-governed, or sent to
a user-provided gateway. `WorkflowSpec.llm_route` declares the workflow default;
`WorkflowStep.llm_route` optionally replaces it for one step. There is no named
route catalog or second layer of indirection.

The durable route contract is:

```text
LlmRoute =
  Native
  | WyrdGateway
  | ExtGateway {
      protocol: ExternalGatewayProtocol,
      base_url: AbsoluteUrl,
      headers: map<HTTP field name, string>,
      credential_binding: CredentialBindingName,
    }

ExternalGatewayProtocol =
  OpenAiChat
  | OpenAiResponses
  | AnthropicMessages
  | GeminiGenerateContent
  | VertexGenerateContent

```

The wire names use `snake_case`. Header names are case-insensitive and unique.
`host`, `content-length`, `connection`, `transfer-encoding`, `te`, `trailer`,
`upgrade`, `forwarded`, `x-forwarded-*`, `proxy-*`,
`x-wyrd-access-token`, `wyrd-request-id`, `authorization`, and header names
containing `api-key` or `token` are forbidden in `headers`. `base_url` is an
absolute URL without userinfo, fragment, or query. HTTPS is required outside
an explicit local/test profile, where loopback HTTP is permitted for
deterministic fixtures.

`credential_binding` is a non-secret alias resolved by the execution
environment. The binding fixes the permitted endpoint origin and the secret
header map supplied from local configuration or server operator configuration.
The binding is required even when the gateway needs no secret headers; in that
case it acts only as the environment's explicit authorization of the protocol
and endpoint origin.
The runtime rejects an absent binding, origin mismatch, or attempt by the Card
to replace a bound secret header. A Workflow Card never names an environment
variable, secret-store path, credential value, or arbitrary secret header.
Server bindings are assigned to exactly one tenant, protocol, and endpoint
origin and cannot be listed or selected across tenants. Local bindings are
caller-owned configuration and carry the same protocol, origin, and header-map
constraints without acquiring server tenancy.

Direct native provider calls are the default:

```yaml
spec:
  llm_route:
    kind: native
  steps: [...]
```

The Wyrd gateway belonging to the execution environment requires no endpoint
or credential in the Card:

```yaml
spec:
  llm_route:
    kind: wyrd_gateway
  steps: [...]
```

An external gateway carries its non-secret connection contract inline. Secret
headers come only from the named execution-environment binding:

```yaml
spec:
  llm_route:
    kind: ext_gateway
    protocol: openai_chat
    base_url: https://llm-gateway.acme.com/v1
    headers:
      x-portkey-provider: "@production"
    credential_binding: acme-production

  steps:
    - id: security
      action:
        type: agent
        target:
          path: ./agents/security.yaml

    - id: correctness
      action:
        type: agent
        target:
          path: ./agents/correctness.yaml
      llm_route:
        kind: native
```

The resolved route is
`step.llm_route -> workflow.llm_route -> LlmRoute::Native`. The Prompt remains
the sole owner of provider, model, native request fields, tool projection, and
response shape. `LlmRoute` selects the execution boundary; for `WyrdGateway`,
the gateway may perform only the typed translation allowed by its configured
adapter and capability matrix. A registered invocation cannot replace the
stored route through its input payload.

`WorkflowStep.fallback: GatewayFallbackOverride?` is a separate, ordered list of
exact `ModelRef` candidates used only when that step resolves to
`LlmRoute::WyrdGateway`. The Wyrd gateway specification owns the shared
`GatewayFallbackOverride` shape and fallback semantics. Native and external
gateway routes reject this field rather than silently ignoring it.

Local file execution reads the route from the authored YAML. Registered local
execution fetches the same stored route. An `ExtGateway` route resolves its
binding from the executing local or server environment. `WyrdGateway` resolves
no provider secret in Skald: configured gateway credentials remain entirely
behind `wyrd-server`. Server execution accepts `WyrdGateway` and `ExtGateway`
and rejects `Native`. Registration validates the declarative route but performs
no network call and resolves no secret.

### Host registered workflows asynchronously

`wyrd-server` does not currently host Skald Workflow runs. This change adds a
bounded in-memory host around Skald rather than introducing a workflow
scheduler or a second executor. Before acceptance, `wyrd-server` authenticates
and audits the request, resolves the exact registered Workflow/Agent/Prompt
graph, validates it, applies server suitability and admission limits, and
creates a process-local WorkflowRun. It then returns `202 Accepted` and runs
the same Skald DAG executor as tracked process-owned work.

For each Agent step, the host supplies the existing provider registry with the
provider implementation selected by the stored route: the in-process governed
gateway boundary for `WyrdGateway`, or direct bounded egress for `ExtGateway`.
Skald remains unaware of server routes, tenants, and credentials. The server
adds only lifecycle state, authorization, audit, Card resolution, route-owned
dependencies, cancellation, and HTTP projection around it.

This V1 lifecycle is intentionally ephemeral. A run survives the submitting
HTTP connection but not process restart. Status and cancellation require the
owning process, so multi-replica deployments provide request affinity. There is
no database table, durable queue, recovery protocol, ownership lease, or replay
machinery in this change.

### Public WorkflowRun contract

`wyrd-spec` owns the following pure, schema-generating contract. The Rust
declarations below fix public names, fields, optionality, and wire values; they
do not prescribe private module layout or helper methods.

```rust
#[serde(transparent)]
pub struct WorkflowRunId(Uuid); // UUIDv7; serialized as its canonical string

#[serde(rename_all = "snake_case")]
pub enum WorkflowRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
}

#[serde(rename_all = "snake_case")]
pub enum WorkflowStepStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Unstarted,
}

pub struct WorkflowRunError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub details: JsonValue,
    pub remediation: String,
}

pub struct WorkflowStepResult {
    pub status: WorkflowStepStatus,
    pub text: Option<String>,
    pub structured_output: Option<JsonValue>,
    pub attempts: u32,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub error: Option<WorkflowRunError>,
}

pub struct WorkflowRun {
    pub run_id: WorkflowRunId,
    pub workflow: Option<CardRef>,
    pub status: WorkflowRunStatus,
    #[serde(default)]
    pub outputs: BTreeMap<String, JsonValue>,
    #[serde(default)]
    pub steps: BTreeMap<String, WorkflowStepResult>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub error: Option<WorkflowRunError>,
}

#[serde(deny_unknown_fields)]
pub struct CreateWorkflowRunRequest {
    pub workflow: CardRef,
    #[serde(default)]
    pub input: BTreeMap<String, JsonValue>,
    pub timeout_seconds: Option<u64>,
}
```

`WorkflowRunId` is Workflow-specific rather than a relocation or widening of
Vala's existing `RunId`. Local and server runs both mint UUIDv7 identifiers.
`workflow` is absent only for an unregistered local run; a registered local or
server run carries its exact pinned `CardRef`. Deterministic maps use
`BTreeMap`. The portable result does not echo invocation input, provider-native
responses, internal scheduling events, tenant identity, principal identity, or
credentials.

`WorkflowRunError` is not a second error catalog. It is the terminal-snapshot
projection of derive-backed Wyrd error metadata and carries the same stable
code, safe message/details, and remediation. HTTP request failures continue to
use the canonical problem response. Provider, step, cancellation, and total
deadline outcomes are represented by a successfully returned WorkflowRun
rather than converting a later GET into an HTTP error.

Snapshot fields obey these exact invariants:

- `queued` and `running` have no run-level error. `queued` has no start time;
  `running` has one. A step may already carry its own error while the run stays
  `running` only while same-stage peers drain.
- `succeeded` has complete declared outputs, no run error, and only succeeded
  steps. `failed` has empty workflow outputs and exactly one run error;
  individual failed steps retain their own errors. `cancelled` has empty
  workflow outputs and no run error because cancellation is an outcome, not a
  failure. `timed_out` has empty workflow outputs and the exact total-deadline
  error.
- Every terminal run has `ended_at` and no pending or running step. A succeeded
  or failed step has `ended_at`; an unstarted step has neither timestamp.
  Interrupted active steps are cancelled and retain their start/end times.
- A failed step has exactly one error; pending, running, succeeded, cancelled,
  and unstarted steps have none. Portable text or structured output is retained
  only for a succeeded step; failed, cancelled, and unstarted steps carry no
  provider payload.
- `attempts` is the number of step attempts begun, including an attempt that
  fails during binding before provider dispatch. Pending and unstarted steps
  have zero. Running steps and cancelled active steps have at least one.
  Success on the first attempt is one; retry exhaustion is
  `max_retries + 1`.

When same-stage peers fail, the failed step in the earliest topological stage
is the run's primary error; ties use lexicographically smallest step ID, never
wall-clock completion order. Its safe projected error becomes
`WorkflowRun.error`, while every failed peer retains its own step error. A
total-deadline, explicit cancellation, or aggregate-size terminal transition
that wins the atomic terminal compare-and-set determines the run status before
ordinary step-error selection.

Size accounting uses canonical payload bytes rather than allocator size:
`max_input_bytes` counts the JCS UTF-8 serialization of the decoded input map;
`max_step_result_bytes` counts UTF-8 text plus JCS-serialized structured output;
`max_run_bytes` counts the sum of retained step payloads plus JCS-serialized
declared workflow outputs. Metadata and stable errors are outside this payload
budget. Oversized decoded input fails before acceptance. An oversized step
payload is discarded, the step fails with
`WYRD_WORKFLOW_413_STEP_RESULT_TOO_LARGE`, and ordinary primary-error selection
applies. Exceeding the aggregate payload budget discards the candidate payload
that crossed the bound and fails the run with
`WYRD_WORKFLOW_413_RUN_TOO_LARGE`. No oversized provider payload is retained in
the snapshot, log, observation, or error details.

The three HTTP routes use the contract directly without response wrappers:

| Route | Success response |
|---|---|
| `POST /v1/workflow-runs` | `202` plus the queued `WorkflowRun` on first acceptance; `200` plus the current snapshot on same-request replay |
| `GET /v1/workflow-runs/{run_id}` | `200` plus the current `WorkflowRun` |
| `POST /v1/workflow-runs/{run_id}/cancel` | `200` plus the winning terminal `WorkflowRun`; an already-terminal run is unchanged |

The derive-backed public catalog adds these exact codes. Existing permission,
audit, registry, provider, and transport errors remain authoritative where they
already describe the failure.

| Code | HTTP use | Meaning |
|---|---:|---|
| `WYRD_WORKFLOW_404_RUN_NOT_FOUND` | 404 | Unknown, foreign, expired, evicted, non-owning-replica, or process-lost run |
| `WYRD_WORKFLOW_409_IDEMPOTENCY_CONFLICT` | 409 | Same scoped key with a different canonical request |
| `WYRD_WORKFLOW_422_RUN_REQUEST` | 422 | Invalid input, timeout, CardRef, or request shape |
| `WYRD_WORKFLOW_422_ROUTE_UNSUPPORTED` | 422 | Route/protocol combination unsupported in this environment |
| `WYRD_WORKFLOW_422_SERVER_NATIVE_UNSUPPORTED` | 422 | Server graph resolves a `Native` step |
| `WYRD_WORKFLOW_422_SERVER_TOOLS_UNSUPPORTED` | 422 | Server graph contains an Agent with tools |
| `WYRD_WORKFLOW_429_RUN_CAPACITY` | 429 | Global or tenant active-run capacity is exhausted |
| `WYRD_WORKFLOW_503_BINDING_UNAVAILABLE` | 503 | Required execution-environment credential binding is absent or unavailable |
| `WYRD_WORKFLOW_503_RUN_UNAVAILABLE` | 503 | Server is shutting down or cannot accept tracked work |
| `WYRD_WORKFLOW_413_INPUT_TOO_LARGE` | 413 | Decoded Workflow input exceeds the configured input bound |
| `WYRD_WORKFLOW_413_STEP_RESULT_TOO_LARGE` | terminal snapshot | A normalized step result exceeds its configured bound |
| `WYRD_WORKFLOW_413_RUN_TOO_LARGE` | terminal snapshot | Aggregate retained output payload exceeds its configured bound |
| `WYRD_WORKFLOW_504_STEP_TIMEOUT` | terminal snapshot | A step exhausted retries after an attempt timeout |
| `WYRD_WORKFLOW_504_RUN_TIMEOUT` | terminal snapshot | The total Workflow deadline expired |

The shared Rust client exposes one cohesive handle over the existing
`WyrdClient` transport:

```rust
pub struct Workflows { /* shared WyrdClient */ }

impl Workflows {
    pub fn new(client: WyrdClient) -> Self;
    pub async fn create(
        &self,
        request: &CreateWorkflowRunRequest,
    ) -> Result<WorkflowRun, WyrdError>;
    pub async fn get(
        &self,
        run_id: &WorkflowRunId,
    ) -> Result<WorkflowRun, WyrdError>;
    pub async fn cancel(
        &self,
        run_id: &WorkflowRunId,
    ) -> Result<WorkflowRun, WyrdError>;
    pub async fn wait(
        &self,
        run_id: &WorkflowRunId,
    ) -> Result<WorkflowRun, WyrdError>;
}
```

`create` uses the existing idempotent submission transport. `wait` polls once
per second until the run is terminal; dropping the future stops polling and
does not cancel the server run. V1 adds no configurable polling framework.

### One asynchronous execution architecture

Skald has one asynchronous Workflow executor. There is no parallel synchronous
engine and no public Rust `run_blocking` method. Construction, YAML parsing,
contract validation, resolved-graph validation, binding selection, DAG
planning, route compatibility, and snapshot transformations remain
synchronous. Agent/provider calls, gateway calls, retry delay, cancellation,
concurrent step orchestration, server transport, and polling are asynchronous
because they await IO or intentionally compose operations that do.

One concrete dependency owner supplies every route. Native execution reuses
Skald's existing `ProviderRegistry`. WyrdGateway execution uses one narrow
per-call trait because its public and in-process implementations must receive
Workflow-owned fallback and call context that cannot enter `ProviderRequest`.
The two real implementations earn this trait; there is no global registry or
mutable adapter call state. Fields on the dependency owner remain private so
route selection stays on the Workflow plan:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalEndpointProfile {
    Local,
    Production,
}

pub struct WyrdGatewayCall {
    pub request: ProviderRequest,
    pub fallback: Option<GatewayFallbackOverride>,
    pub timeout: Duration,
    pub correlation: WorkflowGatewayCorrelation,
}

pub struct WorkflowGatewayCorrelation {
    pub run_id: WorkflowRunId,
    pub step_id: String,
    pub attempt: u32,
}

#[async_trait]
pub trait WyrdGatewayCaller: Send + Sync {
    async fn call(
        &self,
        call: WyrdGatewayCall,
        cancellation: &CancellationToken,
    ) -> Result<ProviderResponse, ProviderError>;
}

pub struct WorkflowExecutionDependencies {
    /* owned ProviderRegistry, optional Arc<dyn WyrdGatewayCaller>,
       ExternalGatewayBindings, and ExternalEndpointProfile */
}

impl WorkflowExecutionDependencies {
    pub fn new(native: ProviderRegistry) -> Self; // native-only, Local profile
    pub fn with_wyrd_gateway(self, gateway: Arc<dyn WyrdGatewayCaller>) -> Self;
    pub fn with_external_gateways(self, bindings: ExternalGatewayBindings) -> Self;
    pub fn with_endpoint_profile(self, profile: ExternalEndpointProfile) -> Self;
}
```

`wyrd-client` supplies `pub struct PublicWyrdGatewayCaller` with
`pub fn new(client: WyrdClient) -> Self` and implements `WyrdGatewayCaller`;
`wyrd-server` supplies a private concrete in-process implementation. The
in-process implementation converts the typed provider request as today, puts
`fallback` and the remaining `timeout` on `GatewayCallRequest`, passes
`cancellation` as the existing separate `GatewayInvocation::run` argument, and
records Workflow correlation only as scrubbed trace fields. The public
implementation projects the request into the existing protocol-specific
ingress, uses the remaining duration as its client-call timeout, races the HTTP
future with `cancellation`, carries fallback through the authenticated header
below, and propagates correlation only through the existing tracing context.
No run, step, attempt, or cancellation field is added to `GatewayCallRequest`
or a provider wire body. The client adapter is the only new client-to-Skald
dependency and Skald never depends back on `wyrd-client`.
ExtGateway
dispatch remains a concrete Skald runtime path using the named bindings and
the shared endpoint policy. Existing resolved Agent tools and Workflow
observers remain on their current Agent/Workflow owners and are not duplicated
in this dependency bundle.

For each WyrdGateway step attempt, the Workflow executor creates a private
immutable `Provider` adapter containing the shared caller plus that step's
fallback, absolute effective deadline, cancellation token, and correlation,
then supplies it through the Agent's ordinary provider registry. Each Agent
model call computes the deadline's remaining duration and constructs a fresh
`WyrdGatewayCall`; no call is made when no duration remains. The adapter is
never reused by another step, so the existing Agent loop needs no Workflow
awareness and concurrent steps cannot exchange call metadata.

To preserve canonical remote errors through the existing Agent `Provider`
interface, `ProviderError` adds one generic redacted variant:

```rust
RemoteProblem {
    code: String,
    status: u16,
    message: String,
    field: Option<String>,
    remediation: String,
}
```

The public caller parses the protocol-native OpenAI, Anthropic, or Google error
envelope and retains only HTTP status, safe message, stable Wyrd code when the
envelope carries one, and OpenAI's safe `param` as `field` when present. The
in-process caller normalizes its `WyrdError` to that same smallest common shape,
discarding all details except `details.field`. When no Wyrd code is present,
status maps to the existing provider timeout, rate-limit, 5xx, or bad-request
code. Remediation is looked up from the derive-backed Wyrd catalog for a known
code and otherwise uses the existing provider-category remediation.

`ProviderError::code()` consequently returns `&str` rather than requiring every
code to be static. The private per-step `Provider` adapter passes
`RemoteProblem` through unchanged. `WorkflowRunError.details` becomes
`{"field": value}` when `field` is present and `{}` otherwise; its remaining
fields come from the normalized problem. This generic variant carries no
response body, prompt, credential, arbitrary upstream error text, or
protocol-only envelope fields and is not a second error catalog.

For local public-gateway calls, the optional HTTP header is exactly
`wyrd-gateway-fallback`. Its value is URL-safe base64 without padding over the
JCS UTF-8 serialization of `GatewayFallbackOverride`. Absence means no
per-request override. The encoded header is limited to 8 KiB and the decoded
JSON to 4 KiB. After ordinary Wyrd authentication and before entering the
gateway invocation pipeline, each OpenAI, Anthropic, and Gemini public ingress
decodes, deserializes, and validates the header against the requested model,
then assigns it to `GatewayCallRequest.fallback`. Malformed, oversized, empty,
duplicate, or self-referential fallback values fail with the existing
`WYRD_GATEWAY_400_INVALID_REQUEST` and `details.field = "fallback"`; no upstream
dispatch occurs. The header is consumed at ingress and is never forwarded to a
provider. Native Vertex remains unavailable at the public edge.

The header and its encoding are documented on all affected OpenAPI operations
and in the generated Rust contract documentation. Each
`PublicWyrdGatewayCaller::call` builds request headers from its owned
`WyrdGatewayCall`; concurrently executing steps therefore cannot share or
overwrite fallback, deadline, cancellation, or correlation state. Unmodified
gateway clients omit the header and retain tenant-policy behavior.

The local Rust surface preserves the ordinary methods and adds one explicit
bounded form:

```rust
pub struct WorkflowExecutionLimits {
    pub max_concurrency: NonZeroUsize,
    pub deadline: Option<Duration>,
    pub max_input_bytes: Option<usize>,
    pub max_step_result_bytes: Option<usize>,
    pub max_run_bytes: Option<usize>,
}

pub struct WorkflowRunOptions {
    pub limits: WorkflowExecutionLimits,
    pub cancellation: CancellationToken,
}

impl Workflow {
    pub async fn run(
        &self,
        input: impl Into<WorkflowInput>,
    ) -> WorkflowResult<WorkflowRun>;

    pub async fn run_with(
        &self,
        providers: &ProviderRegistry,
        input: impl Into<WorkflowInput>,
    ) -> WorkflowResult<WorkflowRun>;

    pub async fn run_with_options(
        &self,
        dependencies: &WorkflowExecutionDependencies,
        input: impl Into<WorkflowInput>,
        options: WorkflowRunOptions,
    ) -> WorkflowResult<WorkflowRun>;
}
```

The default local path permits eight concurrent ready steps, has no total
deadline, and preserves the existing absence of local input/result-size caps.
`run` and `run_with` construct native-only local dependencies; a graph that
requires another route fails with the stable binding/route error. Callers and
the CLI use `run_with_options` when they require WyrdGateway, ExtGateway,
cancellation, or server-equivalent bounds.
Existing synchronous Python behavior remains a compatibility adapter over the
repository's shared async runtime bridge and the same Skald executor; this
revision adds no new Python method or second execution implementation. The CLI
runs inside its existing Tokio process boundary and awaits the same Rust APIs.

### Workflow retry and deadline contract

One Workflow attempt is one complete Agent-step execution: resolve its already
validated bindings, run the Agent loop to a final response, normalize the
response, and validate the declared output. Provider-internal retries and Wyrd
gateway candidate retry/fallback remain inside that attempt and do not increase
`WorkflowStepResult.attempts`. `max_retries` is the number of additional
Workflow attempts after the first, so the maximum attempt count is
`max_retries + 1`.

A Workflow retry is eligible only for these typed outcomes:

- provider connection failure, provider timeout, provider response decode, or
  provider HTTP 408, 429, or 5xx after provider-internal retries finish;
- `WYRD_GATEWAY_429_LIMIT_EXCEEDED`,
  `WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE`, or
  `WYRD_GATEWAY_504_DEADLINE_EXCEEDED` from a WyrdGateway call;
- the Agent's own timeout, structured-output decode failure, or final response
  validation failure; and
- expiry of the Workflow attempt's `timeout_seconds`.

Authentication, authorization, budget, binding, route, missing-provider,
request-shape, tool, callback, session, journal, max-iteration, cancellation,
and invariant failures are not retryable. A non-retryable failure terminates
the step immediately even when retries remain. The final eligible failure after
retry exhaustion becomes the step error; an outer Workflow attempt timeout uses
`WYRD_WORKFLOW_504_STEP_TIMEOUT`, while an Agent or gateway timeout retains its
own stable projected code.

`initial_backoff_ms: None` and an explicit zero both mean no Workflow-level
delay, preserving today's immediate retry default. Otherwise, before retry
number `r` (one-based), the deterministic delay is
`min(initial_backoff_ms * 2^(r - 1), 30_000 ms)` with saturating arithmetic and
no jitter. Provider-internal retry retains its existing bounded jitter and
`Retry-After` behavior. A Workflow backoff is cancellation-aware and races the
total run deadline; cancellation or deadline expiry during the delay prevents
the next attempt.

`WorkflowStep.timeout_seconds` is a per-attempt wall-clock bound. The attempt's
effective deadline is the earliest of that bound, the Agent Card's existing
run timeout, and the remaining total Workflow deadline. Provider-internal
retry, gateway fallback, Agent tool loops, response normalization, and output
validation all run within that effective attempt deadline. Workflow backoff is
outside the next attempt's step timeout but remains inside the total run
deadline. When boundaries become ready together, an already-signalled explicit
cancellation wins; otherwise total Workflow deadline wins over step-attempt
timeout, which wins over the Agent timeout. No retry begins after cancellation
or total deadline, and the existing atomic terminal compare-and-set still
settles a true completion race.

Retrying a complete local Agent attempt can repeat tool effects performed by a
prior attempt. This is an explicit consequence of authoring `max_retries > 0`;
the runtime provides no compensation or exactly-once tool guarantee. Server
execution rejects Agent tools in V1, so server retries cannot repeat tool
effects. Observer events record every Workflow attempt start and terminal
outcome plus the scheduled backoff duration; provider and gateway internal
attempts remain visible only through their existing observation boundaries.

One parent Workflow future owns a bounded task set for active Skald steps. The
task set MUST abort those step futures when the parent is dropped and MUST be
drained on explicit cancellation or total deadline, so no Skald-owned step task
outlives its Workflow execution. A repository-native Tokio `JoinSet` is the
approved mechanism because its ownership and drop behavior provide that
guarantee. Provider IO MUST occur outside Workflow state locks. Each provider
attempt races the attempt timeout, the total deadline, and the cancellation
token. Dropping a Native or ExtGateway attempt releases its locally owned IO;
dropping a WyrdGateway caller signals the public connection or in-process child
token, while the gateway's separate tracked owner completes bounded provider
cancellation, accounting, capture, and audit settlement.

After the first ordinary step failure, the executor stops scheduling new work,
allows every already-running peer in that dependency-ready stage to finish,
then records remaining steps as `unstarted` and commits one failed run. Explicit
cancellation and the total deadline instead abort active step futures, drain the
owned task set, preserve already-terminal step results, mark interrupted active
steps `cancelled`, mark never-started steps `unstarted`, and return a complete
terminal snapshot. A timed-out step attempt is `failed` after retry exhaustion;
only expiry of the total deadline yields `WorkflowRunStatus::TimedOut`.

### Shared external-egress ownership

The DNS screening, address pinning, redirect refusal, no-proxy behavior, TLS,
and connection/request bounds currently owned by
`wyrd_gateway::EndpointPolicy` move to the existing lower-level
`skald-providers` transport owner. Its consumers are the existing gateway HTTP
dispatch, gateway Vault transport, server admin endpoint validation, their
current boot/test composition sites, and the new local and server ExtGateway
paths. Existing Native OpenAI, Anthropic, Google, and Vertex clients retain
their current transport behavior in this revision. Skald MUST NOT depend on
`wyrd-gateway` or `wyrd-client`, and no second SSRF implementation is
permitted. This is an ownership move within existing Rust crates, not a new
crate or dependency; enabling the existing workspace Tokio dependency's `net`
feature in `skald-providers` is the one permitted manifest change required by
the moved resolver.

The runtime-only credential shape is fixed as follows and is never a Card,
schema-generated wire payload, result, or loggable secret container:

```rust
pub struct ExternalGatewayBinding {
    pub name: CredentialBindingName,
    pub protocol: ExternalGatewayProtocol,
    pub origin: Url,
    pub secret_headers: BTreeMap<HeaderName, SecretString>,
}

pub struct ExternalGatewayBindings {
    /* bindings keyed by CredentialBindingName */
}

impl ExternalGatewayBindings {
    pub fn new() -> Self;
    pub fn insert(&mut self, binding: ExternalGatewayBinding) -> Result<(), WyrdError>;
}
```

The route's `credential_binding` uses the existing
`CredentialBindingName`. The binding origin is the normalized scheme, host,
and effective port; the route may retain its declared base path but MUST match
that origin. A Card-authored header whose case-insensitive name collides with a
secret header is rejected rather than overwritten. Local execution owns an
ordinary `ExternalGatewayBindings` collection. Server composition qualifies
the same binding by verified `DataTenantId`; cross-tenant listing, lookup, or
fallback is impossible. A new resolver trait is not part of V1.

Local CLI configuration extends the existing user-scoped `config.toml`; server
configuration extends `WyrdServerConfig`. `wyrd-spec` owns the shared pure
`ExternalGatewayBindingConfig`, and each configuration owner wraps it for its
environment. Both deserialize secret references, never plaintext values:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalGatewayBindingConfig {
    pub protocol: ExternalGatewayProtocol,
    pub origin: Url,
    pub secret_headers: BTreeMap<String, SecretRef>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalWorkflowConfig {
    #[serde(default)]
    pub external_gateway_bindings:
        BTreeMap<CredentialBindingName, ExternalGatewayBindingConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerExternalGatewayBindingConfig {
    pub tenant: DataTenantId,
    #[serde(flatten)]
    pub binding: ExternalGatewayBindingConfig,
}
```

`GlobalConfig.workflow: LocalWorkflowConfig` owns
`[workflow.external_gateway_bindings.<name>]`. The server's
`ServerWorkflowConfig.external_gateway_bindings` uses the same keyed TOML path,
with the required `tenant` member. The CLI resolves each `SecretRef` when it
constructs local execution dependencies. The server retains the references and
resolves only the selected tenant-qualified binding during run preparation so
rotation is observed without restart. Header-name parsing, forbidden-header
checks, origin/protocol checks, and secret resolution fail before dispatch.
Programmatic Rust callers may construct `ExternalGatewayBindings` directly and
need not serialize this configuration.

### Server run owner and capacity contract

One cohesive server owner holds process-local Workflow runs, idempotency
reservations, tracked tasks, cancellation, limits, retention, and shutdown.
Its state is guarded by one process-local lock because V1 admits at most a
small configured number of runs and every protected mutation is short and
contains no IO. Per-run locks, actors, queues, and distributed coordination are
deferred until measured contention requires them.

The top-level `WyrdServerConfig.workflow: ServerWorkflowConfig` section has the
following exact serialized fields and defaults:

| Field | Default |
|---|---:|
| `default_timeout_seconds` | 1,800 |
| `max_timeout_seconds` | 7,200 |
| `max_concurrency_per_run` | 8 |
| `max_active_global` | 32 |
| `max_active_per_tenant` | 4 |
| `max_retained_global` | 128 |
| `max_retained_per_tenant` | 32 |
| `max_input_bytes` | 1 MiB |
| `max_step_result_bytes` | 1 MiB |
| `max_run_bytes` | 4 MiB |

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerWorkflowConfig {
    pub default_timeout_seconds: u64,
    pub max_timeout_seconds: u64,
    pub max_concurrency_per_run: usize,
    pub max_active_global: usize,
    pub max_active_per_tenant: usize,
    pub max_retained_global: usize,
    pub max_retained_per_tenant: usize,
    pub max_input_bytes: usize,
    pub max_step_result_bytes: usize,
    pub max_run_bytes: usize,
    #[serde(default)]
    pub external_gateway_bindings:
        BTreeMap<CredentialBindingName, ServerExternalGatewayBindingConfig>,
}
```

The required manual `Default` implementation returns the table values, and
container-level `serde(default)` applies them to omitted fields; operators may
override fields under `[workflow]`. It MUST NOT derive zero-valued `Default`.

Terminal retention is fixed at 24 hours rather than exposed as another
operator knob. The default retained-run and aggregate-size bounds cap retained
payloads at approximately 512 MiB before ordinary map and task overhead.
Configuration MUST reject zero timeout, concurrency, active, retained, or byte
ceilings; a default timeout above the maximum; a per-tenant active or retained
ceiling above its matching global ceiling; or a step-result bound above the
aggregate run bound.

### Normative create and replay flow

The following order is part of the security, idempotency, and side-effect
contract:

1. Authenticate and derive tenant and principal exclusively from verified
   credentials.
2. Parse and validate the required `Idempotency-Key`, request body, body-size
   bound, exact Workflow `CardRef`, and requested timeout ceiling.
3. Evaluate `workflows:run` and transactionally append the canonical allow or
   deny audit event. Failure stops the request. Replay performs and audits this
   fresh permission decision.
4. Canonicalize the request with JCS and hash it with BLAKE3. The idempotency
   scope is `(tenant, principal, IdempotencyKey)` and its value is the request
   hash.
5. Under the run-state lock, lazily expire retained terminal runs and inspect
   that scoped key. The same hash for an accepted run returns its current
   snapshot with `200`; a different hash returns the stable conflict; a
   matching request already being prepared waits for that preparation rather
   than resolving or executing a second graph.
6. Construct a tracked preparation future behind a start signal. Under the
   lock, reject shutdown admission and global or tenant active-cap exhaustion,
   then install one internal preparation reservation that consumes one global
   and tenant admission slot and is owned by that future. Release its start
   signal before the handler can next suspend. The reservation is not a public
   run and exposes no additional Workflow status.
7. The tracked preparation future, not the HTTP handler, resolves and pins the
   exact active Workflow/Agent/Prompt graph outside the lock, validates it,
   applies policy and server suitability, resolves the exact tenant ExtGateway
   binding declarations, and enforces input and projected result limits. No
   provider or tool call may occur.
8. On ordinary failure or shutdown cancellation, the preparation owner uses
   one idempotent completion path to remove its reservation and capacity,
   notify all matching waiters, and publish the structured failure without
   creating a run or deduplication entry. A retry then re-enters the ordinary
   flow and may become the next preparer; failed preparations are not cached.
   Cancellation of any waiting HTTP handler only drops that waiter and cannot
   cancel or strand the tracked preparation.
9. Construct the queued snapshot, run cancellation token, pinned execution
   environment, and a tracked executor future held behind a start signal.
   Tracked Tokio task creation is treated as infallible after shutdown
   admission; the plan MUST NOT invent a persistent spawn-failure record.
10. Under the lock, recheck that admission remains open. If shutdown has
    closed it, take the exact-once cleanup path. Otherwise atomically replace
    the preparation reservation with the accepted run and its scoped
    idempotency entry while transferring, not incrementing, the reservation's
    active-cap accounting.
11. Publish the accepted snapshot to every preparation waiter and release the
    executor start signal. The still-connected reservation creator receives
    `202`; matching replay waiters receive `200`, exactly like a replay that
    arrived after acceptance. The tracked tasks own their graph and
    dependencies and cannot borrow from an HTTP request. Creator disconnect
    before or after acceptance, waiter cancellation, or response-decoding
    failure does not cancel preparation or the accepted run. Shutdown signals
    both tracked preparations and active executors and drains their shared
    tracked-task owner; every preparation terminal path releases its
    reservation exactly once and wakes its waiters.

The replay's fresh permission decision is a new audit event because a
permission was evaluated. It does not repeat provider work, create a second run,
or repeat any accepted-run lineage/observation event.

### Normative execution, cancellation, retention, and shutdown flow

The tracked task changes `queued` to `running` only while the stored run remains
non-terminal. It executes the one Skald async engine and applies normalized
step transitions as short atomic snapshot replacements. A transition that
would regress a status or exceed a configured result-size bound is rejected and
terminalizes the run with its stable error. GET therefore observes either the
old complete snapshot or the new complete snapshot, never a partially updated
step.

Completion, ordinary failure, explicit cancellation, and total deadline all
converge on one atomic terminal compare-and-set. The first committed terminal
snapshot wins; later provider results or competing terminal outcomes are
discarded. Normal success/failure reaches this boundary only after its active
step set has drained.

Cancel uses this order:

1. Authenticate, evaluate `workflows:run`, and transactionally audit that
   decision before revealing run existence.
2. Look up the run by verified tenant, verified principal, and run ID. Missing,
   foreign, expired, evicted, and process-lost identities return the same 404.
3. Return an already-terminal run unchanged. Otherwise record cancellation,
   signal its token, and await the tracked executor's bounded abort-and-drain
   path. Disconnecting the cancel request does not revoke the recorded signal.
4. The executor preserves completed results, marks interrupted active steps
   `cancelled`, marks never-started steps `unstarted`, and attempts the one
   terminal compare-and-set. The cancel response returns whichever terminal
   snapshot won the race.

Total deadline follows the same Skald abort, drain, and terminal boundary but
proposes `timed_out`. A normal completion committed first remains successful;
an explicit cancellation committed first remains cancelled. Once Skald-owned
work drains, the Workflow active-run slot may be released. A signalled
WyrdGateway task may remain briefly under the gateway's own bounded admission
and tracked-task owner to settle its authoritative accounting/capture/audit; it
cannot mutate the terminal WorkflowRun, retain a Workflow admission slot, or
cause a Workflow retry.

GET authenticates, evaluates and transactionally audits `workflows:run`, lazily
expires terminal entries, performs the same tenant/principal-qualified lookup,
and returns the current snapshot or common 404. It does not resolve the Card
graph again.

On create, get, and terminal commit, expired terminal entries are removed.
When a tenant retained ceiling is exceeded, that tenant's oldest eligible
terminal entry is removed first; the global ceiling then evicts the globally
oldest eligible terminal entry. Each removal deletes its idempotency entry.
Running or not-yet-drained tracked work is never eligible for eviction.

Shutdown first closes Workflow admission under the state lock, then signals
every preparation token and active-run token, closes the existing tracked-task
owner, and drains all tracked work within the process shutdown budget.
Preparation cleanup releases its reservations and wakes waiters even during
that drain. Process exit drops every run, idempotency entry, binding snapshot,
and preparation reservation. No provider call is resumed or automatically
retried after restart.

## Required behavior

### One declarative workflow model

- **REQ-001:** A runnable workflow MUST be a normal `apiVersion: wyrd/v1`,
  `kind: Workflow` Card. Local execution MUST NOT introduce a second YAML
  format or require a registration wrapper.
- **REQ-002:** A Workflow MUST support one or more Agent steps identified by a
  stable step ID. An Agent step MUST accept an inline Agent spec, an authored
  `path` to an Agent Card, or a versioned Agent `ref` through the existing
  reference model.
- **REQ-003:** Agent Prompts MUST continue to use the existing Agent-to-Prompt
  reference and Skald Prompt binding mechanics. The workflow runtime MUST
  resolve each declared step input to one Prompt variable value and invoke the
  existing Prompt binding API. A Workflow MUST NOT introduce a parallel
  template renderer, system-prompt schema, or provider-request schema.
- **REQ-004:** Direct Prompt and MCP workflow actions MUST NOT be executable
  step kinds. The unimplemented `WorkflowAction::Prompt` and
  `WorkflowAction::Mcp` contract variants and the untyped, unimplemented step
  `condition` field MUST be removed. A Prompt is invoked through an Agent; MCP
  tools are resolved through the Agent tool registry. Conditional branching is
  not part of the requested basic DAG.

### DAG and context semantics

- **REQ-005:** `depends_on` MUST define an explicit acyclic step graph. Step
  declaration order MUST NOT determine dependency order. Step IDs and workflow
  input/output names MUST use the repository's existing validated parameter
  identifier grammar so binding paths are unambiguous.
- **REQ-006:** Steps whose dependencies are satisfied MUST be eligible to run
  concurrently. A dependent step MUST observe a stable snapshot of completed
  dependency outputs; it MUST NOT observe partial or racing writes from its
  peers.
- **REQ-007:** Workflow invocation input MUST be a JSON object. Values declared
  in `WorkflowSpec.inputs` are defaults; invocation values with the same names
  override them only when their JSON type matches the declared
  `ParameterValue` variant. Unknown invocation keys and an `input.<name>`
  binding to an undeclared workflow input MUST be rejected before provider
  execution. This revision reuses the existing default-bearing input contract;
  it does not add a second required-input schema.
- **REQ-008:** `WorkflowStep.inputs` MUST map Prompt variable names to exact
  `WorkflowBinding` source paths. The stable roots are
  `input.<name>`, `steps.<step_id>.output.text`, and
  `steps.<step_id>.output.structured` (including nested JSON fields).
  Bindings MUST NOT perform interpolation, contain surrounding text, compose
  sources, address arrays, or mutate the Prompt. Every unresolved Prompt
  variable MUST have exactly one step binding, and extra bindings that do not
  name a declared unresolved Prompt variable MUST be rejected. Before calling
  the existing Prompt binder, a selected JSON string remains unchanged, null
  becomes the empty string, and every other JSON value becomes compact JSON;
  these are the existing Skald workflow-to-Prompt conversion semantics.
- **REQ-009:** A step MAY reference only its declared transitive dependencies.
  A reference to another step without a dependency path MUST be rejected as a
  hidden dependency rather than silently changing execution order.
- **REQ-010:** Step outputs MUST remain namespaced by step ID. Concurrent steps
  producing the same structured key MUST NOT overwrite one another.
- **REQ-011:** Dependency outputs MUST enter a downstream Agent only through
  its declared step-input bindings. `depends_on` alone MUST NOT implicitly
  prepend provider-specific message history or populate a flat shared
  parameter namespace.
- **REQ-012:** Workflow `outputs` MUST map each output name to one exact
  `WorkflowBinding`. An output may select a workflow input or any declared step
  output; step-level dependency visibility does not constrain this final
  projection. Output selection preserves the selected JSON value's type. A
  succeeded run MUST resolve every declared output. A non-succeeded terminal
  run MUST return an empty workflow-output map and expose partial work only
  through its step results. Runtime order or the last
  lexicographic/topological step MUST NOT implicitly select the final result.

### Validation and execution

- **REQ-013:** The shared pure contract validation MUST reject an empty step
  graph, empty or duplicate step IDs, missing dependencies, self-dependencies,
  cycles, duplicate dependencies, malformed bindings, bindings to inaccessible
  steps or undeclared workflow inputs, and absent or malformed workflow outputs.
- **REQ-013A:** Validation of a resolved Workflow/Agent/Prompt graph MUST reject
  missing or extra Prompt-variable bindings and route/request-dialect
  incompatibility. A syntactically valid nested structured-output field that
  cannot be proven from the Prompt response schema is resolved at runtime and
  MUST fail before the downstream provider call when absent. This cross-Card
  validation MUST NOT introduce IO or Agent/Prompt ownership into `wyrd-spec`.
- **REQ-014:** Pure Workflow contract validation and resolved-graph validation
  MUST both run at the earliest boundary that has the required information:
  local bundle loading, composite Card registration, registered local loading,
  and server acceptance. Execution environments MAY then apply an explicit
  suitability check, including the server's rejection of `Native` and Agent
  tools; they MUST NOT silently reinterpret an otherwise valid graph.
- **REQ-015:** The Skald runtime MUST execute the same validated workflow plan
  locally and on the server when every resolved step route is `WyrdGateway` or
  `ExtGateway`.
  Environment adapters MAY differ only in dependency resolution, gateway
  entry, tool configuration, authorization, audit, observation, and resource
  limits. A server invocation containing `Native` MUST fail validation before
  any step starts.
- **REQ-016:** Per-step timeout and retry declarations MUST be honored in both
  environments according to the exact Workflow retry and deadline contract
  above. A retry MUST be visible in the step result and observer stream. A
  timed-out attempt MUST not continue running in the background. Exhausting
  retries after a step timeout produces a failed step with the exact Workflow
  step-timeout error; the distinct Workflow `timed_out` status is reserved for
  expiry of the total run deadline.
- **REQ-017:** Ready-step concurrency MUST be bounded by the execution
  environment. The server MUST enforce an operator-configured ceiling and total
  run deadline regardless of the workflow declaration. Server configuration
  MUST provide a default run timeout and a maximum accepted
  `timeout_seconds`; omission uses the default. Local Rust callers MUST be able
  to supply equivalent execution limits. Server admission and execution MUST
  also enforce configured global and per-tenant active-run limits plus input,
  step-result, and aggregate WorkflowRun size limits.
- **REQ-018:** On a terminal step failure, already-running peers in the same
  stage MAY finish, but no new step may start. The runtime MUST return a failed
  WorkflowRun containing every completed, failed, and unstarted step rather
  than discard partial results in an error-only return.
- **REQ-019:** Local caller cancellation or an accepted server cancellation
  request MUST stop scheduling new work, cancel active
  provider/tool operations where their interfaces permit it, and return or
  record a distinct cancelled status. Cancellation MUST NOT be reported as a
  provider failure.

### Results and intermediate output capture

- **REQ-020:** Local and server execution MUST expose one semantically
  equivalent WorkflowRun envelope containing a run ID, workflow Card identity
  when registered, workflow status, named workflow outputs, creation/start/end
  timestamps, and a map of step results keyed by step ID. Workflow status MUST
  be the closed set `queued`, `running`, `succeeded`, `failed`, `cancelled`, and
  `timed_out`. Local runs normally enter at `running`; accepted server runs
  enter at `queued`. Expiry of the total run deadline stops new scheduling,
  requests cancellation of active operations, marks remaining steps
  `cancelled` or `unstarted` as applicable, and terminates the run as
  `timed_out`.
- **REQ-021:** Each step result MUST expose status, normalized text and/or
  structured output, attempt count, timing, and a stable error when applicable.
  Step status MUST be the closed set `pending`, `running`, `succeeded`,
  `failed`, `cancelled`, and `unstarted`. A terminal run MUST contain no
  `pending` or `running` step. `unstarted` identifies a step prevented from
  starting by an upstream terminal outcome; there is no `skipped` status
  because conditional branching is not part of V1.
- **REQ-022:** Intermediate step results MUST be retained for the lifetime of
  the WorkflowRun and emitted through the existing Skald observation boundary.
  A server GET while work is active MAY expose only the current run and step
  statuses. Complete normalized step results MUST be present when the run is
  terminal, and every declared workflow output MUST be present when it
  succeeds. Provider-specific raw responses MAY remain a local Rust diagnostic
  but MUST NOT become the portable Workflow wire contract.
- **REQ-023:** Workflow output and step-result ordering MUST be deterministic
  for human and machine rendering of the same completed graph even when
  independent steps finish in a different order. JSON object member order is
  not semantic; consumers MUST address outputs and results by name or step ID.

### Local authoring and execution surfaces

- **REQ-024:** Rust MUST support all three user journeys through the shared
  Skald runtime: load and run an unregistered local YAML bundle; fetch, hydrate,
  and run a registered Workflow locally; and submit, inspect, and cancel a
  registered Workflow run on the server. This revision ships YAML, Rust, HTTP,
  and CLI surfaces only. It adds no Python, TypeScript, or MCP Workflow surface
  and MUST preserve the compilation and existing behavior of retained Python
  Workflow bindings.
- **REQ-025:** Local YAML loading MUST use the shared loader's `path`, `inline`,
  and `ref` semantics. `path` dependencies are loaded from disk without being
  registered. A `ref` requires registry access and MUST fail clearly when no
  registry client is available.
- **REQ-026:** The CLI MUST expose `wyrd workflow run`,
  `wyrd workflow status <run-id>`, and `wyrd workflow cancel <run-id>`.
  `wyrd workflow run` MUST
  accept exactly one source: `--file <path>`, `--uid <uid>`, or the complete
  registered selector `--space <space> --name <name> --version <version>`.
  It MUST accept JSON input from `--input <json>` or
  `--input-file <path>`, and `--execution local|server` with `local` as the
  default. File sources MUST be local; `--execution server` MUST require a
  registered selector. A registered selector with local execution MUST fetch
  and hydrate the locked Card graph before running it in the CLI process. The
  existing `--server <url>` connection option continues to select the Wyrd
  endpoint and MUST NOT be overloaded as an execution-mode flag. Server
  execution MUST print the accepted run ID before polling and wait for a
  terminal result by default; `--detach` MUST return after acceptance so a
  caller can later use `status` or `cancel`. `status` and `cancel` apply only to
  server runs and use the same server/authentication configuration.
- **REQ-027:** CLI success output MUST support machine-readable JSON containing
  the current WorkflowRun snapshot. Human output MAY summarize status and final
  outputs but MUST provide an explicit way to include intermediate step
  results. CLI polling interruption MUST report the already-printed run ID and
  MUST NOT cancel or resubmit the server run. CLI failures MUST retain stable
  Wyrd error codes.

### Registration and server execution

- **REQ-028:** `wyrd apply <path>` MUST register a valid Workflow Card and its
  local Agent and Prompt dependencies through the existing composite
  registration path. Registration MUST remain declarative and MUST NOT execute
  the workflow.
- **REQ-029:** Server execution MUST require a registered, active, exactly
  versioned Workflow Card. The server MUST resolve the locked Agent and Prompt
  graph within the authenticated tenant before starting the first step. An
  accepted run pins that resolved Card graph for its lifetime; later Card
  updates or deactivation do not mutate or implicitly cancel it. Gateway policy,
  deployment eligibility, and credential resolution remain governed at each
  gateway call under the existing gateway contract.
- **REQ-030:** `wyrd-server` MUST expose an asynchronous Workflow-run resource:
  `POST /v1/workflow-runs` accepts an exact Workflow CardRef, JSON-object
  input, optional `timeout_seconds` bounded by the server ceiling, and the
  existing required `Idempotency-Key` header. First acceptance returns
  `202 Accepted` with the queued WorkflowRun snapshot and its run ID;
  `GET /v1/workflow-runs/{run_id}` returns the current snapshot; and
  `POST /v1/workflow-runs/{run_id}/cancel` records cancellation and returns the
  resulting snapshot. Snapshots MUST be internally consistent and state
  transitions MUST NOT regress. Cancellation is idempotent: an already-terminal
  run remains unchanged, and when completion races cancellation the first
  committed terminal transition wins. Runtime failure is a terminal
  WorkflowRun, while request, identity, authorization, resolution, validation,
  admission, and acceptance-audit failures are structured Wyrd errors and
  create no run.
- **REQ-031:** `wyrd-client` MUST own the shared remote invocation method used
  by Rust and CLI, including create, get, cancellation, and polling. Local
  execution MUST continue to use Skald directly; the shared client MAY compose
  registry fetching and Skald hydration but MUST NOT duplicate the workflow
  executor. Create MUST reuse the existing idempotent HTTP submission support
  so transport retry carries one stable key rather than starting duplicate
  provider work.
- **REQ-032:** Create, get, and cancel MUST each authorize the typed
  `workflows:run` permission, backed by a new `Resource::Workflows`, the
  existing `Action::Run`, `PermissionScope::All`, and
  `Permission::workflow_run()` constructor. The built-in `writer` and `agent`
  roles MUST receive it; `admin` continues to cover it through wildcard, while
  `reader` and `runtime_admin` MUST NOT receive it. The server MUST derive
  tenant and actor from verified credentials. Create MUST enforce active Card
  state and invocation policy and fail closed before provider or tool side
  effects when identity, authorization, policy, dependency resolution, or
  acceptance audit fails. Get and cancel MUST authorize the caller's current
  permission and run ownership without re-resolving or revalidating the Card
  graph. Run lookup and cancellation MUST be tenant- and owner-qualified before
  returning state; a foreign tenant, different principal, expired or
  capacity-evicted entry, process-lost entry, and nonexistent run ID MUST be
  indistinguishable as the same not-found result. Every evaluated permission
  decision uses the repository's canonical audit path; polling does not create
  an audit exception.
- **REQ-033:** The server's `workflows:run` acceptance authorization decision
  MUST use the canonical transactional audit path and fail closed before the
  first step when that decision cannot be recorded. Acceptance MUST complete
  this decision, exact Card-graph resolution, validation, and bounded admission
  before returning `202`; only provider execution continues asynchronously.
  Each `WyrdGateway` step then
  receives its own gateway authorization decision and tracked non-blocking
  canonical audit append under the implemented gateway contract. Terminal
  workflow status is a run result and observation, not a second authorization
  audit event. No audit record may contain raw workflow input, prompts,
  credentials, or model output.
- **REQ-034:** A `WyrdGateway` step MUST use only gateway deployments and
  credentials configured for the verified tenant; Skald MUST neither resolve
  nor receive their plaintext. An `ExtGateway` step MUST resolve only its named
  execution-environment credential binding after verifying the binding's exact
  endpoint origin and secret-header contract. Server suitability validation
  MUST reject every resolved Agent whose `tool_names` is non-empty before the
  run is accepted; local execution retains the existing caller-supplied tool
  registry. Invocation input and Workflow Cards MUST NOT contain secret values
  or secret-provider coordinates.
- **REQ-034A:** Accepted server runs MUST execute as tracked process-owned work
  after the creating response disconnects. Process-local state MUST be bounded
  by operator-configured global and per-tenant active and retained-run ceilings.
  A terminal run is queryable until 24 hours after completion or until the
  retained-run ceiling requires oldest-terminal-first eviction; expired entries
  are treated as not found even when physical removal is lazy. Active work MUST
  NOT be evicted to make retention space. Server shutdown cancels active
  Workflow runs and drains them within the existing process shutdown budget. A
  process restart loses every queued, running, and retained terminal entry; no
  automatic resume, replay, or provider-call retry occurs across that boundary.
- **REQ-034B:** V1 server Workflow runs are process-local. A multi-replica
  deployment MUST route create, get, and cancel requests for one run to the
  owning replica through deployment-level affinity. Without that affinity a
  non-owning replica returns the same not-found response. Cross-replica lookup,
  restart survival, and ownership transfer require a later durable run service
  and are outside this revision.
- **REQ-034C:** Create idempotency is process-local and scoped by tenant,
  principal, and the existing typed `IdempotencyKey`. Reusing a key with the
  same canonical request MUST return `200 OK` with the existing run's current
  snapshot and MUST NOT create another run, repeat the original acceptance
  event, or repeat provider execution. The replay still evaluates and audits
  the caller's current `workflows:run` permission. Reusing the key with a
  different request MUST return the stable idempotency-conflict error. The
  deduplication entry expires or is evicted with its run. Restart therefore
  loses both run and deduplication state, consistent with the V1 durability
  boundary.

### Provider and gateway portability

- **REQ-035:** Workflow execution MUST continue to depend on Skald's provider
  abstraction/registry rather than provider-specific workflow branches.
- **REQ-036:** `WorkflowSpec` and `WorkflowStep` MUST each expose an optional
  `llm_route: LlmRoute`. `LlmRoute` MUST be a closed enum with exactly the
  `Native`, `WyrdGateway`, and `ExtGateway { ... }` variants and exact nested
  contracts defined above in this revision.
- **REQ-036A:** `WorkflowStep` MUST expose the optional
  `fallback: GatewayFallbackOverride` field defined by the Wyrd gateway V1
  contract. It is valid only when the resolved route is `WyrdGateway`, is stored
  with the registered Workflow version, and cannot be replaced by invocation
  input. `Native` and `ExtGateway` steps MUST reject it during validation. Each
  WyrdGateway model call MUST carry the stored value through a fresh immutable
  `WyrdGatewayCall`; local execution MUST project it through the exact
  authenticated header above, and server execution MUST place it directly in
  `GatewayCallRequest.fallback`.
- **REQ-037:** Route resolution MUST be
  `step.llm_route -> workflow.llm_route -> LlmRoute::Native`. The resolved
  route MUST be deterministic and retained with the registered Workflow
  version.
- **REQ-038:** `Native` MUST call the native provider declared by the Agent's
  Prompt. A locally executed `WyrdGateway` step MUST call the public
  `wyrd-server` gateway ingress with the environment's Wyrd credential; a
  server-executed step MUST call the same governed invocation pipeline through
  its in-process boundary and verified caller context. Both paths MUST use the
  exact concurrency-safe `WyrdGatewayCaller` interface. Neither path may
  require or expose a gateway endpoint or provider credential in the Card.
  `ExtGateway` MUST call its declared protocol and base URL directly from the
  local or server runtime after resolving the exact configured credential
  binding; it MUST NOT enter `wyrd-gateway`.
- **REQ-039:** `Native` and `ExtGateway` MUST retain their declared request
  protocol. An `ExtGateway.protocol` MUST match the Prompt request dialect and
  MUST be rejected before dispatch when it does not; the workflow runtime does
  not translate external-gateway requests. `WyrdGateway` MUST submit the
  Prompt's typed request and permit only the translations supported by the
  gateway's adapter and operation-capability matrix. OpenAI Chat, OpenAI
  Responses, Anthropic Messages, Google
  GenerateContent, and Vertex GenerateContent MUST work through the route
  combinations the implemented gateway supports; an incompatible or
  unsupported combination MUST fail before upstream dispatch. In V1, a local
  `WyrdGateway` route MUST reject native Vertex GenerateContent because the
  public gateway edge has no Vertex ingress; a server-executed `WyrdGateway`
  step MAY use the existing in-process Vertex dialect.
- **REQ-040:** The Prompt MUST remain the sole owner of provider, model, native
  request fields, tool projection, and response shape. `LlmRoute` selects the
  execution boundary; it does not duplicate those fields or choose a gateway
  deployment or credential.
- **REQ-041:** A registered invocation MUST execute the stored route and MUST
  NOT accept an invocation-input override that could bypass the registered
  routing decision. All execution modes apply the same route-resolution
  precedence; server execution additionally rejects any resolved `Native` step
  before running the graph.
- **REQ-042:** An external gateway route MUST use the exact non-secret header
  and credential-binding contract defined above. Secret values MUST never be
  serialized into a Workflow Card, returned result, error, log, observation,
  audit record, schema example, or generated artifact. The URL MUST reject
  userinfo, fragments, and every query; credentials belong only to the resolved
  binding.
- **REQ-043:** Native and gateway-backed execution MUST preserve the same DAG,
  binding, output, Workflow retry, timeout, cancellation, and observation
  semantics defined above. Provider-internal retry and Wyrd-gateway
  retry/fallback remain route-owned within one Workflow attempt.
  Route-specific transport failures MUST retain stable Wyrd error semantics.
- **REQ-044:** Each `LlmRoute` variant MUST be proven by a credential-free
  registered-workflow journey. `Native` and `ExtGateway` journeys execute
  locally after fetching the locked graph from a real `wyrd-server` and reach
  their respective local mock boundaries. The `WyrdGateway` journey MUST prove
  both local execution through the public gateway ingress and server execution
  through the in-process gateway boundary. Unit or in-process
  provider-registry tests MUST NOT substitute for these journeys.
- **REQ-045:** The public Workflow-run contract MUST use the exact
  `WorkflowRunId`, `WorkflowRunStatus`, `WorkflowStepStatus`,
  `WorkflowRunError`, `WorkflowStepResult`, `WorkflowRun`, and
  `CreateWorkflowRunRequest` shapes defined above. HTTP success bodies MUST be
  the direct `WorkflowRun` value, without a route-specific wrapper, and public
  failures MUST use the listed stable error codes and the derive-backed Wyrd
  error projection. Status-specific field presence, attempt counts, concurrent
  failure selection, and payload-size accounting MUST follow the snapshot
  invariants above.
- **REQ-046:** `wyrd_client::Workflows` MUST expose the exact asynchronous
  `create`, `get`, `cancel`, and `wait` methods defined above. `wait` MUST poll
  once per second until a terminal snapshot; dropping it MUST stop only the
  polling future and MUST NOT cancel the run.
- **REQ-047:** Skald MUST have one asynchronous Workflow execution engine. Pure
  parsing, validation, binding selection, planning, status construction, and
  output projection MUST remain synchronous. Local Rust and CLI execution MUST
  await that engine directly; the retained Python synchronous API MAY use only
  the repository's existing runtime bridge. This change MUST NOT add a second
  synchronous engine, a `run_blocking` Rust API, or an ad hoc async runtime.
  Route dependencies MUST enter through the exact concrete
  `WorkflowExecutionDependencies` interface; they MUST NOT use hidden global
  state or store Workflow call metadata in a shared native `ProviderRegistry`.
  The immutable private per-step adapter described above is the only route
  bridge supplied through an Agent registry.
- **REQ-048:** Each execution MUST own its ready-step futures in one bounded
  task set with the cancellation and drain semantics defined above. Ordinary
  failure waits for already-running stage peers; explicit cancellation, total
  deadline, and shutdown abort and drain Skald-owned active work before the run
  is terminal. No Skald step task may outlive its Workflow executor. A
  WyrdGateway caller MUST signal cancellation but MUST NOT claim ownership of
  or wait for the gateway's separately tracked settlement task.
- **REQ-049:** External endpoint validation and DNS/address pinning MUST have
  one implementation in the existing `skald-providers` transport boundary.
  The exact existing and new consumers enumerated above MUST consume that
  lower-level policy rather than depending on one another or implementing a
  second SSRF check. Existing Native provider transports MUST remain unchanged.
  Runtime bindings and configuration MUST use the exact contracts above; this
  revision MUST NOT add a new crate, resolver trait, or dependency cycle, and
  MAY enable only Tokio's existing `net` feature for the moved resolver.
- **REQ-050:** Server Workflow-run configuration MUST expose the exact fields,
  defaults, and validation rules listed above, with a fixed 24-hour terminal
  retention. Local and server external bindings MUST use the exact config DTOs,
  ownership, secret-resolution timing, and TOML paths above. Server
  create/replay, execution, cancel, GET, eviction, and shutdown MUST follow the
  normative flows above, including tracked preparation ownership, one
  process-local state lock with no IO while held, and one atomic terminal
  compare-and-set.
- **REQ-051:** The Workflow contract change and every existing Skald consumer
  directly affected by it, including the generic remote `ProviderError`
  projection, Agent/provider adapters, and retained Python-feature compilation,
  MUST move as one cohesive buildable change boundary. No implementation task
  may intentionally leave the workspace unable to compile while a later task
  repairs the consumer.

## Invariants and boundaries

- **INV-001:** Skald owns the reusable workflow runtime. `wyrd-server` hosts
  that runtime and may add Card resolution, authorization, audit,
  route-specific dependencies, bounded process-local lifecycle state, and HTTP
  projection. It MUST NOT reimplement Skald's DAG, Prompt binding, retry,
  result, or cancellation behavior. Vala may consume Skald workflows but Skald
  must not depend on Vala or Eval types.
- **INV-002:** `wyrd-spec` owns pure, synchronous, IO-free Workflow contracts,
  validation, schemas, and public errors. It remains PyO3-free.
- **INV-003:** Workflow Cards remain declarative. They contain no live provider,
  registry, server, database, credential, or tool objects.
- **INV-004:** Local `Native` and `ExtGateway` execution remains available
  without registration and does not acquire server tenancy, policy, or audit
  semantics merely because the same definition can also be registered. A local
  `WyrdGateway` step intentionally crosses the server boundary and receives the
  gateway's tenant, authorization, audit, and governance semantics.
- **INV-005:** Registered local and server execution use the exact registered
  Workflow, Agent, and Prompt versions; resolution must not silently float to a
  newer dependency.
- **INV-006:** Server execution preserves tenant isolation across registry
  reads, policy, provider/tool configuration, audit, observations, caches, and
  returned results. Invocation input never selects tenant identity.
- **INV-007:** Every shipped public surface projects the same WorkflowRun and
  stable error semantics. CLI or Rust convenience must not create a competing
  durable contract. This capability's deliberate YAML/Rust/HTTP/CLI rollout
  does not weaken the repository-wide first-class language rule; Python,
  TypeScript, and MCP gain no new Workflow surface in this revision.
- **INV-008:** Local execution may use only the tools declared by the Agent and
  supplied by the caller's existing tool registry. Server execution rejects
  every Agent with declared tools in this revision. A workflow is not an
  arbitrary code or shell execution engine.
- **INV-009:** `LlmRoute` is declarative, versioned Workflow behavior. Runtime
  provider registries implement the stored choice but cannot silently replace
  it.
- **INV-010:** Server `ExtGateway` execution treats the stored base URL as a
  tenant-supplied network target and MUST apply DNS resolution, SSRF screening,
  address pinning, redirect, TLS, and bounded-IO controls before sending a
  secret or request data. Local execution MUST enforce the credential binding's
  exact origin, TLS, redirect, and bounded-IO policy but MAY reach private
  addresses explicitly permitted by local configuration; it does not inherit
  the server production profile's private-address prohibition.
- **INV-010A:** `ExtGateway` is execution-local. Both a local process and
  `wyrd-server` call the declared external gateway directly from the runtime
  executing that step; neither sends the request through the Wyrd gateway.
- **INV-011:** Route selection never mutates the Prompt contract. Any
  provider-dialect translation is gateway-owned, typed, capability-checked,
  and invisible to Workflow DAG and result semantics.
- **INV-012:** Gateway provider credentials never cross into Skald, a Workflow
  Card, invocation input, WorkflowRun, error, log, observation, or audit
  record. Workflow implementation MUST NOT add a gateway credential mutation
  surface.
- **INV-013:** Server WorkflowRun lifecycle state is bounded, process-owned,
  and deployment-affine. It is not a persistent run registry, cannot survive
  restart, and does not claim cross-replica lookup or recovery semantics.
- **INV-014:** Skald's existing Prompt binder is the only interpolation engine.
  Workflow bindings select values and assign them to declared Prompt variables;
  they do not parse or render template text.
- **INV-015:** Before implementation completes, the repository's architecture
  authority MUST be synchronized with this approved revision for the
  capability-scoped surface rollout, execution-local `ExtGateway` meaning, and
  bounded ephemeral server-run lifecycle. The Wyrd gateway authority and
  generated public operations MUST also record the authenticated fallback
  header without changing unmodified-client behavior.
- **INV-016:** There is one Workflow execution algorithm. Async orchestration
  owns IO and task lifetimes; pure work stays synchronous, and synchronous
  language ergonomics remain boundary adapters rather than alternate engines.
- **INV-017:** There is one external-endpoint security implementation below
  both Skald and the Wyrd gateway. A higher application crate MUST NOT become a
  dependency of Skald to share egress policy. Its consumer set is the narrow
  enumerated migration above; Native provider transport is not broadened by
  this change.
- **INV-018:** Accepted work is either represented by one tracked run or not
  accepted. Every preparation reservation is owned by one tracked preparation
  future until atomic promotion or exact-once cleanup. Idempotency entries,
  active accounting, and terminal snapshots MUST not expose a state in which
  duplicate work can start or untracked work can survive.
- **INV-019:** Server run-state locks protect only short in-memory mutations.
  Registry, policy, audit, provider, binding-resolution, polling, and task-drain
  IO MUST occur outside those locks.
- **INV-020:** WyrdGateway fallback, deadline, cancellation, and correlation are
  immutable per-call values. They are never stored in a shared adapter or
  inferred from a Prompt/provider request, and public ingress consumes rather
  than forwards the Wyrd fallback header.
- **INV-021:** Workflow terminalization owns only Workflow and Skald task state.
  A signalled WyrdGateway task remains gateway-owned until its required bounded
  settlement completes and has no capability to mutate the terminal
  WorkflowRun or retain Workflow admission.

## Non-goals

- Python, TypeScript, or MCP workflow invocation surfaces. This change
  intentionally ships YAML authoring, Rust, and CLI only; any later projection
  must reuse the same contracts and runtime/client boundaries.
- A durable workflow queue, scheduler, background job service, persisted or
  cross-replica run registry, ownership transfer, restart recovery, or
  resume/replay service. The bounded process-local lifecycle required above is
  in scope.
- Persisting local or server WorkflowRun payloads beyond existing observation
  and audit integrations.
- Dynamic graph mutation, loops, branches that add steps, map/reduce fan-out,
  human approval pauses, compensation transactions, or nested Workflow steps.
- A new expression language or conditional branching. Bindings select one
  exact value from the declared input and step-output roots; Prompt rendering
  remains owned by the existing Prompt binder.
- Gateway deployment selection, tenant fallback policy, load balancing, spend
  budgets, caching, semantic routing, or translation implementation. A
  Workflow step only declares its ordered `GatewayFallbackOverride`; the Wyrd
  gateway owns those behaviors and their authorization and audit semantics.
- Wyrd gateway credential administration, resolution, protection, or
  persistence. The workflow runtime consumes no Wyrd gateway provider
  credential. External-gateway credential bindings remain execution-environment
  configuration, not Workflow resources.
- A Gateway Card kind, route registry, named route catalog, or mutable
  invocation-time route override.
- Direct Prompt or MCP workflow steps; Agents already own Prompt execution and
  MCP tool use.
- Provider-specific raw responses as a cross-language or server wire contract.
- A second synchronous Workflow executor, public Rust `run_blocking` API, new
  Tokio runtime, or configurable polling subsystem. Existing boundary adapters
  and the fixed client polling interval cover the shipped surfaces.
- A new egress-policy crate, resolver trait, per-run actor, per-run lock,
  internal Workflow queue, or distributed coordination layer. The existing
  provider transport boundary, one bounded task set, and one short-held server
  state lock are sufficient for V1.

## Expensive-to-reverse decisions

1. One Workflow Card drives both local and server execution; there is no
   server-only workflow definition.
2. Workflow steps invoke Agents only. Prompt and MCP composition remains inside
   Agent contracts and runtime registries.
3. Dependencies are explicit; data references never create hidden graph edges.
   A Workflow binding selects one exact context value for one already-declared
   Prompt variable, and the existing Prompt binder remains the sole renderer.
4. Step outputs are namespaced and workflow outputs are explicit. Flat shared
   parameter mutation and last-step-wins output are not public semantics.
5. Server invocation is asynchronous and bounded, but deliberately
   process-local: acceptance creates tracked work, status/cancel operate on the
   owning process, terminal state is retained for up to 24 hours subject to
   configured capacity eviction, deployment affinity is required, and restart
   loses the run. Durable queue, replay, recovery, and cross-replica machinery
   remain deferred until those guarantees are required.
6. Runtime failures return a terminal WorkflowRun with partial results;
   request/auth/validation failures remain structured Wyrd errors.
7. Provider routing is versioned Workflow behavior represented by one closed
   `LlmRoute` enum. Workflow defaults and step overrides use the same type; no
   named route registry exists.
8. The Prompt owns provider request and model semantics; an LLM route selects
   the execution boundary. Only the Wyrd gateway may translate a typed request,
   under its adapter and capability contract.
9. Registered server runs cannot override their stored route. This prevents an
   invocation payload from bypassing a governed gateway.
10. Server workflow execution permits `WyrdGateway` and `ExtGateway` and
    rejects `Native` before the graph starts. Server `ExtGateway` is direct
    bounded egress through an operator-configured credential binding; it does
    not enter `wyrd-gateway`.
11. This capability ships YAML, Rust, HTTP, and CLI only. It does not add
    Python, TypeScript, or MCP Workflow invocation surfaces.
12. Server execution rejects Agent tools in V1. Local execution continues to
    use the existing caller-provided tool registry.
13. Server create uses the existing idempotency-key contract, but deduplication
    lasts only as long as the owning process and retained run.
14. Acceptance pins the resolved Workflow/Agent/Prompt graph. Later Card state
    changes do not rewrite a running job; gateway governance remains live at
    each gateway invocation.
15. The public run IDs, statuses, request/results, stable errors, direct HTTP
    bodies, and `wyrd_client::Workflows` methods are the exact V1 interfaces
    defined in this revision; implementations do not invent wrapper envelopes
    or a second client facade.
16. Workflow execution has one async engine. Pure phases stay synchronous,
    local Rust and CLI await it, and retained Python sync behavior uses only the
    existing runtime bridge. No Rust `run_blocking` surface is added.
17. External endpoint policy moves down to the existing `skald-providers`
    transport boundary for the enumerated gateway, Vault, admin-validation, and
    ExtGateway consumers. Native provider transports remain unchanged; no new
    crate, trait, or upward dependency is introduced, and only Tokio's existing
    `net` feature is enabled for the moved resolver.
18. Server acceptance uses a private preparation reservation and atomically
    publishes a queued run, idempotency entry, and active accounting. A tracked
    preparation future owns the reservation across IO and cleans it exactly
    once on failure or shutdown, independent of HTTP waiter lifetimes.
    Execution owns a bounded task set, holds no state lock across IO, and
    converges on one terminal compare-and-set after required drain.
19. Workflow contract and directly affected Skald consumers form one cohesive,
    independently buildable implementation boundary, including the generic
    remote-provider error projection and Agent adapter, rather than separate
    tasks with a deliberately broken intermediate workspace.
20. WyrdGateway calls use one narrow two-implementation per-call trait. Local
    calls carry the Workflow fallback in the exact authenticated header;
    server calls set `GatewayCallRequest.fallback` and remaining timeout,
    passing cancellation separately. Workflow correlation is trace-only, and
    errors normalize to the smallest protocol-common safe shape. Shared
    mutable adapter context and changes to `ProviderRequest` are prohibited.
21. Workflow retry repeats a whole Agent step attempt, uses the exact typed
    eligibility and deterministic exponential schedule above, and composes
    per-attempt step timeout, Agent timeout, provider-internal retry, and total
    Workflow deadline in the fixed order above. Local tool effects may repeat;
    server tools remain rejected.
22. Workflow cancellation drains only Skald-owned work. It signals a
    WyrdGateway caller boundary, after which the gateway's tracked owner may
    finish bounded provider cancellation and settlement without holding a
    Workflow slot or mutating its terminal snapshot.

## Acceptance criteria and evidence

- **AC-001:** A checked-in code-review Workflow YAML with two independent
  reviewer Agents and one dependent final-review Agent loads and executes
  locally through Rust and CLI, with both reviewers running in the same DAG
  stage and their distinct outputs assigned to the final reviewer's declared
  Prompt variables through the existing Prompt binder.
- **AC-002:** The same YAML bundle registers through `wyrd apply`; its stored
  Workflow relationships point to the exact Agent and Prompt Card versions.
- **AC-003:** The registered code-review Workflow can be fetched and executed
  locally through Rust and CLI with results equivalent in shape and binding
  semantics to unregistered execution.
- **AC-004:** A real Rust client submits the registered Workflow through a real
  `wyrd-server`, receives `202 Accepted`, polls by run ID, and receives final
  plus intermediate results. The matching CLI server invocation exercises the
  same client path, prints the run ID before waiting, and proves both the
  default wait and `--detach` plus later `status` behavior. A simulated lost
  acceptance response proves the client's stable idempotency key recovers the
  existing run without duplicate provider calls. A Card mutation after
  acceptance proves the run continues with its pinned graph.
- **AC-005:** A concurrency-sensitive test proves independent steps overlap and
  a dependent step starts only after all declared dependencies complete.
- **AC-006:** Contract tests prove duplicate/missing/self/cyclic dependencies,
  hidden step references, malformed or inaccessible binding paths, missing or
  extra Prompt-variable bindings, undeclared or type-incompatible invocation
  inputs, binding expressions or surrounding text, invalid output selectors,
  empty outputs, and unsupported actions fail with stable field-specific errors
  before provider execution.
- **AC-007:** A collision test proves two parallel steps can both emit a field
  named `summary` and the final step receives the intended
  `steps.<id>.output.structured.summary` values deterministically.
- **AC-008:** A failure journey proves a terminal step error returns a failed
  WorkflowRun containing completed sibling/intermediate results and distinct
  unstarted downstream steps. Result tests cover every closed run and step
  status and prove a terminal run contains no pending or running step.
- **AC-009:** Server journeys prove unregistered/inactive/cross-tenant Workflow
  refs, under-privileged callers, unresolved dependencies, excessive input,
  excessive concurrency, Agent tools, and deadline expiry are rejected without
  unauthorized provider/tool execution. Cancellation evidence proves no new
  step starts and active operations receive cancellation where supported.
- **AC-010:** Audit evidence proves create, get, and cancel authorization under
  `workflows:run` is recorded through the canonical audit path, correlated, and
  redacted, and that acceptance-audit failure prevents run creation. A workflow
  journey proves each gateway step produces its separate correlated
  gateway-invocation audit decision without creating a terminal-outcome
  authorization event; the gateway's existing evidence remains authoritative
  for non-blocking append, failure observability, and graceful-shutdown drain.
- **AC-011:** Contract and execution tests prove `LlmRoute` serialization and
  the exact step, workflow, and native-default precedence without a named route
  registry.
- **AC-011A:** Contract and execution tests prove a `WyrdGateway` step retains
  and sends its ordered `GatewayFallbackOverride`, while `Native` and
  `ExtGateway` steps reject that field and invocation input cannot replace it.
  Concurrent steps with different overrides prove immutable per-call isolation.
  Public-ingress tests for OpenAI, Anthropic, and Gemini prove exact header
  encoding, parsing, validation, non-forwarding, and assignment to
  `GatewayCallRequest.fallback`; the in-process test proves the direct field.
  Malformed and oversized headers fail with the exact gateway error before any
  upstream request. Caller tests also prove remaining-duration projection,
  separate in-process cancellation, trace-only Workflow correlation, parsing
  of all three native error envelopes, and identical smallest-common
  `WorkflowRunError` fields without inventing a canonical public problem body.
- **AC-012:** Generated Workflow schema, OpenAPI, error catalog, and CLI JSON
  output agree with the Rust source contract and regenerate without drift. The
  affected gateway OpenAPI operations document the optional fallback header
  and its encoding.
- **AC-013:** Focused unit and integration evidence covers pure validation,
  Skald runtime execution, loader hydration, registration, client transport,
  server authorization/audit, and CLI behavior. Real Rust/CLI
  client-to-server user journeys provide the completion evidence; unit tests
  alone are insufficient.
- **AC-014:** Credential-free registered-workflow journeys execute `Native`
  and `ExtGateway` locally after fetching the locked graph from a real server,
  and execute `WyrdGateway` both locally through the public gateway ingress and
  on the server through the in-process gateway boundary. A server journey also
  executes `ExtGateway` through its direct bound egress path. Each returns
  declared workflow and intermediate outputs against deterministic local
  upstreams. A local mixed-route journey proves step-over-workflow precedence;
  a local WyrdGateway journey proves the stored fallback reaches the public
  gateway, and a server journey proves both in-process fallback projection and
  rejection of a graph containing `Native` before execution.
- **AC-015:** Protocol journeys prove OpenAI Chat, OpenAI Responses, Anthropic
  Messages, Google GenerateContent, and Vertex GenerateContent retain their
  typed Prompt contract across supported route/adapter combinations, including
  gateway-owned translation where required. They also prove local
  `WyrdGateway` Vertex refusal and server-internal Vertex success. An
  incompatible route or gateway capability is rejected before the mock
  upstream receives a request.
- **AC-016:** Local and server external-gateway negative evidence proves
  missing bindings, endpoint-origin mismatch, Card-authored secret headers,
  disallowed or rebound server addresses, redirects outside the bound origin,
  reserved outbound headers, and oversized or timed-out responses fail closed
  without credential disclosure. Local evidence also proves an explicitly
  configured private gateway remains reachable.
- **AC-017:** Server journeys prove invocation input cannot replace a
  registered `WyrdGateway` route with `Native` or `ExtGateway`, that gateway
  provider credentials never enter Skald or any workflow-visible artifact,
  and that workflow execution exposes no credential-administration surface.
- **AC-018:** A long-running server journey proves the run continues after the
  submitting HTTP connection closes, can be polled and cancelled by its owner,
  and is indistinguishable from not found to another tenant or principal. A
  lifecycle test proves shutdown cancels and drains Workflow-owned work while
  the gateway drains its own tracked settlement, terminal entries expire or
  are evicted oldest-first at the configured retention bound, and process
  restart does not resume or replay provider calls. Idempotency
  tests prove same-request replay returns the existing run while key reuse with
  a changed request conflicts. Deployment evidence documents request affinity
  as a V1 requirement rather than claiming cross-replica lookup.
- **AC-019:** Contract and client tests prove the exact public types, enum wire
  names, direct HTTP response bodies, stable error mappings, and
  `wyrd_client::Workflows` method behavior. Snapshot cases assert every
  status-specific optional field and attempt count; concurrent peer failures
  select the primary error by stage and step ID; input, step-result, and
  aggregate-run size violations return the exact 413 codes without retaining
  oversized payloads. A dropped `wait` future leaves the accepted run active
  without issuing cancellation.
- **AC-020:** Executor tests prove the configured concurrency ceiling, ordinary
  failure drain, explicit cancellation abort-and-drain, total-deadline drain,
  executor-drop cleanup, and all three routes through the concrete execution
  dependency owner. Deterministic-time tests assert total attempt counts,
  retryable versus terminal classifications, exponential delays and cap,
  cancellation and total deadline during backoff, step/Agent/run timeout
  precedence, provider-internal versus Workflow attempt accounting, and no
  retry after a terminal outcome. A local-tool fixture proves an authored
  Workflow retry may repeat a tool effect; the server suitability test proves
  that risk is rejected there. No Skald-owned step or Native/ExtGateway IO
  future remains after the executor completes. WyrdGateway tests prove caller
  connection/token cancellation, no later WorkflowRun mutation, release of the
  Workflow admission slot, and completion through the gateway's existing
  authoritative cancellation/settlement evidence. Existing Python Workflow
  tests prove the retained synchronous boundary still works without a second
  engine.
- **AC-021:** Concurrent-create tests prove that matching idempotency requests
  share one preparation and one accepted run, conflicting requests fail, and a
  failed preparation releases its reservation and capacity without caching the
  failure. Creator disconnect during preparation does not cancel or duplicate
  it; cancelling a matching waiter does not affect the owner or other waiters;
  shutdown during preparation releases capacity and wakes every waiter exactly
  once. The evidence counts provider calls and accepted-run observations so
  duplicate work cannot hide behind equivalent results.
- **AC-022:** Lifecycle race tests cover completion versus cancel, completion
  versus deadline, GET during snapshot replacement, tenant and global
  oldest-first eviction, and shutdown during queued and running work. Every
  terminal snapshot has no pending or running step, and no state lock is held
  while awaiting IO or task drain.
- **AC-023:** Boundary and security evidence proves all external endpoint paths
  enumerated above consume the one `skald-providers` policy, existing Native
  provider transport remains unchanged, server bindings are tenant-qualified,
  local bindings retain their explicit private-address allowance, config
  secrets resolve only at the stated boundary, and Skald does not depend on
  `wyrd-gateway` or a Wyrd application crate. The boundary check permits only
  the existing Tokio dependency's added `net` feature.
- **AC-024:** The cohesive contract-and-runtime change passes its focused tests,
  retained Python-feature compilation, workspace all-feature lint gate, and
  generated-contract checks before any later server or CLI task begins. No
  accepted task boundary relies on a subsequent task to restore compilation.

## Open material decisions

None in proposed Revision 6. Revision 5 remains the last approved authority;
Revision 6 is draft until its exact text receives explicit human approval. The
new material decisions are the exact public run/client interfaces, one async
execution engine and concrete route-dependency owner, local/server binding
configuration, lower ownership and narrow consumers of shared egress policy,
normative server preparation/acceptance and terminal-state ordering,
deterministic snapshot errors, per-call WyrdGateway fallback projection,
Workflow retry/deadline composition, cross-owner cancellation/drain semantics,
fixed server defaults, and the cohesive contract-plus-consumer implementation
boundary. Any later change to those
decisions, the `LlmRoute` variants, route precedence, server route boundary,
external-gateway credential binding, registered-route immutability, action
kinds, binding roots, Prompt-binding ownership, execution failure semantics,
or asynchronous server lifecycle requires another material spec revision
before implementation planning.

## Revision history

- **Revision 6 — draft (2026-09-20):** Makes the approved direction
  decision-complete for planning. Defines the exact WorkflowRun wire and Rust
  client interfaces and stable errors; selects one async Skald engine with
  owned bounded tasks and existing sync boundary adapters; moves reusable
  endpoint policy into the existing provider transport owner; fixes server
  capacity defaults; specifies create/replay, cancellation, retention, and
  shutdown ordering; and requires the Workflow contract and its direct Skald
  consumers to remain one buildable implementation boundary. Adds focused
  race, idempotency, cleanup, boundary, and public-contract evidence for every
  readiness finding. The readiness re-review further fixed concrete route
  dependency injection and config DTOs, tracked pre-acceptance cleanup,
  deterministic terminal error and attempt semantics, result-size codes, and
  the exact narrow endpoint-policy migration including Tokio networking. The
  second readiness pass added the immutable `WyrdGatewayCall` seam and exact
  public fallback header, plus complete Workflow retry eligibility, backoff,
  timeout precedence, cancellation, tool-effect, and observer semantics. The
  third pass aligned that seam to the gateway's relative timeout, separate
  cancellation, trace correlation, protocol-native error envelopes, and
  gateway-owned post-cancel settlement.
- **Revision 5 — approved (2026-09-20):** Reuses the existing Prompt binder as the
  only renderer and narrows Workflow bindings to exact context-value selection.
  Changes server execution to an asynchronous, tracked, process-local
  WorkflowRun resource with create/get/cancel, bounded terminal retention,
  cancellation, stable create idempotency, `workflows:run` authorization, and
  deployment-affinity semantics—without adding a durable queue, persisted run
  registry, recovery, or replay. Clarifies that
  `ExtGateway` always calls the external gateway directly from the executing
  runtime, rejects Agent tools on the server, and limits this capability to
  YAML, Rust, HTTP, and CLI while preserving existing Python behavior.
- **Revision 4 — approved (2026-09-20):** Replaces the Stage 1 gateway baseline
  with implemented gateway Revision 21. Fixes local `WyrdGateway` execution to
  the public server edge, server `WyrdGateway` execution to the in-process
  governed seam, permits bound `ExtGateway` execution locally and on the
  server, keeps `Native` local-only, delegates typed translation and provider
  credentials entirely to the Wyrd gateway, and separates transactional
  workflow-invocation audit from non-blocking gateway-invocation audit.
- **Revision 3 — draft (2026-09-12):** Aligns governed Workflow execution with
  Wyrd gateway V1 by adding one optional, ordered `GatewayFallbackOverride` to
  `WorkflowStep`. It is valid only for `WyrdGateway`; tenant fallback policy,
  deployment balancing, authorization, and execution remain gateway-owned.
- **Revision 2 — draft (2026-09-11):** Replaces environment-only provider
  routing with the registered `LlmRoute` contract. Adds `Native`,
  `WyrdGateway`, and `ExtGateway` routes, workflow-default and step-override
  precedence, native-protocol preservation, secret indirection, external URL
  security, and representative route/protocol journey obligations. Records the
  verified Stage 1 Wyrd gateway boundary from `gateway-journeys`.
- **Revision 1 — draft (2026-09-10):** Initial proposal after repository
  research. Preserves existing Skald local execution, completes the
  WorkflowSpec-to-runtime path, adds Rust/CLI local registered execution and
  typed client-to-server invocation, namespaces intermediate outputs, and
  leaves gateway routing behind the existing provider abstraction. Expanded
  with the intended multi-agent YAML, local/registered/server CLI journeys,
  and direct-provider versus gateway runtime configuration.

## Authority links

- `AGENTS.md` §§2–12, 14
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md` §§Workflow, Spec-file authoring, Runtime authz
- `architecture/wyrd-doctrine.mdx` §§Service boundaries, Runtime boundary,
  Public surfaces
- `architecture/wyrd-security-posture.md` §§Authorization and policy, Tenant
  and data isolation, Audit integrity and privacy
- `architecture/references/doctrine/architecture-constraints.md`
- `architecture/references/architecture/patterns.md`
- `architecture/references/languages/agent-harness.md`
- `architecture/references/languages/errors.md`
- `architecture/references/languages/rust-core.md`
- `architecture/references/languages/testing-workflows.md`
- `architecture/references/languages/spec-driven-development.md`
- `changes/active/wyrd-gateway-v1/spec.md` Revision 21
