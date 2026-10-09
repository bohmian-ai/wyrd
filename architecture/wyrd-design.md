# Wyrd Design

**Version:** v1

This document defines the Wyrd protocol. It is normative and stateless:
decision history and implementation progress live outside the architecture.

When this disagrees with generated contracts, examples, or the Rust code in
`crates/wyrd-spec`, **this file wins**. Downstream artifacts are brought up to
this version in a sync pass.

Current authority is not immutable design. An approved feature may replace a
decision here when the new design better serves the user workflow. Such a
change names the superseded decision and updates this document before or in the
same cohesive change as the contracts and implementation that depend on it.
Until that update lands, an unexplained disagreement is unresolved design
drift, not permission for code and documentation to diverge.

---

## Table of contents

- [Doctrine](#doctrine) — 21 design principles
- [Client model](#client-model) — language-agnostic protocol and first-class SDKs
- [Kind catalog](#kind-catalog) — 15 registrable kinds + External discriminator
- [Per-kind specs](#per-kind-specs) — field shapes per kind
  - [Data](#data) · [Model](#model) · [Artifact](#artifact) · [Experiment](#experiment)
  - [Prompt](#prompt) · [Agent](#agent) · [Workflow](#workflow) · [Mcp](#mcp)
  - [Service](#service) · [Policy](#policy) · [Audit](#audit)
  - [Verifier](#verifier) · [Source](#source) · [Bifrost](#bifrost)
  - [Trigger](#trigger) · [Operator](#operator)
- [Registry lifecycle](#registry-lifecycle) — composite registration, card blob, idempotency
- [Bifrost design](./bifrost-design.md) — OLAP warehouse: tables, Scribe ingest, Oracle admission, Forge, public surface
- [Spec-file authoring](#spec-file-authoring) — `ref` / `path` / `inline`, pre-registration matrix
- [Workspace config](#workspace-config-wyrdtoml) — `wyrd.toml` defaults and merge rules
- [Reference-direction quick reference](#reference-direction-quick-reference) — who refs whom
- [Worked directory layout](#worked-directory-layout) — example deployment tree
- [Decisions](#decisions) — protocol choices that constrain implementations

---

## Doctrine

Wyrd is open-source verification and assurance infrastructure for AI systems.
Its primary product outcome is verification: evaluating observed behavior
against declared expectations for exact, versioned system components. Assurance
is the confidence supported by durable evidence that makes those judgments
attributable, reproducible, governable, and auditable. Cards, observations,
lineage, policy, audit, and analytical storage are the verification substrate,
not a passive integration or inventory product.

1. **Cards are independent.** No card "owns" another. The
   deployment unit is a directory of card YAMLs applied together.
2. **One fact, one owning Kind.** If a field could live in two places, the
   doctrine has a gap. Surface it.
3. **Deployment composition declares verification bindings; Verifiers are subject-less.**
   `verified_by` on a Service component, Service, or standalone Agent binds a
   reusable Verifier Card to that exact owner version and subject occurrence.
   Runtime observations supply subject identity; the binding supplies the
   Verifier, Trigger activation, and optional failure Operators.
4. **Reactions are Operators; activation is Triggers.** Verifier/Policy never
   inline reaction logic.
5. **Service composes deployment and subscription wiring, not monitor
   definitions.** Verifier/Trigger/Operator/Audit/Source are peer cards, not
   Service components; component verification bindings point to those peers.
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
13. **No event vocabulary on the wire.** "Observation" comes from Verifier implementations;
    "activation" comes from the closed `TriggerSpec` activation. Free-form
    event-name strings are doctrine drift.
14. **Host config stays off Cards.** Permission modes,
    sandboxes, isolation, effort, and per-CLI compatibility are properties of
    the host that runs the Agent, not of the Agent contract.
15. **Verifiers are pure judgment producers.** Drift and Eval implementations describe what
    is observed and what counts as an observation. They do not carry
    scheduling and they do not carry dispatch. Scheduling lives on
    `Trigger.schedule`. Dispatch lives on `Operator`. There is no `Alert`
    kind — alerting is an Operator with a notification adapter.
16. **Heavy cards anchor lineage; light cards are spec-only.** Model, Data, and
    Experiment carry durable artifact bytes and MUST be pre-registered before
    anything else can point at them — they're the lineage anchors. Every other
    kind (Prompt, Agent, Verifier, Policy, Trigger, Operator, Source, Mcp,
    Workflow, Audit, Service) is spec-only: `wyrd apply -f file.yaml` reads
    and registers in one move. No separate storage step, no programmatic
    registration prerequisite.
17. **Light cards may inline in place of a `CardRef`.** Wherever a spec slot
    permits an embedded child, it uses `InlineableRef<T>`. Inline definitions
    have no card identity, are not registered standalone, and cannot be
    referenced from outside their parent. To reuse a child, register it as a
    card and reference it with `Ref::Ref(CardRef)`. Durable-only slots use
    `Ref`.
18. **Auth and Policy are two distinct planes.** Emit is **not** a third
    plane: a deployed service's observation/ingest writes are ordinary
    Auth-plane routes, authorized by the same JWT and a
    `Permission { resource, action, scope }` like every other call. The legacy
    per-card **governance token is removed** — the JWT proves the principal and
    bounds its emittable **card scope**. For card-bound principals, the scope is
    the principal's own `card_ref` plus the **observation-target** cards reachable
    through the transitive card-ref graph declared in that card's spec. A card
    enters the scope only if its kind is an observation target — a kind a client
    (`wyrd.observer`, Bifrost, drift, eval) attributes records to: `Data`,
    `Model`, `Experiment`, `Prompt`, `Agent`, `Workflow`, `Service`, `Mcp`,
    `Artifact`, `Source`. `Verifier` and control-plane kinds (`Policy`,
    `Audit`, `Operator`, `Trigger`) may be referenced for governance or
    verification but never enter the emit scope. Service principals start from their Service card and
    therefore include declared `Service.components`; Agent principals start from
    their Agent card and include its declared card refs. Every token mint —
    including the re-exchange a machine performs when its access token expires
    — resolves those bounded scope identities to Card UIDs and signs that
    mapping. A principal bound to no Card — a User, a tenant administrator, or
    an unbound Service — has no bounded scope: it may attribute to any
    registered observation-target Card in its tenant, which ingest resolves
    against the tenant registry.
    An observation may carry its subject `card_ref`; when present, the
    server authorizes it against that scope and stamps the mapped `card_uid`.
    Generic telemetry may omit it and retains the authenticated publisher through
    `principal_id`. A separate emit credential was redundant — see "Observation
    identity — Card → Run → Observation".
    - **Auth** gates Wyrd API calls: `Permission { resource, action, scope }` on
      the handler, stateless pubkey verify of the access token. Answers "is this
      principal allowed to hit this Wyrd route, and to reach this object?" This
      covers data-plane ingest (e.g. `bifrost_record:write`) exactly like any
      other route. Wyrd RBAC is the standard operation/object model: `resource`
      and `action` name the operation and the typed `PermissionScope` names the
      objects, so a static role grant over one Bifrost schema or table is still
      RBAC and never enters the Policy plane. The rejected legacy vocabulary was
      a free-form OAuth-style `Scope` *string*; a typed closed object scope
      inside `Permission` is the sanctioned model — see
      `v1/00-foundations/permission-model.md`.
    - **Policy** gates card states (`classify` at register-time, `gate` at
      deploy-time). There is no runtime cross-service invoke gate: the
      `PolicyAction::Invoke` enum member and the `POST /v1/authz/check`
      runtime evaluation surface it referenced were removed and have no
      replacement in this design.

    Runtime identity is a `Principal { id: PrincipalId, kind: PrincipalKind,
    tenant_id, roles, effective_permissions, credential_id }`. `PrincipalId`
    is a `Uuid` newtype; no string-prefix encoding (no `user:`, `sa:`,
    `agent:`) — discrimination lives on the kind. The closed wire set is
    `PrincipalKindTag`: `GlobalAdmin`, `TenantAdmin`, `User`, `Service`,
    `Agent`, `System`, serialized as `global_admin`, `tenant_admin`, `user`,
    `service`, `agent`, `system`. A `GlobalAdmin` is a platform-plane principal
    and has no tenant,
    carried at runtime as `PlatformPrincipal { id, kind,
    effective_permissions, credential_id }`; a platform-plane human is a
    `User` with no tenant. Every tenant-plane principal has exactly one
    tenant and carries the card-bearing `PrincipalKind`: `TenantAdmin`,
    `User`, `Service { card_ref: Option<CardRef>, card_ref_scope }`,
    `Agent { card_ref, card_ref_scope }`, or the internal-only
    `System { card_ref_scope }`. Card binding is a property of a
    machine principal, not a precondition for being one: an Agent is always
    card-bound and a deployed Service carries its Card, each projecting a
    `card_ref_scope` authorization set derived at mint time, while a tenant
    administrative or tenant-created automation principal is representable
    with no Card and therefore no emit scope. `User` is the marker for human
    identity. `System` is a tenant-local server identity that attributes
    verification results and authorizes Verifier input reads in-process. It
    never appears in a token: no issuer mints one, every verifier rejects a
    `system` claim set, and it has no public credential, role, refresh,
    workload, delegation, or principal-management path. Platform authority
    is a grant held at platform scope, not a property of a kind, and neither
    plane's credential or token is accepted by the other. Gateway capture is a
    server-internal write, not a principal that authenticates: captured rows
    carry the reserved `GATEWAY_CAPTURE_PRINCIPAL`, which, like
    `PLATFORM_AUDIT_PRINCIPAL`, never appears in a token. Every issuer refuses
    it, every verifier rejects a token naming it, and it never replaces the
    invocation caller in audit. `wyrd apply -f service.yaml` (or an Agent card) creates
    or updates the principal row idempotently, keyed on
    `(tenant_id, card_kind, card_uid)`; re-apply preserves the same
    `principal_id`. No secret is returned. Credentials are issued out-of-band
    by `wyrd auth issue-key <card_ref>`, which mints a card-bound API key
    against the existing principal. The caller uploads the key to the deploy
    environment's secret store (Vault, AWS Secrets Manager, GCP Secret
    Manager); deploy-time secret injection puts it into the pod as
    `WYRD_API_KEY`. The SDK exchanges it at `POST /auth/token` for a short-lived
    JWT and re-exchanges the same durable key whenever that token expires or
    the server refuses it; a machine holds no refresh token. `/auth/token` derives `tenant_id` and
    `principal_id` from the verified API-key record — never from a
    client-supplied header. The JWT carries top-level `principal` (the subject;
    under delegation, the party being acted for) and an RFC 8693 `act` chain
    whose outermost layer is the current actor; a single delegated token
    carries both sides of an invoke. Service B obtains it with
    `WyrdClient::on_behalf_of(A token, audience)`, presenting A's token as
    `subject_token` and its own as `actor_token`; its permissions are the
    intersection of both. On cross-service calls the SDK puts that delegated
    Wyrd JWT in the `X-Wyrd-Access-Token` header. The application's own
    `Authorization` header is never touched. `Wyrd-Caller-Identity` is
    rejected legacy — do not reintroduce. Both subject and actor identities
    are server-verified from one signed delegated JWT — no enforcement-point
    JWT, no SPIFFE/mTLS actor derivation. Delegated exchange consults no
    policy; it records `auth.token.exchange` as its audit permission.
19. **Reference slots use exactly `Ref` or `InlineableRef<T>`.** `Ref` carries
    durable identity or an authored `Path`; `InlineableRef<T>` additionally
    permits an inline child body. `Path` is loader-only. During composite
    registration, the loader may rewrite an authored path to the
    registration-only `Sibling { sibling: CardRef }` form so the server can
    distinguish a target submitted in the same request from an external Card
    referenced with `Ref(CardRef)`. The server validates both forms and
    rewrites them to UID-bearing `Ref(CardRef)` before hashing, relationship
    derivation, blob persistence, or any read response. `Sibling` is not a
    durable reference form, and there is no separate selector reference enum.
    The server, registry, and evaluator reject unresolved paths at their
    boundaries.

    `path` resolves to a typed registration reference before send; only `ref`,
    registration-only `sibling`, and `inline` cross a composite registration
    wire. Heavy cards (Model, Data, Experiment, Artifact) accept `ref` only —
    `inline` is rejected because identity anchors lineage. Environment/stage is
    target-card metadata (`labels` /
    `annotations`), never `space`; `space` is team/workspace scope only, and
    one server may hold development, staging, and production cards side by side.
    See [Reference forms](#reference-forms) for loader rules and the slot inventory.
20. **User journeys are the primary test contract.** A capability is not done
    until a real user/agent path proves it end-to-end — client → server →
    client, against a real server (`WyrdTestServer` + repository-managed
    Postgres), not a
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
21. **`verified_by` is the versioned verification subscription contract.**
    A Service component, Service, or standalone Agent declares typed
    `VerificationBinding` values. Each binding resolves one exact Verifier,
    optional Trigger activation, and zero or more failure Operators. The binding is
    static declaration and never a per-request routing table; changing it
    changes the containing Card spec. There is no separate publication or
    monitor-routing field, and clients never select a Verifier per
    observation.

## Client model

Wyrd is language-agnostic at the protocol boundary. The server owns durable
behavior, and its typed wire contracts are the source of truth. Any language
can implement a client by following those contracts; no SDK owns a separate
registry, lifecycle, validation, or storage model.

Rust, Python, and TypeScript are Wyrd's first-class client languages. Wyrd
maintains idiomatic SDKs, generated types, examples, and client → server →
client journeys for all three. Surface ergonomics may differ, but durable
nouns, fields, errors, permissions, side effects, and lifecycle semantics do
not. Go is not a first-class client language.

The public protocol remains open to every language. HTTP, MCP, generated
schemas, stable errors, and machine-readable documentation are sufficient to
implement a complete client without depending on Rust, Python, or TypeScript
internals.

`wyrd-client` is the one shared Rust client implementation and SDK-facing
surface. The first-class language packages live under `sdks/wyrd-sdk-rust`,
`sdks/wyrd-sdk-python`, and `sdks/wyrd-sdk-ts`; all three consume
`wyrd-client` rather than reaching through to server or domain implementation
crates. Language-specific code is limited to behavior earned by a
foreign-runtime boundary, such as Python async integration, Node loading,
generated declarations, or idiomatic local authoring helpers. It does not
duplicate transport or durable behavior.

Existing Rust owner crates may retain their optional `python` features during
this integration. Only `wyrd-sdk-python` enables and aggregates those features;
the Rust and TypeScript SDKs do not. New or materially relocated Python logic
belongs in `wyrd-sdk-python`. Consolidating all existing Python logic there is
the target direction, not a reason for unrelated code movement during this
merge.

Bifrost follows the same rule:

```text
Rust / Python / TypeScript SDK
  -> wyrd_client::Bifrost
  -> shared wyrd-client HTTP and gRPC transport
  -> wyrd-server public edge
  -> server-owned Scribe, Oracle, and Forge
```

Gate is the server dispatcher, not a deployment role or client type. Language
bindings wrap `wyrd_client::Bifrost`; they do not assemble independent query
and ingest clients or implement their own HTTP or gRPC transports.

---

## Kind catalog

Wyrd registers 15 native Card kinds. `CardKind::External` is a non-registrable
discriminator used when reading foreign or unknown kind metadata. The foreign
payload remains opaque and source-specific; it is not a Wyrd Card. There is no
`ExternalSpec` and no External Card registration path.

| Domain        | Kinds |
|---------------|-------|
| Data plane    | Data, Model, Artifact, Experiment |
| Agent plane   | Prompt, Agent, Workflow, Mcp |
| Composition   | Service |
| Governance    | Policy, Audit |
| Observability | Verifier, Source |
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
  card_refs: [CardRef]           # → Artifact
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
  card_refs: [CardRef]           # → Artifact
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
Provider-native request envelope, the serialized Skald `Prompt`.
```yaml
spec:
  model: string
  provider?: ProviderName          # destination; absent = the request schema's default provider
  request:
    provider: <wire schema>        # open_ai_chat_completion | open_ai_responses | anthropic_messages | gemini_generate_content | ...
    body: { ... }                  # that schema's native request body
  variables: [string]
  response_type: text | json
```

`request.provider` names a wire schema, never a destination; each schema has
one variant. `provider` names where native dispatch sends it, so a custom
OpenAI-compatible endpoint is an `open_ai_chat_completion` request with
`provider: {custom: <name>}`, and a Vertex Prompt is a Gemini GenerateContent
body with `provider: vertex`. Gateway routes ignore `provider` and select the
upstream themselves. Standalone Prompt Cards may still be authored with the
declarative `provider`/`model`/`messages` shorthand, which compiles to this
envelope; an Agent's inline Prompt is the native envelope itself.

### Agent
Agent contract: prompt + tools + run config. Tool names resolve through the
runtime tool registry (host tools + MCP server registrations). Approval,
per-tool blocks, and hook gates live on `Policy`, not here.
```yaml
spec:
  prompt: InlineableRef<Prompt>  # Ref (→ Prompt), authored Path, or inline Prompt
  tool_names: [string]
  run_config: AgentRunConfigSpec # max_iterations, tool_concurrency_cap, session_recent_limit, timeout_ms
  verified_by: [VerificationBinding]
```

### Workflow
DAG of steps invoking other cards.
```yaml
spec:
  description?: string
  inputs: { string: ParameterValue }
  llm_route?: LlmRoute             # native (default) | wyrd_gateway | ext_gateway
  steps: [WorkflowStep]            # id, action, depends_on, inputs, llm_route?, fallback?, timeout_seconds?, retry?
  outputs: { string: WorkflowBinding }
  governance?: Governance
  details: { string: NonSecretValue }
```

Data flow is explicit. A step's `inputs` bind exact sources (`input.<name>`,
`steps.<id>.output.text`, a structured-output path) to declared Prompt
variables, `depends_on` alone orders steps, and the Workflow's `outputs` name
every result a run returns. Nothing is forwarded implicitly and there is no
last-step result. Local and server runs return the same `WorkflowRun`
snapshot, addressed by output name and step ID.

A step's `llm_route` overrides the Workflow's. `native` uses the local
process's provider registry and the Prompt's destination; server execution
refuses it at submission with `WYRD_WORKFLOW_422_SERVER_NATIVE_UNSUPPORTED`. `wyrd_gateway` calls
the governed Wyrd gateway with the caller's identity: locally through the
public protocol ingress (Chat Completions, Responses, Anthropic Messages, or
Gemini GenerateContent; a Vertex Prompt is refused before dispatch because
the public ingress serves no Vertex dialect), and on the server in process,
where Vertex is served. `ext_gateway` is execution-local: the process running
the step sends the native body directly to the named binding's origin. A
local run reads that binding from the `[workflow.external_gateway_bindings]`
section of the shared client `config.toml`, whose secret headers are secret
references resolved at run start for the selected bindings only; a server run
uses the tenant-assigned server binding. An `ext_gateway` call never passes
through the Wyrd gateway, and a binding of another protocol or with an
unreadable secret is refused before any dispatch. Loading and registration
never read a binding or secret.

Workflow is a declarative Card, not a principal. It owns no credential, role,
or authority; invocation uses the caller's identity. WyrdState remains a
Service-root hydrated state and is not required for standalone Workflow loading.

Clients expose `Workflow.from_path(path)` (Python), `Workflow.fromPath(path)`
(TypeScript), and async `Workflow::from_path(path)` (Rust), including hydration.
Fully local paths/inline bodies require no registry or credential; external
Agent/Prompt refs and their transitive dependencies automatically use lazy
existing default client configuration. Loading never registers, executes, or
resolves execution secrets. Missing/denied/inactive dependencies fail before
execution. Registered loading is `cards.workflow.load` (Python/TypeScript) or
`cards.workflow().load` (Rust), selecting an exact named version or UID and
following locked relationships. No loaded dependency floats to a newer version.

The shared client composes existing wyrd-loader, Cards hydration, and Skald
resolved validation/execution. Its thin Workflow facade adds the client IO
boundary without duplicating graph state, traversal, parsing, validation, or
runtime logic. Language SDKs project that facade; no public loader or hydrator
is required. Shared local execution configuration serves SDKs and CLI, while
Skald remains independent of registry IO and server tenancy.

A `wyrd_gateway` step calls the existing authenticated public gateway ingress.
Its per-step fallback travels in the optional `wyrd-gateway-fallback` request
header: unpadded base64url over the JCS UTF-8 serialization of a
`GatewayFallbackOverride`, at most 8 KiB encoded and 4 KiB decoded. The ingress
authenticates the caller before parsing the header, refuses a repeated,
malformed, oversized, empty, duplicated, or self-listing value with
`WYRD_GATEWAY_400_INVALID_REQUEST` naming `fallback` before any dispatch, and
consumes the header without forwarding it to a provider. Without the header,
tenant fallback policy applies unchanged, so unmodified clients behave as
before.

A server-hosted Workflow run is an accepted job. The authenticated,
`workflows:run`-authorized, audited submission pins the exact active graph and
captures token-free execution authority: the caller's principal attribution
and scopes, bounded to that run's pinned graph and total deadline. Neither a
bearer token nor a secret is retained. Token expiry and later grant or
credential changes neither cancel nor widen an accepted run. Later HTTP
create/replay, get, and cancel requests authenticate and authorize afresh,
and replay never replaces run authority. Within the run, Cards reads, Bifrost
queries, and gateway calls still make and audit their own live per-call
decisions under the captured authority and current owner admission rules.
Runs are process-local; restart loses them.

The server bounds runs before acceptance: step count, dependency edges,
resolved graph bytes, decoded input bytes, and a complete-snapshot budget
from which it reserves room for a payload-free terminal snapshot of every
declared step, including bounded run and step error projections. A graph whose terminal snapshot cannot fit is
refused at submission. An oversized step result fails its step, and a
transition whose full snapshot would exceed the budget discards the provider
payload and fails the run, so no terminal transition lacks room for its
statuses or errors. Active and retained runs have global and per-tenant
ceilings; terminal runs are queryable for 24 hours or until oldest-first
eviction. Shutdown cancels and drains active runs. A multi-replica
deployment must route a run's create, get, and cancel requests to the replica
that owns it; any other replica answers not found.

Workflow surfaces are scoped by capability. Rust, Python, and TypeScript load
and run authored files and registered Workflows locally. Server-run
submission, status, and cancellation ship only through the Rust
`wyrd_client::Workflows` handle, the `/v1/workflow-runs` HTTP resource, and
the CLI. MCP has no Workflow surface. `wyrd workflow run` takes exactly one
source (`--file`, `--uid`, or `--space`/`--name`/`--version`) and
`--execution local|server`; a file runs only locally. `wyrd workflow`
commands take no `--server` option; the endpoint comes from the ambient client
configuration. Server execution prints
the accepted run ID and waits unless `--detach`; an interrupted wait leaves
the run running and never cancels or resubmits it. `wyrd workflow status` and
`wyrd workflow cancel` act on server runs.

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
Verifier/Trigger/Operator/Audit/Source are peer cards in the deployment
directory, not Service components.
```yaml
spec:
  description?: string
  components: [ServiceComponent] # { alias, ref | path, verified_by: [...] }
  entry_point?: string           # SDK AppState bootstrap module (e.g. `acme.copilot.app:app`).
                                 # Importing it materializes the service's locked card snapshot
                                 # at runtime. Wyrd doesn't import this; the deploy image does.
  verified_by: [VerificationBinding]
```

Identity is derived from the Service's `card_ref` and bound on first deploy
contact — no `service_account` field on the spec. See "Runtime identity".

### Policy
Declarative governance rules. CEL-evaluated. Three lifecycle phases share one
rule shape; the `action` field on each rule says when it fires:

  - `classify` (register-time): rule derives attrs onto the card (e.g.
                                `risk.tier = "high"`). Never blocks.
  - `gate`     (deploy-time):   rule allows/denies the card's deploy-time
                                credential issuance (`wyrd auth issue-key`),
                                and thus its emit eligibility. Blocks when Deny.
  - `invoke`   (runtime, per cross-service call): declared but unenforced.
                                The runtime evaluation surface this action
                                referenced (`POST /v1/authz/check`) was
                                removed with no replacement gate.

Composition: `org_global ∪ service_local`, deny-overrides. Service-local can
only tighten. CEL parse and evaluation are owned by the Policy engine;
`wyrd-spec` enforces only `CelExpression` transport invariants
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

#### Two administration planes

Administration is split in two, and an identity belongs to exactly one plane.
The **platform plane** operates the deployment — tenant lifecycle and recovery
of a tenant's administration — through tenantless principals in
`platform.principals`, reached only over `/platform/*` and only with a
platform credential or a session from the deployment's single platform-scope
OIDC connection. The **tenant plane** operates one tenant's resources through
principals under `wyrd.*` behind RLS. A platform principal is never implicitly
authorized over a tenant's resources, a tenant principal can never reach the
platform plane, and neither plane's credential or token is accepted by the
other.

Both planes write their authorization decisions to the one canonical audit
path: `vala.audit_staging`, then `AuditPublisher` into
`vala.system.audit_log`. Permissions are blocking; audits are non-blocking:
a decision is staged on the shared audit outbox and never delays or fails
the operation. A decision records the deciding principal's
stored kind and, when its token was minted from a credential, that
credential's non-secret id.

#### Principal model

Wyrd principals are UUID-backed runtime identities that exist independently of
any credential: issuing, rotating, revoking, or losing a credential never
creates, destroys, or alters a principal or its role grants. The reserved
`GATEWAY_CAPTURE_PRINCIPAL` is not a credential-bearing identity: it is the
id Scribe stamps on rows gateway capture writes server-internally, holding no
token, role, or grant. Each `wyrd-server` process owns one capture writer,
chosen from pod topology (`WYRD_TARGET`): it submits in-process when Scribe
runs in the pod, and otherwise over the peer plane through a capture-only RPC
to a live, ready Scribe. A capture is delivered when Scribe acknowledges it
before the call's deadline; backpressure and an unavailable Scribe are retried
with bounded backoff until then, and anything else drops the capture with a
counted reason without changing the call. No per-tenant client, queue, or
backlog outlives the call. The same writer submits Verifier results to the
three result tables, attributed to the tenant's `System` principal and the
run's exact Verifier Card, which Scribe stamps as `principal_id` and
`card_uid`; a result batch is never dropped for time and is retried under its
own batch id until Scribe acknowledges it. Gate refuses every public write to
`vala.gateway.calls` and the three result tables.
`Service` and
`Agent` principal is always card-bound and a deployed `Service` carries its
Card — the `card_ref` is discriminated on `PrincipalKind`, and the JWT carries a
mint-time `card_ref_scope` derived from the transitive card-ref graph rooted at
that card — but a `Service` binding is optional, and the administrative and
automation principals a tenant creates for itself hold no Card at all. `wyrd apply` for a Service or Agent card creates or updates the
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
`WYRD_API_KEY`. The SDK exchanges it at `POST /auth/token` for a short-lived
JWT (five minutes) and re-exchanges the same durable key when that token nears expiry
or is refused — a machine grant issues no refresh token, so the key in the
secret store is the only renewable authority and revoking it ends renewal.
Renewal authority is a human concern: only a tenant human login returns a
refresh token (a platform login is access-only), and only the public CLI client's refresh token rotates. `/auth/token`
derives `tenant_id` and `principal_id` from the verified API-key record;
no client-supplied tenant header is accepted. The JWT carries the principal
as the top-level `principal` claim, the `permissions` resolved from its current
grants at issuance as its only authority, and for delegated tokens (token
exchange) an RFC 8693 `act` chain naming the actors, the outermost being the
current one. A delegated token's audience is `wyrd` or `bifrost`; a `bifrost`
token is accepted only on the Bifrost ingest and query surfaces.

OTLP is the one exception to exchange-first. A stock OpenTelemetry exporter
carries static headers and cannot renew a token, so the OTLP/HTTP and OTLP/gRPC
endpoints also accept the API key itself in an `x-wyrd-api-key` header. On
**every** such request, before any payload is decoded, the server verifies the
key exactly as the `/auth/token` API-key grant does and resolves the principal,
permissions, and tenant scope that grant's token would carry, without minting
or signing one; the request then runs under the same authorization, audit,
and limits as a bearer caller. A revoked key fails at its next export. `x-wyrd-access-token`
remains valid on OTLP, and no other route accepts `x-wyrd-api-key`. An exporter
is configured only with standard settings, for example
`OTEL_EXPORTER_OTLP_HEADERS=x-wyrd-api-key=<key>`; no SDK exporter helper exists.

Env vars in deployed services:

| Env var | Required? | Source | Used for |
|---|---|---|---|
| `WYRD_API_KEY` | REQUIRED | Deploy environment's secret store (key minted by `wyrd auth issue-key <card_ref>`) | Exchanged at `POST /auth/token` for a short-lived JWT and re-exchanged by the SDK when that token expires or is refused; no refresh token is issued. JWT carries the card-bound `principal` claim (kind, id, tenant, `card_ref`). |
| `WYRD_SERVER_URL` | REQUIRED | Static config | Wyrd server HTTP base URL (default `http://localhost:8080`). Read by `ClientConfig::from_env`. |
| `WYRD_GRPC_URL` | OPTIONAL | Static config | Wyrd server gRPC endpoint (default `http://localhost:50051`). Read by `ClientConfig::from_env`. |

#### Human sign-in

Human federation is optional per tenant. A tenant has at most one active OIDC
connection, administered through `/v1/identity/oidc/*`; Wyrd is an OpenID
Connect relying party of that provider (`openidconnect`) and the OAuth 2.0
authorization server for its own clients. The provider authenticates the
person; Wyrd resolves the tenant `User` by verified `(issuer, subject)`, maps
provider groups only to that tenant's roles, and issues Wyrd tokens. Provider
tokens are never Wyrd authority.

| Client | OAuth client | Grant | Session holder |
|---|---|---|---|
| Web app | `wyrd-ui`, confidential (`client_secret_basic`) | Authorization code + PKCE (`GET /auth/authorize`) | The BFF's encrypted HttpOnly cookie; refresh token does not rotate, 12-hour absolute lifetime |
| CLI and SDKs | `wyrd-cli`, public | Device authorization (RFC 8628) | `credentials.toml`, owned by `wyrd-client`; refresh token rotates on every use |

The BFF uses `openid-client`; the shared Rust client uses `oauth2`, so Rust,
Python, and TypeScript share one implementation. The four OAuth form
endpoints (`POST /auth/token`, `/auth/platform/token`,
`/auth/device_authorization`, and `/auth/revoke`) take form bodies, refuse
with RFC 6749 §5.2 JSON, and are the one exception to `WyrdError`
problem+json; their successes keep their own standard shapes (RFC 6749 §5.1
tokens, the RFC 8628 §3.2 device response, an empty RFC 7009 revocation
`200`). Browser interactions are not form endpoints: `GET /auth/authorize`
redirects with an RFC 6749 §4.1.2.1 `error` once the client and redirect URI
are trusted and shows a local HTML page before that, while `GET /auth/callback`
and the `/auth/device` page answer with redirects, HTML, or Problem Details. Deployed Services and Agents keep their own API keys or
workload assertions; human SSO never replaces or disables them.

#### Cross-service delegation

The API key is exchanged at `/auth/token` — never on the wire. The JWT — not the API
key — is what travels on cross-service calls in the dedicated
`X-Wyrd-Access-Token: Bearer <jwt>` header. The JWT must be a **delegated**
Wyrd token: its top-level `principal` identifies the subject being acted
for, and its `act` chain identifies the actor(s) acting for it, the
outermost being the calling Service or Agent. Its `permissions` are the
intersection of the actor's and the subject's; `act` is attribution only. The application's own `Authorization` header belongs to
the application and is never read or written by the SDK or by Wyrd.
`Wyrd-Caller-Identity` is rejected legacy — do not reintroduce.

```
POST /charge HTTP/1.1
Host: billing-svc.acme.svc.cluster.local
X-Wyrd-Access-Token: Bearer <delegated Wyrd JWT — principal=subject, act=actor chain>
Wyrd-Request-Id:     <UUIDv7>                     ← SDK adds; request correlator
Authorization: Bearer <app's own token>           ← app's own auth; Wyrd never reads
Content-Type: application/json

{ "amount": 100 }
```

#### Request correlator — `Wyrd-Request-Id`

A Wyrd-owned, request-scoped opaque ID that joins every hop of a logical
request. It is the sole correlator for request ancestry and audit replay —
Wyrd does not depend on `traceparent`, mesh tracing, or any external
propagation contract.

Contract:

- Opaque UUIDv7 minted by Wyrd at first sighting.
- Propagated unchanged by Wyrd SDK middleware on outbound calls. Never
  mutated, never re-minted mid-request.
- Every Wyrd-emitted observation carries it as a label.
- Ancestry of any request (service1 → service2 → service3) is
  reconstructable by joining observations on this ID; per-hop caller
  identity comes from the verified `X-Wyrd-Access-Token` JWT at each
  call (top-level `principal` is the subject acted for, `act` chain is the
  actor path).

Storage, query, and CEL surfaces must preserve this correlator and may not
introduce a competing request identity.

#### Observation identity — `Card → Run → Observation`

How Card-correlated telemetry ties to a Run and a Card, and how the server
resolves it. A Run is one client application invocation with one `run_id`;
each correlated row names its own exact subject Card. A telemetry row may omit
Card correlation; when supplied, the `(card_ref, run_id)` pair anchors it to
that subject and invocation. The server owns resolution of that identity.

**A principal is not a card.** A Service or Agent principal is bound to one card
(its `card_ref`), but a Service card *nests components* — each a card in its own
right (e.g. Model A, Model B, a Prompt; `Service.components`). One
`state.run()` opens one invocation and one `run_id`, initially scoped to the
root Service Card. The first-class SDKs may select an initial hydrated Card
alias when opening the run, and `for_card(alias)` returns an immutable
Card-scoped view of that same invocation for multi-component work. Views for
different components share the `run_id` and the JWT, but each row carries its
view's exact subject `card_ref`. Switching Cards never requires a distinct Run
ID, and the principal's root Card cannot say which card a correlated record
belongs to — the subject Card must be carried on that row. Generic telemetry
may omit a subject Card.

Python `Run` values are synchronous context managers for optional ambient span
correlation. Entering a scope best-effort attaches the selected CardRef and
run ID to Python OpenTelemetry context, annotates an active recording span,
and lets an idempotently installed span processor copy `wyrd.card_ref` and
`wyrd.run_id` onto spans created inside the scope. Exiting restores the prior
context. Missing or incompatible Python OpenTelemetry support and enrichment
failures are no-ops; they never fail application execution or explicit Wyrd
observation emission. Card lookup and authorization remain strict. The scope
does not create, end, flush, or persist a Run or span. Signal-specific log and
metric enrichment is not implied.

A view identifies itself by the alias it was opened with (`alias`; the root
Service view is `root`); the state returns the exact typed `CardRef` for an
alias. `run.observe.verify(verifier, input)` judges the view's subject in real
time: the client resolves the named Verifier among those bound in
`verified_by` to that subject in the hydrated graph, shapes the input for the
Verifier's implementation (one JSON context for Eval or Task, or Drift feature
rows), and calls
`POST /v1/verification/execute` once, without replay, returning the typed
`Judgment`. An unbound name (`WYRD_SDK_404_UNKNOWN_VERIFIER`) or a wrongly
shaped input (`WYRD_SDK_400_INVALID_OBSERVATION`) fails locally before any
network IO, and a `failed` verdict is an ordinary return. Judging records no
observation, durable verification run, or dispatch and does not require Bifrost
startup. Its completed result is staged through the Scribe outbox. SDKs carry
no separate verification handle; binding and run
operations remain server HTTP and MCP surfaces, and verification history is
read with parameterized SQL through the Bifrost client.

Every accepted row carries authenticated publisher and request identity; Card
and Run correlation are optional per-row values:

| Value | Source | Grain | Means |
|---|---|---|---|
| `card_ref` | optional client assertion of the row's exact subject Card (the run view's selected Card); server authorizes it when present | per row | the optional Card-version anchor — *which* Card |
| `run_id` | optional client-generated value per `.run()`, shared by every Card-scoped view of that invocation; passed through opaquely | per row | the optional Run anchor — *which* execution |
| `principal_id` | server-stamped from the verified JWT | per request | the authenticated publisher — *who* emitted it |
| `tenant_id` | server-stamped from the verified JWT | per request | the tenancy boundary |
| `wyrd_request_id` | the propagated `Wyrd-Request-Id` (minted at first sighting) | per request | the request spine — one request spans **many** runs and hops |

Resolution rule: **tenant and `principal_id` come from the token; a present
`card_ref` is client-asserted, server-authorized, and resolved from its trusted
signed scope mapping, or, for a principal bound to no Card, from the tenant
registry; absent Card correlation produces null `card_uid`;
`run_id` and `wyrd_request_id` pass through untouched.**

On OTLP input, canonical table projection reads these optional values from the
record-level attributes named exactly `wyrd.card_ref` and `wyrd.run_id`. The
final duplicate key wins, matching Wyrd's existing OTLP attribute lookup rule,
while every original attribute entry remains in the lossless payload.
`wyrd.card_ref` uses the compact `CardRef` text grammar; a client `#uid` suffix
is syntactically valid but untrusted and ignored when the server selects the UID
from signed scope. `wyrd.run_id` uses the existing `RunId` text grammar.
Consequences, stated so they stop drifting:

- **`card_ref` and `run_id` are optional per-row columns on the observation payload, not
  request metadata.** A client-side queue batches records from different runs —
  and different cards — before it flushes, so one sealed batch (one
  `wyrd_batch_id`) freely mixes them. The producer is keyed by **table only**; it
  never splits a batch by card or run. The server therefore authorizes every
  present `card_ref` **per row** (every distinct asserted Card in the batch must
  be in the principal's scope),
  validates the client-generated UUIDv7 `wyrd_batch_id` request field (the
  idempotency key; it is not a row column), stamps request-scoped
  `wyrd_request_id` and `wyrd_ingested_at` — the PostgreSQL admission instant,
  read once per batch and never caller-supplied — and validates
  caller-supplied `wyrd_event_time` against a bounded acceptance window around
  that instant, rejecting out-of-range values (never clamping or normalizing
  them). A row without `wyrd_event_time` takes `wyrd_ingested_at`. Row
  identity is batch-level; no per-row position is stamped.

- **`card_ref` is optional and authorized, not trusted.** Its absence is valid
  generic telemetry and produces null `card_uid`; the authenticated publisher
  remains available through non-null `principal_id`. When present, the server checks the asserted
  `card_ref` against the principal's **card scope**. For Service and Agent
  principals, the scope is the principal's own `card_ref` plus the
  **observation-target** cards reachable through the transitive card-ref graph
  declared in that card's spec. A card is in scope only if its kind is an
  observation target (`Data`, `Model`, `Experiment`, `Prompt`, `Agent`,
  `Workflow`, `Service`, `Mcp`, `Artifact`, `Source`); `Verifier` and
  control-plane kinds (`Policy`, `Audit`, `Operator`, `Trigger`) never enter the
  emit scope. Service cards contribute `Service.components`; other reachable
  specs contribute their declared card refs according to the shared card-ref
  extraction rules. A `card_ref` outside that set is rejected: a principal may
  not attribute records to a card outside its declared graph. The scope can be resolved from the
  signed `card_ref_scope` claim minted into the JWT at `/auth/token`. Every
  token mint — a first exchange, a machine's re-exchange, or a human refresh
  rotation — resolves each bounded member against the tenant Card registry and
  signs its authoritative UID with the identity. Ingest uses that verified
  in-memory mapping and performs no Card-registry Postgres or cache lookup.
  A principal bound to no Card (User, tenant administrator, unbound Service)
  carries no bounded scope; its scope is every registered observation-target
  Card in its tenant. Gate resolves the distinct `card_ref` values of each
  frame (at most 32) against the tenant registry before dispatch and hands
  Scribe the resolved UID-bearing members as that frame's scope, so an
  unregistered or non-observation Card is refused exactly like a reference
  outside a signed scope.
- **This is not the governance token.** `card_ref` is one field in the
  observation envelope, authorized by the existing JWT plus the principal's
  declared card-ref graph — not a separate per-card credential (doctrine #18).
  The token still proves the principal; it bounds a *set* of emittable cards,
  and the envelope selects one within it.
- **There is no run registry.** Runs are client-side execution records; the
  server never persists a run table and never resolves `run_id` back to a card
  — the card is the authorized `card_ref` on the row. `run_id` is an **opaque**
  correlation id, never a composite that encodes the card.
- **When present, `Card → Run → Observation` is the `(card_ref, run_id)` pair on
  the row;** generic telemetry remains attributable to `principal_id`, and the
  request spine is the `wyrd_request_id` label that joins many runs across hops.
- **The observation owns subject identity.** Under pub/sub (Doctrine #3, #21),
  the observation's `card_ref` IS its subject — no separate `subject_ref` on
  the envelope and no monitor-emits-about-a-different-card case. For a Service
  principal, the server validates the subject against the locked Service
  version and matching component `card_ref`. Verification routing is resolved
  later from that subject's `verified_by` bindings; authorization still
  reduces to the one subject `card_ref` on the row.

### Audit
Immutable case file. Records the result of an investigation against the
provenance graph; never user-declared scope.

**Creation.** Audit cards are created on-demand only. An investigator —
human or agent — runs a provenance query, decides what is worth pinning,
and snapshots the result. Wyrd does not auto-create Audit cards in v1.

**Storage.** Light-card pattern (doctrine #16). The case file lives inline
on the Audit card itself — no separate `Artifact`, no separate chain
table. The card carries the investigation metadata, the query that
produced the chain, the lineage subgraph at snapshot time (cards + edges
by `card_ref`), and the criteria for re-fetching the relevant observations
from vala. Size is bounded by lineage depth, not by observation count.

**Replay.** The Audit Card's `spec` is the source of truth. The lineage
half is read inline from the card; the observation half is re-fetched by
running the inline criteria against vala's observation store.
`card_ref`s are version-locked (doctrine #8), so lineage anchors stay
valid as long as the registry retains the cited cards. Observation
retention policy must cover the requested replay window; creation fails with a
typed retention error when it cannot.

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
identity (the same posture as every server-derived `Principal`, doctrine #15).

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
  giving later readers a forensic false signal that the investigator
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

| Variant           | Source-card fields                                                          |
|-------------------|-----------------------------------------------------------------------------|
| `Verification`    | `Service.components[].verified_by[].verifier`, `Service.verified_by[].verifier`, `Agent.verified_by[].verifier` |
| `Component`       | `Service.components[].ref`, `Workflow.steps[].target`                       |
| `Artifact`        | `Data.card_refs[]`, `Model.card_refs[]`                                     |
| `Prompt`          | `Agent.prompt`                                                              |
| `Dataset`         | `Verifier.implementation.spec.dataset` (eval)                               |
| `Source`          | `Verifier.implementation.spec.source_ref` (eval)                            |
| `Baseline`        | `Verifier.implementation.spec.signal.baseline_ref` (drift)                  |
| `Trigger`         | `*.verified_by[].runs_on`                                                   |
| `Operator`        | `*.verified_by[].on_failure[]`                                              |
| `Workflow`  | `Operator.workflow_ref`                                           |
| `Hook`      | `Operator.pre_invoke`, `Operator.post_invoke`                     |

For `Service.components[].verified_by`, derived edges retain the exact
Verifier, Trigger, and Operator references and `Relationship.via` preserves
the component field path. The reusable component Card remains the observation
subject at runtime.

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
  actors?: [ActorRef]                          # filter on verified emitting principal; absent = all
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
  would silently narrow replay and give later readers a forensic false
  signal.
- **No `limit` / `cursor`.** Replay must return the deterministic full
  set; pagination is a render-side concern at the API boundary.

**Replay determinism.** The same `ObservationCriteria` against the same
vala store at the same logical time returns the same observation set —
the property the audit's `digest` depends on. The deployment's qualified
retention policy must cover the window. If a referenced subject
is purged from the registry, the criteria still validates structurally
and vala returns whatever observations remain; the audit's `digest`
captures what *was* materialized at `snapshot_at`.

Multi-party attestations are not part of the v1 Audit wire contract. Values in
`details` are descriptive and never acquire approval or authorization meaning.

### Verifier

A subject-less verification declaration with exactly one typed implementation.
`Verifier` is the only registrable verification kind: the closed
`implementation.kind` variants are `drift`, `eval`, and `task`, and `kind: Drift` /
`kind: Eval` registrations are rejected. Scheduling and failure reaction
remain on the binding's Trigger and Operators.
```yaml
spec:
  description?: string
  implementation:
    kind: drift | eval | task                 # closed; variant fields below
    spec: DriftSpec | EvalSpec | TaskVerifierSpec
```

`task` deserializes into `TaskVerifierSpec`, not `EvalSpec`. This is the only new
struct for the task implementation. Its `spec` is one flattened, `kind`-tagged
`VerifierTask` enum value that reuses the existing `AssertionTask` or
`LlmJudgeTask` payload; there is no nested `task` field, Eval task map, or
replacement payload struct. The task uses the existing check behavior over
direct JSON input, without Eval-only dependencies or conditions. It produces one
judgment. Registration and binding do not grant invocation authority.

A Verifier runs only through a `verified_by` binding on a Service component,
Service, or standalone Agent, or through an analysis-only direct invocation:
```yaml
verified_by:
  - verifier: Ref                            # → Verifier (exact version)
    runs_on?: InlineableRef<TriggerSpec>     # absent means explicit-only
    on_failure: [InlineableRef<OperatorSpec>] # zero or more reactions
```

One binding is one subscription. Its subject is the containing Service,
standalone Agent, or Service component occurrence; the same Verifier, Trigger,
and Operator Cards may be reused by any number of bindings. Registration
UID-pins every referenced Verifier, Trigger, and Operator, and rejects
unresolved, wrong-kind, unauthorized, and cross-tenant references, duplicate
bindings to the same Verifier version for one subject, and `workflow` in
`on_failure`. The runtime direction is fixed:

```text
binding activation -> Verifier run -> Verification Result
  -> zero or more on_failure dispatches (failed verdict only)
```

Trigger decides when; the Verifier decides the verdict; the generic runner,
not the Drift or Eval implementation, creates dispatches; each Operator owns
its own delivery. A Verifier run never creates another Trigger. Direct
Verifier invocation is analysis-only and dispatches nothing.

Public direct and manual invocation checks `verifier:run` against the selected
Verifier's exact Card UID. `PermissionScope::Verifier(CardUid)` names one UID;
`All` covers every Verifier in the tenant. `evals:run` alone does not authorize
the generic verification endpoint. Permission is checked before execution and
the allowed or denied decision is audited. The subject Card is resolved in the
same tenant; direct invocation does not use observation-attribution Card scope.
A binding does not replace the invocation permission.

The runner drains `wyrd.verifier_runs` in claim rounds. Each round considers
every tenant with a claimable run, oldest first, with no tenant limit, and
claims at most one run per tenant. Execution has no count limit. The shared
resources that already admit work bound it: the Postgres pool, Oracle
admission and memory, and the Bifrost memory budget. A run refused by one of
them returns to the queue without consuming an attempt, and the process
claims nothing new until one of its running runs finishes. The claim
transaction also returns the Verifier Card's status, its spec on a cache miss,
and any fitted Drift baseline. Parsed Verifiers are cached per process by
tenant and Card UID in a fixed 64 MiB least-recently-used cache that is never
shared across tenants. A deleted Verifier terminates the run `errored`.

A run's result is decided once. When execution completes, the runner encodes
the result and stores it, with its result ID, event time, batch IDs, and
Arrow IPC bytes, in `wyrd.verifier_run_results` in one lease-fenced
transaction before writing any of it. A stale lease stores nothing. Any later
claimant of a run with a stored result writes those stored batches instead of
executing again, so Scribe's batch fence absorbs every repeat. The settlement
that completes or terminates the run deletes the stored result. While a run is
in flight, one statement per tenant renews its leases on the PostgreSQL clock
once a third of the lease has passed. An expired lease is never revived, and a
renewal that no longer finds a run's token cancels that run's work. A run
holds a connection only to claim, store, settle, and renew. The store and the
settlement retry with backoff while the lease holds.

#### Drift implementation
Subject-less observation definition. The implementation is orthogonal: signal +
condition + math. Subject identity is supplied by the publisher at
observation time (Doctrine #3, #21). No scheduling, no dispatch.
Scheduling is a `Trigger`; dispatch is an `Operator`.
```yaml
spec:
  description?: string
  method: DriftMethod            # Spc | Psi | Custom
  signal: DriftSignal            # how the measurement enters the monitor
  condition: DriftCondition      # when a sample becomes an emittable observation
  profile: DriftProfile          # matching PSI, SPC, or Custom config
```

**`Agent`, `External`, and Eval-score methods/signals are deliberately absent
from this delivery.** They have no complete input, fitting, or scoring contract
here; adding them would freeze behavior Wyrd cannot yet honor.

`DriftSignal` is a closed enum for this implementation:

| Variant         | Carries                                          | Use |
|-----------------|--------------------------------------------------|-----|
| `Distribution`  | `baseline_ref: CardRef` (→ Data), `features: [string]` | PSI / SPC over a baseline dataset |
| `Metric`        | `name: string`                                   | Named scalar from subject runtime (mae, p99_latency_ms, tokens_per_call, cost_per_run_usd) |

`DriftCondition` is a closed enum — one comparator vocabulary, no separate
"baselined" shape (baseline + delta resolves to `Outside { lower, upper }` at
authoring; the card stores resolved bounds):

| Variant         | Carries                          | Fires when |
|-----------------|----------------------------------|------------|
| `Statistical`   | —                                | The method's profile decides (PSI threshold, SPC X-bar/S limits) |
| `Above`         | `limit: f64`                     | Sample > `limit` |
| `Below`         | `limit: f64`                     | Sample < `limit` |
| `Outside`       | `lower: f64`, `upper: f64`       | Sample < `lower` or > `upper` |

#### Eval implementation
Subject-less behavioral assessment definition. The implementation is orthogonal:
**how** to judge (`tasks` DAG), **where to read observations from**
(`source_ref`, deferred), and an optional **offline driver** (`dataset`).
Subject identity is supplied by the publisher at observation time
(Doctrine #3, #21). No scheduling, no dispatch, no fire condition — its
activation is an `observations_ready` Trigger and its failure reactions are
the binding's Operators. Eval is a single typed task workflow, not a parallel
mode/profile split.
```yaml
spec:
  description?: string
  tasks: [EvalTask]              # evaluation workflow — DAG via depends_on
  dataset?: DatasetRef           # → Data — offline scenario driver
  source_ref?: CardRef           # → Source — archived or external observation input
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
| set           | unset        | Offline scenario driver. No shipped route runs it; a future Verifier-backed offline route owns it. There is no Eval pull protocol. |
| unset         | set          | Online / archived (deferred — DESIGN §13). Engine reads the user's sink, filters records by publisher `card_ref`, samples records into the task workflow. |
| set           | set          | Same tasks, both modes (online deferred — DESIGN §13). Offline gate and online monitor share one task definition. |
| unset         | unset        | Continuous: each committed `vala.eval.observations` record activates its subject's matching `observations_ready` bindings. |

**Directional flow.** A Service component binding or standalone Agent declares
`verified_by` with an Eval-backed Verifier and the runtime emits observations
carrying the component's `card_ref` as subject identity. Continuous evaluation
loads the committed subject record, executes the typed task workflow, and
persists the common Verification Result plus Eval item details.

`EvalTask` is a closed tagged union. Every variant carries `id: TaskId`,
`depends_on: Vec<TaskId>`, and `condition: Option<EvalCondition>`.
`EvalCondition` supports AND/OR chaining bounded at depth 16.

| Variant          | Variant-specific carries                                                                          | Use |
|------------------|---------------------------------------------------------------------------------------------------|-----|
| `Assertion`      | `context_path?: string`, `operator: ComparisonOperator`, `expected: ParameterValue`, `description?: string` | Deterministic check on a dot-path into a record |
| `LlmJudge` (`llm_judge`) | `judge_ref: InlineableRef<AgentSpec>`, `operator: ComparisonOperator`, `expected: ParameterValue`, `max_retries: u32` | LLM judge composed as Eval → Agent → Prompt; the Agent runs one constrained structured-output turn |
| `TraceAssertion` | `span_selector: JsonPath`, `operator: ComparisonOperator`, `expected: ParameterValue`             | OTel span selector (tokens, duration_ms, retry_count, etc.) read via `source_ref` (deferred — see DESIGN.md §13) |
| `AgentAssertion` | `workflow_field_path: JsonPath`, `operator: ComparisonOperator`, `expected: ParameterValue`       | Tool-call / response-shape check read via `source_ref` (deferred — see DESIGN.md §13) |

`ComparisonOperator` is a closed typed catalog covering numeric, string,
collection, type, and tolerance operations. Parameterless variants
serialize as scalar `snake_case`; parameterized variants as `kind`-tagged
objects. The collection, type, and tolerance families are required to keep
authors out of LLM-judge calls
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

### Source
Read-side reference to an external data system. **Wyrd reads, never writes.**

`source` is a **read-shape bucket**, not a vendor. The top-level discriminator is
the shape of data a consuming Verifier implementation sees — a row set, a time
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

The bucket set lines up with what Drift and Eval implementations already read:
`object_store`/`sql_warehouse` feed `DriftSignal::Distribution` (rows),
`metrics` feeds `DriftSignal::Metric`, `traces` feeds
`EvalTask::TraceAssertion`.

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

### Bifrost

Bifrost is Wyrd's public OLAP warehouse surface and the analytical substrate
used by Vala. It is server state, not a Card kind, external Source, deployment
role, or client-side runtime. Its physical analytical identity is
`(tenant, logical table)`; there are no shared physical analytical tables.

The complete Bifrost architecture—including Scribe ingestion and recovery,
Oracle admission and query execution, Forge publication and maintenance,
resource limits, WALs, tenancy, and audit acceptance—lives in
[`bifrost-design.md`](./bifrost-design.md), which is authoritative for the
subsystem.

The public client path is singular:

```text
Rust / Python / TypeScript SDK
  -> wyrd_client::Bifrost
  -> shared wyrd-client HTTP and gRPC transport
  -> wyrd-server public edge
  -> server-owned Scribe, Oracle, and Forge
```

`wyrd_client::Bifrost` owns table registration, listing and description,
buffered ingestion, query streaming and lifecycle operations over shared
authentication, HTTP, and gRPC. `QueryClient` and
`BifrostGrpcTransport` are facade implementation mechanics, not sibling public
clients. Gate is the server dispatcher; it is neither a client type nor a
deployment target.

Every Bifrost query, including a Verifier's query, uses the same query service
and published-plus-live source behavior. The request exposes no visibility,
freshness, or query-class choice. Oracle selects Interactive or Analytical
automatically from the one DataFusion physical plan. Live coverage is best
effort; a successful query is not proof that every acknowledged write was
included. `bifrost-design.md` owns the failure and publication-overlap details.

### Coordination clock

The system evaluating a time predicate owns the timestamp used by that
predicate. PostgreSQL owns database coordination time: work availability,
schedule eligibility, claims, leases, heartbeats, retry/backoff eligibility,
worker deadlines, retention cutoffs, and ordering timestamps that affect those
decisions use `statement_timestamp()` in the owning SQL statement. Relative
deadlines are computed in SQL from a bound duration, and PostgreSQL returns a
verdict or remaining interval when Rust must react to a database deadline.
Rust does not bind its wall clock into coordination columns or compare a
PostgreSQL timestamp with `Utc::now()`.

Rust `Instant` owns in-process timeouts, sleeps, poll deadlines, and latency.
The producer owns domain and event facts such as observation time, execution
start/end, caller-declared expiry, and JWT `iat`/`exp`. Explicit future
scheduling remains representable, but a computed "next future" schedule uses a
PostgreSQL timestamp returned by the owning statement as its anchor. Wyrd adds
no clock abstraction, skew tolerance, synchronization setting, safety margin,
or permanent source checker for this rule; each shared coordination write path
is protected by a behavioral regression test.

### Trigger
Decides when a bound Verifier runs. A Trigger is one flattened, closed
activation; it names no Verifier, Operator, subject, threshold, pass-rate
criterion, or other verdict matcher. A verification binding's `runs_on`
supplies it inline or by reference; the same Trigger Card may serve any number
of bindings.
```yaml
spec:
  description?: string
  kind: schedule | observations_ready     # closed activation discriminator
  cron: string                            # schedule only
  tz?: string                             # schedule only — IANA tz name, default UTC
```

| Activation           | Valid Verifier implementation | Creates |
|----------------------|-------------------------------|---------|
| `schedule`           | `drift`                       | One run per active binding per due occurrence, over that binding's subject window. |
| `observations_ready` | `eval`                        | One run per matching active binding for each committed Eval observation of that binding's subject. |

Registration rejects an activation paired with the wrong Verifier
implementation. A Trigger never fires on its own: nothing runs merely because
a Trigger Card is registered, and one schedule occurrence shared by two
subjects creates two subject-scoped runs, never one mixed-subject run.

Eval run creation follows Scribe's acknowledgement and never delays it.
After the ack, the server puts one run request per committed record into an
in-process outbox. One writer drains it, one multi-row insert per tenant,
keyed by binding and record, so a repeated request inserts nothing. The
outbox has no count limit and never drops a request because PostgreSQL is
slow or down: a failed write keeps its batch and retries with backoff.
Graceful shutdown flushes the outbox. A hard kill loses only unwritten
requests, and every loss the process observes is counted and logged.

External pushes are deliberately not an activation — Rule 7 ("Wyrd reads, it
does not push") means external data enters through a `Source`.

### Operator
A failure reaction. Performs exactly one action — a typed notification, a
generic HTTP call, or a Workflow dispatch. Operator is a pure side-effect
template with no `pre_invoke` / `post_invoke` hooks and no subscription of its
own: it runs only as an `on_failure` entry of a verification binding. A
referenced Operator Card and an inline `on_failure` mapping use the same
flattened `OperatorSpec` shape.
```yaml
spec:
  description?: string
  kind: notify | http | workflow  # flattened OperatorAction discriminator
  # ...variant fields below
  budget?: { max_wall_seconds?: u32, max_tool_calls?: u32 }
```

`OperatorAction` is a closed tagged union (snake_case `kind` discriminator):

| Variant    | Variant-specific carries                                                              | Server does                                                                   |
|------------|---------------------------------------------------------------------------------------|-------------------------------------------------------------------------------|
| `Notify`   | `channel: NotifyChannel` (closed tagged union — typed vendor shape)                  | Sends the notification through the vendor-specific adapter the server owns.   |
| `Http`     | `method`, `url`, `headers?`, `body?`, `auth?`, `timeout_seconds?`, `expect_status?` | Builds and sends the HTTP request.                                            |
| `Workflow` | `workflow_ref: Ref` (→ Workflow)                                                      | Not yet executable. Registerable as an Operator Card, but registration rejects it in `on_failure`. |

**Dispatch.** Only a completed, binding-created Verifier run with a `failed`
verdict creates Operator work: one durable dispatch per configured
`on_failure` Operator. Passed, inconclusive, cancelled, timed-out, errored,
and direct analysis-only runs create none. Each dispatch has its own status,
bounded retry, and terminal error; one failure never blocks siblings or
rewrites the result. There is no `Alert` resource, table, or alert router — a
Notify Operator *is* the alert.

`NotifyChannel` (v1 set; closed tagged union; additional channels are
protocol-versioned additions):

| Channel     | Carries                                                                            |
|-------------|------------------------------------------------------------------------------------|
| `Slack`     | `connection`, `channel_id`, `text` — `chat.postMessage` with the bot token; success is JSON `ok` |
| `PagerDuty` | `connection`, `route`, `severity`, `summary`, `dedup_key?` — one Events API v2 `trigger` event sent with the tenant connection's PagerDuty Global Integration key. The authored `route` travels as `payload.custom_details.wyrd_route` for the tenant to match in PagerDuty Service Routes; `payload.source` is the subject Card ref. The `dedup_key` is the rendered authored value or, by default, the dispatch ID, and stays the same across retries of one dispatch. PagerDuty owns service routing and escalation; Wyrd maps no escalation, provisions no per-team key, and does not treat Events API acceptance as proof that an incident or page was created |

`HttpMethod`: closed enum — `Get | Post | Put | Patch | Delete`.

`HttpAuth` (closed tagged union; absent means no credential):
- `Bearer { connection }`
- `Basic { connection }`
- `Header { name, connection }` (covers `X-API-Key`)

**Operator connections.** Cards never carry secret material; they name a
tenant connection by provider and name. Tenant administrators manage
connections through `/v1/operator-connections` (`operators:read` /
`operators:write`) and every SDK, CLI, and MCP projection. Wyrd stores each
secret itself in Postgres, envelope-encrypted: a per-row data key sealed with
AAD binding tenant, connection, provider, name, and secret version, wrapped by
a versioned key-encryption key from an environment variable, owner-only
files, or HashiCorp Vault KV v2. Multi-tenant production requires Vault over
HTTPS so each tenant has its own key, and fails startup if the provider or any
active tenant's active 32-byte key is unavailable; environment keys are
development-only, and owner-only key files may serve an explicitly
single-tenant deployment. Without a readable key only connection
create/update refuses; other surfaces keep working and delivery retries with
`credential_store_unavailable`. Vault reads use the shared screened and
pinned `wyrd-vault` KV v2 reader that Gateway also uses for provider
credentials; only the Operator owner interprets and validates the KEK, and no
general secret resolver backs Operator keys. Rewrap onto a new active version
runs beside delivery in bounded, cancellable tenant-scoped passes. Responses, errors,
logs, and audit carry only redacted metadata, never key locations. Registration
and every delivery attempt require an active connection whose provider — and,
for HTTP, auth scheme, header name, and origin of every effective URL —
match; a mismatch fails closed with one indistinguishable error. Rotation
takes effect on the next attempt with no Card revision.

**Delivery.** A generic Operator worker takes a global (16) and per-tenant (4)
permit, claims a due dispatch, re-checks authority, decrypts the latest
credential for that attempt only, and screens and pins every destination
address before a credential is attached; HTTP follows only same-origin
redirects and sends `Idempotency-Key: <dispatch_id>`. Each attempt is bounded
to 30 seconds; a dispatch gets three attempts with 30-second then two-minute
backoff (or a longer provider `Retry-After`) inside a five-minute deadline.
Connection failures, timeouts, credential-store outages, 408, 429, and 5xx
retry; missing or unauthorized credentials, invalid destinations or
templates, and other 4xx fail terminally. Delivery is at least once.

`HttpBody`: structured JSON (`JsonValue`). Any string leaf may contain
`{{...}}` placeholders the server interpolates at fire time. Same templating
applies to `Http.url` and to text fields in `NotifyChannel` variants.

Templating context is one bounded, immutable failure context derived from
the failed Verification Result: dispatch, run, result, and binding IDs; the
exact Verifier and subject Card identities; the `failed` verdict; completion
time; a bounded result summary; and one count block for the failed run's
Verifier implementation (`verifier`, tagged by `implementation`). It never
carries raw observation context, media, feature rows, feature names, Eval task
detail, or secret material, and registration rejects unknown template fields.
Exact field schema lives in OpenAPI.

| Implementation | Template fields |
|---|---|
| any | `dispatch_id`, `run_id`, `result_id`, `binding_id`, `verifier_ref`, `verifier_uid`, `subject_ref`, `subject_uid`, `verdict`, `completed_at`, `summary` |
| `drift` | `drift.drifted_features`, `drift.total_features` |
| `eval` | `eval.passed_tasks`, `eval.failed_tasks`, `eval.total_tasks`, `eval.pass_rate_percent` (rounded down; `0` with no tasks) |

Counts render as base-10 digits and come from the same result as `summary`.
A standalone Operator Card may use fields of either implementation; a
binding refuses an attached Operator (inline or referenced) that uses the
other implementation's fields, naming the binding field path.

```yaml
# Drift Verifier's on_failure
kind: notify
channel: { kind: slack, connection: ops-slack, channel_id: C0123456789,
           text: "{{drift.drifted_features}}/{{drift.total_features}} features drifted on {{subject_ref}} ({{verifier_ref}})" }
# Eval Verifier's on_failure
kind: notify
channel: { kind: slack, connection: ops-slack, channel_id: C0123456789,
           text: "{{subject_ref}} passed {{eval.passed_tasks}}/{{eval.total_tasks}} tasks ({{eval.pass_rate_percent}}%)" }
```

---

## Registry lifecycle

How authored specs become durable cards. This section pins the wire
shape of composite registration, the card blob state machine, and the
idempotency contract. Implementation lives in `wyrd_client::cards`; active change
specifications and task packets follow the repository specification-first
workflow and do not override this contract.

### Composite registration

`POST /v1/cards` is a **composite** endpoint. One request may register
one card or many, and the server derives the root from the DAG of
submissions.

```rust
struct CreateCardRequest {
    submissions: Vec<CardSubmission>,
}

struct CardSubmission {
    api_version: ApiVersion,
    kind: CardKind,
    metadata: Metadata,
    spec: Value,                                       // kind-specific spec body
    artifacts: Vec<ArtifactManifestEntry>,             // heavy uploads; default empty
}

struct CreateCardResponse {
    root: CardRef,                                     // server-derived, single no-in-degree node
    outcomes: Vec<CardRegistrationOutcome>,           // one per submission
    upload_plans: Vec<CardUploadPlan>,                // one per heavy submission (see §Heavy artifacts)
}
```

**Server-derived root.** Clients never nominate a root. The server
builds the DAG from typed `CardRef` edges (including `verified_by`),
topo-sorts, and picks the single node with no incoming edges. Multi-root
DAGs are an SDK bug and surface as `WYRD_INTERNAL_500` — not a stable
public error. Cycles surface as
`WYRD_REGISTRY_400_DEPENDENCY_CYCLE`.

**Wrapper structs are forbidden.** No `{card, dependencies[]}`, no
`{primary, dependencies}`, no `PlanCardRequest`. Submissions are a flat
`Vec<CardSubmission>`; edges live inside each `spec`.

**Composite transaction.** Per-node inserts run in one Postgres
transaction. Post-commit side effects (card blob write, presigned URL
mint) are best-effort with reconcile — see below.

### Card blob state

Every card carries two columns on `wyrd.cards`:

- `card_blob_uri: TEXT` — presence of a URI signals the fully-hydrated
  card envelope has been written to durable object storage.
- `blob_failed_at: TIMESTAMPTZ` — presence of a timestamp signals the
  most recent write attempt failed; this row is the reconcile key.

There is **no** `blob_status` enum column and no third state. Success
and failure are exclusive: reconcile writes `card_blob_uri` and clears
`blob_failed_at`; a failed write sets `blob_failed_at` and leaves
`card_blob_uri` NULL. A card reaches `Active` only when its blob is
written AND, for heavy cards, every declared artifact upload is
verified.

Reconcile sweep: pending rows past 24h since `blob_failed_at` are
retried up to 3 times with exponential backoff, then dead-lettered.
The dead letter is reconciliation lineage on the Card row, not a canonical
audit event: reconciliation evaluates no principal permission.

### Heavy artifacts

A submission whose `artifacts` manifest is non-empty MUST be the sole
submission in the request — enforced by
`WYRD_REGISTRY_400_HEAVY_ARTIFACT_NOT_SOLE_SUBMISSION`. Multiplexing
`/upload/init` across N heavy submissions in one composite request
complicates recovery for negligible authoring benefit, so v1 refuses
it. Presigned upload URLs are tenant-scoped, `PUT`-only,
single-artifact, and expire after 3600s.

### Idempotency

Idempotency is a client responsibility. The `Idempotency-Key` header is
required on `POST /v1/cards`. Missing header →
`WYRD_REGISTRY_400_IDEMPOTENCY_KEY_REQUIRED`.

Key derivation (client-side):

1. Canonicalize submissions by `(kind, space, name)`.
2. For each submission, compute `spec_hash` = BLAKE3 hex over JCS
   canonical bytes of the spec plus `artifact_manifest_hash` (BLAKE3
   over the manifest, empty if no artifacts).
3. `request_hash` = BLAKE3 hex over the concatenation of per-submission
   `(spec_hash || artifact_manifest_hash)` pairs in canonical order.
4. `Idempotency-Key` = BLAKE3 hex over `tenant_id || principal || request_hash`.

Server-side behavior:

- Same key, same `request_hash` → `IdempotentNoop` replay, returns the
  original response.
- Same key, different `request_hash` →
  `WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT`.
- Key not seen → normal processing; key + hash stored on completion.

Idempotency state lives in `wyrd.card_registration_operations` with a
24h TTL. Expired records outside the retry window surface as
`WYRD_REGISTRY_410_OPERATION_EXPIRED`.

### Error catalog

The public error catalog covering registration is fixed and versioned.
Codes and semantics live in `crates/wyrd-spec/src/error.rs` behind the
`WyrdError` derive. Codes referenced above:
`WYRD_REGISTRY_400_DEPENDENCY_CYCLE`,
`WYRD_REGISTRY_400_HEAVY_ARTIFACT_NOT_SOLE_SUBMISSION`,
`WYRD_REGISTRY_400_IDEMPOTENCY_KEY_REQUIRED`,
`WYRD_REGISTRY_400_UNRESOLVED_PATH_REF`,
`WYRD_REGISTRY_400_UNRESOLVED_INLINE_REFERENCE`,
`WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT`,
`WYRD_REGISTRY_410_OPERATION_EXPIRED`,
`WYRD_REGISTRY_507_ARTIFACT_VERIFY_FAILED`,
`WYRD_SPEC_400_DUPLICATE_VERIFICATION_BINDING`,
`WYRD_SPEC_400_INVALID_VERIFIER_REF_KIND`,
`WYRD_SPEC_400_INVALID_BINDING_REF_KIND`,
`WYRD_SPEC_400_UNSUPPORTED_OPERATOR_ACTION`,
`WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH`,
`WYRD_INTERNAL_500`. The catalog is the source of truth for problem-json
serialization across HTTP, Python, TypeScript, MCP, and CLI surfaces.

---

## Spec-file authoring

Two complementary mechanisms — `wyrd apply -f file.yaml` reads + registers in
one move, and reference slots accept `Ref` or `InlineableRef<T>`. A path string is
loader-only authoring syntax projected to a Ref/Sibling before send; inline
is available only at `InlineableRef<T>` slots.

### Pre-registration matrix (Rule 16)

| Kind | Must pre-register? | Why |
|------|---------------------|-----|
| `Model`        | **Yes** | Carries weight artifacts; lineage anchor. |
| `Data`         | **Yes** (unless used purely as inline eval scenarios, which v1 does not support — `dataset` is `DatasetRef`-only) | Carries dataset bytes; lineage anchor. |
| `Experiment`   | **Yes** | Carries run history. |
| `Artifact`     | **Yes** (typically derived from heavy cards) | Pointer to durable bytes. |
| `Prompt`       | Optional | Light. Inlineable as `InlineableRef<Prompt>` inside `Agent.prompt`. |
| `Agent`        | Optional | Light. Spec-only; `apply` registers it. `LlmJudgeTask.judge_ref` is `InlineableRef<AgentSpec>`. |
| `Verifier`, `Policy`, `Trigger`, `Operator`, `Source`, `Mcp`, `Workflow`, `Audit`, `Service` | Optional | Light. Spec-only; `apply` registers each card as it's read. |

A single YAML file may contain many `---`-separated card documents — `wyrd
apply -f eval-suite.yaml` registers all of them in dependency order. The Agent
under test, the Source it reads from, and the Verifier that judges it can all
ship in one file.

### Reference forms

`Ref` and `InlineableRef<T>` are untagged values. A CardRef mapping is an
external reference, a string is a loader-local path, and an inlineable slot may
contain its native child body directly. There is no additional keyed
`ref`/`path`/`inline` discriminator wrapper around a slot value.

```yaml
# Registered Prompt reference at Agent.prompt.
prompt: { kind: Prompt, name: triage, version: "1.0.0", space: prod }

# Local full Card envelope, resolved relative to the containing file.
prompt: ./prompts/triage.yaml

# Inline native Prompt body: no independent Card identity.
prompt:
  model: example-model
  request:
    provider: open_ai_chat_completion
    body:
      model: example-model
      messages: [{role: user, content: "Hello"}]
  variables: []
  response_type: text
```

The owning spec may itself name a field `ref`; this does not introduce a
reference discriminator. Service components retain that field for every form:

```yaml
components:
  - alias: agent
    ref: { kind: Agent, name: support-triage, version: "1.0.0", space: prod }
  - alias: model
    ref: { kind: Model, name: churn-classifier, version: "1.0.0", space: prod }
  - alias: prompt
    ref: ./prompts/triage-system.yaml
```

`CardRef` carries `kind`, `name`, one `version`, optional `space`, and optional
`uid`; never labels, annotations, or a version requirement. An omitted space
inherits the containing authored context. Local loading never registers. Apply
registers path-loaded Cards in the same composite request, projects paths to
Sibling references, and preserves external refs even at matching identities.
Inline bodies remain embedded. Only resolved Ref/Sibling and inline bodies
cross registration; durable persistence contains no Path or Sibling.

### Reference-slot inventory

The same two reference types apply at **every** reference slot, light or
heavy. There is one canonical slot inventory, and the loader, diagnostics, and
relationship tests all project from it:

- `Service.components[].ref` (light and heavy targets)
- `Service.components[].verified_by`, `Agent.verified_by`,
  `Service.verified_by` — each binding's `verifier`, `runs_on`, and
  `on_failure[]`
- `Agent.prompt`
- `Workflow.steps[].target`
- `Operator.workflow_ref`
- Verifier implementation refs: Eval `dataset` and
  `EvalTask::LlmJudge.judge_ref`; Drift `signal.baseline_ref`
- Heavy anchors: `Model`/`Data`/`Experiment` `*_refs`, `Artifact` refs

 A new reference-bearing field is added to this inventory in one place; it then
 participates in path rewrite and relationship derivation without a second
 edit. This is the single-source rule for reference slots.

### Path resolution rules (loader contract)

A path string is **client-side authoring sugar**, not a durable wire variant.
It targets a full Card envelope on disk; the loader reads it without registering.
Apply submits it as an independent Card and rewrites the referring slot to an explicit
registration reference. When the target is part of the same composite
submission, the registration reference is `Sibling { sibling: CardRef }`;
an authored external `ref: CardRef` remains `Ref(CardRef)`. This preserves
binding intent instead of inferring it from identity. The server validates
both forms and rewrites them to UID-bearing `Ref(CardRef)` before hashing,
relationship derivation, or durable persistence. The server, registry, and
`vala` never see a `path:` value or a durable `Sibling` value.

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
- Transitive: a `path:`-loaded envelope may itself contain `ref`/`path`/`inline`
  slots. The loader resolves transitively with a hard depth
  limit (≤8) to catch cycles.
- `ref`, `path`, and `inline` are mutually exclusive on any single slot. Any
  combination is a validation error.

This keeps the wire contract typed (`ref | inline`, with an explicit
registration-only sibling projection where a composite write needs it), keeps
`path` resolution client-side, prevents filesystem-on-server, and gives authors
the file-splitting ergonomic they expect from JSON-Schema `$ref` / OpenAPI
external-file imports. Durable and read contracts remain the simpler direct
`ref` / `inline` forms.

---

## Workspace config (`wyrd.toml`)

Wyrd's wire contract identifies every card by `kind`, `name`, and one version
field, with optional `space` and `uid`. Authors writing many cards in one bundle
still benefit from consistent metadata defaults. This section formalizes the
loader-side ergonomic without changing the wire identity shape.

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
- Explicit override: the CLI and SDK configuration contract exposes
  `wyrd --config <path>` and `WYRD_CONFIG`; both pass an exact file path to
  `WyrdConfig::load(Some(&path))`.
  Explicit relative paths are preserved as-given; the
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
  even though the field itself remains in the payload. Therefore `version` is
  never read from `[defaults]`.
- **Per-kind table keys are PascalCase** matching
  `CardKind::wire_name()` — the same identifier appears identically
  in YAML (`kind: Model`), TOML (`[kind.Model]`), and Rust source.
  `[kind.External]` is reserved as a forward-compat catch-all on
  the wire and is **not** a valid override-table key here; using
  it raises `WYRD_CFG_400_SCHEMA_MISMATCH`.
- **Unknown tables and unknown keys are errors**
  (`#[serde(deny_unknown_fields)]`). A typo like `[default]`
  (missing `s`) surfaces at parse time, not as silent drop. The
  `deny_unknown_fields` posture means adding a top-level table requires a
  coordinated protocol/client release.

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
  identity or intent; the last three
  are server-derived.

This is the same architectural slot as the loader-side
`metadata.space` inheritance documented in [Reference forms](#reference-forms):
the wire payload still arrives at the server fully
populated; the server never reads `wyrd.toml`.

### No lockfile by design

Wyrd has no version-range resolution. Every `CardRef` is authored
exact (strict `MAJOR.MINOR.PATCH`, no `latest`, no comparators), so
there is nothing to "pin" that isn't already pinned at the source.
Adding a `wyrd.lock` would teach users a mental model that contradicts
the wire contract — they would expect `version: latest` to be
resolvable, and the honest answer is "no."

`wyrd.toml` has no `[dependencies]` table. Cross-tenant card import and
foreign-card content pinning are not supported workflows.

---

## Reference-direction quick reference

| Card    | Refs that authored on it             | Refs that point at it          |
|---------|--------------------------------------|--------------------------------|
| Data    | `card_refs`, `splits`                 | Verifier Drift `signal.baseline_ref`, Verifier Eval `dataset`, `Experiment.target_refs` |
| Model   | `card_refs`                           | `Service.components.ref`, `Experiment.target_refs` |
| Agent   | `prompt`, `tool_names`, `verified_by` | `Service.components.ref`, Agent prompts (sub-agent calls), Verifier Eval `LlmJudge.judge_ref` |
| Workflow| `steps.*.target`                     | `Service.components.ref`, `Operator.workflow_ref` |
| Mcp     | `server_name`, `transport`, `scopes` | `Service.components.ref` |
| Verifier| Drift `signal.baseline_ref`; Eval `dataset`, `tasks[].LlmJudge.judge_ref` | `*.verified_by[].verifier` |
| Audit   | `subject_refs`, `query` (roots), `lineage` (nodes), `investigator` (Agent variant) | — |
| Service | `components[].ref`, `components[].verified_by`, `verified_by` | — |
| Policy  | `rules`                              | `Service.components.ref` |
| Trigger | `schedule` or `observations_ready` activation | `*.verified_by[].runs_on` |
| Operator| `action` (`workflow_ref` \| typed `channel` shape \| tenant connection) | `*.verified_by[].on_failure` |
| Source  | `kind` (bucket), `connection` (vendor + `*_env`) | Verifier Eval `source_ref` |

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
│   ├── judge-agent.yaml
│   └── judge-prompt.yaml
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

## Decisions

The following choices constrain every implementation:

- **Source vendor read adapters.** `SourceKind` is a closed, bucket-keyed tagged
  union (`object_store`, `sql_warehouse`, `metrics`, `logs`, `traces`); each
  bucket nests a `vendor`-keyed `*Connection` union, and secrets are server-side
  env-var names (`SourceAuth`), never card values. Adding a vendor is a new
  `*Connection` variant plus a `vala` read adapter — no bucket or consumer churn.
  Runtime uses the `vala` read trait keyed on `(kind, vendor)` and exposes the
  connectivity preflight as `wyrd source check <ref>`.

- **Eval signal decomposition.** The Eval implementation does not carry its
  own `signal` decomposition; its per-task pass/fail and pass gate produce the
  Verifier's verdict directly. The input edges (`dataset` vs `source_ref`) are
  optional refs, not a tagged enum: presence is the mode (offline driver,
  online sink, both, or neither → continuous `vala.eval.observations`).
