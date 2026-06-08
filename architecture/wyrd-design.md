# Wyrd Design

**Version:** v1

This document is the current design of the Wyrd protocol. It is **stateless**:
it reflects the shape as it stands now. Decision history lives in git
(`git log architecture/wyrd-design.md`).

When this disagrees with `wyrd-protocol.openapi.yaml`, `wyrd-protocol.md`,
`specs/*.yaml`, or the Rust code in `crates/wyrd-spec`, **this file wins**.
Downstream artifacts are brought up to this version in a sync pass.

---

## Doctrine

1. **Cards are independent registry entries.** No card "owns" another. The
   deployment unit is a directory of card YAMLs applied together.
2. **One fact, one owning Kind.** If a field could live in two places, the
   doctrine has a gap. Surface it.
3. **Monitors declare subjects.** Drift, Eval, Audit reference what they
   observe. Subjects do not list their observers. Drift and Eval declare a
   single `subject_ref`; Audit may declare many.
4. **Reactions are Operators. Wiring is Triggers.** Drift/Eval/Policy never
   inline reaction logic.
5. **Service composes for deployment, not observation.** Drift/Eval/Trigger/
   Operator/Audit/Source are peer cards, not Service components.
6. **Enforcement is composed at the surface that enforces it.** Service
   composes Policy for runtime gates. Audit references Policy as evidence.
7. **Wyrd reads. It does not push.** Application code emits to its own
   observability stack with its own SDKs. Wyrd records internal observations
   in `vala`; external data is queried through `Source` cards.
8. **Lineage is server-derived from `*_refs`.** Never authored. Never edited.
9. **Status is server-managed.** Authors never write `status:`.
10. **Every cross-card pointer is a `CardRef`.** No string-typed parents or
    path-typed lookups in the protocol. Path hints belong on
    `ServiceComponent.source` only, where they're an authoring convenience.
11. **Sub-agency is a relationship, not a noun.** An Agent invoking another
    Agent is the sub-agent call. The callee is an `AgentCard`. The caller's
    prompt / runtime expresses the invocation. No `SubAgent` kind.
12. **Tools are runtime names, not cards.** `AgentSpec.tool_names: Vec<String>`
    resolves through the runtime tool registry. MCP servers auto-register
    their tools by name; host tools register themselves. No `Tool` kind.
13. **No event vocabulary on the wire.** "Observation" comes from Drift/Eval/
    Audit; "trigger firing" comes from the `TriggerSource` enum. Free-form
    event-name strings are doctrine drift.
14. **Harness-host config does not belong on Cards.** Permission modes,
    sandboxes, isolation, effort, and per-CLI compatibility are properties of
    the host that runs the Agent, not of the Agent contract.
15. **Monitors are pure observation producers.** Drift and Eval describe what
    is observed and what counts as an observation. They do not carry
    scheduling and they do not carry dispatch. Scheduling lives on
    `Trigger.schedule`. Dispatch lives on `Operator`. There is no `Alert`
    kind — alerting is an Operator with a notification adapter.
16. **Heavy cards anchor lineage; light cards are spec-only.** Model, Data, and
    Experiment carry durable artifact bytes and MUST be pre-registered before
    anything else can point at them — they're the lineage anchors. Every other
    kind (Prompt, Agent, Eval, Policy, Trigger, Operator, Source, Mcp,
    Workflow, Audit, Service) is spec-only: `wyrd apply -f file.yaml` reads
    and registers in one move. No separate storage step, no programmatic
    registration prerequisite.
17. **Light cards may inline in place of a `CardRef`.** Wherever a `CardRef`
    points at a light card and the inline target has no need for cross-spec
    identity, the parent spec MAY embed the full definition instead. Today
    `Agent.prompt` and `EvalTask::Judge.prompt` accept `PromptRef = CardRef |
    Inline`. Inline definitions have no card identity, are not registered
    standalone, and cannot be referenced from outside their parent. To reuse,
    register as a card and reference by `CardRef`. Heavy refs (`subject_ref`,
    `dataset_ref`, `Service.components.ref`, `Workflow.steps.target`) stay
    `CardRef`-only — identity is the point.
18. **Auth, Policy, and Emit are three distinct planes.**
    - **Auth** gates Wyrd API calls: `Scope` on the handler, stateless pubkey
      verify of the access token. Answers "is this principal allowed to hit
      this Wyrd route?"
    - **Policy** gates card states (`classify` at register-time, `gate` at
      deploy-time) and cross-service invokes (`invoke` at runtime). Runtime
      invoke evaluation is centralized at `POST /v1/authz/check`, called
      transparently by the service mesh's ext_authz filter or by the SDK
      middleware in non-mesh shops.
    - **Emit** is the data-plane channel from a deployed service to Wyrd's
      ingest, signed with the per-card governance token; never propagated
      between services and never read by Policy CEL.

    A deployed Service card auto-provisions a service account at registration
    (named in `spec.service_account`) and receives a card-bound API key
    returned once in the response. The pipeline injects `WYRD_API_KEY` into
    the pod; the SDK exchanges it once at startup for a short-lived JWT
    carrying the card's `card_ref` claim. On cross-service calls the SDK adds
    `Wyrd-Caller-Identity: Bearer <jwt>` — the application's `Authorization`
    header is never touched. The mesh's ext_authz filter (or the SDK
    middleware) authenticates itself to `/v1/authz/check` with its own
    card-scoped JWT and forwards the caller's JWT plus the original
    method/path as headers; the body is empty. Both identities are
    server-verified from signed claims.

---

## Kind catalog

16 native kinds + `External { name, schema_hash }` for forward-compat.

| Domain        | Kinds |
|---------------|-------|
| Data plane    | Data, Model, Artifact, Experiment |
| Agent plane   | Prompt, Agent, Workflow, Mcp |
| Composition   | Service |
| Governance    | Policy, Audit |
| Observability | Drift, Eval, Source |
| Reaction      | Trigger, Operator |

---

## Per-kind specs

Compact view. Field shape only; type details (`DataInterface`, `ModelInterface`,
`DriftProfile`, etc.) live in the OpenAPI contract.

### Data
Dataset declaration with typed interface and schema.
```yaml
spec:
  interface: DataInterface       # Pandas | Polars | Arrow | Parquet | Numpy | Torch | Sql | Jsonl | Image | Text | Huggingface | Custom
  schema: DataSchema
  artifact_refs: [CardRef]       # → Artifact
  splits: { SplitName: DataSplit }
  target_columns: [ColumnName]
  sql?: SqlLogic
  stats: DataStats
```

### Model
ML/LLM model declaration with framework interface and signature.
```yaml
spec:
  interface: ModelInterface      # Sklearn | Xgboost | Lightgbm | Catboost | Torch | TorchScript | Onnx | Huggingface | Custom
  task_type: TaskType            # BinaryClassification | MultiClassClassification | Regression | Generation | Embedding | Custom
  signature: ModelSignature
  sample_input?: SampleInput
  artifact_refs: [CardRef]       # → Artifact
```

### Artifact
Durable bytes record. Pointer to bytes, not the bytes themselves.
```yaml
spec:
  artifact_kind: string
  artifact_uris: [string]
  content_type?: string
  size_bytes?: u64
  integrity?: string             # digest
  schema_ref?: CardRef
  framework_adapter?: FrameworkAdapterRef
  external_uri?: string          # for upstream registries (MLflow, etc.)
  metadata: { string: NonSecretValue }
```

### Experiment
Run grouping for comparison and lineage.
```yaml
spec:
  description?: string
  experiment_type?: string
  target_refs: [CardRef]
  default_parameters: { string: ParameterValue }
  run_refs: [RunRef]
  summary_metrics: [MetricEntry]
  best_run_ref?: RunRef
  artifact_refs: [CardRef]
  details: { string: NonSecretValue }
```

### Prompt
Provider-specific request shape, flat-flattened from Skald.
```yaml
spec:
  # Flattened Skald Prompt: provider, model, messages, variables, response_format, ...
```

### Agent
Agent contract: prompt + tools + run config. Tool names resolve through the
runtime tool registry (host tools + MCP server registrations). Approval,
per-tool blocks, and hook gates live on `Policy`, not here.
```yaml
spec:
  prompt: PromptRef              # CardRef (→ Prompt) or inline PromptSpec
  tool_names: [string]
  run_config: AgentRunConfigSpec # max_iterations, tool_concurrency_cap, session_recent_limit, timeout_ms
```

### Workflow
DAG of steps invoking other cards.
```yaml
spec:
  description?: string
  inputs: { string: ParameterValue }
  steps: [WorkflowStep]
  outputs: { string: json }
  governance?: Governance
  details: { string: NonSecretValue }
```

### Mcp
MCP server registration. The server enumerates its own tools at runtime; we do
not shadow them as cards.
```yaml
spec:
  description?: string
  server_name: string
  transport?: string             # stdio | http | sse
  scopes: [string]
  credential_refs: [CredentialRef]
  details: { string: NonSecretValue }
```

### Service
Runtime composition for deployment. **Components are runtime-aliased only.**
Drift/Eval/Trigger/Operator/Audit/Source are peer cards in the deployment
directory, not Service components.
```yaml
spec:
  description?: string
  service_type?: string          # agent | workflow | api | mcp | observability-only sources etc.
  components: [ServiceComponent] # { alias, ref, source?, config, credential_refs }
  entry_point?: string
  deployment: { string: NonSecretValue }
  runtime?: ServiceRuntime       # { kind, framework?, mode?, strict?, config, policy }
  service_config: { string: NonSecretValue }
  credential_refs: [CredentialRef]
  service_account: string        # SA name. Wyrd auto-provisions the SA at registration and
                                 # issues a card-bound API key returned ONCE in the response.
                                 # The SDK exchanges this key at startup for the JWT that
                                 # carries the card's `card_ref` claim (see Runtime identity).
  content_hash?: string
  lock_hash?: string
  metadata: { string: NonSecretValue }
```

### Policy
Declarative governance rules. CEL-evaluated. Three lifecycle phases share one
rule shape; the `action` field on each rule says when it fires:

  - `classify` (register-time): rule derives attrs onto the card (e.g.
                                `risk.tier = "high"`). Never blocks.
  - `gate`     (deploy-time):   rule allows/denies governance-token issuance
                                for the card. Blocks when Deny.
  - `invoke`   (runtime, per cross-service call): evaluated by
                                `POST /v1/authz/check`. Returns Allow/Deny to
                                the mesh ext_authz filter or the SDK
                                middleware. Developers never write enforcement
                                code.

Composition: `org_global ∪ service_local`, deny-overrides. Service-local can
only tighten. CEL parse + evaluation is owned by the Stage-5 enterprise
engine; `wyrd-spec` enforces only `CelExpression` transport invariants
(non-empty, ≤4096 chars, no control chars).

```yaml
spec:
  description?: string
  rules: [PolicyRule]            # { name, expression: CelExpression, action: PolicyAction, metadata }
  enforcement?: Enforcement       # active | inert. Default: active.
  scope: PolicyScope              # org_global | service_local. Default: service_local.
  target?: PolicyTarget           # selector for org_global; absent for service_local
  details: { string: NonSecretValue }
```

Closed enums:
- `PolicyAction = Classify | Gate | Invoke`
- `Enforcement = Active | Inert`
- `PolicyScope = OrgGlobal | ServiceLocal`
- `PolicyTarget = { spaces: [SpaceName], kinds: [CardKind], actions: [PolicyAction] }`
- `PolicyDecision = Allow | Deny { reason: string }`

### Runtime identity

A deployed Service card runs as the service account named in
`spec.service_account`. At registration, Wyrd idempotently creates the SA
(least-privilege default role) and issues an API key bound to the card,
returned ONCE in the registration response. The CI/CD pipeline writes the key
directly to the shop's secret store — no human paste.

Env vars in deployed services:

| Env var | Required? | Source | Used for |
|---|---|---|---|
| `WYRD_API_KEY` | REQUIRED | CI writes from registration response | Exchanged ONCE at startup at `POST /auth/token` for short-lived JWT (~15m). SDK auto-refreshes. JWT carries `card_ref` claim. |
| `WYRD_API_URL` | REQUIRED | Static config | Wyrd server base URL. |
| `WYRD_GOV_TOKEN` | OPTIONAL | CI writes from `wyrd gov-token issue` response | Only if the app calls `wyrd.observe(...)`. |

The API key is exchanged at startup — never on the wire. The JWT — not the
API key — is what travels on cross-service calls in the dedicated
`Wyrd-Caller-Identity: Bearer <jwt>` header. The application's own
`Authorization` header is never touched by the SDK.

```
POST /charge HTTP/1.1
Host: billing-svc.acme.svc.cluster.local
Wyrd-Caller-Identity: Bearer <Service-A's JWT>     ← SDK adds; carries caller's card_ref
Wyrd-Request-Id:      <ULID>                       ← SDK adds; request correlator
Authorization: Bearer <app's own JWT>              ← app's own auth; Wyrd never reads
Content-Type: application/json

{ "amount": 100 }
```

#### `Wyrd-Request-Id` — request correlator

A Wyrd-owned, request-scoped opaque ID that joins every hop of a logical
request. It is the sole correlator for policy ancestry and audit replay —
Wyrd does not depend on `traceparent`, mesh tracing, or any external
propagation contract.

Contract:

- Opaque ULID minted by Wyrd at first sighting (no inbound
  `Wyrd-Request-Id` at `/v1/authz/check`).
- Propagated unchanged by Wyrd SDK middleware and ext_authz on outbound
  calls. Never mutated, never re-minted mid-request.
- Every Wyrd-emitted observation carries it as a label.
- Ancestry of any request (service1 → service2 → service3) is
  reconstructable by joining observations on this ID; per-hop caller
  identity comes from the verified `Wyrd-Caller-Identity` JWT at each
  call.

Storage tier, query API, and CEL surface (e.g. a `chain.*` binding) are
implementation concerns deferred to the runtime stage.

### Runtime authz: `POST /v1/authz/check`

The single CEL evaluation surface for `PolicyAction::Invoke`. Two delivery
paths, identical semantics:

- **Service mesh (ext_authz).** The mesh's local Envoy/Istio sidecar
  intercepts the inbound request transparently (iptables redirect, standard
  k8s/Istio behavior), and the configured ext_authz filter calls Wyrd's
  `/v1/authz/check`. The developer's application code makes a normal HTTP
  call — it does not address Envoy explicitly. One-time platform-team filter
  config covers every workload.
- **SDK middleware (non-mesh).** Identical semantics in-process. One-line
  developer install (`app.add_middleware(PolicyMiddleware)`). The middleware
  reads `Wyrd-Caller-Identity` from the inbound request and calls the same
  `/v1/authz/check` route.

The check is **headers-only**. Body is empty. All inputs are headers, which
matches how Envoy's ext_authz filter natively forwards data — zero
translation logic on either end.

```
POST /v1/authz/check HTTP/1.1
Host: wyrd.acme.com
Authorization:         Bearer <middleware/sidecar's own JWT — its card identity>
Wyrd-Caller-Identity:  Bearer <caller's JWT — forwarded from the original request>
Wyrd-Request-Id:       <ULID — forwarded from inbound, or absent on first hop>
X-Original-Method:     POST
X-Original-Path:       /charge
Content-Length: 0
```

Wyrd:
1. Verifies `Authorization` JWT → builds `callee` from claims (`card_ref`,
   `actor`, `scopes`).
2. Verifies `Wyrd-Caller-Identity` JWT → builds `caller` from claims.
3. Reads `X-Original-Method` / `X-Original-Path` → builds `request`.
4. Reads `Wyrd-Request-Id` if present; mints a fresh ULID if absent and
   echoes it back so the middleware/sidecar can inject it on the outbound
   call.
5. Assembles `InvokeContext { caller, callee, request, attrs }` (attrs are
   merged Classify-derived attributes from caller + callee cards).
6. Evaluates CEL rules where `action == invoke` for the callee card
   (org-global ∪ service-local, deny-overrides).
7. Returns `200 OK` (Allow) or `403 Forbidden` with `PolicyDecision::Deny { reason }`.
8. Asynchronously emits one `PolicyInvokeDecision` observation per check,
   labeled with `Wyrd-Request-Id` (signed with Wyrd internal authority —
   no caller/callee gov-token consumed). Every allow and every deny is
   audited automatically; no developer wiring.

Both identities are server-signed and verified from claims. The pod cannot
self-assert its identity — no env var, no body field, no header carries
identity data the pod authored.

### Audit
Governance attestation; references the policies and subjects in scope.
```yaml
spec:
  description?: string
  subject_refs: [CardRef]
  policy_refs: [CardRef]
  evidence_refs: [CardRef]
  source_refs: [CardRef]         # → Source — durable evidence stream
  details: { string: NonSecretValue }
```

### Drift
Observation producer for a single subject. Envelope is orthogonal: subject +
signal + condition + math. No scheduling, no dispatch. Scheduling is a
`Trigger`; dispatch is an `Operator`.
```yaml
spec:
  description?: string
  method: DriftMethod            # Spc | Psi | Custom | Agent | External
  subject_ref: CardRef           # → Model | Agent | Service | Data — singular
  signal: DriftSignal            # how the measurement enters the monitor
  condition: DriftCondition      # when a sample becomes an emittable observation
  profile?: DriftProfile         # method-specific math config (PSI bins, SPC window, etc.)
  details: { string: NonSecretValue }
```

`DriftSignal` is a closed enum:

| Variant         | Carries                                          | Use |
|-----------------|--------------------------------------------------|-----|
| `Distribution`  | `baseline_ref: CardRef` (→ Data), `features: [string]` | PSI / SPC over a baseline dataset |
| `Metric`        | `name: string`                                   | Named scalar from subject runtime (mae, p99_latency_ms, tokens_per_call, cost_per_run_usd) |
| `EvalScore`     | `eval_ref: CardRef` (→ Eval)                     | Score stream from an Eval card — the typed Eval↔Drift bridge |
| `External`      | `source_ref: CardRef` (→ Source)                 | Measurement from an external system (Prometheus, OTel) |

`DriftCondition` is a closed enum — one comparator vocabulary, no separate
"baselined" shape (baseline + delta resolves to `Outside { lower, upper }` at
authoring; the card stores resolved bounds):

| Variant         | Carries                          | Fires when |
|-----------------|----------------------------------|------------|
| `Statistical`   | —                                | The method's profile decides (PSI threshold, SPC sigma) |
| `Above`         | `limit: f64`                     | Sample > `limit` |
| `Below`         | `limit: f64`                     | Sample < `limit` |
| `Outside`       | `lower: f64`, `upper: f64`       | Sample < `lower` or > `upper` |

### Eval
Behavioral assessment workflow for a single subject. Envelope is orthogonal:
**what** is judged (`subject_ref`), **how** (`tasks` DAG), **where to read its
observations from** (`source_ref`), and an optional **offline driver**
(`dataset_ref`). No scheduling, no dispatch, no fire condition — fire lives on
`Drift` with `DriftSignal::EvalScore`. Lifted from Scouter's
`AgentEvalProfile`; collapses the parallel `EvalType`+`EvalProfile` enums into
a single typed task workflow.
```yaml
spec:
  description?: string
  subject_ref: CardRef           # → Agent | Workflow | Service | Model — WHAT is judged
  tasks: [EvalTask]              # evaluation workflow — DAG via depends_on
  dataset_ref?: CardRef          # → Data — offline scenario driver
  source_ref?: CardRef           # → Source — WHERE Wyrd reads observations
  governance?: Governance
  details: { string: NonSecretValue }
```

**Three modes the same shape supports** (no `eval_mode` discriminator; presence
of refs is the mode):

| `dataset_ref` | `source_ref` | Runtime behavior |
|---------------|--------------|------------------|
| set           | unset        | Offline batch. Engine invokes `subject_ref` against the Data card's scenario rows, captures traces inline. |
| unset         | set          | Online / archived. Engine reads the user's sink, filters records by subject identity, samples records into the task workflow. |
| set           | set          | Same tasks, both modes — Scouter's "define once, reuse everywhere." Offline gate and online monitor share one task definition. |
| unset         | unset        | Online over `vala`'s default observation archive. |

**Directional flow.** All three refs are `CardRef`s authored on `Eval`; nothing
points back. At runtime: engine resolves `subject_ref` (identity filter),
resolves `source_ref` (read location), opens the Source, queries records,
feeds them into the `tasks` workflow, aggregates per-task pass/fail into a
score stream consumed downstream by a `Drift` card with
`DriftSignal::EvalScore`.

`EvalTask` is a closed tagged union (lift of Scouter's four task types). Each
variant carries `id: string`, optional `depends_on: [string]` (DAG edges), and
`condition: bool` (conditional gate — short-circuit downstream when this fails):

| Variant          | Variant-specific carries                                                                          | Use |
|------------------|---------------------------------------------------------------------------------------------------|-----|
| `Assertion`      | `context_path?: string`, `operator: ComparisonOperator`, `expected: ParameterValue`, `description?: string` | Deterministic check on a dot-path into a record |
| `Judge`          | `prompt: PromptRef` (`CardRef` → Prompt OR inline `PromptSpec`), `operator: ComparisonOperator`, `threshold: ParameterValue` | LLM judge: one Prompt per task, judge score compared to threshold |
| `TraceAssertion` | `span_property: string`, `operator: ComparisonOperator`, `expected: ParameterValue`               | OTel span property (tokens, duration_ms, retry_count, …) read via `source_ref` |
| `AgentAssertion` | `check: AgentCheckKind`, `expected: ParameterValue`                                               | Tool-call / response-shape check (`tool_called` \| `tool_args` \| `response_format` \| `step_count`) read via `source_ref` |

`ComparisonOperator` is a closed enum:
`eq | neq | gt | gte | lt | lte | contains | matches | exists`.

`EvalScenario` is the row shape carried by a `Data` card bound to
`dataset_ref` (not an Eval field — scenarios and datasets are the same noun):
```yaml
- id: string
  initial_query: string
  predefined_turns?: [string]      # scripted multi-turn
  simulated_user_persona?: string  # interactive driver
  termination_signal?: string
  max_turns?: u32
  expected_outcome?: string
  tasks?: [EvalTask]               # scenario-local tasks (passenger view: final response)
  metadata?: { string: NonSecretValue }
```

Scenario-local `tasks` are the **passenger view** (judged against the agent's
final response for that scenario); top-level `Eval.tasks` are the **mechanic
view** (judged against intermediate sub-agent records / spans / tool calls).
Both run in one pass — Scouter's scenario-vs-workflow split lifted intact.

### Source
Read-side reference to external data system. **Wyrd reads, never writes.**
```yaml
spec:
  kind: object_store             # v1: object_store only
  uri: string                    # s3:// | gs:// | az:// | file://
  format: parquet | jsonl | arrow_ipc | csv
  credential_ref?: CredentialRef
  defaults: { string: NonSecretValue }
```

### Trigger
Fires an Operator. A Trigger declares when (`schedule`), what to evaluate
(`source`, optional), and what to fire (`operator_ref`). On each schedule
tick, the server evaluates the source if present; if its condition matches
(or no source is declared), the operator fires.
```yaml
spec:
  description?: string
  schedule: { cron: string, tz?: string }   # required — IANA tz name, default UTC
  source?: TriggerSource                     # closed tagged union — see below
  operator_ref: CardRef                      # → Operator (the only valid target kind)
```

`TriggerSource` is a closed tagged union (snake_case `kind` discriminator):

| Variant | Variant-specific carries        | Server does on each schedule tick                                              |
|---------|---------------------------------|--------------------------------------------------------------------------------|
| `Drift` | `drift_ref: CardRef` (→ Drift)  | Evaluates the Drift. Condition match → fire `operator_ref`. Else record metric. |
| `Eval`  | `eval_ref: CardRef` (→ Eval)    | Runs the Eval. Any task failure → fire `operator_ref`. Else record scores.     |

If `source` is omitted, the operator fires unconditionally on every schedule
tick (cron-driven webhook or workflow dispatch with no monitor gate).

vala chooses the evaluation strategy (windowed PSI/SPC compute, per-record
aggregation, threshold-on-latest) based on the (signal, condition) pair of
the referenced card. The card schema does not declare strategy — it's
implementation.

External pushes are deliberately not a Trigger source — Rule 7 ("Wyrd reads,
it does not push") means external signals enter through a `Source`, are read
by a `Drift` with `DriftSignal::External { source_ref }`, and fire through
`source.Drift` like any other drift.

### Operator
Fires when a Trigger references it. Performs exactly one action — a Workflow
dispatch, a typed notification, or a generic HTTP call — gated by optional
Policy hooks before and after.
```yaml
spec:
  description?: string
  action: OperatorAction          # closed tagged union — see below
  pre_invoke?: [CardRef]          # → Policy, runs before action
  post_invoke?: [CardRef]         # → Policy, runs on action result
  budget?: { max_wall_seconds?: u32, max_tool_calls?: u32 }
```

`OperatorAction` is a closed tagged union (snake_case `kind` discriminator):

| Variant    | Variant-specific carries                                                              | Server does                                                                   |
|------------|---------------------------------------------------------------------------------------|-------------------------------------------------------------------------------|
| `Workflow` | `workflow_ref: CardRef` (→ Workflow)                                                  | Dispatches the Workflow with the firing context as entrypoint payload.        |
| `Notify`   | `channel: NotifyChannel` (closed tagged union — typed vendor shape)                  | Sends the notification through the vendor-specific adapter the server owns.   |
| `Http`     | `method`, `url`, `headers?`, `body?`, `auth?`, `timeout_seconds?`, `expect_status?` | Builds and sends the HTTP request; records response code and latency in vala. |

`NotifyChannel` (v1 set; closed tagged union; additional channels are
protocol-versioned additions):

| Channel     | Carries                                                                       |
|-------------|-------------------------------------------------------------------------------|
| `PagerDuty` | `routing_key_ref: CredentialRef`, `severity`, `summary`, `dedup_key?: string` |
| `Slack`     | `webhook_url_ref: CredentialRef`, `text: string`                              |

`HttpMethod`: closed enum — `Get | Post | Put | Patch | Delete`.

`HttpAuth` (closed tagged union):
- `None`
- `Bearer { credential_ref: CredentialRef }`
- `Basic { credential_ref: CredentialRef }`
- `Header { name: string, credential_ref: CredentialRef }` (covers `X-API-Key`,
  `Authorization: token <foo>`)

`HttpBody`: structured JSON (`JsonValue`). Any string leaf may contain
`{{...}}` placeholders the server interpolates at fire time. Same templating
applies to `Http.url` and to text fields in `NotifyChannel` variants.

Templating context comes from the Trigger that fired the Operator:
- `Trigger.source = Drift { drift_ref }`: `drift.{name, subject_ref.{kind, name, version}}`, `observation.{value, threshold, fired_at}`.
- `Trigger.source = Eval { eval_ref }`: `eval.{name, subject_ref.*}`, `failures[]` (per-task failure entries).
- `Trigger.source` absent: `schedule.fired_at` only.

Exact field schema for each context lives in OpenAPI.

---

## Foundations

Shared types embedded into specs. Owned by `wyrd-spec`.

| Foundation          | Purpose |
|---------------------|---------|
| `CardRef`           | `{ kind, name, version, space?, uid? }` — the only authored pointer between Cards |
| `PromptRef`         | Two-variant tagged union: `Card(CardRef → Prompt)` \| `Inline(PromptSpec)`. Inline has no card identity and cannot be referenced from outside its parent. Used by `Agent.prompt` and `EvalTask::Judge.prompt`. |
| `Governance`        | Compliance metadata: approvals, residency, retention |
| `FrameworkAdapterRef` | `{ name, version?, config }` — runtime adapter binding (MLflow, Skald, LangGraph, etc.) |
| `CredentialRef`     | `{ provider, name }` — provider+name lookup at runtime |
| `NonSecretValue`    | Tagged union for typed non-secret config values |
| `ParameterValue`    | Tagged union for typed parameter values |
| `MetricEntry`       | `{ name, value, ... }` — surfaced metric record |
| `AgentInterface`    | Closed enum: model/runtime capability tags |
| `ProtocolProfile`   | Closed enum: client tier (Minimum, Standard, Full) |

Foundations explicitly **removed** from v1 protocol surface:
- `ObservationHooks` — Wyrd doesn't push. Triggers + Operators carry reaction
  routing; Sources carry read-side history.
- Free-form event-name strings (`events: Vec<String>`, `hook_events:
  Vec<String>`) — observations are typed by source CardRef + kind. If hook
  phases ever land on `Policy`, they're a closed enum (`PreInvoke |
  PostInvoke | OnError`), not strings.
- `Drift.target_refs: Vec<CardRef>` (plural, untyped intent) — replaced by
  singular `subject_ref: CardRef`.
- `Drift.baseline_ref` (top-level), `Drift.features` (top-level) — only
  meaningful for distribution drift; folded into `DriftSignal::Distribution`.
- `Drift.thresholds: BTreeMap<String, f64>` (untyped bag) — replaced by the
  typed `DriftCondition` enum (`Statistical | Above | Below | Outside`).
- `Drift.source_refs` (top-level) — read-side source for distribution/metric
  monitoring arrives via `DriftSignal::External { source_ref }` or implicitly
  through the subject's runtime emission.
- `Eval.target_refs: Vec<CardRef>` (plural) — replaced by singular
  `subject_ref: CardRef`. One Eval covers one subject; author multiple Eval
  cards for multiple subjects.
- `Eval.eval_type: EvalType` + parallel `Eval.profile: EvalProfile` enums
  (`AssertionEvalProfile` / `JudgeEvalProfile` / `BenchmarkEvalProfile` /
  `AgenticEvalProfile` / `CustomEvalProfile`) — same redundancy that
  `DriftMethod`+`DriftProfile` had. The shape of an eval emerges from which
  `EvalTask` variants appear in `tasks`; the discriminator was duplicated
  information.
- `Eval.judge_refs: Vec<CardRef>` (top-level) — folded into per-task
  `prompt_ref` on the `EvalTask::Judge` variant. One judge per task is the
  Scouter shape.
- `Eval.assertions: Vec<EvalAssertion>` (top-level) — folded into `tasks` as
  the `EvalTask::Assertion` variant.
- `Eval.pass_gates: Vec<EvalPassGate>` — fire condition belongs on `Drift`,
  not `Eval`. "Fire when pass_rate < 0.95" is a `Drift` card with
  `DriftSignal::EvalScore { eval_ref }` + `DriftCondition::Below { limit }`.
  One fire-vocabulary in the protocol, not two.
- `Eval.dataset_refs: Vec<CardRef>` (plural) — replaced by singular
  `dataset_ref: CardRef`. Scenarios and datasets are the same noun; one
  scenario set per Eval. Inline `scenarios: Vec<EvalScenario>` is also
  removed — the canonical record-set noun is `Data`, and the Data card
  carries `EvalScenario` rows.
- `Eval.source_refs: Vec<CardRef>` (plural) — replaced by singular
  `source_ref: CardRef`. One sink declares where Wyrd reads observations
  for the subject from.
- `Eval.default_parameters: BTreeMap<String, ParameterValue>` — no v1 use
  case; runtime parameters belong on the runtime, not the durable contract.
- Any `schedule` / `cron` / `alert_config` / `sample_ratio` / dispatch fields
  on Drift or Eval — scheduling is `TriggerSource::Schedule`; dispatch is
  `Operator`; sampling cadence is a runtime knob. No scheduling, sampling,
  or notification leaks onto observation cards.
- `TriggerSource::DriftObservation` (downstream wire: "fire when Drift emits")
  — the variant assumed a hidden subsystem evaluating Drifts on an unstated
  cadence. Replaced by upstream `TriggerSource::Drift { drift_ref }`: the
  Trigger's own `schedule` drives the Drift evaluation, and the operator fires
  on condition match. No hidden cadence; every monitor evaluation is the
  visible firing of a Trigger.
- `TriggerSource::Schedule { cron, tz? }` (cron as a source variant) — schedule
  is intrinsic to every Trigger, not a variant alternative to a monitor. Moved
  to top-level `Trigger.schedule`. `source` is now optional; absent `source`
  means unconditional fire on schedule.
- `TriggerSource::EvalObservation` (downstream wire: "fire when Eval emits")
  — never landed. Eval invocation lives on Trigger as upstream
  `TriggerSource::Eval { eval_ref }` (runs the Eval on schedule, fires the
  operator on any task failure). For windowed/aggregate fire conditions over
  eval scores (e.g., "alert when pass_rate < 0.95 over 24h"), author a Drift
  with `DriftSignal::EvalScore { eval_ref }` and watch it via
  `TriggerSource::Drift`.
- `Trigger.config: BTreeMap<String, NonSecretValue>` — second-layer filtering
  on the wire is doctrine drift. Severity / threshold / shape decisions belong
  on `Drift.condition`; reaction behavior belongs on `Operator`. Trigger is
  pure wiring.
- `Trigger.cooldown_seconds` (top-level and variant-level) — the `schedule`
  cadence IS the cooldown. Triggers fire at most once per schedule tick; a
  second-layer debounce field is redundant.
- `Trigger.target: CardRef` (untyped) — replaced by `operator_ref: CardRef`.
  Naming convention: a `CardRef` that points at exactly one kind is named
  `<kind>_ref`. Operator is the only valid Trigger target (Workflow lacks
  Policy gates; Eval is invoked via `source.Eval`, not the target slot).
- `TriggerSource.card` (generic field name on the prior drift variant) —
  replaced by `drift_ref`. Same `<kind>_ref` convention as
  `Drift.signal.baseline_ref` / `eval_ref` / `source_ref`.
- `Operator.adapter: FrameworkAdapterRef` — Workflow execution lives on
  Workflow cards; Notify/Http are stock server primitives. No framework
  binding on the reaction wire.
- `Operator.inputs: Vec<OperatorInput>` (with free-string `schema_ref`) —
  untyped wire vocabulary. Each `OperatorAction` variant declares its own
  input shape via the templating context bound by the firing Trigger.
- `OperatorInput` (foundation) — dropped with `inputs`.
- `OperatorBudget.max_memory_mb` — host/sandbox concern (Rule 14). `Operator.budget`
  retains `max_wall_seconds` and `max_tool_calls` only.

---

## Spec-file authoring

Two complementary mechanisms — `wyrd apply -f file.yaml` reads + registers in
one move, and a `PromptRef` may be inlined where its own identity isn't
needed. Together they support the single-file Scouter/opsml-style workflow
without breaking Rule 1 ("cards are independent registry entries").

### Pre-registration matrix (Rule 16)

| Kind | Must pre-register? | Why |
|------|---------------------|-----|
| `Model`        | **Yes** | Carries weight artifacts; lineage anchor. |
| `Data`         | **Yes** (unless used purely as inline eval scenarios, which v1 does not support — `dataset_ref` is `CardRef`-only) | Carries dataset bytes; lineage anchor. |
| `Experiment`   | **Yes** | Carries run history. |
| `Artifact`     | **Yes** (typically derived from heavy cards) | Pointer to durable bytes. |
| `Prompt`       | Optional | Light. Inlineable as `PromptRef` inside `Agent.prompt` / `EvalTask::Judge.prompt`. |
| `Agent`        | Optional | Light. Spec-only; `apply` registers it. No v1 field accepts inline `AgentRef` (see Q11). |
| `Eval`, `Policy`, `Trigger`, `Operator`, `Source`, `Mcp`, `Workflow`, `Audit`, `Service` | Optional | Light. Spec-only; `apply` registers each card as it's read. |

A single YAML file may contain many `---`-separated card documents — `wyrd
apply -f eval-suite.yaml` registers all of them in dependency order. The Agent
under test, the Source it reads from, and the Eval that judges it can all
ship in one file.

### `PromptRef` authoring forms

The typed contract has two variants. The **authoring layer** adds a third
load-time form (`path`) for splitting prompts into separate files.

```yaml
# 1. Card reference — points at a registered Prompt.
prompt:
  kind: ref
  ref: { kind: Prompt, name: helpfulness-judge, version: "1.0.0", space: prod }

# 2. Inline — full PromptSpec embedded in the parent.
prompt:
  kind: inline
  provider: anthropic
  model: claude-opus-4-7
  messages: [ ... ]
  variables: [input, output]
  response_format: json

# 3. Path — loader-time directive (NOT a wire variant; resolved client-side).
prompt:
  kind: path
  value: "./prompts/helpfulness-judge.yaml"
```

### Path resolution rules (loader contract)

`path:` is **client-side authoring sugar**, not a `PromptRef` wire variant.
The loader splices the referenced file's content into the parent at read
time; the payload that leaves the client contains only `ref` or `inline`. The
server, registry, and `vala` never see a `path:` value.

- Resolved **relative to the file containing the `kind: path` reference** —
  not CWD, not apply-root.
- Absolute paths are allowed but discouraged (breaks portability across
  machines and CI).
- The referenced file is a **bare spec body** (no `apiVersion` / `kind` /
  `metadata` wrapper) — a fragment, not a card document. Card documents go
  in their own multi-doc YAML entries and get registered separately.
- Path imports are **never auto-registered as cards**. The result is inline.
  If you want a registered, reusable `Prompt`, write a full card document
  (with `apiVersion` + `kind` + `metadata`) and apply it; then reference by
  `CardRef`.
- Transitive: a `path:`-loaded fragment may itself contain `path:` refs.
  Loader resolves transitively with a hard depth limit (≤8) to catch cycles.
- `path`, `inline`, and `ref` are mutually exclusive on a single `PromptRef`.
  Any combination is a validation error.

This keeps the wire contract tight (two-variant `PromptRef`), prevents
filesystem-on-server, and gives authors the file-splitting ergonomic they
expect from JSON-Schema `$ref` / OpenAPI external-file imports.

---

## Reference-direction quick reference

| Card    | Refs that authored on it             | Refs that point at it          |
|---------|--------------------------------------|--------------------------------|
| Data    | `artifact_refs`, `splits`            | `Drift.signal.baseline_ref`, `Eval.dataset_ref`, `Experiment.target_refs` |
| Model   | `artifact_refs`                      | `Drift.subject_ref`, `Eval.subject_ref`, `Service.components.ref`, `Experiment.target_refs` |
| Agent   | `prompt`, `tool_names`               | `Drift.subject_ref`, `Eval.subject_ref`, `Service.components.ref`, Agent prompts (sub-agent calls) |
| Workflow| `steps.*.target`                     | `Eval.subject_ref`, `Service.components.ref`, `Operator.action.workflow_ref` |
| Mcp     | `credential_refs`                    | `Service.components.ref` |
| Drift   | `subject_ref`, `signal.*` (`baseline_ref` \| `eval_ref` \| `source_ref`) | `Trigger.source.drift_ref`, `Drift.signal.eval_ref` (other Drifts watching an Eval indirectly) |
| Eval    | `subject_ref`, `dataset_ref`, `source_ref`, `tasks[].Judge.prompt` (PromptRef) | `Drift.signal.eval_ref`, `Trigger.source.eval_ref` |
| Audit   | `subject_refs`, `policy_refs`, `evidence_refs`, `source_refs` | — |
| Service | `components[].ref`                   | `Drift.subject_ref` (service-level), `Eval.subject_ref` |
| Policy  | `rules`                              | `Service.components.ref`, `Audit.policy_refs`, `Operator.pre_invoke`, `Operator.post_invoke` |
| Trigger | `schedule`, `source.drift_ref` \| `source.eval_ref`, `operator_ref` | — |
| Operator| `action` (`workflow_ref` \| `channel.*_ref` \| `auth.credential_ref`), `pre_invoke`, `post_invoke` | `Trigger.operator_ref` |
| Source  | `credential_ref`                     | `Drift.signal.source_ref` (External variant), `Eval.source_ref`, `Audit.source_refs` |

`Service.components` accepts: Agent, Prompt, Model, Workflow, Mcp, Policy. No
other kinds are runtime-aliased into a Service.

---

## Worked directory layout

A deployment is a folder. `wyrd apply -f <dir>` registers every card.

```
services/ops-copilot/
├── service.yaml                 # runtime composition only
├── sources/
│   ├── run-archive.yaml         # object_store Source — vala archive
│   └── audit-log.yaml           # object_store Source — audit dump
├── agents/
│   ├── incident-triage.yaml
│   └── runbook-executor.yaml
├── prompts/
│   ├── triage.yaml
│   ├── runbook.yaml
│   └── judge.yaml
├── policies/
│   ├── triage-policy.yaml
│   ├── runbook-policy.yaml
│   └── service-policy.yaml
├── observability/
│   ├── triage-eval.yaml         # source_ref → run-archive
│   ├── runbook-eval.yaml
│   ├── triage-drift.yaml
│   ├── runbook-drift.yaml
│   └── service-latency-drift.yaml
├── audits/
│   └── service-quarterly.yaml   # source_refs → audit-log
└── reactions/
    ├── pageroncall-operator.yaml
    └── latency-page-trigger.yaml
```

---

## Open questions

1. Per-component Policy binding on `ServiceComponent`. Workaround: rule
   expressions scope by `agent.name`. Decision pending a real use case.
2. Source vendor read adapters (Datadog metrics, PromQL, Tempo, Loki).
   v1 ships `object_store` only.
3. Format negotiation for `object_store` Source — schema-on-read vs registered
   schema reference.
4. Time-window semantics for how Drift/Eval cards describe the read range
   over `source_ref`.
5. Service-level Drift subject semantics — what "drift on a Service" computes
   when Wyrd reads internal traces vs external Sources, given the subject is
   singular.
6. Default Source binding at the Service or Agent level to avoid repeating
   `source_ref` on every Drift/Eval.
7. Audit `source_refs` vs `evidence_refs` boundary — Source is queryable
   history; `evidence_refs` are concrete card pointers.
8. Whether tool hook phases need a closed enum on `Policy.rules` or can stay
   off the wire entirely (no consumer today).
9. Whether `Audit.subject_refs` stays plural. An audit may genuinely cover a
   Service plus its component Agents; collapse to singular if real audits
   don't span multiple cards in practice.
10. **Closed.** `Eval` does not carry its own `signal` decomposition. Eval IS
    the signal — its per-task pass/fail aggregates into a score stream
    consumed downstream by `Drift` with `DriftSignal::EvalScore`. The input
    edges (`dataset_ref` vs `source_ref`) are two optional `CardRef`s, not a
    tagged enum: presence is the mode (offline driver, online sink, both, or
    neither → vala default archive).
11. Whether `AgentRef = CardRef | Inline` should land in v1. The Rule 17
    pattern permits it; no current field has a clean use case that doesn't
    cross a doctrine boundary (`subject_ref`, `Service.components.ref`,
    `Workflow.steps.target` all need stable identity). Likely first home:
    one-off `Workflow.steps[].target` for ephemeral agent steps. Deferred
    until a concrete request surfaces.
