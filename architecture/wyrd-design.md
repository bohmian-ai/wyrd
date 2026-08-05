# Wyrd Design

**Version:** v1

This document is the current design of the Wyrd protocol. It is **stateless**:
it reflects the shape as it stands now. Decision history lives in git
(`git log architecture/wyrd-design.md`).

When this disagrees with `wyrd-protocol.openapi.yaml`, `wyrd-protocol.md`,
`specs/*.yaml`, or the Rust code in `crates/wyrd-spec`, **this file wins**.
Downstream artifacts are brought up to this version in a sync pass.

---

## Table of contents

- [Doctrine](#doctrine) — 20 design principles
- [Client model](#client-model) — language-agnostic protocol and first-class SDKs
- [Kind catalog](#kind-catalog) — 16 native kinds + External
- [Per-kind specs](#per-kind-specs) — field shapes per kind
  - [Data](#data) · [Model](#model) · [Artifact](#artifact) · [Experiment](#experiment)
  - [Prompt](#prompt) · [Agent](#agent) · [Workflow](#workflow) · [Mcp](#mcp)
  - [Service](#service) · [Policy](#policy) · [Audit](#audit)
  - [Drift](#drift) · [Eval](#eval) · [Source](#source) · [Bifrost](#bifrost--wyrds-olap-warehouse)
  - [Trigger](#trigger) · [Operator](#operator)
- [Spec-file authoring](#spec-file-authoring) — `ref` / `select` / `path` / `inline`, pre-registration matrix
- [Workspace config](#workspace-config-wyrdtoml) — `wyrd.toml` defaults and merge rules
- [Reference-direction quick reference](#reference-direction-quick-reference) — who refs whom
- [Worked directory layout](#worked-directory-layout) — example deployment tree
- [Questions and resolutions](#questions-and-resolutions) — open and resolved design decisions

---

## Doctrine

1. **Cards are independent.** No card "owns" another. The
   deployment unit is a directory of card YAMLs applied together.
2. **One fact, one owning Kind.** If a field could live in two places, the
   doctrine has a gap. Surface it.
3. **Monitors declare subjects.** Drift and Eval reference what they
   observe. Subjects do not list their observers. Each declares a single
   `subject_ref`.
4. **Reactions are Operators; wiring is Triggers.** Drift/Eval/Policy never
   inline reaction logic.
5. **Service composes for deployment, not observation.** Drift/Eval/Trigger/
   Operator/Audit/Source are peer cards, not Service components.
6. **Enforcement is composed at the enforcing surface.** Service
   composes Policy for runtime gates.
7. **Native observation first; external data by reading only.** AI services
   record runtime behavior through Wyrd's native observation system
   (`wyrd.observe` → `vala`, linked to registered cards) — this is the
   primary path. When data already lives in an external system (Prometheus,
   Snowflake, object storage), `Source` cards let Wyrd query it. Wyrd never
   writes to external data stores and never requires external systems to push
   data in.
8. **Lineage is server-derived.** Derived from `*_refs`. Never authored. Never edited.
9. **Status is server-managed.** Authors never write `status:`.
10. **Every cross-card pointer is a `CardRef`.** No string-typed parents or
    path-typed lookups in the protocol. Co-location is expressed by the
    deployment directory, not by a path field on any card.
11. **Sub-agency is a relationship, not a noun.** An Agent invoking another
    Agent is the sub-agent call. The callee is an `AgentCard`. The caller's
    prompt / runtime expresses the invocation. No `SubAgent` kind.
12. **Tools are runtime names, not cards.** `AgentSpec.tool_names: Vec<String>`
    resolves through the runtime tool registry. MCP servers auto-register
    their tools by name; host tools register themselves. No `Tool` kind.
13. **No event vocabulary on the wire.** "Observation" comes from Drift/Eval;
    "trigger firing" comes from the `TriggerSource` enum. Free-form
    event-name strings are doctrine drift.
14. **Host config stays off Cards.** Permission modes,
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
    `Agent.prompt` accepts `PromptRef = CardRef | Inline`. Inline definitions
    have no card identity, are not registered
    standalone, and cannot be referenced from outside their parent. To reuse,
    register as a card and reference by `CardRef`. Heavy refs (`subject_ref`,
    `dataset`, `Service.components.ref`, `Workflow.steps.target`) stay
    `CardRef`-only — identity is the point.
18. **Auth and Policy are two distinct planes.** Emit is **not** a third
    plane: a deployed service's observation/ingest writes are ordinary
    Auth-plane routes, authorized by the same JWT and a
    `Permission { resource, action }` like every other call. The legacy
    per-card **governance token is removed** — the JWT proves the principal and
    bounds its emittable **card scope**. For card-bound principals, the scope is
    the principal's own `card_ref` plus the **observation-target** cards reachable
    through the transitive card-ref graph declared in that card's spec. A card
    enters the scope only if its kind is an observation target — a kind a client
    (`wyrd.observer`, Bifrost, drift, eval) attributes records to: `Data`,
    `Model`, `Experiment`, `Prompt`, `Agent`, `Workflow`, `Eval`, `Drift`,
    `Service`, `Mcp`, `Artifact`, `Source`. Control-plane kinds (`Policy`,
    `Audit`, `Operator`, `Trigger`) may be referenced for governance but never
    enter the emit scope. Service principals start from their Service card and
    therefore include declared `Service.components`; Agent principals start from
    their Agent card and include its declared card refs. The observation envelope
    carries the run's Target `card_ref`, which the server authorizes against that
    scope, and `run_id` carries which action emitted it. A separate emit
    credential was redundant — see "Observation identity — Card → Run →
    Observation".
    - **Auth** gates Wyrd API calls: `Permission { resource, action }` on the
      handler, stateless pubkey verify of the access token. Answers "is this
      principal allowed to hit this Wyrd route?" This covers data-plane ingest
      (e.g. `bifrost_record:write`) exactly like any other route. The legacy
      `Scope` vocabulary is rejected — do not introduce it in new code.
    - **Policy** gates card states (`classify` at register-time, `gate` at
      deploy-time) and cross-service invokes (`invoke` at runtime). Runtime
      invoke evaluation is centralized at `POST /v1/authz/check`, called
      transparently by the service mesh's ext_authz filter or by the SDK
      middleware in non-mesh shops.

    Runtime identity is a `Principal { id: PrincipalId, kind: PrincipalKind,
    tenant_id, roles, effective_permissions }`. `PrincipalId` is a `Uuid`
    newtype; no string-prefix encoding (no `user:`, `sa:`, `agent:`) —
    discrimination lives on `PrincipalKind`. `PrincipalKind` is closed:
    `User`, `Service { card_ref }`, `Agent { card_ref }`. Service and Agent
    are deployable, card-bound, non-human principals; their JWT projection also
    carries a `card_ref_scope` authorization set derived at mint time. User is
    the marker for human identity. `wyrd apply -f service.yaml` (or an Agent card) creates
    or updates the principal row idempotently, keyed on
    `(tenant_id, card_kind, card_uid)`; re-apply preserves the same
    `principal_id`. No secret is returned. Credentials are issued out-of-band
    by `wyrd auth issue-key <card_ref>`, which mints a card-bound API key
    against the existing principal. The caller uploads the key to the deploy
    environment's secret store (Vault, AWS Secrets Manager, GCP Secret
    Manager); deploy-time secret injection puts it into the pod as
    `WYRD_API_KEY`. The SDK exchanges it once at startup at `POST /auth/token`
    for a short-lived JWT. `/auth/token` derives `tenant_id` and
    `principal_id` from the verified API-key record — never from a
    client-supplied header. The JWT carries top-level `principal` (current
    actor / callee under delegation) and an RFC 8693 `act` chain
    (initiator-first delegators); a single delegated token carries both sides
    of an invoke. On cross-service calls the SDK puts that delegated Wyrd
    JWT in the `X-Wyrd-Access-Token` header. The application's own
    `Authorization` header is never touched. `Wyrd-Caller-Identity` is
    rejected legacy — do not reintroduce. The mesh's ext_authz filter (or
    the SDK middleware) forwards `X-Wyrd-Access-Token`, `Wyrd-Request-Id`,
    and `X-Original-*` to `/v1/authz/check`; the body is empty. Both caller
    and callee identities are server-verified from one signed delegated
    JWT — no enforcement-point JWT, no SPIFFE/mTLS callee derivation.
19. **Reference slots accept `ref | select | path | inline`.** Wherever a
    card spec references another card — light or heavy (Prompt, Agent,
    Workflow, Mcp, Policy, Eval, Trigger, Operator, Source, Audit, Service,
    Model, Data, Experiment, Artifact) — the slot is a tagged-by-key union,
    mutually exclusive. The key IS the discriminator:
    - `ref:`    — a `CardRef` with exact identity: `kind`, `name`, `version`,
                  optional `space`, optional `uid`. The only durable form that
                  crosses the wire. `CardRef` never carries `labels` or
                  `annotations`.
    - `select:` — client-side authoring sugar. A `CardSelector` that matches on
                  target-card **metadata** (`kind`, optional `name`, `version`,
                  `space`, `labels`, `annotations`, `latest`). `wyrd plan` /
                  `wyrd apply` resolves it to exactly one `uid`-bearing
                  `CardRef` before registration, relationship derivation,
                  hydration, or any runtime observation. Zero matches →
                  `CARD_SELECTOR_NOT_FOUND`; more than one →
                  `CARD_SELECTOR_AMBIGUOUS`. Never on the wire.
    - `path:`   — client-side authoring sugar. Targets a **full card envelope**
                  on disk (`apiVersion` + `kind` + `metadata` + `spec`). The
                  loader registers it as an independent card and rewrites the
                  parent slot to `ref: CardRef`. Never on the wire.
    - `inline:` — spec body embedded in the parent, prefixed with `kind:`. No
                  card identity; not addressable from outside the parent. Light
                  cards only.

    `select` and `path` resolve to `ref` before send; only `ref` and `inline`
    cross the wire. Heavy cards (Model, Data, Experiment, Artifact) accept
    `ref` or `select` only — `inline` is rejected because identity anchors
    lineage. Environment/stage is target-card metadata (`labels` /
    `annotations`), never `space`; `space` is team/workspace scope only, and
    one server may hold development, staging, and production cards side by side.
    See §"Light-card reference forms" for loader rules and the slot inventory.
20. **User journeys are the primary test contract.** A capability is not done
    until a real user/agent path proves it end-to-end — client → server →
    client, against a real server (`WyrdTestServer` + embedded Postgres), not a
    mock. The journey is the unit of correctness: for a data surface,
    instantiate → write → shutdown/flush → read; for an agent/MCP surface,
    discover → act → observe. Unit and integration tests support journeys by
    isolating a seam or a branch that is awkward to drive end-to-end; they never
    substitute for the journey. Each journey covers the happy path **and** the
    edge/negative flows a real caller actually hits — re-register with a
    conflicting schema, an under-privileged token, a rejected query, a replayed
    batch. A bug that only appears when state crosses a module boundary is
    exactly what a journey catches and an isolated test misses. See AGENTS.md
    §11 for the tier definitions and gates.

## Client model

Wyrd is language-agnostic at the protocol boundary. The server owns durable
behavior, and its typed wire contracts are the source of truth. Any language
can implement a client by following those contracts; no SDK owns a separate
registry, lifecycle, validation, or storage model.

Rust, Python, and TypeScript are Wyrd's first-class client languages. Wyrd
maintains idiomatic SDKs, generated types, examples, and client → server →
client journeys for all three. Surface ergonomics may differ, but durable
nouns, fields, errors, permissions, side effects, and lifecycle semantics do
not. Go is planned. It becomes first-class only when its SDK and the same
contract and journey gates ship.

The public protocol remains open to every language. HTTP, MCP, generated
schemas, stable errors, and machine-readable documentation are sufficient to
implement a complete client without depending on Rust, Python, or TypeScript
internals.

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
  card_refs: [CardRef]       # → Artifact
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
  card_refs: [CardRef]       # → Artifact
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
  card_refs: [CardRef]
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
  details: { string: NonSecretValue }
```

### Service
Runtime composition for deployment. **Components are runtime-aliased only.**
Drift/Eval/Trigger/Operator/Audit/Source are peer cards in the deployment
directory, not Service components.
```yaml
spec:
  description?: string
  components: [ServiceComponent] # { alias, ref | select | path | inline }
  entry_point?: string           # SDK AppState bootstrap module (e.g. `acme.copilot.app:app`).
                                 # Importing it materializes the service's locked card snapshot
                                 # at runtime. Wyrd doesn't import this; the deploy image does.
```

Identity is derived from the Service's `card_ref` and bound on first deploy
contact — no `service_account` field on the spec. See "Runtime identity".

Worked examples: `architecture/specs/01-ml-prediction-service.yaml`,
`architecture/specs/02-llm-agent-service.yaml`,
`architecture/specs/06-multi-agent-service.yaml`.

### Policy
Declarative governance rules. CEL-evaluated. Three lifecycle phases share one
rule shape; the `action` field on each rule says when it fires:

  - `classify` (register-time): rule derives attrs onto the card (e.g.
                                `risk.tier = "high"`). Never blocks.
  - `gate`     (deploy-time):   rule allows/denies the card's deploy-time
                                credential issuance (`wyrd auth issue-key`),
                                and thus its emit eligibility. Blocks when Deny.
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

#### Principal model

Wyrd principals are UUID-backed runtime identities for `User`, `Service`,
and `Agent` kinds. `Service` and `Agent` principals are card-bound: each
carries a `card_ref` discriminated on `PrincipalKind`, and its JWT carries a
mint-time `card_ref_scope` derived from the transitive card-ref graph rooted at
that card. `wyrd apply` for a Service or Agent card creates or updates the
principal row idempotently (keyed on `(tenant_id, card_kind, card_uid)`);
re-apply preserves the same
`principal_id`. No secret is returned. The declarative and credential
operations are separated, matching the kubectl pattern (`apply` then
`create token`):

| Operation | Wyrd command | What it does |
|---|---|---|
| Register card + create principal | `wyrd apply -f service.yaml` | Idempotent. Writes card, upserts the Service/Agent principal keyed on `(tenant_id, card_kind, card_uid)`. Re-apply preserves `principal_id`. No secret. |
| Mint a card-bound API key | `wyrd auth issue-key <card_ref>` | Admin-authenticated. Requires an applied principal. Returns the key to the caller. Caller uploads to the deploy environment's secret store. Re-issuable for rotation. |

#### API key → JWT flow

Deploy-time secret injection (Vault Agent, External Secrets Operator, AWS
Secrets Manager CSI driver, etc.) puts the API key into the pod as
`WYRD_API_KEY`. The SDK exchanges it ONCE at startup at `POST /auth/token` for
a short-lived JWT (~15m), and auto-refreshes before expiry. `/auth/token`
derives `tenant_id` and `principal_id` from the verified API-key record;
no client-supplied tenant header is accepted. The JWT carries the principal
as the top-level `principal` claim, and for delegated tokens (token
exchange) an RFC 8693 `act` chain of upstream delegators.

Env vars in deployed services:

| Env var | Required? | Source | Used for |
|---|---|---|---|
| `WYRD_API_KEY` | REQUIRED | Deploy environment's secret store (key minted by `wyrd auth issue-key <card_ref>`) | Exchanged ONCE at startup at `POST /auth/token` for short-lived JWT. SDK auto-refreshes. JWT carries the card-bound `principal` claim (kind, id, tenant, `card_ref`). |
| `WYRD_SERVER_URL` | REQUIRED | Static config | Wyrd server HTTP base URL (default `http://localhost:50050`). Read by `ClientConfig::from_env`. |
| `WYRD_GRPC_URL` | OPTIONAL | Static config | Wyrd server gRPC endpoint (default `http://localhost:50051`). Read by `ClientConfig::from_env`. |

#### Cross-service delegation

The API key is exchanged at startup — never on the wire. The JWT — not the API
key — is what travels on cross-service calls in the dedicated
`X-Wyrd-Access-Token: Bearer <jwt>` header. The JWT must be a **delegated**
Wyrd token: its top-level `principal` identifies the protected callee
(Service or Agent), and its `act` chain identifies the caller(s) that
delegated to it. The application's own `Authorization` header belongs to
the application and is never read or written by the SDK or by Wyrd.
`Wyrd-Caller-Identity` is rejected legacy — do not reintroduce.

```
POST /charge HTTP/1.1
Host: billing-svc.acme.svc.cluster.local
X-Wyrd-Access-Token: Bearer <delegated Wyrd JWT — principal=callee, act=caller chain>
Wyrd-Request-Id:     <UUIDv7>                     ← SDK adds; request correlator
Authorization: Bearer <app's own token>           ← app's own auth; Wyrd never reads
Content-Type: application/json

{ "amount": 100 }
```

#### Request correlator — `Wyrd-Request-Id`

A Wyrd-owned, request-scoped opaque ID that joins every hop of a logical
request. It is the sole correlator for policy ancestry and audit replay —
Wyrd does not depend on `traceparent`, mesh tracing, or any external
propagation contract.

Contract:

- Opaque UUIDv7 minted by Wyrd at first sighting (no inbound
  `Wyrd-Request-Id` at `/v1/authz/check`).
- Propagated unchanged by Wyrd SDK middleware and ext_authz on outbound
  calls. Never mutated, never re-minted mid-request.
- Every Wyrd-emitted observation carries it as a label.
- Ancestry of any request (service1 → service2 → service3) is
  reconstructable by joining observations on this ID; per-hop caller
  identity comes from the verified `X-Wyrd-Access-Token` JWT at each
  call (top-level `principal` is the hop's callee, `act` chain is the
  caller path).

Storage tier, query API, and CEL surface (e.g. a `chain.*` binding) are
implementation concerns deferred to the runtime stage.

#### Observation identity — `Card → Run → Observation`

How an observation ties to a Run and a Card, and how the server resolves it.
The lineage spine is fixed by the concept docs — `Card → Run`
([`run.mdx`](../docs/src/content/docs/concepts/run.mdx): every Run is bound to a
Card version, its **Target**) and `Run → Observation`
([`observation.mdx`](../docs/src/content/docs/concepts/observation.mdx): every
Observation anchors to the Card version **and** Run it belongs to). This section
states only the runtime resolution, which lives in the server, not the concept
docs.

**A principal is not a card.** A Service or Agent principal is bound to one card
(its `card_ref`), but a Service card *nests components* — each a card in its own
right (e.g. Model A, Model B, a Prompt; `Service.components`). `wyrd_state["a"]
.run()` and `wyrd_state["b"].run()` execute under the **same** JWT yet target
**different** component cards, and a Run is specific to the card that opened it.
So the JWT alone cannot say which card a record belongs to — the run's Target
card must be carried on the wire.

Every observation row carries:

| Value | Source | Grain | Means |
|---|---|---|---|
| `card_ref` | **client asserts the run's Target card; server authorizes it** | **per row** | the Card-version anchor — *which* card |
| `run_id` | client-generated per `.run()`; passed through opaquely | **per row** | the Run anchor — *which* execution |
| `tenant_id` | server-stamped from the verified JWT | per request | the tenancy boundary |
| `wyrd_request_id` | the propagated `Wyrd-Request-Id` (minted at first sighting) | per request | the request spine — one request spans **many** runs and hops |

Resolution rule: **tenant comes from the token; `card_ref` is client-asserted
and server-authorized; `run_id` and `wyrd_request_id` pass through untouched.**
Consequences, stated so they stop drifting:

- **`card_ref` and `run_id` are per-row columns on the observation payload, not
  request metadata.** A client-side queue batches records from different runs —
  and different cards — before it flushes, so one sealed batch (one
  `wyrd_batch_id`) freely mixes them. The producer is keyed by **table only**; it
  never splits a batch by card or run. The server therefore authorizes `card_ref`
  **per row** (every distinct card in the batch must be in the principal's scope),
  validates the client-generated UUIDv7 `wyrd_batch_id`, stamps
  request-scoped `data_tenant_id`, `wyrd_request_id`, and
  `wyrd_ingested_at`, validates or normalizes `wyrd_event_time`, and assigns
  one `wyrd_row_ordinal` per row across the complete logical batch.

- **`card_ref` is authorized, not trusted.** The server checks the asserted
  `card_ref` against the principal's **card scope**. For Service and Agent
  principals, the scope is the principal's own `card_ref` plus the
  **observation-target** cards reachable through the transitive card-ref graph
  declared in that card's spec. A card is in scope only if its kind is an
  observation target (`Data`, `Model`, `Experiment`, `Prompt`, `Agent`,
  `Workflow`, `Eval`, `Drift`, `Service`, `Mcp`, `Artifact`, `Source`);
  control-plane kinds (`Policy`, `Audit`, `Operator`, `Trigger`) never enter the
  emit scope. Service cards contribute `Service.components`; other reachable
  specs contribute their declared card refs according to the shared card-ref
  extraction rules. A `card_ref` outside that set is rejected: a principal may
  not attribute records to a card outside its declared graph. The scope can be resolved from the
  registry at ingest or carried as a claim minted into the JWT at `/auth/token`
  — an implementation choice deferred to the runtime stage.
- **This is not the governance token.** `card_ref` is one field in the
  observation envelope, authorized by the existing JWT plus the principal's
  declared card-ref graph — not a separate per-card credential (doctrine #18).
  The token still proves the principal; it bounds a *set* of emittable cards,
  and the envelope selects one within it.
- **There is no run registry.** Runs are a client-side execution record
  ([`run.mdx`](../docs/src/content/docs/concepts/run.mdx)); the server never
  persists a run table and never resolves `run_id` back to a card — the card is
  the authorized `card_ref` on the row. `run_id` is an **opaque** correlation id,
  never a composite that encodes the card.
- **`Card → Run → Observation` is the `(card_ref, run_id)` pair on the row;** the
  request spine is the `wyrd_request_id` label that joins many runs across hops.
- **Subject ≠ emitter is deferred to produced kinds.** A monitor emitting Drift/
  Eval about a card *outside* its own scope carries an explicit `subject_ref`
  with its own authorization — distinct from the in-scope `card_ref` above. That
  lands with `vala-drift`/`vala-eval`, against a real consumer — not on the
  Stage-3 `Record` envelope.

### Runtime authz: `POST /v1/authz/check`

The single CEL evaluation surface for `PolicyAction::Invoke`. Two delivery
paths, identical semantics:

- **Service mesh (ext_authz).** The mesh's local Envoy/Istio sidecar
  intercepts the inbound request transparently (iptables redirect, standard
  k8s/Istio behavior), and the configured ext_authz filter calls Wyrd's
  `/v1/authz/check`. Application code makes a normal HTTP call. One-time
  platform-team filter config covers every workload — no per-team
  middleware is required. The mesh is configured to forward exactly the
  Wyrd-defined headers via `includeRequestHeadersInCheck`, e.g.

  ```yaml
  apiVersion: install.istio.io/v1alpha1
  kind: IstioOperator
  spec:
    meshConfig:
      extensionProviders:
        - name: wyrd-authz
          envoyExtAuthzHttp:
            service: wyrd-server.wyrd-system.svc.cluster.local
            port: "8080"
            pathPrefix: /v1/authz/check
            includeRequestHeadersInCheck:
              - x-wyrd-access-token
              - wyrd-request-id
              - x-original-method
              - x-original-path
              - x-original-host
  ```
- **SDK middleware (non-mesh).** Identical semantics in-process. One-line
  developer install (`app.add_middleware(PolicyMiddleware)`). The middleware
  reads `X-Wyrd-Access-Token` from the inbound request and calls the same
  `/v1/authz/check` route.

The check is **headers-only**. Body is empty. All inputs are headers, which
matches how Envoy's ext_authz filter natively forwards data — zero
translation logic on either end.

```
POST /v1/authz/check HTTP/1.1
Host: wyrd.acme.com
X-Wyrd-Access-Token: Bearer <delegated Wyrd JWT — principal=callee, act=caller chain>
Wyrd-Request-Id:     <UUIDv7 — forwarded from inbound, or absent on first hop>
X-Original-Method:   POST
X-Original-Path:     /charge
X-Original-Host:     billing-svc.acme.svc.cluster.local
Content-Length: 0
```

Wyrd:
1. Verifies `X-Wyrd-Access-Token`. Rejects with `403 Forbidden` if the JWT
   is not a **delegated** token — i.e. if `principal.kind ∉ {Service,
   Agent}`, if `principal.card_ref` is `None`, or if the `act` chain is
   empty. `/v1/authz/check` will not authorize on a direct (non-delegated)
   token.
2. Derives `callee = verified.principal` (the protected Service/Agent
   card identity) and `caller = verified.delegation_chain.last()` (the
   immediate delegator); the full chain is retained for policy bindings
   and audit. There is no enforcement-point JWT and no SPIFFE/mTLS
   callee derivation — one delegated token carries both sides.
3. Reads `X-Original-Method` / `X-Original-Path` / `X-Original-Host`
   → builds `request`.
4. Reads `Wyrd-Request-Id` if present; mints a fresh UUIDv7 if absent and
   echoes it back so the middleware/sidecar can inject it on the outbound
   call.
5. Assembles `InvokeContext { caller, callee, chain, request, attrs }`
   (attrs are merged Classify-derived attributes from caller + callee
   cards).
6. Evaluates CEL rules where `action == invoke` for the callee card
   (org-global ∪ service-local, deny-overrides).
7. Returns `200 OK` (Allow) or `403 Forbidden` with `PolicyDecision::Deny { reason }`.
8. Asynchronously emits one `PolicyInvokeDecision` observation per check,
   labeled with `Wyrd-Request-Id` (emitted under Wyrd's internal authority —
   server-authored, not caller-signed). Every allow and every deny is
   audited automatically; no developer wiring.

Identity is server-verified from one signed delegated JWT. The pod cannot
self-assert its identity — no env var, no body field, no header carries
identity data the pod authored.

### Audit
Immutable case file. Records the result of an investigation against the
provenance graph; never user-declared scope.

**Creation.** Audit cards are created on-demand only. An investigator —
human or agent — runs a provenance query, decides what is worth pinning,
and snapshots the result. Wyrd does not auto-create Audit cards in v1;
teams that want auto-snapshots wire a `Trigger` + `Operator` using
existing nouns.

**Storage.** Light-card pattern (doctrine #16). The case file lives inline
on the Audit card itself — no separate `Artifact`, no separate chain
table. The card carries the investigation metadata, the query that
produced the chain, the lineage subgraph at snapshot time (cards + edges
by `card_ref`), and the criteria for re-fetching the relevant observations
from vala. Size is bounded by lineage depth, not by observation count.

**Replay.** The card's attributes are the source of truth. The lineage
half is read inline from the card; the observation half is re-fetched by
running the inline criteria against vala's observation store.
`card_ref`s are version-locked (doctrine #8), so lineage anchors stay
valid as long as the registry retains the cited cards. Observation
retention in vala (years) covers the replay window.

```yaml
spec:
  # Why this investigation exists
  purpose: AuditPurpose              # Incident | Compliance | Review | Adhoc
  description?: string               # free-form investigator notes

  # What is under investigation (indexed for search/listing)
  subject_refs: [CardRef]            # the cards this audit centers on

  # What was asked
  query: ProvenanceQuery             # structured, replayable

  # What was found, frozen at snapshot time
  lineage: LineageSubgraph           # cards + edges by card_ref (version-locked)
  observation_criteria: ObservationCriteria  # how to re-fetch from vala

  # What the investigator decided
  status?: AuditStatus               # Open | Mitigated | Resolved | FalsePositive | Suppressed
  findings?: string                  # narrative conclusion

  # Provenance of the audit itself — server-authored, not user-asserted
  investigator: Investigator         # Human(sub) | Agent(card_ref → Agent)
  snapshot_at: Timestamp             # server-stamped
  digest: string                     # canonical-form integrity hash

  details: { string: NonSecretValue }
```

`AuditPurpose` is a closed enum:

| Variant      | Use |
|--------------|-----|
| `Incident`   | Post-incident investigation. Why did the system behave this way? |
| `Compliance` | Evidence pin for a regulatory regime (SOC 2, SR 11-7, AI Act, etc.). |
| `Review`     | Routine review — quarterly, model-risk, change-board. |
| `Adhoc`      | Investigator-initiated; no formal frame. |

`AuditStatus` is a closed enum capturing disposition **at `snapshot_at`**,
not forever. When disposition changes (Open → Mitigated → Resolved), the
investigator writes a new Audit; the chain of Audits is the remediation
timeline.

| Variant         | Use |
|-----------------|-----|
| `Open`          | Confirmed concern, action pending. |
| `Mitigated`     | Compensating control in place; root cause not fixed. |
| `Resolved`      | Addressed — fixed, or review confirmed no issue. |
| `FalsePositive` | Investigation showed no real concern. |
| `Suppressed`    | Real concern; risk explicitly accepted. |

`Investigator` is a closed enum identifying who ran the investigation. The
server fills this from the calling principal; callers cannot self-assert
identity (same posture as `ServiceIdentity`, doctrine #15).

| Variant   | Carries                           |
|-----------|-----------------------------------|
| `Human`   | `sub: string` (verified subject claim) |
| `Agent`   | `card_ref: CardRef` (→ Agent)     |

`ProvenanceQuery` is the structured, replayable question that produced the
lineage. Parametric, not a DSL — Wyrd has no graph DB and no query engine
to drive a DSL through, and the query is stored on every Audit card so it
must replay verbatim forever.

```yaml
ProvenanceQuery:
  roots: [CardRef]              # 1+ subjects, version-locked (doctrine #8)
  direction: TraversalDirection # Upstream | Downstream | Both
  depth?: u32                   # default 8, server-capped (e.g. 32)
```

`TraversalDirection` walks the `card_edges` derived index:

| Variant      | Walks                                  | Means |
|--------------|----------------------------------------|-------|
| `Upstream`   | `card_edges WHERE source ∈ frontier`   | What produced X / what X consumes. |
| `Downstream` | `card_edges WHERE target ∈ frontier`   | What consumes X / what depends on X. |
| `Both`       | union                                  | Full neighborhood. |

The query carries no `kinds` or `space` filter. Both were removed by
design:

- **No `kinds`.** Filtering would silently narrow the pinned subgraph,
  giving future readers a forensic false signal that the investigator
  considered only those kinds. Display filtering is a UI concern; the
  audit pins the whole neighborhood.
- **No `space`.** Visibility is already RBAC-enforced server-side. A
  `space` filter at query time would hide cross-space references — which
  in an investigation are often the finding (e.g. a prod Service
  referencing a staging Model).

`roots` is a list because multi-version subjects are the common case for
incident windows: a Service that bumped from `1.0.0` to `1.1.0` mid-window
has two distinct version-locked roots, and an investigator pinning the
window needs both in one Audit.

`LineageSubgraph` is the frozen graph that `ProvenanceQuery` produces.
Nodes are cards (by reference, not inlined); edges are the typed
`card_ref` fields inside each card's spec that point at other cards. The
subgraph is the explicit topology at `snapshot_at` — readers don't
re-derive it from card specs.

```yaml
LineageSubgraph:
  nodes: [LineageNode]
  edges: [LineageEdge]

LineageNode:
  card_ref: CardRef           # version-locked identity (doctrine #8)
  kind: CardKind              # redundant with card_ref; lets readers filter without parsing
  attributes_digest: string   # JCS canonicalization (RFC 8785) + SHA-256 of the card

LineageEdge:
  source: CardRef             # the card that authored the reference
  target: CardRef             # the card being referenced
  edge_kind: EdgeKind         # semantic relation (closed enum)
  via: string                 # dot-notation path inside source spec, e.g. "spec.components[0].ref"
```

`EdgeKind` is a closed enum, derived directly from the typed `card_ref`
fields in the locked spec model. A new card kind that introduces new
typed refs is a versioned breaking change that adds variants.

| Variant     | Source-card fields                                                |
|-------------|-------------------------------------------------------------------|
| `Subject`   | `Drift.subject_ref`, `Eval.subject_ref`                           |
| `Component` | `Service.components[].ref`, `Workflow.steps[].target`             |
| `Artifact`  | `Data.card_refs[]`, `Model.card_refs[]`                   |
| `Prompt`    | `Agent.prompt`, `Eval.tasks[].LlmJudge.judge_ref`                 |
| `Dataset`   | `Eval.dataset`                                                    |
| `Source`    | `Eval.source_ref`, `Drift.signal.External.source_ref`             |
| `Baseline`  | `Drift.signal.Distribution.baseline_ref`                          |
| `Trigger`   | `Trigger.source.drift_ref \| eval_ref`                            |
| `Operator`  | `Trigger.operator_ref`                                            |
| `Workflow`  | `Operator.action.workflow_ref`                                    |
| `Hook`      | `Operator.pre_invoke`, `Operator.post_invoke`                     |

**Integrity.** Both `LineageNode.attributes_digest` and the Audit card's
own `digest` use the same recipe: **JCS canonicalization (RFC 8785) +
SHA-256**. JCS pins key ordering, number formatting, and whitespace so
two valid JSON serializations of the same record hash identically.
SHA-256 is FIPS 140-3 approved — the boring, auditor-friendly choice
that needs no defense in a SOC 2, SR 11-7, or AI Act review.

**`via` syntax.** Dot notation (`spec.components[0].ref`), not RFC 6901
JSON Pointer. Reasoning:

- Reads like the YAML the author wrote and the agent already sees.
- Grammar is tiny: `<key>`, `<key>.<key>`, `<key>[<index>]`. No quoting.
- Card spec field names are controlled snake_case identifiers — no
  ambiguity risk from user-supplied keys at any structural position
  where a `via` can point.
- LLM training-data weight strongly favours dot notation; agents reading
  audits parse it natively.

**Why edges aren't redundant with nodes.** Nodes carry `card_ref` only —
not the spec content. Without an explicit edge list a reader would have
to fetch every card from the registry and re-parse its spec to rebuild
the graph; that ties replay to live spec-parsing logic that drifts
across API versions. The edge list is the connectivity finding itself,
recorded once at snapshot time.

`ObservationCriteria` is the structured, replayable filter that the audit
hands to vala on creation **and on every replay** to re-materialize the
runtime evidence behind the lineage. Parametric, not a DSL — same posture
as `ProvenanceQuery`. Two required fields fix the floor (what + when);
four optional fields narrow.

```yaml
ObservationCriteria:
  subject_refs: [CardRef]                      # what observations are about (version-locked, doctrine #8)
  time_range: TimeRange                        # closed [from, to] — bounds the case file

  signals?: [Signal]                           # closed-enum filter; absent = all signals
  actors?: [ActorRef]                          # filter on emit_actor (ServiceIdentity / Human); absent = all
  request_ids?: [WyrdRequestId]                # pin to specific runtime hops; absent = no pin
  labels?: { string: string }                  # attribute equality filter (tenant, region, etc.)
```

The required pair gives every audit a deterministic minimum: **the
lifecycle axis** (`subject_refs`, doctrine #8) and **the time axis**
(`time_range`). Everything else narrows.

| Field         | Why it's optional                                                  |
|---------------|--------------------------------------------------------------------|
| `signals`     | Lets the audit pin to e.g. `Drift` only, or `EvalScore` only.      |
| `actors`      | Lets the audit pin to one service identity's emissions.            |
| `request_ids` | Lets the audit pin to specific runtime hops (doctrine #15) when the investigator already knows them. |
| `labels`      | Open-shape narrowing the registry doesn't model — tenant, region, deployment slice. |

Excluded fields, with reasoning matching `ProvenanceQuery`:

- **No `space`.** Already pinned by version-locked `subject_refs`.
- **No `kinds`.** Implied by `subject_refs[i].kind`; an explicit filter
  would silently narrow replay and give future readers a forensic false
  signal.
- **No `limit` / `cursor`.** Replay must return the deterministic full
  set; pagination is a render-side concern at the API boundary.

**Replay determinism.** The same `ObservationCriteria` against the same
vala store at the same logical time returns the same observation set —
the property the audit's `digest` depends on. Vala's append-only,
time-bounded retention (years) covers the window. If a referenced subject
is purged from the registry, the criteria still validates structurally
and vala returns whatever observations remain; the audit's `digest`
captures what *was* materialized at `snapshot_at`.

**Under design (wire shape deferred):**
- Multi-party `attestations` — deferred to v1.1; `details` may carry informally in v1.

### Drift
Observation producer for a single subject. Envelope is orthogonal: subject +
signal + condition + math. No scheduling, no dispatch. Scheduling is a
`Trigger`; dispatch is an `Operator`.
```yaml
spec:
  description?: string
  method: DriftMethod            # Spc | Psi | Custom | External
  subject_ref: CardRef           # → Model | Agent | Service | Data — singular
  signal: DriftSignal            # how the measurement enters the monitor
  condition: DriftCondition      # when a sample becomes an emittable observation
  profile?: DriftProfile         # method-specific math config (PSI bins, SPC window, etc.)
  details: { string: NonSecretValue }
```

Worked examples: `architecture/specs/01-ml-prediction-service.yaml`,
`architecture/specs/04-external-mlflow.yaml`.

**`Agent` is deliberately absent from `DriftMethod` in v1.** Agent-behavior drift
(tool-call distribution shifts, response-format drift, step-count anomalies) is
real but underspecified: it has no settled signal vocabulary, no profile shape,
and no canonical scoring algorithm. Adding the enum variant before that work
lands would freeze a contract we cannot honor. The variant is re-added once a
follow-up design dialogue locks `AgentDriftProfile`, its signal channels, and
the scoring algorithm — tracked as Drift-7 in
`wyrd-plan/plans/phase-4-vala/02-foundations/implementation_plan/03-drift-primitive/12-followups.md`.
Eval-score drift on agents is addressable today via `DriftSignal::EvalScore` +
`DriftMethod::Spc`.

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
observations from** (`source_ref`, deferred), and an optional **offline
driver** (`dataset`). No scheduling, no dispatch, no fire condition — fire
lives on `Drift` with `DriftSignal::EvalScore`. Eval is a single typed task
workflow, not a parallel mode/profile split.
```yaml
spec:
  description?: string
  subject_ref: CardRef           # → Agent | Workflow | Service | Model — WHAT is judged
  tasks: [EvalTask]              # evaluation workflow — DAG via depends_on
  dataset?: DatasetRef           # → Data — offline scenario driver
  source_ref?: CardRef           # DEFERRED — landing in §Eval online-mode commit; not implemented
  sampling?: EvalSampling
  pass_gate?: EvalPassGate
  context_capture?: EvalContextCapture
  workflow?: Workflow
  governance?: Governance
  details: { string: NonSecretValue }
```

The typed-id grammar (`TaskId`, `ScenarioId`, `JsonPath`, `SessionId`,
`RecordId`, `TraceId`, `SpanId`) lives in `wyrd-spec::vala::eval::ids` and
`wyrd-spec::vala::ids`. `EvalStatus` (`Pending | AwaitingTrace | Processing |
Completed | Failed | DeadLettered`) lives in
`wyrd-spec::vala::eval::status`.

**Three modes the same shape supports** (no `eval_mode` discriminator; presence
of refs is the mode):

| `dataset` | `source_ref` | Runtime behavior |
|---------------|--------------|------------------|
| set           | unset        | Offline batch. Engine invokes `subject_ref` against the Data card's scenario rows, captures traces inline. |
| unset         | set          | Online / archived (deferred — DESIGN §13). Engine reads the user's sink, filters records by subject identity, samples records into the task workflow. |
| set           | set          | Same tasks, both modes (online deferred — DESIGN §13). Offline gate and online monitor share one task definition. |
| unset         | unset        | Online over `vala`'s default observation archive. |

**Directional flow.** All three refs are `CardRef`s authored on `Eval`; nothing
points back. At runtime: engine resolves `subject_ref` (identity filter),
resolves `source_ref` (read location, deferred — see DESIGN.md §13), opens the
Source, queries records, feeds them into the `tasks` workflow, aggregates
per-task pass/fail into a score stream consumed downstream by a `Drift` card with
`DriftSignal::EvalScore`.

`EvalTask` is a closed tagged union. Every variant carries `id: TaskId`,
`depends_on: Vec<TaskId>`, and `condition: Option<EvalCondition>`.
`EvalCondition` supports AND/OR chaining bounded at depth 16.

| Variant          | Variant-specific carries                                                                          | Use |
|------------------|---------------------------------------------------------------------------------------------------|-----|
| `Assertion`      | `context_path?: string`, `operator: ComparisonOperator`, `expected: ParameterValue`, `description?: string` | Deterministic check on a dot-path into a record |
| `LlmJudge` (`llm_judge`) | `judge_ref: CardRef` (→ Prompt card), `operator: ComparisonOperator`, `expected: ParameterValue`, `max_retries: u32` | LLM judge: one Prompt card per task, judge response compared to expected value |
| `TraceAssertion` | `span_selector: JsonPath`, `operator: ComparisonOperator`, `expected: ParameterValue`             | OTel span selector (tokens, duration_ms, retry_count, etc.) read via `source_ref` (deferred — see DESIGN.md §13) |
| `AgentAssertion` | `workflow_field_path: JsonPath`, `operator: ComparisonOperator`, `expected: ParameterValue`       | Tool-call / response-shape check read via `source_ref` (deferred — see DESIGN.md §13) |

`ComparisonOperator` is a frozen 56-variant catalog (12 numeric, 15 string,
14 collection, 9 type, 6 tolerance / advanced). Parameterless variants
serialize as scalar `snake_case`; parameterized variants as `kind`-tagged
objects. Canonical source:
`crates/wyrd-spec/src/vala/eval/operator.rs:62-264`. The locked collection /
type / tolerance families are required to keep authors out of LLM-judge calls
for deterministic checks ("agent only used allowed tools" → `IsSubset`, "no
duplicate tool calls" → `UniqueValues`, "score within 10% of baseline" →
`WithinPctTolerance`, "output is valid JSON" → `IsJson`).

`EvalScenario` is the row shape carried by a `Data` card bound to
`dataset` (not an Eval field — scenarios and datasets are the same noun):
```yaml
- id: string
  initial_query: string
  predefined_turns?: [string]      # scripted multi-turn
  simulated_user_persona?: string  # interactive driver
  termination_signal?: string
  max_turns?: u32
  expected_outcome?: string
  tasks?: [ScenarioTask]           # scenario-local tasks (passenger view: final response)
```

`ScenarioTask` is narrower than `EvalTask` — `id`, `operator`, `expected`,
optional `condition` — because scenario evaluation operates on `{response,
expected_outcome}` and is always evaluated together (no DAG).

Scenario-local `tasks` are the **passenger view** (judged against the agent's
final response for that scenario); top-level `Eval.tasks` are the **mechanic
view** (judged against intermediate workflow records / spans / tool calls).
Both run in one pass.

Worked examples: `architecture/specs/02-llm-agent-service.yaml`,
`architecture/specs/03-rag-workflow-service.yaml`.

### Source
Read-side reference to an external data system. **Wyrd reads, never writes.**

`source` is a **read-shape bucket**, not a vendor. The top-level discriminator is
the shape of data a consuming `Drift`/`Eval` Card sees — a row set, a time
series, blobs — so a consumer binds to the shape and never to a vendor. The
vendor (BigQuery vs Snowflake, Prometheus vs Datadog, GCS vs S3) is a
**connection detail nested below the bucket**. This is the same axis `object_store`
already used: `source.kind` was never `gcs` or `s3`; the vendor was the URI scheme.

The bucket set is derived from what consumers read, not from a vendor taxonomy —
which keeps it small, closed, and stable (Doctrine #2). Adding a vendor is a new
`*Connection` variant plus a runtime read adapter in `vala`; it never grows the
bucket set and never touches a consuming Card.

```yaml
spec:
  description?: string
  source:                         # SourceKind — the read-shape bucket; vendor nested below
    kind: <bucket>
  defaults: { string: NonSecretValue }   # non-secret read hints (projection, page size)
```

`SourceKind` is a closed tagged union keyed by **bucket** (`kind` discriminator).
Each bucket carries one uniform read contract; the vendor is a nested
`*Connection` union keyed by `vendor`:

| Bucket (`kind`)  | Read contract (uniform within bucket) | Vendor union (`vendor`)                 |
|------------------|---------------------------------------|------------------------------------------|
| `object_store`   | blobs at `uri` → records by `format`  | URI scheme (`gs://`/`s3://`/`az://`/`file://`) |
| `sql_warehouse`  | SQL → row set                         | `bigquery` \| `snowflake` \| `postgres`  |
| `metrics`        | query → labeled time series           | `prometheus` \| `datadog` \| `cloudwatch`|
| `logs`           | query → log records                   | `loki` \| `elasticsearch` \| `splunk`    |
| `traces`         | query → spans                         | `tempo` \| `datadog_apm` \| `jaeger`     |

The bucket set lines up with what `Drift`/`Eval` already read: `object_store`/
`sql_warehouse` feed `DriftSignal::Distribution` (rows), `metrics` feeds
`DriftSignal::Metric`, `traces` feeds `EvalTask::TraceAssertion`.

**Secrets never live on the Card** (Doctrine #7). Every vendor connection carries
a `SourceAuth` whose secret material is a **named server-side env var**, resolved
at read time — only the env-var *name* is on the Card, exactly like
`Operator.Http.auth.env`. `SourceAuth` is a closed tagged union (`scheme`
discriminator): `None`, `Env { env }`, `Basic { username, password_env }`,
`MultiEnv { vars: { logical_name: env_var } }` (covers multi-key vendors like
Datadog's api-key + app-key), `SecretStore { provider, name }` (Vault / AWS SSM /
GCP Secret Manager — carries the store identifier and secret path, never the value).

```yaml
# sql_warehouse — vendor + non-secret coordinates + env ref for the secret
source:
  kind: sql_warehouse
  connection:
    vendor: snowflake
    account: acme-prod
    warehouse: analytics
    database: telemetry
    schema: public
    role: reader
    auth:
      scheme: multi_env
      vars:
        user: SNOWFLAKE_USER
        private_key: SNOWFLAKE_PRIVATE_KEY   # names only; never values

# object_store — vendor implied by URI scheme; auth defaults to None (ambient IAM)
source:
  kind: object_store
  uri: gs://acme-telemetry/runs
  format: parquet
```

**Runtime read adapter.** `vala` owns the read driver, keyed on
`(kind, vendor)`. The three connection families (object/blob list-and-read, SQL
query, HTTP query) are different drivers behind one read trait; the Card schema
never declares strategy — `vala` chooses it from the bucket and vendor, the same
way it chooses the Drift/Eval evaluation strategy.

### Bifrost — Wyrd's OLAP warehouse

`Source` is the external read side ("Wyrd reads, never writes" — Doctrine #7).
**Bifrost** is its Wyrd-owned counterpart: the public OLAP warehouse surface and
analytical storage substrate `vala` uses to record Wyrd's **own** observations
(drift events, eval records, OTel / GenAI traces, audit projections, and future
analytical tables). It is Wyrd server state, not an external system and not a
vendor.

**Bifrost is not a Card kind, and there is no `WarehouseCard`.** It is the
general-case storage *shape*, not a registry entry. Per Doctrine #2 (one fact,
one owning Kind) and Doctrine #7, internal observation storage is owned wholly by
`vala`; nothing an author writes points at it, so it has no card identity. The
external read-shape buckets above (`object_store`, `sql_warehouse`, …) describe
data Wyrd *reads*; `sql_warehouse` is an external `SourceKind` and is unrelated
to Bifrost. Use `Bifrost` for Wyrd's OLAP warehouse. Do not introduce a
`warehouse` noun on public API paths, Python modules, Card kinds, resources, or
internal surfaces — it would collide with the external `sql_warehouse` Source
semantics.

**Everything is a Bifrost table.** One table shape underlies every internal
analytical table. Every physical schema has a server-owned managed envelope
with these required non-null columns:

- `wyrd_event_time`: the validated or server-derived event timestamp;
- `wyrd_ingested_at`: the server-stamped ingestion timestamp;
- `wyrd_batch_id`: the immutable 16-byte identity of one accepted logical
  batch;
- `wyrd_row_ordinal`: the zero-based position of a row in that complete
  logical batch, stored as an Iceberg `int` / Arrow `Int32`;
- `wyrd_request_id`: the server-minted or validated request-correlation ID;
- `data_tenant_id`: the authenticated tenant-isolation key.

Physical schemas also reserve nullable `run_id`, `card_uid`, and
`principal_id` correlation columns. Their values may be absent according to
the table's correlation policy and do not participate in physical row
identity.

Within one organization-qualified physical table, the immutable row identity
is:

```text
(wyrd_batch_id, wyrd_row_ordinal)
```

The globally qualified row identity is:

```text
(organization_id, logical_table, wyrd_batch_id, wyrd_row_ordinal)
```

`wyrd_row_ordinal` is contiguous across the logical request order and never
resets at an Arrow `RecordBatch`, WAL segment, Parquet file, or Forge rewrite
boundary. The accepted batch-row limit is below `i32::MAX`; negative or
non-contiguous ordinals are invalid. The client SDK generates one UUIDv7 batch
identity and preserves it unchanged across retries. Gate validates that
envelope value and assigns ordinals before Scribe admission; payload columns
cannot supply or override either identity field or other server-owned request,
ingestion, or tenant columns. Scribe, WAL, Parquet, Iceberg, and Forge preserve
the pair unchanged. Within the configured Scribe idempotency-retention window,
a replay may reuse an accepted batch identity only when schema fingerprint,
row count, row order, and payload digest match; otherwise it fails with the
stable Bifrost batch-identity conflict. Outside that window, callers must never
reuse a batch ID for a different logical batch. Oracle uses the pair to
reconcile live and sealed sources, with the fixed Iceberg snapshot winning when
both sources contain the same identity.

The physical table identity is always the authenticated organization plus the
logical table:

```text
(organization_id, logical_table) → one physical Iceberg table
```

The logical `TableRef` remains tenant-free and carries the table namespace and
local name. The Bifrost catalog derives the organization-qualified Iceberg
namespace, object-store prefix, and physical table from the authenticated
organization and `TableRef`. Every physical Parquet file carries the
server-stamped `data_tenant_id`. Gate and Scribe reject a binding whose tenant
does not match the authenticated organization; Oracle adds the plan-root
`TenantTripwireExec` and fails closed with `WYRD_VALA_500_TENANT_TRIPWIRE` on a
row mismatch. There is no shared physical table layout and no deployment-
specific storage mode.

Logical definition ownership is separate from physical identity. Wyrd ships
server-owned built-in definitions for tables such as spans, GenAI, eval, drift,
and audit, while users register dataset definitions. “Built-in” and “system”
describe who owns the immutable logical definition; they are not table scopes.
Every instantiated built-in or user-defined table uses the same
organization-qualified physical binding above.

The substrate is Apache Iceberg-managed Parquet in object storage, with Postgres
as the Iceberg catalog and control plane and DataFusion as the query engine —
consistent with Doctrine #4 (Postgres is control-plane only; analytical data
lives in object store). Runtime ownership stays in `vala`: the `vala-bifrost`
engine crate owns the Iceberg/DataFusion warehouse engine, `vala-ingest` owns
gRPC ingest _(under revision — serving ownership moving to wyrd-server, reconciled in a follow-up design pass)_, and `wyrd-spec::vala::api` owns the
public wire contracts. HTTP serving for these routes now belongs to `wyrd-server`:
the eval consolidation dissolved the former `vala-http` crate, per the principle
below. Python-visible Bifrost behavior lives in `vala-sdk` (the
approved Vala Python owner crate) behind its optional `python` feature.

**Principle — wyrd-server is the only serving surface.** `vala-*` crates are
engine and data-plane libraries; they are never HTTP or gRPC serving crates.
The Redux Gate may implement approved tonic protocol adapters and bearer-token
verification through `wyrd-auth-verify`, but it does not serve the network.
Only `wyrd-server` binds sockets, owns listeners and top-level routing,
terminates TLS, performs boot/readiness, and controls server lifecycle.
`wyrd-server` is the single process that binds ports and owns all HTTP/gRPC
serving. The eval consolidation (commits 01–05) is the first realization of
this principle; Bifrost/ingest serving reconciliation follows in a separate
design pass. The eval pull-protocol session-run (`/v1/eval/runs/{run_id}`) is an
ephemeral server-side session entry for concurrency and ownership tracking; it
is distinct from the doctrinal `RunRef` — the Card→Run→Observation run is a
client-side execution record (see the "There is no run registry" note under
_Observation identity — `Card → Run → Observation`_), never server-persisted.

**Platform audit sentinel.** `DataTenantId::SYSTEM_OWNER` is the established
durable platform tenant for security events that cannot safely be attributed
to caller-controlled tenant data, including peer tickets rejected before
verified claim decoding. Platform migrations must provision this sentinel
idempotently. Callers must never create or select an audit tenant from
unverified payload bytes.
Its canonical row is UUID `00000000-0000-0000-0000-000000000000`, slug
`wyrd-system`, display name `Wyrd System`, status `active`, and
`deleted_at IS NULL`. Provisioning and boot verification fail closed rather
than overwriting or accepting conflicting UUID/slug ownership or incompatible
attributes.

**Public surface.** Bifrost is a stable Wyrd public surface across HTTP, gRPC,
Python, generated schemas, MCP/agent documentation, and stable error codes.
The public contract includes:

- HTTP table management under `/v1/bifrost/tables`, served by `wyrd-server`.
- HTTP query surfaces served by `wyrd-server` under the `/v1` nest:
  `POST /v1/query` returns the terminal-safe, length-delimited Oracle frame
  stream with media type
  `application/vnd.wyrd.bifrost-query-stream` and only
  `x-wyrd-schema-fingerprint` as initial query metadata. There is no
  asynchronous query-job route family, status polling contract, result
  redirect, or initial row-count header. Future asynchronous jobs require a
  new architecture decision rather than a compatibility surface. The typed
  observation query routes under `/v1` (see `ValaQueryService` below) are the
  companion projection surface. `wyrd-spec::vala::api` owns all retained
  query/freshness wire types.
- gRPC ingest through `wyrd.v1.BifrostIngestService` _(under revision — serving ownership moving to wyrd-server, reconciled in a follow-up design pass)_.
- The `wyrd.bifrost` Python SDK submodule.
- Generated `wyrd-spec::vala::api` wire types such as `BifrostTableEntry`,
  register-table types, and query request/response types. Physical table
  identity is server-derived from the authenticated organization and logical
  table; it is not a caller-selected scope.
- The `WYRD_VALA_*_BIFROST_*` error catalog crossing HTTP, MCP, Python, and
  generated documentation boundaries.

Bifrost permissions are resource-scoped through `BifrostTable`, `BifrostRecord`,
and `BifrostQuery`. Generic record writes must not write reserved or
system-managed Bifrost tables. There is no `wyrd.warehouse` submodule and no
`WarehouseCard`.

**`ValaQueryService` — typed observability query surface (accepted, Stage 4).** `wyrd-server`
exposes `wyrd.v1.ValaQueryService` (gRPC-first) with an axum HTTP projection as the
**query-only** typed surface for the observability domain namespaces. There is no
`vala-http` crate — `wyrd-server` is the only serving surface. The gRPC service and
its HTTP projection are backed by `wyrd-spec::vala::api` request/response contracts with
cursor pagination, mandatory time windows, and stable `WYRD_VALA_*` error codes.

The accepted domain namespaces and tables:

| Namespace | Tables | Notes |
|---|---|---|
| `vala.traces` | `spans`, `events`, `links` | Raw OTel spans — source of truth |
| `vala.metrics` | `points` | OTel metric data points with exemplars |
| `vala.logs` | `records` | OTel LogRecord signal |
| `vala.genai` | `messages`, `embeddings`, `tool_calls`, `memory` | Derived from `vala.traces` spans carrying `gen_ai.*` attributes |
| `vala.eval` | `runs`, `assertions` | Agent/LLM evaluation records |
| `vala.drift` | `*` | Traditional ML drift records |
| `vala.dev` | `agent_traces` | High-fidelity coding-harness traces; carries the code axis |
| `vala.system` | `audit_log` | Transactional audit log (relay-written) |

All domain tables use the organization-qualified physical-table rule above.
Typed query routes build bound DataFusion `LogicalPlan`s (never `ctx.sql`); a
query-admission gate requires the authenticated organization/table binding and
a bounded time window before execution. Oracle's provider applies the
tenant-tripwire boundary before execution.

**Elevated payload-read permissions.** Four payload-bearing table families are
`PayloadClass::Sensitive` and gate their sensitive columns on an elevated read permission
beyond the base `BifrostQuery:Read`:

| Permission resource | Gates |
|---|---|
| `BifrostTracePayload` | `vala.traces.*` `attributes` column on `GetTrace` / `QueryTraces` |
| `BifrostLogPayload` | `vala.logs.records` `body` / `attributes` on log-query methods |
| `BifrostGenAiPayload` | `vala.genai.*` message and tool I/O columns |
| `BifrostAgentTracePayload` | `vala.dev.agent_traces` message/tool payload columns |

These four permissions extend the existing `Permission`/`Resource` model in
`crates/shared/wyrd-runtime/src/permission.rs`. The generic-SQL analyzer enforces the
same payload gate so `SELECT vala.traces.spans.attributes` without
`BifrostTracePayload:Read` is denied through both the typed and the generic-SQL path.

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
| `PagerDuty` | `severity`, `summary`, `dedup_key?: string`                                   |
| `Slack`     | `text: string`                                                                |

`HttpMethod`: closed enum — `Get | Post | Put | Patch | Delete`.

`HttpAuth` (closed tagged union):
- `None`
- `Bearer { env: string }` — `env` names a server-side env var holding the token
- `Basic { env: string }` — env var holds `user:password`
- `Header { name: string, env: string }` (covers `X-API-Key`,
  `Authorization: token <foo>`)

Wyrd-the-server resolves `env` by reading the process environment at fire time;
missing env vars fail the action closed. Cards never carry secret material.
Notify channels (Slack/PagerDuty) and Source (S3/GCS/Azure) resolve their
credentials from the server's own configuration — webhook URLs, routing keys,
and object-store credentials live in the server's env or its operator config,
not in the card.

`HttpBody`: structured JSON (`JsonValue`). Any string leaf may contain
`{{...}}` placeholders the server interpolates at fire time. Same templating
applies to `Http.url` and to text fields in `NotifyChannel` variants.

Templating context comes from the Trigger that fired the Operator:
- `Trigger.source = Drift { drift_ref }`: `drift.{name, subject_ref.{kind, name, version}}`, `observation.{value, threshold, fired_at}`.
- `Trigger.source = Eval { eval_ref }`: `eval.{name, subject_ref.*}`, `failures[]` (per-task failure entries).
- `Trigger.source` absent: `schedule.fired_at` only.

Exact field schema for each context lives in OpenAPI.

---

## Spec-file authoring

Two complementary mechanisms — `wyrd apply -f file.yaml` reads + registers in
one move, and reference slots accept `ref | select | path | inline` so a card
can be pinned by identity, matched by metadata, split into its own file, or
inlined where its own identity isn't needed. `select` and `path` are authoring
sugar the loader resolves to `ref` before send.

### Pre-registration matrix (Rule 16)

| Kind | Must pre-register? | Why |
|------|---------------------|-----|
| `Model`        | **Yes** | Carries weight artifacts; lineage anchor. |
| `Data`         | **Yes** (unless used purely as inline eval scenarios, which v1 does not support — `dataset` is `DatasetRef`-only) | Carries dataset bytes; lineage anchor. |
| `Experiment`   | **Yes** | Carries run history. |
| `Artifact`     | **Yes** (typically derived from heavy cards) | Pointer to durable bytes. |
| `Prompt`       | Optional | Light. Inlineable as `PromptRef` inside `Agent.prompt`; referenced by `EvalTask::LlmJudge.judge_ref`. |
| `Agent`        | Optional | Light. Spec-only; `apply` registers it. No v1 field accepts inline `AgentRef` (see Q11). |
| `Eval`, `Policy`, `Trigger`, `Operator`, `Source`, `Mcp`, `Workflow`, `Audit`, `Service` | Optional | Light. Spec-only; `apply` registers each card as it's read. |

A single YAML file may contain many `---`-separated card documents — `wyrd
apply -f eval-suite.yaml` registers all of them in dependency order. The Agent
under test, the Source it reads from, and the Eval that judges it can all
ship in one file.

### Reference forms

A reference slot accepts exactly one of four keys: `ref`, `select`, `path`,
or `inline`. The key IS the discriminator; there is no separate `kind:` tag
for the variant. Two of the four are durable wire forms (`ref`, `inline`);
the other two (`select`, `path`) are client-side authoring sugar that the
loader resolves to `ref` before anything leaves the client. The four forms in
isolation:

```yaml
# 1. ref — points at a registered card by exact identity.
#    kind, name, version, space, optional uid. No labels/annotations here.
ref: { kind: Policy, name: pii-redaction, version: "1.0.0", space: prod }

# 2. select — authoring sugar. Match a registered card by metadata; the loader
#    resolves it to exactly one uid-bearing ref before send.
select:
  kind: Model
  name: churn-classifier
  labels: { environment: production, stage: champion }

# 3. path — authoring sugar. Targets a FULL card envelope on disk
#    (apiVersion + kind + metadata + spec). The loader registers it as an
#    independent card and rewrites this slot to `ref: CardRef`.
path: ./policies/pii-redaction.yaml

# 4. inline — full spec body embedded in the parent. No card identity.
inline:
  kind: Policy
  rules:
    - name: redact-ssn
      expression: "message.contains_pii('ssn')"
      action: gate
  scope: service_local
```

The wire `CardRef` requires `space`. Omitting `space` in an authored `ref:` is
a loader-time convenience: the YAML loader splices the enclosing card's
`metadata.space` into each child `ref:` before deserialization. A `CardRef`
that has crossed an API boundary always carries `space` verbatim. `CardRef` is
exact identity only — it never carries `labels` or `annotations`. Metadata
matching is the job of `select`, not `ref`.

In context — `Service.components[]` mixing all four forms plus a heavy-card ref:

```yaml
components:
  - alias: agent
    ref:  { kind: Agent, name: support-triage,   version: "1.0.0", space: prod }
  - alias: model
    select:                       # heavy card resolved by metadata, then pinned
      kind: Model
      name: churn-classifier
      labels: { environment: production, stage: champion }
  - alias: prompt
    path: ./prompts/triage-system.yaml
  - alias: pii-policy
    inline:
      kind: Policy
      rules:
        - name: redact-ssn
          expression: "message.contains_pii('ssn')"
          action: gate
      scope: service_local
```

### Reference-slot inventory

The four-key shape applies at **every** reference slot, light or heavy — not a
per-surface subset. There is one canonical slot inventory, and the loader,
`$service.*` sugar rewrite, diagnostics, and relationship tests all project
from it (they do not maintain parallel hand-written lists):

- `Service.components[]` (light and heavy targets)
- `Agent.prompt`
- `Trigger.target`
- `Workflow.steps[].target`
- `Eval` task refs, including `EvalTask::LlmJudge.judge_ref`
- `Drift.subject_ref`, `Drift.signal.eval_ref`, `Drift.signal.source_ref`
- Heavy anchors: `Model`/`Data`/`Experiment` `*_refs`, `Artifact` refs

A new reference-bearing field is added to this inventory in one place; it then
participates in path rewrite, select resolution, and relationship derivation
without a second edit. This is the single-source rule for reference slots.

### Selector resolution rules (loader contract)

`select:` is **client-side authoring sugar**, not a wire variant. It matches a
registered card by metadata and resolves to a concrete `ref`. Use it when the
referrer and the target carry independent metadata — e.g. a `Service` that
wants "the production champion Model" without hard-coding the Model's version.

- `CardSelector { kind, name?, version?, space?, labels?, annotations?,
  latest? }`. `kind` is required; every other field narrows the match.
- `wyrd plan` / `wyrd apply` resolves `select` against target-card metadata to
  exactly one `uid`-bearing `CardRef` **before** durable registration,
  relationship derivation, hydration, or any runtime observation.
- Zero matches is the stable error `CARD_SELECTOR_NOT_FOUND`; more than one is
  `CARD_SELECTOR_AMBIGUOUS`. `latest: true` breaks a version tie by newest
  registered version but never resolves a `labels`/`annotations` ambiguity.
- Environment and stage are selected here, over `labels`/`annotations` — never
  over `space`. `space` is team/workspace scope.
- After resolution the durable payload carries only the pinned `ref`; the
  registry, relationships, and `vala` never see a `select:` value.

### Path resolution rules (loader contract)

`path:` is **client-side authoring sugar**, not a wire variant. It targets a
full card envelope on disk; the loader registers that envelope as an
independent card and rewrites the referring slot to `ref: CardRef`. The
payload that leaves the client contains only `ref` or `inline`. The server,
registry, and `vala` never see a `path:` value.

- Resolved **relative to the file containing the `path:` reference** — not
  CWD, not apply-root.
- Absolute paths are allowed but discouraged (breaks portability across
  machines and CI).
- The referenced file is a **full card envelope** — `apiVersion` + `kind` +
  `metadata` (`space`, `name`, `version`) + `spec`. Registration derives the
  child's identity from that `metadata`, and the parent slot becomes a `ref:`
  to it. This is what makes a `path:` target reusable and lineage-anchoring,
  unlike `inline:`.
- Path imports **are registered as independent cards**, in dependency order,
  ahead of the parent that references them. A heavy-card envelope reached by
  `path:` still follows the normal pre-registration + blob-upload flow before
  the parent registers.
- Transitive: a `path:`-loaded envelope may itself contain `ref`/`select`/
  `path`/`inline` slots. The loader resolves transitively with a hard depth
  limit (≤8) to catch cycles.
- `ref`, `select`, `path`, and `inline` are mutually exclusive on any single
  slot. Any combination is a validation error.

This keeps the wire contract tight (two-variant `LightRef` post-loader:
`ref | inline`), keeps `select`/`path` resolution client-side, prevents
filesystem-on-server, and gives authors both the file-splitting ergonomic they
expect from JSON-Schema `$ref` / OpenAPI external-file imports (`path`) and
metadata-driven binding (`select`).

---

## Workspace config (`wyrd.toml`)

Wyrd's wire contract pins every card to a full `(kind, name, version,
space)` identity. Authors writing many cards in one bundle pay a
verbosity tax for that strictness — `space: prod` and
`version: "1.4.2"` repeat across every doc in a file. This section
formalizes the loader-side ergonomic that resolves it without
touching the wire.

### File and discovery

- Filename: `wyrd.toml`, at the apply-root of the bundle.
- Discovery: the CLI and SDK ancestor-walk from the working
  directory and use the first `wyrd.toml` found, **capped at the
  nearest `.git` ancestor or `$HOME`, whichever comes first**. The
  walk does not cross those boundaries even when no `wyrd.toml` is
  present. The cap mirrors the affordances Cargo (`.git`) and npm
  (package root) already give engineers and prevents a stray
  `wyrd.toml` outside the user's workspace from being silently
  loaded in CI runners, containers, or shared user homes.
- Explicit override: `WyrdConfig::load(Some(&path))` accepts an
  exact file path (used by the future `wyrd --config <path>` flag
  and `WYRD_CONFIG` env var; both are spec'd but not wired in this
  packet). Explicit relative paths are preserved as-given; the
  ancestor walk does not run.
- Absent file: not an error. The CLI/SDK operates with system
  defaults only.

### Schema

```toml
[defaults]
space = "prod"

[defaults.labels]
team   = "churn-ml"
domain = "customer"

[defaults.annotations]
"acme.com/owner" = "data-platform"

# Per-kind override; PascalCase keys match CardKind serialization.
[kind.Model]
space = "ml-prod"

[kind.Policy.labels]
tier = "governance"
```

- Defaultable fields: `space`, `labels`, `annotations`.
- **`name` is never defaultable.** Every card's identity must be
  authored. A `name` key under `[defaults]` or any `[kind.<X>]`
  deserializes to a typed error.
- **`version` is intentionally not defaultable in v1.** The register
  path treats a `None`, `Scope`, or `Pin` version as three distinct
  authored intents (auto-bump from latest, prefix-line bump, exact
  pin). Filling `metadata.version` from the loader would silently
  change which branch the server takes — a wire-shape violation
  even though the field itself remains in the payload. The
  `bump_intent` semantics required to make defaulting safe are
  out of scope for this packet; `version` returns to `[defaults]`
  alongside them.
- **Per-kind table keys are PascalCase** matching
  `CardKind::wire_name()` — the same identifier appears identically
  in YAML (`kind: Model`), TOML (`[kind.Model]`), and Rust source.
  `[kind.External]` is reserved as a forward-compat catch-all on
  the wire and is **not** a valid override-table key here; using
  it raises `WYRD_CFG_400_SCHEMA_MISMATCH`.
- **Unknown tables and unknown keys are errors**
  (`#[serde(deny_unknown_fields)]`). A typo like `[default]`
  (missing `s`) surfaces at parse time, not as silent drop. The
  `deny_unknown_fields` posture means any future top-level table
  addition requires a coordinated client release.

### Precedence (most specific wins)

1. Value explicit in the card YAML.
2. `[kind.<Kind>]` table value.
3. `[defaults]` table value.
4. System fallback (`space = "default"`, empty `labels` /
   `annotations`).

### Merge rules

- **Scalars** (`space`): set if the card field is `None`; otherwise
  leave.
- **Maps** (`labels`, `annotations`): per-key merge. For each key
  present in the config, insert into the card's map only if absent.
  Per-card values always win for their own keys; other config keys
  still apply.
- **`version`, `name`, `uid`, `bump`, `spec_hash`, `artifact_hash`
  are never touched** by the loader. The first three are author
  identity / intent (lock L4 plus Q5 for `version`); the last three
  are server-derived.

This is the same architectural slot as the loader-side
`metadata.space` inheritance documented in "Light-card reference
forms" above: the wire payload still arrives at the server fully
populated; the server never reads `wyrd.toml`.

### No lockfile by design

Wyrd has no version-range resolution. Every `CardRef` is authored
exact (strict `MAJOR.MINOR.PATCH`, no `latest`, no comparators), so
there is nothing to "pin" that isn't already pinned at the source.
Adding a `wyrd.lock` would teach users a mental model that contradicts
the wire contract — they would expect `version: latest` to be
resolvable, and the honest answer is "no."

A future `[dependencies]` table in `wyrd.toml` is reserved for
cross-tenant foreign-card content pinning (a `go.sum`-flavored
integrity check, not a range resolver). It is **out of scope** until
cross-tenant card import is a supported workflow.

---

## Reference-direction quick reference

| Card    | Refs that authored on it             | Refs that point at it          |
|---------|--------------------------------------|--------------------------------|
| Data    | `card_refs`, `splits`            | `Drift.signal.baseline_ref`, `Eval.dataset`, `Experiment.target_refs` |
| Model   | `card_refs`                      | `Drift.subject_ref`, `Eval.subject_ref`, `Service.components.ref`, `Experiment.target_refs` |
| Agent   | `prompt`, `tool_names`               | `Drift.subject_ref`, `Eval.subject_ref`, `Service.components.ref`, Agent prompts (sub-agent calls) |
| Workflow| `steps.*.target`                     | `Eval.subject_ref`, `Service.components.ref`, `Operator.action.workflow_ref` |
| Mcp     | `server_name`, `transport`, `scopes` | `Service.components.ref` |
| Drift   | `subject_ref`, `signal.*` (`baseline_ref` \| `eval_ref` \| `source_ref`) | `Trigger.source.drift_ref`, `Drift.signal.eval_ref` (other Drifts watching an Eval indirectly) |
| Eval    | `subject_ref`, `dataset`, `source_ref` (deferred), `tasks[].LlmJudge.judge_ref` (Prompt card ref) | `Drift.signal.eval_ref`, `Trigger.source.eval_ref` |
| Audit   | `subject_refs`, `query` (roots), `lineage` (nodes), `investigator` (Agent variant) | — |
| Service | `components[].ref`                   | `Drift.subject_ref` (service-level), `Eval.subject_ref` |
| Policy  | `rules`                              | `Service.components.ref`, `Operator.pre_invoke`, `Operator.post_invoke` |
| Trigger | `schedule`, `source.drift_ref` \| `source.eval_ref`, `operator_ref` | — |
| Operator| `action` (`workflow_ref` \| typed `channel` shape \| `auth.env`), `pre_invoke`, `post_invoke` | `Trigger.operator_ref` |
| Source  | `kind` (bucket), `connection` (vendor + `*_env`) | `Drift.signal.source_ref` (External variant), `Eval.source_ref` |

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
│   └── service-quarterly.yaml   # case file capturing a periodic service review
└── reactions/
    ├── pageroncall-operator.yaml
    └── latency-page-trigger.yaml
```

---

## Questions and resolutions

**Open**

1. Per-component Policy binding on `ServiceComponent`. Workaround: rule
   expressions scope by `agent.name`. Decision pending a real use case.
2. Format negotiation for `object_store` Source — schema-on-read vs registered
   schema reference.
3. Time-window semantics for how Drift/Eval cards describe the read range
   over `source_ref`.
4. Service-level Drift subject semantics — what "drift on a Service" computes
   when Wyrd reads internal traces vs external Sources, given the subject is
   singular.
5. Default Source binding at the Service or Agent level to avoid repeating
   `source_ref` on every Drift/Eval.
6. Whether tool hook phases need a closed enum on `Policy.rules` or can stay
   off the wire entirely (no consumer today).

**Resolved**

- **Source vendor read adapters.** `SourceKind` is a closed, bucket-keyed tagged
  union (`object_store`, `sql_warehouse`, `metrics`, `logs`, `traces`); each
  bucket nests a `vendor`-keyed `*Connection` union, and secrets are server-side
  env-var names (`SourceAuth`), never card values. Adding a vendor is a new
  `*Connection` variant plus a `vala` read adapter — no bucket or consumer churn.
  Remaining runtime work: the `vala` read trait keyed on `(kind, vendor)`, and a
  connectivity preflight (`wyrd source check <ref>`).

- **Eval signal decomposition.** `Eval` does not carry its own `signal`
  decomposition. Eval IS the signal — its per-task pass/fail aggregates into a
  score stream consumed downstream by `Drift` with `DriftSignal::EvalScore`. The
  input edges (`dataset` vs `source_ref`) are optional refs, not a tagged enum:
  presence is the mode (offline driver, online sink, both, or neither → vala
  default archive).

---

## Appendix: Implementation status

This section tracks reconciliation work in progress. It is internal bookkeeping
and does not affect the protocol contract above.

**Active reconciliation (as of v1):**
- The active doctrine is the 16 native kind catalog plus `External`. Current
  `wyrd-spec` code and generated schemas no longer expose stale `Tool`, `Skill`,
  or `SubAgent` specs; they now expose `SourceSpec` (the bucket-keyed `SourceKind`
  union). New work must follow this document: tools are runtime registry entries,
  sub-agency is an Agent relationship, skills are not a v1 Card kind, and external
  observations are read through `Source` cards.
- Do not expand stale card kinds or cite generated schema presence as doctrine.
- Remaining `Source` follow-up: the runtime read adapter in `vala` keyed on
  `(kind, vendor)` and the `wyrd source check` preflight.

**Governance token removal (doctrine #18):**
- Emit is an Auth-plane route, not a third plane. The JWT (`principal.card_ref`)
  plus `run_id` carry everything an emission needs.
- The following scaffolding has been deleted: the `wyrd.auth_governance_tokens`
  table (migration `20260601000001_auth.sql`), its `GovernanceTokenRow` row mirror
  and query slot, the `migration_pg.rs` table assertion, and `Scope::TokenIssue`
  (`token:issue`).
- Wyrd is open source and independently publishable. It contains no enterprise
  licensing keys, feature gates, startup hooks, or private-product contracts.
  A future private `wyrd-enterprise` repository may depend on and extend public
  Wyrd crates; Wyrd never depends on that private repository. Enterprise
  deployment language in this document describes topology, tenant isolation,
  and operational requirements rather than an in-tree commercial edition.
- No `WYRD_GOV_TOKEN` env var. No `wyrd gov-token` CLI.
