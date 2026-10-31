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
- [Kind catalog](#kind-catalog) — 14 registrable kinds + External discriminator
- [Per-kind specs](#per-kind-specs) — field shapes per kind
  - [Data](#data) · [Model](#model) · [Artifact](#artifact) · [Experiment](#experiment)
  - [Prompt](#prompt) · [Agent](#agent) · [Mcp](#mcp)
  - [Service](#service) · [Policy](#policy) · [Audit](#audit)
  - [Verifier](#verifier) · [Source](#source) · [Bifrost](#bifrost)
  - [Trigger](#trigger) · [Operator](#operator)
- [Registry lifecycle](#registry-lifecycle) — composite declaration, card blob, idempotency
- [Bifrost design](./bifrost-design.md) — OLAP warehouse: tables, Scribe ingest, Oracle admission, Forge, public surface
- [Spec-file authoring](#spec-file-authoring) — `ref` / `path` / `inline`, pre-declaration matrix
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

Wyrd does not become the user's application runtime, training framework,
workflow engine, arbitrary code executor, or cloud platform. User code owns
inference loops, training loops, app servers, agent processes, and deployment
mechanics; runtime results that persist become runs, observations, artifact
Cards, relationships, audit records, or status updates through service-owned
paths.

The verification loop:

```text
Cards and Specs declare the system
  -> verification bindings attach Verifiers
  -> Runs and Observations record measured behavior
  -> Verifiers produce judgments
  -> Bifrost, lineage, and audit preserve the evidence
  -> Policy governs decisions and Operators react
```

| Term | Meaning |
|---|---|
| Verification | Evaluating observed behavior against declared criteria and producing a judgment. |
| Assurance | Confidence supported by durable verification evidence, provenance, policy decisions, and audit history. |
| Observation | A measured fact. An observation is evidence, not a judgment. |
| Verifier | A reusable, subject-less declaration that interprets observations and produces judgments. |

The core nouns are `Card` (a declared thing with identity, kind, version,
spec, relationships, and status), `Spec` (the kind-specific declared payload),
`Run` (one execution of declared behavior), and `Observation` (a measured
fact). A new concept first tries to fit as a Card kind, spec field,
relationship, run, or observation; add a noun only when those shapes cannot
express it honestly. Layers are not skipped: a Card kind specializes `Card`
and changes only its spec, services operate on the foundations (envelope,
metadata, `CardRef`, relationships, status, version), and surfaces expose
services.

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
   (`wyrd.observe` → `vala`, linked to declared cards) — this is the
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
13. **No event vocabulary on the wire.** Evidence comes from typed Verifier input slots;
    "activation" comes from the closed `TriggerSpec` activation. Free-form
    event-name strings are doctrine drift.
14. **Host config stays off Cards.** Permission modes,
    sandboxes, isolation, effort, and per-CLI compatibility are properties of
    the host that runs the Agent, not of the Agent contract.
15. **Verifiers are pure judgment producers.** A Verifier declares its evidence,
    preparation, and requirements. It does not carry
    scheduling and they do not carry dispatch. Scheduling lives on
    `Trigger.schedule`. Dispatch lives on `Operator`. There is no `Alert`
    kind — alerting is an Operator with a notification adapter.
16. **Heavy cards anchor lineage; light cards are spec-only.** Model, Data, and
    Experiment carry durable artifact bytes and MUST be pre-declared before
    anything else can point at them — they're the lineage anchors. Every other
    kind (Prompt, Agent, Verifier, Policy, Trigger, Operator, Source, Mcp,
    Audit, Service) is spec-only: `wyrd declare file.yaml` reads
    and declares in one move. No separate storage step, no programmatic
    declaration prerequisite.
17. **Light cards may inline in place of a `CardRef`.** Wherever a spec slot
    permits an embedded child, it uses `InlineableRef<T>`. Inline definitions
    have no card identity, are not declared standalone, and cannot be
    referenced from outside their parent. To reuse a child, declare it as a
    card and reference it with `Ref::Ref(CardRef)`. Durable-only slots use
    `Ref`. One exception: declaration materializes a declared Agent's
    inline Prompt as its own Prompt Card named `<agent-name>-prompt` in the
    Agent's tenant, versioned on its own registry line, and persists the Agent
    with that Card's exact UID-bearing reference. Identical inline content in
    differently named Agents is never shared.
18. **Auth and Policy are two distinct planes.** Emit is **not** a third
    plane: a deployed service's observation/ingest writes are ordinary
    Auth-plane routes, authorized by the same JWT and a
    `Permission { resource, action, scope }` like every other call. The legacy
    per-card **governance token is removed** — the JWT proves the principal and
    bounds its emittable **card scope** through its signed `card_access`. A
    token with a selected root (`workload_uid`) is `scoped`: the root plus the
    **observation-target** cards reachable through the transitive card-ref graph
    declared in that root's spec. A card
    enters the scope only if its kind is an observation target — a kind a client
    (`wyrd.observer`, Bifrost, drift samples) attributes records to: `Data`,
    `Model`, `Experiment`, `Prompt`, `Agent`, `Service`, `Mcp`, `Artifact`,
    `Source`. `Verifier` and control-plane kinds (`Policy`,
    `Audit`, `Operator`, `Trigger`) may be referenced for governance or
    verification but never enter the emit scope. A Service root therefore
    includes declared `Service.components` and their reachable observation targets.
    Workload roots are Services only; Agents remain component subjects. Every token mint — including the re-exchange a machine performs
    when its access token expires — resolves the selected root's bounded graph
    to Card UIDs and signs that set. A rootless User, tenant administrator, or
    unrestricted Service is `any_registered`: it may attribute to any declared
    observation-target Card in its tenant, which ingest resolves against the
    tenant registry. A rootless family-restricted Service is `root_required`
    and may attribute to no Card until a root is selected.
    An observation may carry its subject `card_uid`; when present, the
    server authorizes it against that scope's mapped UIDs and stores it.
    Generic telemetry may omit it and retains the authenticated publisher through
    the managed `sub_principal_id`/`act_principal_id` pair. A separate emit credential was redundant — see "Observation
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
      [Permission model](wyrd-security-posture.md#permission-model).
    - **Policy** gates card states (`classify` at declare-time, `gate` at
      deploy-time). There is no runtime cross-service invoke gate: the
      `PolicyAction::Invoke` enum member and the `POST /v1/authz/check`
      runtime evaluation surface it referenced were removed and have no
      replacement in this design.

    Runtime identity is a `Principal { id: PrincipalId, kind: PrincipalKind,
    tenant_id, roles, effective_permissions, credential_id, card_access,
    workload_uid }`. `PrincipalId`
    is a `Uuid` newtype; no string-prefix encoding (no `user:`, `sa:`,
    `agent:`) — discrimination lives on the kind. The closed wire set is
    `PrincipalKindTag`: `GlobalAdmin`, `TenantAdmin`, `User`, `Service`,
    `System`, serialized as `global_admin`, `tenant_admin`, `user`,
    `service`, `system`. A `GlobalAdmin` is a platform-plane principal
    and has no tenant,
    carried at runtime as `PlatformPrincipal { id, kind,
    effective_permissions, credential_id }`; a platform-plane human is a
    `User` with no tenant. Every tenant-plane principal has exactly one
    tenant and a Card-free `PrincipalKind`: `TenantAdmin`, `User`, `Service`,
    or the internal-only `System`. Agent is a Card kind only. A Card root is
    request execution context — the optional `workload_uid` and its
    `card_access` limit — never part of the account's identity, so one stable
    Service account runs every release of its workload and a tenant
    administrative or automation account needs no Card at all. `User` is
    the marker for human identity. `System` is a tenant-local server identity that attributes
    verification results and authorizes Verifier input reads in-process. It
    never appears in a token: no issuer mints one, every verifier rejects a
    `system` claim set, and it has no public credential, role, refresh,
    workload, delegation, or principal-management path. Platform authority
    is a grant held at platform scope, not a property of a kind, and neither
    plane's credential or token is accepted by the other. Gateway capture is a
    server-internal write, not a principal that authenticates: captured rows
    carry the verified pair the call ran under. Two existing boundaries,
    not a transport identity, protect that attribution: public Gate
    ingestion always stamps the verified token's pair and refuses supplied
    SUB/ACT columns, so only server-owned outbox writes (directly or over
    the mTLS peer capture RPC) send per-row pairs, which Scribe still
    validates as principal ids; and every outbox lane and batch is one
    tenant's slice, which Scribe checks against the authenticated tenant
    before binding the tenant's physical location. Different verified pairs
    may share one tenant's frame; tenants never do.
    `PLATFORM_AUDIT_PRINCIPAL` is the single no-verified-caller marker. It
    fills both `sub_principal_id` and `act_principal_id` on an audited
    decision with no verified identity and on a creation row no verified
    caller caused: SYSTEM bootstrap, startup built-in reconciliation,
    reserved audit-owner tables, the defining SQL backfill, and dev-fixture
    seeding. It is not SYSTEM and authorizes nothing. Tenant provisioning by
    an authenticated platform operator records that operator's platform pair
    on the tenant's SYSTEM principal and built-in tables, an explicit
    exception like platform audit; SYSTEM is never its own creator, and
    retries and reconciliation keep the first pair. A request refused before
    any principal resolves (no credential, a malformed bearer token, or one
    failing signature or claim verification) records no audit decision; the
    credential-exchange refusals audit records keep the sentinel pair when no
    identity is verified. `wyrd declare service.yaml` (or an Agent card)
    writes Card versions only; it creates no principal and returns no secret.
    An administrator creates the stable Service account and its first
    credential explicitly at `POST /v1/principals`. The caller uploads the key to the deploy
    environment's secret store (Vault, AWS Secrets Manager, GCP Secret
    Manager); deploy-time secret injection puts it into the pod as
    `WYRD_API_KEY`, optionally with `WYRD_WORKLOAD_UID` naming the exact
    release root. The SDK exchanges it at `POST /auth/token` for a short-lived
    JWT and re-exchanges the same durable key and root whenever that token expires or
    the server refuses it; a machine holds no refresh token. `/auth/token` derives `tenant_id` and
    `principal_id` from the verified API-key record — never from a
    client-supplied header. The JWT carries top-level `principal` (the subject;
    under delegation, the party being acted for) and an RFC 8693 `act` chain
    whose outermost layer is the current actor; a single delegated token
    carries both sides of an invoke. Service B obtains it with
    `WyrdClient::on_behalf_of(A token, audience)`, presenting A's token as
    `subject_token` and its own as `actor_token`; its permissions are the
    exact intersection of both. If the existing scopes cannot represent the
    complete intersection, exchange is refused without issuing a token;
    permissions are never widened or silently dropped to make issuance work.
    On cross-service calls the SDK puts that delegated
    Wyrd JWT in the `X-Wyrd-Access-Token` header. The application's own
    `Authorization` header is never touched. `Wyrd-Caller-Identity` is
    rejected legacy — do not reintroduce. Both subject and actor identities
    are server-verified from one signed delegated JWT — no enforcement-point
    JWT, no SPIFFE/mTLS actor derivation. Delegated exchange consults no
    policy; it records `auth.token.exchange` as its audit permission.
19. **Reference slots use exactly `Ref` or `InlineableRef<T>`.** `Ref` carries
    durable identity or an authored `Path`; `InlineableRef<T>` additionally
    permits an inline child body. `Path` is loader-only. During composite
    declaration, the loader may rewrite an authored path to the
    declaration-only `Sibling { sibling: CardRef }` form so the server can
    distinguish a target submitted in the same request from an external Card
    referenced with `Ref(CardRef)`. The server validates both forms and
    rewrites them to UID-bearing `Ref(CardRef)` before hashing, relationship
    derivation, blob persistence, or any read response. `Sibling` is not a
    durable reference form, and there is no separate selector reference enum.
    The server, registry, and evaluator reject unresolved paths at their
    boundaries.

    `path` resolves to a typed declaration reference before send; only `ref`,
    declaration-only `sibling`, and `inline` cross a composite declaration
    wire. Heavy cards (Model, Data, Experiment, Artifact) accept `ref` only —
    `inline` is rejected because identity anchors lineage. Environment/stage is
    target-card metadata (`labels` /
    `annotations`), never identity. Card identity is tenant, kind, name, and
    version, and one server may hold development, staging, and production cards
    side by side under distinct names or versions.
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
    edge/negative flows a real caller actually hits — re-declare with a
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

Framework adapters follow the same rule. The Python ASGI middleware
(`wyrd.integrations.asgi`) starts and drains an already loaded `WyrdState`
with the application's lifespan and enters one Run per HTTP request or
WebSocket connection. It registers nothing, owns no writer, provider, or
credential, and creates no HTTP spans.

Existing Rust owner crates may retain their optional `python` features during
this integration. Only `wyrd-sdk-python` enables and aggregates those features;
the Rust and TypeScript SDKs do not. New or materially relocated Python logic
belongs in `wyrd-sdk-python`. Consolidating all existing Python logic there is
the target direction, not a reason for unrelated code movement during this
merge.

Every server-facing handle shares one construction vocabulary. Rust spells
it `connect()` for the ambient client and `connect_with(&client)` for a
supplied one; TypeScript has only `static async connect({ client? })` behind
a private constructor; Python has `connect(client=None)` beside an IO-free
constructor that connects once on the handle's first server operation.
`connect` is eager: it resolves the client and completes any credential
exchange that client requires before returning, while a supplied bearer or a
fresh cached token makes no request and no health probe is added. A handle
retains that one client, with its renewable token cache, for its lifetime.
Local loaders (`from_path` / `from_path_with`) are not connected handles and
stay lazy and offline. `WyrdState.declare` is the online constructor: with one
client it declares a Card file, pulls the declared graph, and returns a ready
state that keeps no files unless the caller names a bundle directory. Bifrost lists the caller's tables with `list_tables` /
`listTables`, which returns compact entries, and describes one table's schema
on demand.

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

Wyrd registers 14 native Card kinds. `CardKind::External` is a non-registrable
discriminator used when reading foreign or unknown kind metadata. The foreign
payload remains opaque and source-specific; it is not a Wyrd Card. There is no
`ExternalSpec` and no External Card declaration path.

| Domain        | Kinds |
|---------------|-------|
| Data plane    | Data, Model, Artifact, Experiment |
| Agent plane   | Prompt, Agent, Mcp |
| Composition   | Service |
| Governance    | Policy, Audit |
| Observability | Verifier, Source |
| Reaction      | Trigger, Operator |

`Tool` (a Skald runtime registry name), `SubAgent` (an Agent-to-Agent
relationship), `Skill`, and `Bifrost` (server state) are not Card kinds.

Every Card uses one envelope: `apiVersion: wyrd/v1`, `metadata`, `kind`,
`spec`, server-derived `relationships`, and server-managed `status`. There is
no outer `kind: Card` wrapper and no `spec.type` discriminator.
`metadata.labels` are short indexed classification fields that humans,
agents, policy, and inventory views filter on; `metadata.annotations` carry
longer descriptive context that travels with the Card but is not indexed.
`tags` are not part of the public metadata contract.

### Rejected shapes

- Compatibility aliases, legacy names, or route prefixes from predecessor
  projects, including governance tokens (`WYRD_GOV_TOKEN`,
  `wyrd.auth_governance_tokens`) and Delta transaction-log logic in Vala.
- One-off artifact reference wrappers or a second public shape for one
  concept.
- Authored lineage graphs that bypass `CardRef`s and derived relationships.
- Per-kind registration methods on Card objects, or per-kind shadow tables
  beside the single `wyrd.cards` table.
- Server-only fields in client contracts, or surface vocabulary that diverges
  from the spec.
- Live runtime objects in specs, untyped strings where Wyrd has a domain type,
  or runtime plugins where a closed enum is the real contract.
- Public event-shaped payloads where `card`, `run`, `observation`,
  `relationship`, `status`, `policy`, or `audit` names the concept.
- Hidden side effects during local Card construction.

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

`wyrd artifact upload SOURCE --name N [--version V | --bump LEVEL]
[--artifact-kind K] [--extract]`, `Cards.upload_artifact`, and TypeScript
`uploadArtifact` publish one regular file (at its basename) or every regular
file of a directory (at its relative path) unchanged through the ordinary
declaration lifecycle; symlinks, other non-regular sources, empty directories,
and a blank `artifact_kind` are refused, and version decisions stay with the
registry. Nothing is parsed by default. With extraction, the client runs the
shared `wyrd-loader` extractor on each supported document `P`, publishes its
output beside the original as `extracted/P.json`, and records
`metadata.extraction = {version, files: [{source, output}], skipped}`; a
caller-supplied report, an output colliding with an original, a refused
document, or a source changing during preparation fails before anything is
declared. `wyrd artifact files`, `list_artifacts`, and `listArtifacts` read
the stored inventory.

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
A Prompt is authored once, in exactly one of two forms, and is stored,
hashed, returned, and hydrated in that form. Variables, media variables, and
the response type are derived, never authored.
```yaml
# Portable form: compiled by the shared Skald compiler for its provider.
spec:
  provider: ProviderName           # openai | anthropic | google | vertex | jev | <custom>
  model: string
  system?: string
  messages: [{ role: user | assistant, content: string | [ContentBlock] }]
  settings?: PortableSettings      # temperature, top_p, max_output_tokens, top_k, stop_sequences, seed, presence_penalty, frequency_penalty, reasoning_effort (chat_completion only)
  response_format?: JsonSchema | verdict | { kind: verdict, data?: JsonSchema }
# Native form: one protocol body passed through unchanged.
spec:
  provider: ProviderName
  model: string
  format: chat_completion | responses | messages | generate_content | system_one
  body: { ... }                    # that protocol's request body, extension options preserved
  response_format?: verdict | { kind: verdict, data?: JsonSchema }
```

`response_format: verdict` (or `{kind: verdict}`) compiles the closed
Verdict schema generated from Wyrd's canonical `Verdict` type, projected onto
the subset OpenAI strict mode, Anthropic, and Gemini all accept: `oneOf`
becomes `anyOf`, a `const` alternation one `enum`, and `$ref` siblings,
`format`, and `minimum` are dropped, while typed decoding still enforces the
numeric ranges. `data` adds a required custom object with that schema, kept
exactly as authored. A named format lowers to non-strict OpenAI structured
output, because an authored `data` schema need not satisfy strict mode;
validation is Wyrd's either way. A `data` schema is JSON Schema
2020-12 (no other `$schema`) without a root `$id`; it is embedded as its own
resource so its local references resolve as they do standalone. The card keeps and hashes the
authored form, never the expanded provider settings. On a native body the
named format is lowered into that protocol's structured-output settings; a
body that also sets its own response settings, or a `system_one` body, is
refused. Skald owns the
compiler but no format names: Wyrd resolves `verdict` when it compiles a
Prompt, and an unresolved named format cannot render.

`provider` names the Prompt's protocol family and the native destination; a
custom name is an OpenAI-compatible Chat provider. It never names a gateway
deployment: the Agent's `execution_route` chooses where the call executes.
An Agent's inline Prompt takes the same two forms and declares as the
Agent's own `<agent-name>-prompt` Prompt Card (doctrine 17).

### Agent
Agent contract: prompt + tools + run config. Tool names resolve through the
runtime tool registry (host tools + MCP server registrations). Approval,
per-tool blocks, and hook gates live on `Policy`, not here.
```yaml
spec:
  prompt: InlineableRef<Prompt>  # Ref (→ Prompt), authored Path, or inline Prompt
  execution_route: ExecutionRoute # required: {kind: native} | {kind: wyrd_gateway, deployment: <name>}
  tool_names: [string]
  run_config: AgentRunConfigSpec # max_iterations, tool_concurrency_cap, session_recent_limit, timeout_ms
  verified_by: [VerificationBinding]
```
Every declared Agent persists `prompt` as an exact Prompt Card reference:
Path and Sibling forms resolve to the authored Prompt Card, and an inline body
declares as `<agent-name>-prompt` (doctrine 17). Hydrated state therefore
always contains the Agent's Prompt Card without a separate Service component;
a hydrated Agent's `prompt()` returns that Prompt Card, whose `ref` is its
exact identity, and `prompt_ref` / `promptRef` is the stored reference.
Generic state lookup (`state[alias]` / `get` in Python, `get` in Rust and
TypeScript) returns the kind-appropriate hydrated Card. A bundle whose Agent
Prompt has no declared identity is `WYRD_SDK_400_INVALID_STATE_BUNDLE`;
re-declare and pull it again.

`native` calls the Prompt's provider with the executing host's own provider
configuration; server-controlled execution refuses it. `wyrd_gateway` sends
every model call of the bounded tool loop to `<deployment>/<Prompt model>`
through the caller's tenant gateway, with ordinary authorization, capture,
and audit; the deployment is never derived from the Prompt provider. An
invocation may supply a whole replacement route for that invocation only;
it never changes the Card, its hash, or a concurrent invocation.

A structured-output Agent validates every proposed final answer against the
exact response schema its outgoing request sent, after rendering and
`before_model` hooks. An invalid answer (malformed JSON, a non-object, or a
schema violation) is answered with one bounded correction turn citing the
violated rules and schema fragments, and the loop continues: a repair spends
one iteration of `max_iterations`, shared with tool turns. An answer still
invalid on the last iteration ends the run with
`WYRD_AGENT_422_STRUCTURED_INVALID`, which is terminal and never retried; its
message and details carry at most ten schema locations, never reply values,
reply keys, or parser text. A provider refusal without an answer is never
repaired: it ends the run at once with the same code and a fixed diagnostic. The correction is conversation content, captured
under the run's sensitive capture policy, not error or log content.

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

  - `classify` (declare-time): rule derives attrs onto the card (e.g.
                                `risk.tier = "high"`). Never blocks.
  - `gate`     (deploy-time):   rule allows/denies the card's deploy-time
                                deployment, and thus its emit eligibility.
                                Blocks when Deny.
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
- `PolicyTarget = { kinds: [CardKind], actions: [PolicyAction] }`
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
path: the in-memory Scribe outbox into `bifrost.system.audit_log`. Permissions
are blocking; audits are non-blocking: a decision is staged on the shared
Scribe outbox and never delays or fails the operation. A decision records the deciding principal's
stored kind and, when its token was minted from a credential, that
credential's non-secret id.

#### Principal model

Wyrd principals are UUID-backed runtime identities that exist independently of
any credential: issuing, rotating, revoking, or losing a credential never
creates, destroys, or alters a principal or its role grants. Gateway capture
is a server-internal write holding no token, role, or grant; Scribe stamps
each captured row with the verified pair the call ran under. Each `wyrd-server` process stages captures on its one
Scribe outbox, which submits in-process when Scribe runs in the pod
(`WYRD_TARGET`) and otherwise over the peer plane to a live, ready Scribe,
retrying Scribe backpressure. A capturing call reserves its outbox slot during
admission, before it is charged or dispatched; while the outbox is full,
admission backs off and retries until the call's deadline, and then refuses the
call with the retryable `WYRD_GATEWAY_429_CAPTURE_STAGING_FULL`. A reserved
capture is never shed. Payload, storage, or deadline failures while the
capture is projected drop it with a counted reason without changing the
answered call. The same outbox submits Verifier results to the
three result tables, attributed to the pair the result was produced under —
the caller's verified pair for a realtime invocation, and the run's subject
with the tenant's `System` principal as actor for a queued run — which Scribe
stamps as `sub_principal_id` and `act_principal_id`; each row names the verified Card as `card_uid` and the
exact Verifier Card as `verifier_card_uid`; a result batch is never dropped for time and is retried under its
own batch id until Scribe acknowledges it. Gate refuses every public write to
`bifrost.gateway.calls` and the three result tables.
Tenant principal kinds are `TenantAdmin`, `User`, and `Service`, plus the
internal credentialless `System` actor. Agent is a Card kind, not a principal
kind. A `Service` principal is a stable nonhuman account for Services, Agents,
CI, and automation: an administrator creates it explicitly once, the server
assigns its id, and it outlives every release. Registering or re-declaring a
Service or Agent Card creates Card versions only, never a principal, role
grant, or credential. Exact versions are request execution context, not
account identity.

A Service account carries one `workload_restriction`, a closed union:
`{"mode":"unrestricted"}` (the default) or
`{"mode":"family","kind":"Service","name":<card name>}`. A family
restriction limits Card-attributed emission and verification to roots of that
family, and makes the account that family's unique scheduled runtime account
in the tenant; a second live account for the same family conflicts. Setting a
family restriction, at creation or later, additionally requires
tenant-administrative wildcard permission; a role name never proves it.
Restrictions grant no operation authority and do not filter unrelated
registry or table operations.

| Operation | Surface | What it does |
|---|---|---|
| Create account | `POST /v1/principals`, `wyrd principal credential create-principal` | Atomically creates the account, its roles, optional restriction, and first credential, returned once. |
| Replace restriction | `PUT /v1/principals/{principal_id}/workload-restriction`, `wyrd principal credential set-restriction` | Service only; idempotent for an unchanged value; atomically releases and acquires the family association. Id, roles, and credentials are unchanged. |
| Rotate credentials | `POST`/`GET`/`DELETE /v1/principals/{principal_id}/credentials[/{credential_id}]` | Issue, verify, deploy, then revoke the old key; overlap is allowed and releases never require rotation. |
| Register a release | `wyrd declare service.yaml` | Writes Card versions; no principal and no secret. |

#### Configured Service context

A WyrdState application loads a Service-rooted bundle. Workloads and account
family restrictions accept Service only; Agent Cards remain independently
registered components and verification subjects. There is no workload_kind.
The Service account authorizes a family; the bundle supplies an exact release.

Before its first Run/client/writer/telemetry use, state.configure accepts optional
service_name, service_version and deployment_environment. Omitted name/version
come from the loaded Service; explicit values must match it. Environment uses
explicit nonempty input, then nonempty WYRD_DEPLOYMENT_ENVIRONMENT, then null;
explicit empty input is invalid. First successful configure or implicit use
freezes the context, and every later configure conflicts, even after shutdown.
Loading/configuring remains offline and states do not mutate shared clients or
global telemetry resources. Startup injects the same context on all three
telemetry signals and native observations, including records outside a Run.

Root Runs select the Service subject; component Runs select the exact component
and binding occurrence without changing Service/version/environment. The server
resolves each validated subject UID to canonical kind/name/version and the root
UID to Service name/version. Gateway calls, direct/queued verification and
trusted delivery retain that accepted context independently of worker, Verifier
or model identity. Raw OTel labels are retained; owned promoted columns match
the registered Service. Environment is descriptive context, not permission.

Direct and queued verification capture one context
{service_name, service_version, deployment_environment} at acceptance. Queued
work stores it with the accepted execution; both paths write it unchanged to
the result and inputs rows. Nothing else reads it.

#### Telemetry setup ownership

Telemetry supports two setup paths:

- **Wyrd-managed telemetry.** Configure the loaded state once, then call
  `state.start_telemetry()` (or its language-native equivalent). Wyrd supplies
  `service.name`, `service.version` and optional `deployment.environment.name`
  from the frozen state context to its managed OpenTelemetry setup, and exports
  with the state's validated workload context and existing authentication.
- **Application-managed telemetry.** The application owns its OpenTelemetry
  providers, resources and lifecycle. It supplies `service.name`,
  `service.version` and optional `deployment.environment.name` on its resources,
  and sends spans, logs and metrics through standard OTLP exporters to Wyrd's
  supported endpoints with supported authentication. This path does not require
  WyrdState, `state.start_telemetry()`, or an exporter accepting a state. Wyrd
  does not copy private state configuration into an application-owned provider.

Both paths use the same ingest validation and table projection. An application
may supply the existing workload/subject/Run context where attribution is
wanted; the server validates UIDs, permissions and graph membership. Resource
names and versions never establish Card ownership, and owned promoted identity
columns remain canonical for validated UIDs. When an accepted request supplies
an environment, retain that value as before; without that context, promote the
resource environment with the existing typed stable-key-first fallback, including
on owned telemetry. Preserve raw resource attributes and leave absent environment
null. Environment remains descriptive and grants no permission.

Automatic startup must not silently reuse a provider bound to another state or
replace an application-owned provider. Refuse incompatible startup with existing
`WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS`; its remediation is to keep the existing
application-managed setup and use the second path. Wyrd never replaces or flushes
that provider. Native state/Run contexts may coexist without multiple ambient
defaults. No new exporter input, public state client accessor, exporter-specific
identity setting or provider pipeline is required.

#### Stock provider SDK context

Python state.http_auth and TypeScript state.httpAuth expose the existing HTTPX/
HTTPX2 and fetch authentication adapters bound to a configured WyrdState. These
are distinct from the existing plain WyrdClient adapters; no new public state
client accessor is needed. Provider clients are constructed once and obtain the
current state/Run snapshot before asynchronous credential refresh. Configured
Service/environment stay fixed; subject/Run come from the active Run, defaulting
to the Service subject outside one. Service/subject/Run context conflicts fail locally
before sending; explicit W3C context retains existing precedence.

The adapters replace placeholder vendor credentials with one current Wyrd bearer
and send x-wyrd-workload-uid, optional x-wyrd-deployment-environment,
wyrd-card-uid, optional wyrd-run-id and trace context. They never mutate a shared
provider client's defaults. Raw token/API-key credential-slot mode with explicit
headers remains supported. Both modes reach the same gateway admission:
validate UID membership before dispatch, derive Service/subject descriptors,
and freeze original context through attempts, streaming and outbox delivery.
Do not trust readable-name headers or cached previous-call context. Wyrd
credentials/context stay at the gateway; existing provider/W3C rules remain.
Provider Batch HTTP operations preserve request origin without new per-item
asynchronous attribution or deduplication machinery.

The normative examples and header contract are in
changes/active/service-identity-and-telemetry/supplements/auth-system.md,
Stock provider SDK context and approved examples. Physical defaults are unchanged.

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
no client-supplied tenant header is accepted. The exchange takes an optional
`workload_uid` naming the exact Service root the account runs as; it
selects context only and never changes the account, permissions, or tenant.
The JWT carries the account as the top-level `principal` claim
(`{id, kind, tenant_id}`, no Card), the `permissions` resolved from its current
grants at issuance as its only operation authority, an optional `workload_uid`
root, and a required `card_access` limit: `any_registered` for a rootless
User, administrator, or unrestricted Service; `root_required` with the family
snapshot for a rootless family-restricted Service, which permits generic
authorized work but no Card-attributed work; or `scoped` with the exact Card
UID graph of the selected root (depth 16, at most 32 Cards). An unusable root
is refused, never ignored. For delegated tokens (token exchange) an RFC 8693
`act` chain names the actors, the outermost being the current one, each with
its own `workload_uid`. A delegated token's audience is `wyrd` or `bifrost`; a `bifrost`
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
`OTEL_EXPORTER_OTLP_HEADERS=x-wyrd-api-key=<key>`. This stock-exporter path
requires no Wyrd SDK. Optional SDK exporter helpers configure the stock OTLP
exporters with the shared client's current access token on each export; they
reuse that client's credential resolution and refresh rather than introducing
another authentication implementation. Applications may attach those exporters
to their own OpenTelemetry providers. Framework instrumentation remains owned
by the application and framework; Wyrd never discovers or patches frameworks.
A state's `start_telemetry` installs one managed global provider; that state's
`flush` and `shutdown` force-flush it before draining the writer, still drain
when it fails, and report the failure as `WYRD_SDK_504_TELEMETRY_FLUSH_TIMEOUT`
or `WYRD_SDK_503_TELEMETRY_FLUSH_FAILED`. An application-owned provider is
never flushed by a state.

Env vars in deployed services:

| Env var | Required? | Source | Used for |
|---|---|---|---|
| `WYRD_API_KEY` | REQUIRED | Deploy environment's secret store (a credential of the stable Service account) | Exchanged at `POST /auth/token` for a short-lived JWT and re-exchanged by the SDK when that token expires or is refused; no refresh token is issued. The JWT names the account, never a Card. |
| `WYRD_WORKLOAD_UID` | OPTIONAL | Deployment config or the loaded WyrdState bundle | Exact Service root the client runs as, sent as the exchange's `workload_uid` and the `x-wyrd-workload-uid` request header. Selects context only. |
| `WYRD_DEPLOYMENT_ENVIRONMENT` | OPTIONAL | Deployment configuration | Default configured workload environment; explicit configure wins, absence remains null. Never authority. |
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
| Web app | First-party browser session, not a separate OAuth client | Rust-owned OIDC login or API-key recovery under `/auth/browser` | Opaque HttpOnly session cookie; only its digest and bound authority persist in Postgres; fixed 12-hour absolute lifetime |
| CLI and SDKs | `wyrd-cli`, public | Device authorization (RFC 8628) | `credentials.toml`, owned by `wyrd-client`; refresh token rotates on every use |

Rust owns browser login transactions, callbacks, session creation, validation,
expiry and revocation. Node renders the UI and forwards its selected tenant
cookie to Rust; it does not redeem an OAuth code, retain a refresh token/API key,
or encrypt a credential cookie. No UI-specific client secret, encryption key,
environment variable, secret provisioning step, session service or sticky
routing is required. Replicas resolve sessions through the existing Postgres.
The shared Rust client uses `oauth2` for CLI/SDK flows, so Rust, Python and
TypeScript share one implementation. The four OAuth form
endpoints (`POST /auth/token`, `/auth/platform/token`,
`/auth/device_authorization`, and `/auth/revoke`) take form bodies, refuse
with RFC 6749 §5.2 JSON, and are the one exception to `WyrdError`
problem+json; their successes keep their own standard shapes (RFC 6749 §5.1
tokens, the RFC 8628 §3.2 device response, an empty RFC 7009 revocation
`200`). First-party browser-session operations are not OAuth token endpoints:
`POST /auth/browser/login` begins a bound tenant provider login;
`POST /auth/browser/api-key` verifies a recovery key and establishes a session;
`GET /auth/browser/session` returns only live browser-safe session metadata,
optionally filtered by tenant route key; and `POST /auth/browser/logout`
revokes the selected session. They use typed Rust contracts and Wyrd errors.
The shared `GET /auth/callback` completes a bound provider login; browser
completion creates the session and redirects to the fixed tenant page, while
device and provider-test completions retain their own behavior. There is no
separate Node `/login/callback` OAuth-code exchange or UI refresh-token flow.
Deployed Services and Agents keep their own API keys or workload assertions;
human SSO never replaces or disables them.

Browser session cookies contain only random identifiers. Records store their
digests, tenant/principal, absolute expiry/revocation and either the exact
human-connection revision or recovery-credential reference, not recoverable
credentials. Recovery logout revokes only its browser session. Rust checks
current authority on browser requests; existing tenant access-token snapshot
semantics remain unchanged. Login transactions expire in five minutes and
sessions in twelve hours, with no sliding renewal. Protected tenant `/v1`
HTTP APIs accept exactly one selected browser cookie through the existing
principal/permission/audit boundary; ambiguous multiple session cookies are
refused. Only metadata enumeration accepts multiple tenant sessions. Other
protocol surfaces do not inherit cookie auth.
An explicit access-token header always takes the bearer path and never falls
back to a cookie after refusal. Cookie-authenticated mutations require exact
trusted Origin checks; provider callback correlation remains transaction-bound.
Production cookies are host-only, HttpOnly, Secure and SameSite=Lax, with the
existing loopback-development exception. No API key or OAuth token reaches
browser cookies, page data, URLs or JavaScript. Store failure is unavailable,
not anonymous success. Prior credential-bearing UI cookies require re-login;
no plaintext-cookie compatibility path is retained.

#### Cross-service delegation

The API key is exchanged at `/auth/token`, or once at
`/auth/browser/api-key` for a revocable first-party browser recovery session.
Other API requests use their scoped access token or explicit browser-session
boundary, with one exception: public gateway ingress (the OpenAI-compatible,
Anthropic Messages,
and Gemini `GenerateContent` inference routes) accepts a `wyrd_sk_` key in its
existing credential headers for stock clients that hold only a static key. The
server exchanges it through the same API-key grant, reuses the minted token per
key hash for at most 60 seconds, and authorizes the call as that token's
principal. `/mcp`, `/v1` management, and Bifrost do not gain it. The JWT
— not the API key — is what travels on cross-service calls in the dedicated
`X-Wyrd-Access-Token: Bearer <jwt>` header. The JWT must be a **delegated**
Wyrd token: its top-level `principal` identifies the subject being acted
for, and its `act` chain identifies the actor(s) acting for it, the
outermost being the calling Service. Its `permissions` are the
exact intersection of the actor's and the subject's; `act` is attribution only.
An exchange whose complete intersection cannot be represented by the existing
token scopes is refused before signing, without silent attenuation or new
authority. Mixed broad and explicit input grants alone are not a refusal:
the complete result must be unrepresentable. The application's own `Authorization` header belongs to
the application and is never read or written by the SDK or by Wyrd.
`Wyrd-Caller-Identity` is rejected legacy — do not reintroduce.

A public gateway ingress request may carry a per-request fallback in the
optional `wyrd-gateway-fallback` header: unpadded base64url over the JCS UTF-8
serialization of a `GatewayFallbackOverride`, at most 8 KiB encoded and 4 KiB
decoded. The ingress authenticates the caller before parsing the header,
refuses a repeated, malformed, oversized, empty, duplicated, or self-listing
value with `WYRD_GATEWAY_400_INVALID_REQUEST` naming `fallback` before any
dispatch, and consumes the header without forwarding it to a provider. Without
the header, tenant fallback policy applies unchanged.

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
request for audit and replay. It identifies one logical request; W3C trace
context (below) orders behavior causally across requests and services.
The two are complementary and neither replaces the other: audit and replay
join on the request id, and the trace timeline follows `traceparent`.

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

#### Trace context — W3C `traceparent` and `tracestate`

Every Wyrd SDK sends the caller's active OpenTelemetry span as W3C
`traceparent`/`tracestate` on each HTTP and gRPC request, and its
`start_telemetry` installs the W3C trace-context propagator, so standard
instrumentation carries the same trace to the next service. A queued Bifrost
write records a `bifrost.enqueue` PRODUCER span in the caller's own tracer;
the server's `bifrost.gate.write` CONSUMER span links each batch's producers.

The server continues a valid inbound trace as the parent of its request span
(HTTP, gRPC, MCP, and the OpenAI-compatible gateway) and ignores a malformed
one. Verification, its agent checks, and their gateway calls stay in that
trace; deferred Verifier runs link to the request that enqueued them rather
than becoming its children.

Server spans export twice from one tracer provider:

- The operator's configured trace backend (`WYRD_OTLP_ENDPOINT`) receives
  the server spans its own sampler samples: 1% of traces by default, or the
  validated `WYRD_OTLP_SAMPLE_RATIO` in `[0, 1]`, where `0` exports none and
  `1` exports all. Without an endpoint nothing is exported to an operator.
  No client-supplied field, including the inbound sampled flag, trace id,
  and `tracestate`, changes what the operator receives. The operator's span
  verbosity is `WYRD_TRACE_FILTER` (default `info`), separate from the log
  filter `WYRD_LOG`/`RUST_LOG`; neither filter nor the ratio can remove a
  tenant copy, because catalog spans and events at `info` or above always
  record. Logs are written whatever the sampling decision, so an error in
  an unsampled request is still logged inside its request span.
- A server span whose parent or one of whose links is a sampled client span
  also writes, with its catalog descendants, to the requesting tenant's
  `bifrost.traces.spans` on the process Scribe outbox, with the same trace,
  span, and parent ids. A continued request's spans therefore form one tree
  with the client's, and a queued write's `bifrost.gate.write` CONSUMER span,
  which reaches the server with its producers as links and no `traceparent`,
  reads back as a trace linked to them. A queued Verifier run stores its
  enqueuing span and that span's sampled flag with the execution, so every
  attempt, including a reclaimed one, starts a trace linked to it that is
  attributed to the enqueuing principal with SYSTEM as actor; a sampled
  caller's attempt, its catalog descendants, and its gateway calls' captured
  attempts read back from the enqueuing trace's timeline. The tenant copy carries only the
  behavior-span catalog — the request span, `verification.*`, the Agent
  loop's `invoke_agent`, `chat`, and `execute_tool`, `dispatch`,
  `agent.gateway.call`, `gateway.*` except `gateway.attempt`,
  `bifrost.gate.*`, `bifrost.oracle.query`, and OTLP ingest — and only spans
  whose ancestors up to the span that opened the copy are all catalog
  spans. It keeps only safe
  attributes (`wyrd.*`, `gen_ai.*`, and HTTP, RPC, and error semantic
  conventions); the tenant id is never a span attribute. The copy is
  non-blocking and never fails the request, and it works with or without an
  operator endpoint and at operator ratio `0`.

Operator export uses the stock OpenTelemetry batch processor and its bounded,
non-blocking queue; a full queue drops spans rather than delaying requests.
The SDK logs one `BatchSpanProcessor.SpanDroppingStarted` warning at the
first drop and the exact lifetime `dropped_span_count` at orderly shutdown.
There is no live dropped-span count; filtering `opentelemetry_sdk` warnings
or abrupt termination removes these signals. Each export call that fails
increments `wyrd_trace_export_failed_batches_total`; a failed batch is not a
count of lost spans. Export failure, stall, or saturation never changes a
request's result or its tenant copy, whose loss is counted separately by the
Scribe outbox. Head sampling cannot promise a trace for every failing
request.

`Bifrost::trace_timeline` (and its Python and TypeScript projections) reads
one trace's spans from `bifrost.traces.spans`, plus traces linking directly to
it, in start-time order, scoped to the caller's tenant.

#### Observation identity — `Card → Run → Observation`

How Card-correlated telemetry ties to a Run and a Card, and how the server
resolves it. A Run is one client application invocation with one `run_id`;
each correlated row names its own exact subject Card. A telemetry row may omit
Card correlation; when supplied, the `(card_uid, run_id)` pair anchors it to
that subject and invocation. The server owns resolution of that identity.

**A principal is not a card.** A Service account runs as one selected root
Card (its `workload_uid`) per request context, but a Service card *nests components* — each a card in its own
right (e.g. Model A, Model B, a Prompt; `Service.components`). One
`state.run()` opens one invocation and one `run_id`, initially scoped to the
root Service Card. The first-class SDKs may select an initial hydrated Card
alias when opening the run, and `for_card(alias)` returns an immutable
Card-scoped view of that same invocation for multi-component work. Views for
different components share the `run_id` and the JWT, but each row carries its
view's exact subject `card_uid`. Switching Cards never requires a distinct Run
ID, and the request's root Card cannot say which card a correlated record
belongs to — the subject Card must be carried on that row. Generic telemetry
may omit a subject Card.

Python `Run` values are synchronous context managers for optional ambient span
correlation. Entering a scope best-effort attaches the selected Card UID and
run ID to Python OpenTelemetry context, annotates an active recording span,
and lets an idempotently installed span processor copy `wyrd.card_uid` and
`wyrd.run_id` onto spans created inside the scope. Exiting restores the prior
context. Missing or incompatible Python OpenTelemetry support and enrichment
failures are no-ops; they never fail application execution or explicit Wyrd
observation emission. Card lookup and authorization remain strict. The scope
does not create, end, flush, or persist a Run or span. Signal-specific log and
metric enrichment is not implied.

A view identifies itself by the alias it was opened with (`alias`; the root
Service view is `root`); the state returns the exact typed `CardRef` for an
alias. `run.observe.verify(verifier, inputs)` and
`run.observe.submit(verifier, inputs)` judge the view's subject: the client
resolves the named Verifier among those bound in `verified_by` to that subject
in the hydrated graph, validates named inputs against its slots, hashes
files, obtains plans for missing ones from `POST /v1/verification/evidence`
sends each missing body to its plan's target and completes it through
`POST /v1/cards/upload/{upload_id}/complete`, and calls
`POST /v1/verification/execute` (once, without replay, returning the typed
`VerificationResult`) or `POST /v1/verification/enqueue` (returning a
`VerificationExecution` handle). `submit` on a Window Verifier records its
samples through the drift observation path and returns `Recorded`. An unbound
name (`WYRD_SDK_404_UNKNOWN_VERIFIER`) or an input its slots refuse
(`WYRD_VERIFICATION_400_INPUT_INVALID`) fails locally before any network IO,
and a `failed` verdict is an ordinary return. `VerificationExecution.connect`
rebuilds a handle from an execution ID in any process. Binding inspection
remains a server HTTP and MCP surface, and verification history is
read with parameterized SQL through the Bifrost client.

Every accepted row carries authenticated publisher and request identity; Card
and Run correlation are optional per-row values:

| Value | Source | Grain | Means |
|---|---|---|---|
| `card_uid` | optional client assertion of the row's exact subject Card UID (the run view's selected Card); server authorizes it when present | per row | the optional Card-version anchor — *which* Card |
| `run_id` | optional client-generated value per `.run()`, shared by every Card-scoped view of that invocation; passed through opaquely | per row | the optional Run anchor — *which* execution |
| `sub_principal_id` | server-stamped from the verified JWT | per request | the subject — *for whom* the work was done; the party a delegated token acts for |
| `act_principal_id` | server-stamped from the verified JWT | per request | the actor — *who* emitted it; equals the subject for a direct publisher |
| `tenant_id` | server-stamped from the verified JWT | per request | the tenancy boundary |
| `wyrd_request_id` | the propagated `Wyrd-Request-Id` (minted at first sighting) | per request | the request spine — one request spans **many** runs and hops |

Resolution rule: **tenant, `sub_principal_id`, and `act_principal_id` come
from the token; a present
`card_uid` is client-asserted and server-authorized against the trusted signed
scope, or, for a principal bound to no Card, against the tenant registry;
absent Card correlation produces null `card_uid`;
`run_id` and `wyrd_request_id` pass through untouched.**

On OTLP input, canonical table projection reads these optional values from the
record-level attributes named exactly `wyrd.card_uid` and `wyrd.run_id`. The
final duplicate key wins, matching Wyrd's existing OTLP attribute lookup rule,
while every original attribute entry remains in the lossless payload.
`wyrd.card_uid` uses the `CardUid` text grammar and `wyrd.run_id` the existing
`RunId` text grammar.
Consequences, stated so they stop drifting:

- **`card_uid` and `run_id` are optional per-row columns on the observation payload, not
  request metadata.** A client-side queue batches records from different runs —
  and different cards — before it flushes, so one sealed batch (one
  `wyrd_batch_id`) freely mixes them. The producer is keyed by **table only**; it
  never splits a batch by card or run. The server therefore authorizes every
  present `card_uid` **per row** (every distinct asserted Card in the batch must
  be in the principal's scope),
  validates the client-generated UUIDv7 `wyrd_batch_id` request field (the
  idempotency key; it is not a row column), stamps request-scoped
  `wyrd_request_id` and `wyrd_ingested_at` — the PostgreSQL admission instant,
  read once per batch and never caller-supplied. Each table partitions on its
  one declared time column, `wyrd_ingested_at` unless the registration names
  another non-null timestamp column; any time value is accepted. Row
  identity is batch-level; no per-row position is stamped.

- **`card_uid` is optional and authorized, not trusted.** Its absence is valid
  generic telemetry and produces null `card_uid`; the authenticated publisher
  remains available through non-null `sub_principal_id` and `act_principal_id`.
  When present, the server checks the asserted
  `card_uid` against the token's **card scope**. A rooted token's scope is its
  `workload_uid` root plus the
  **observation-target** cards reachable through the transitive card-ref graph
  declared in that root's spec. A card is in scope only if its kind is an
  observation target (`Data`, `Model`, `Experiment`, `Prompt`, `Agent`,
  `Service`, `Mcp`, `Artifact`, `Source`); `Verifier` and
  control-plane kinds (`Policy`, `Audit`, `Operator`, `Trigger`) never enter the
  emit scope. Service cards contribute `Service.components`; other reachable
  specs contribute their declared card refs according to the shared card-ref
  extraction rules. A `card_uid` outside that set is rejected: a principal may
  not attribute records to a card outside its root's declared graph. The scope
  is the signed `card_access` claim minted into the JWT at `/auth/token`. Every
  token mint — a first exchange, a machine's re-exchange, or a human refresh
  rotation — resolves each bounded member against the tenant Card registry and
  signs its authoritative UID. Ingest uses that verified
  in-memory set and performs no Card-registry Postgres or cache lookup.
  A rootless `any_registered` token (User, tenant administrator, unrestricted
  Service) carries no bounded scope; its scope is every declared
  observation-target Card in its tenant. Gate looks the distinct `card_uid`
  values of each frame (at most 32) up in the tenant registry before dispatch
  and hands Scribe the declared observation-target UIDs as that frame's scope,
  so an undeclared or non-observation Card is refused exactly like a UID
  outside a signed scope. A rootless `root_required` token may attribute to no
  Card; native Bifrost accepts only the signed root and refuses a
  `x-wyrd-workload-uid` selector it cannot resolve, so such a client exchanges
  its credential with `workload_uid` first.
- **This is not the governance token.** `card_uid` is one field in the
  observation envelope, authorized by the existing JWT plus the principal's
  declared card-ref graph — not a separate per-card credential (doctrine #18).
  The token still proves the principal; it bounds a *set* of emittable cards,
  and the envelope selects one within it.
- **There is no run registry.** Runs are client-side execution records; the
  server never persists a run table and never resolves `run_id` back to a card
  — the card is the authorized `card_uid` on the row. `run_id` is an **opaque**
  correlation id, never a composite that encodes the card.
- **When present, `Card → Run → Observation` is the `(card_uid, run_id)` pair on
  the row;** generic telemetry remains attributable to its subject and actor, and the
  request spine is the `wyrd_request_id` label that joins many runs across hops.
- **The observation owns subject identity.** Under pub/sub (Doctrine #3, #21),
  the observation's `card_uid` IS its subject — no separate `subject_ref` on
  the envelope and no monitor-emits-about-a-different-card case. For a Service
  principal, the server validates the subject against the UIDs its signed scope
  maps from the locked Service version and its component Cards. Verification
  routing is resolved later from that subject's `verified_by` bindings;
  authorization still reduces to the one subject `card_uid` on the row.

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

The query carries no `kinds` filter, by design:

- **No `kinds`.** Filtering would silently narrow the pinned subgraph,
  giving later readers a forensic false signal that the investigator
  considered only those kinds. Display filtering is a UI concern; the
  audit pins the whole neighborhood.

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
| `Component`       | `Service.components[].ref`                                                  |
| `Artifact`        | `Data.card_refs[]`, `Model.card_refs[]`                                     |
| `Prompt`          | `Agent.prompt`                                                              |
| `Baseline`        | `Verifier.requirements[].check.signal.baseline_ref` and step requirements (drift) |
| `Trigger`         | `*.verified_by[].runs_on`                                                   |
| `Operator`        | `*.verified_by[].on_failure[]`                                              |
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
| `signals`     | Lets the audit pin to e.g. `Drift` only, or `Verification` only.   |
| `actors`      | Lets the audit pin to one service identity's emissions.            |
| `request_ids` | Lets the audit pin to specific runtime hops (doctrine #15) when the investigator already knows them. |
| `labels`      | Open-shape narrowing the registry doesn't model — tenant, region, deployment slice. |

Excluded fields, with reasoning matching `ProvenanceQuery`:

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

A subject-less, declarative verification: named evidence `inputs`, optional
preparation `steps`, `requirements` that each produce one verdict, and a
`decide` rule that combines them. `Verifier` is the only verification kind and
has no authored implementation discriminator; drift and agent judging are
check kinds, not Verifier kinds. Scheduling and failure reaction remain on the
binding's Trigger and Operators.
```yaml
spec:
  description?: string
  inputs: { <slot>: InputSlot }              # json | media | trace | samples
  steps?: [Step]                             # id, action, when?, depends_on?, retries?, timeout_seconds?, requirements?
  requirements?: [Requirement]               # id, statement, check — whole-Verifier
  decide?: all | { pass_rate: { threshold } } # default all
```

| Slot | Evidence |
|---|---|
| `json` | JSON, optionally constrained by `schema` |
| `media` | One file pinned by SHA-256, with accepted `formats` and optional lower `max_bytes` |
| `trace` | A trace ID resolved through tenant-scoped trace reads |
| `samples` | Named `features` (or metric names) recorded for a scheduled window |

Slots are `required` by default. A missing required slot is missing evidence,
never a refusal: checks that need it are inconclusive.

| Action | Does |
|---|---|
| `extract` | Reads one `media` slot into the format-discriminated extraction JSON (PDF, PPTX, DOCX, XLSX, CSV, JSON, text/Markdown/HTML). Text and structure only; no OCR or rendering |
| `agent` | Calls an inline, file-authored, or declared Agent with `inputs` matching its Prompt placeholders, optionally over a bounded `workspace` with built-in tools |
| `code` | Runs sandboxed Python (`runtime: python`) from inline source or an exact Artifact, with a named `entrypoint` and bound `inputs`; returns JSON |

| Check | Judges |
|---|---|
| `assert` | A typed comparison operator over `path`, or every element of an `each` selector |
| `trace` | Span existence, order, and attributes over a `trace` slot |
| `agent` | An Agent, with its declared tools and optional `workspace` as in a step, returning the closed Verdict schema `{outcome, reason, citations}` its Prompt declares, plus `data` when that format declares it |
| `code` | Sandboxed Python returning the same Verdict schema; an optional `response_format` on the check declares its `data` |
| `drift` | The existing PSI, SPC, or Custom decision over one `samples` slot, with full statistical detail |

Actions read and compute; they never mutate external systems, declare Cards,
or dispatch Operators. References use `inputs.<slot>`, `steps.<id>.output`,
`steps.<id>.status`, `steps.<id>.requirements.<rid>.outcome|reason`, and
`requirement.id|statement`, with dotted fields and zero-based indexes; `[*]`
appears only in `each`. Step and requirement IDs are unique across the whole
Verifier and key results, route overrides, and fitted drift state. References
in actions and `when` create dependencies; declaration rejects cycles,
unknown references, checks that read another requirement's verdict, and
fields absent from a known output schema. Independent steps run concurrently;
a step's requirements run before its dependents start.

Requirement outcomes are `passed`, `failed`, `inconclusive`, or
`not_applicable`. An intentional skip (false `when`) makes dependent
requirements `not_applicable`; a failed or timed-out step makes them
`inconclusive`, and failure wins when both apply. Provider, malformed-output,
infrastructure, and missing-evidence failures inside one check are
`inconclusive`, never `failed`. `decide` excludes only `not_applicable`;
with none applicable the verdict is `inconclusive`. `all` returns failed on
any failure, else inconclusive on any inconclusive, else passed. `pass_rate`
divides passes by all applicable requirements, inconclusive included.

A `trace` slot is judged over the whole trace: every retained span of that
exact trace ID the accepted caller's ordinary `bifrost.traces.spans` query
may read, whatever workload, Card, or time it carries, ordered by start time
then span ID with each span's own identity. There is no acceptance-relative
time window; the result stays owned by the execution's exact binding and
subject. Each attempt reads one trace once and every check naming it shares
that read. A trace over 10,000 spans, a refused query, or a timed-out read is
inconclusive, never truncated. A trace with no visible span has not landed:
`verify` judges it inconclusive at once, while a queued attempt before its
last ends `retrying` with `evidence_unavailable` under the queue's ordinary
three attempts and 30- then 60-second backoff, publishing nothing. The last
attempt judges it inconclusive and completes under the Verifier's own
`decide`. A completed result is final: spans landing later are judged only
by a new execution.

A check whose format declares `data` must return a matching non-null
object; without a declaration it must not return `data`. The server validates
the whole response against the exact declared schema and bounds `data` to
65,536 bytes of compact JSON and 16 nested containers. Invalid or oversized
output is `inconclusive` with no data, and the reason names the location or
limit, never the content. Accepted data is `RequirementResult.data` on whole
Verifier and step requirements, retained in the sensitive outcome. SDK
typed siblings (`verify_typed`, `wait_typed`, `status_typed`, and Rust
`wait_until_typed`) decode every present `data` as the caller's type after the
ordinary call; a mismatch is `WYRD_SDK_422_VERIFICATION_DATA_INVALID` naming the
requirement with the data root's empty `path` (a deeper path could hold a
returned dictionary key), and never reruns or changes the result.

A citation names an original input slot, or a fixed workspace file's
`attachment_<n>` slot as the `fs.*` tools report it in `sources`, and a typed
locator (`page`, `slide`, `lines`, `json_pointer`, or page `region`);
workspace paths map back to their slots, and `/work` scratch is never a
source. Before an Agent check runs, a `verdict`-format Prompt's citation
slots are narrowed on that invocation's copy to the bare slots its bound
inputs and workspace reach, so the provider request and the Agent's
validation admit the same slots and a wrong or `inputs.`-prefixed slot is
repaired; with no reachable source the slots are left open. The Card and
concurrent invocations are unchanged. The server validates every citation
against the pinned evidence, and a passed Agent check without a valid
citation becomes inconclusive.

Verifiers judge one of two evidence modes, fixed at declaration; mixing them
is refused.

| Mode | Evidence | `verify` | `submit` |
|---|---|---|---|
| Run | `json`, `media`, `trace` | Executes now; returns `VerificationResult` within 60 s | Stages inputs and queues one execution; returns `VerificationExecution` |
| Window | `samples` with `drift` checks | Refused (`WYRD_VERIFICATION_422_INPUT_UNSUPPORTED`) | Records samples through the drift observation path; returns `Recorded` |

A Window Verifier requires a `schedule` binding. Declaration and binding do
not grant invocation authority.

A Verifier runs through a `verified_by` binding on a Service component,
Service, or standalone Agent, or through a caller's `verify`/`submit` on a
bound subject:
```yaml
verified_by:
  - verifier: Ref                            # → Verifier (exact version)
    runs_on?: InlineableRef<TriggerSpec>     # schedule or group_end; absent means caller-driven only
    on_failure: [InlineableRef<OperatorSpec>] # zero or more reactions; not on group_end
    on_verdict_change: [InlineableRef<OperatorSpec>] # group_end only
```

One binding is one subscription. Its subject is the containing Service,
standalone Agent, or Service component occurrence; the same Verifier, Trigger,
and Operator Cards may be reused by any number of bindings. Declaration
UID-pins every referenced Verifier, Trigger, and Operator, and rejects
unresolved, wrong-kind, unauthorized, and cross-tenant references, duplicate
bindings to the same Verifier version for one subject, `workflow` in
`on_failure`, and a schedule binding to a Verifier without `samples` slots.
A binding has no `inputs`: a scheduled execution judges every drift
requirement of the pinned Verifier from the samples recorded under each
`samples` slot's own `features`, and every `samples` slot is required. The runtime direction is fixed:

```text
verify ───────────────► execute inline ─► VerificationResult (no dispatch)
submit / schedule ─► queued execution ─► VerificationResult
  -> zero or more on_failure dispatches (failed verdict only)
```

Trigger decides when; the Verifier decides the verdict; the generic runner
creates dispatches; each Operator owns its own delivery. Only a `failed`
verdict of a queued execution dispatches; `inconclusive`, execution errors,
and `verify` never do, and a replayed submission creates no duplicate
dispatch.

**Grouped verification.** `verify` and `submit` take an optional caller
`group_id`; the server records it with the authenticated owner Service on input
and result rows without producer PostgreSQL IO. A group Verifier declares whole
source slots or requirement outcomes of other Verifiers bound on the same owner
occurrence (`verifiers.<name>.inputs.<slot>` or
`verifiers.<name>.requirements.<id>.outcome`). Declaration pins exact source
identities; tenant, owner, subject and Verifier identity scope every read.

A `group_end` Trigger declares one positive duration, `after`, at most 365 fixed
days. Standalone and inline Trigger writes reject larger durations before
declaration persists them; accepted durations are used exactly, with no silent
SQL shortening. After Scribe
acknowledges a matching source input or result, the existing background writer
upserts one PostgreSQL pending scheduling row per tenant, binding and group ID,
with due time computed as PostgreSQL statement time plus `after`. New activity
updates that due time; evidence stays in Bifrost. Request handlers add no SQL,
source lookup or explicit start/close operation. Continuous activity can postpone
execution indefinitely. No max-age override or permanent finality exists.

The scheduler polls due scheduling rows, not historical Bifrost groups. Ordinary
cron scheduling and group work use separate bounded loops under the existing
scheduler owner, each sleeping the runtime poll interval after a pass (default
one second), visiting at most 64 overdue tenants and handling at most 32 due items
per tenant. Group items are processed sequentially per process. Before claiming,
group preparation acquires a permit from the same existing process execution
capacity used by the runner (default 64). Full capacity leaves pending rows in
PostgreSQL; no new concurrency option or distributed quota is introduced.

A group item is claimed with `FOR UPDATE SKIP LOCKED`. While holding that
scheduling-row lock, one bounded Bifrost query selects the group's settled
history: the declared source inputs and results with `wyrd_ingested_at`
before the claimed row's PostgreSQL `due_at`, its exclusive range end. The
range start is the earliest selected admission time, equal to the end when
nothing is selected. Each run selects cumulative history of its `group_id`
and exact source scope through its own cutoff, never a disjoint next
interval, so later-visible earlier evidence and later results join the next
run. A new selection is a prepared candidate: the preparation transaction
writes nothing and rolls back, releasing the row lock, and the selection
itself — range, per-slot authored references, ordered source execution IDs
and selected result IDs — is staged as the execution's `group` input record
in Bifrost on capacity reserved before the claim. Only after Scribe
acknowledges that record does the batched queue writer, in one tenant
transaction, insert the ordinary execution holding only the selection's
content digest and delete the pending row while it still holds the
generation the candidate was selected from. The digest covers binding,
group, membership, selected results and order, never the cutoff; a
selection whose digest the binding and group already hold is settled during
preparation and consumes the pending row in that transaction, and a
conflicting admission returns the existing execution. Retryable runtime,
capacity, database and query failures retain pending and are traced. Genuine auth
or owner-activity rejection, including an inactive/retired owner, records one
terminal errored group execution and consumes pending atomically. Recording
failure rolls back and preserves pending. This pre-admission error runs no
Verifier/provider, asserts no accepted owner authority or snapshot, has no verdict/
result pointer and dispatches no Operator; actual identity and SYSTEM reporting
actor remain attributable. Every runnable execution keeps its accepted-authority
and input invariants. Missing issuer/SYSTEM/attribution infrastructure is retryable
Wyrd failure, not proof of rejected owner authority. Existing latest binding
execution status then execution readback exposes the terminal error; it is not a
new group-history API. Every activity upsert draws a fresh generation, so
activity acknowledged after a candidate's selection keeps the pending row,
and a row recreated after consumption starts a new one; that activity joins a
later snapshot. While a candidate awaits acknowledgement, the process skips
its group so it is not prepared twice. Release the preparation permit before
Verifier/provider execution. With no new activity, there is no pending row and no further Bifrost
read for that group. Distinct later snapshots may overlap earlier executions.

Source lists share execution ordering, with null for a missing outcome or an
absent selected value. A source result admitted after the cutoff is absent
from that run even when its input was selected. A visible result whose
execution's input or output is not visible is publication-incomplete: the
selection records nothing and the pending row remains for retry. More than five minutes past the current due time, by
PostgreSQL's `statement_timestamp()`, the scheduler instead records one terminal
`WYRD_VERIFICATION_409_SNAPSHOT_INCOMPLETE` group execution through the refusal
path and consumes the pending row in the same transaction: no accepted
authority, inputs, snapshot, evaluation, verdict, result or Operator dispatch.
The stored error message names the missing execution IDs; no persisted details
object exists. New matching source activity schedules the group again normally.
Physical identity duplicates count once; distinct executions remain distinct.
When an execution has multiple visible results, select the latest by ingest time,
then result ID. Grouping does not consult PostgreSQL submission acceptance or
settlement to decide which Bifrost rows participate. Consequently a staged
input later refused by the queue can participate. Ordinary source authorization
and pre-staging pressure refusal still apply.

The pending table contains scheduling metadata only, never source evidence,
readiness inventories or revision state. The execution row holds the
selection digest; the Bifrost input record holds the selection, which every
attempt and read resolves to the same values: source declarations from
`execution_inputs`, requirement outcomes from the selected results'
`execution_outputs`, and a group source's own values by resolving its
selection in turn. Every group execution has its own input record, so another
group can consume it. Ordinary queue claims and retries remain; no additional
group crash-recovery protocol exists. Query
admission/output/deadline bounds apply; failed or incomplete queries defer
execution without truncated inputs or synthetic source-ceiling verdicts.

Scribe acknowledgement schedules work but is not a query-completeness checkpoint.
A selection uses what is visible when read; `wyrd_ingested_at` is an admission
time, not a completeness watermark, and a later visibility change alone does
not schedule another execution. A process death between ACK and scheduling commit
can lose the notification; there is no repair/history scan. Background retries
can postpone expiration. Retention and outbox loss limit available evidence.
Runtime permits bound active work, not durable PostgreSQL backlog. These limits
are explicit; no visibility barrier, activity ledger, quota service or new WAL
is added.

Group bindings use `on_verdict_change` only. The first settled verdict is a
baseline; each subsequent different settled verdict creates existing Operator
dispatches transactionally. Comparison follows settlement order, including when
an older snapshot finishes later; there is no newest-revision adoption or
outstanding-execution gate. A short lock on the existing binding row covers only
completion, previous-verdict comparison and dispatch creation, never evaluation.
Completion replay creates no new transition. Delivery remains at least once.

`run_id` is the client's application invocation; one Run can have many
executions. `ExecutionId` is the server-issued identity of one exact Verifier
execution; scheduled executions have no `run_id`.

| Route | Contract |
|---|---|
| `POST /v1/verification/evidence` | Declare files by digest and length; receive an `upload_id` and shared upload plan for each one with no completed File record, under the tenant's `evidence/sha256/<digest>` namespace |
| `POST /v1/verification/execute` | Exact Verifier and subject, optional Run and `group_id`, named input declarations, optional route overrides → `200 VerificationResult` |
| `POST /v1/verification/enqueue` | The same Run evidence with `Idempotency-Key` → `202 { execution_id }` |
| `GET /v1/verification/executions/{execution_id}` | Status, terminal error, result when visible, and a group execution's `group_id`, owner, and source selection once its input record is visible; a direct execution's completed status once its records are visible |
| `GET /v1/verification/bindings/{binding_id}` | Readiness, cadence, `last_execution_id`, and a `group_end` binding's frozen settings and source bindings |

File contents never pass through Postgres or a verification request body; an
uploaded file does not become a Card. A File upload is a Card-less record in
the shared storage upload lifecycle: it plans with the same planner policy as
a Card file (single PUT, multipart, Azure block blob, or the local upload
capability), transfers through the same client, and is ready only once
`POST /v1/cards/upload/{upload_id}/complete` verifies its stored digest and
length and records a `file_upload` fact in `bifrost.storage.objects`. Each
upload session owns its own object, `evidence/sha256/<digest>/<upload_id>`,
so concurrent uploads of one File never revoke one another, and an abandoned
File session expires through the same storage sweeper as a Card upload. That
completion returns only after the fact is acknowledged; it is the one
completion guarantee for Card files and Files alike. Planning reuses a File
only when a completed, undeleted same-tenant `file_upload` fact matches its
digest and length; object presence, a Card file, or another tenant's record
never stands in for one. Verification reads a File from its recorded storage
location and re-checks its length and digest. Per-file and aggregate limits
(`WYRD_VERIFICATION_FILE_MAX_MIB`, `WYRD_VERIFICATION_FILES_MAX_MIB`) are
enforced at planning. `ArtifactRef` selects a declared Artifact under its own
authorization.

Public invocation checks `verifier:run` against the selected Verifier's exact
Card UID. `PermissionScope::Verifier(CardUid)` names one UID; `All` covers
every Verifier in the tenant. Permission is checked before execution and the
decision is audited. A binding does not replace the invocation permission.
Planning and every follow-up operation on a File upload (part URL, local
transfer, completion, abort) stage bytes and select no Verifier, so they
require only operation-level `verifier:run` admission: any `verifier:run`
grant, exact or `All`, admits them, and the decision is audited the same way.
A follow-up operation derives its permission from the upload's persisted
owner, never the request: a Card upload requires `cards:write`, a File
upload `verifier:run`, and an upload of another tenant is not found.

Neither Run entry point writes Postgres on the request path. `verify` runs
inline and stages its pinned inputs and result on the process Scribe outbox.
`submit` stages its inputs; only after Scribe acknowledges them does the
batched request outbox insert the queued `wyrd.verifier_executions` row. A
server-owned `queue_execution` marker on the input record (false for
`verify`, true for `submit` and for scheduled and group executions) is the
only thing that activates queued work, so replaying input persistence never
re-executes `verify`. Scheduled and group executions follow the same
pipeline: the scheduler prepares the execution and its input without writing
the queue, stages the input, and on its acknowledgement the same batched
writer admits the execution, moving the expected schedule cursor or
consuming the selected pending group generation in that transaction. The batched writer
decides `Idempotency-Key` replay and conflict per tenant, subject, actor, and
key: a matching retry returns the same execution ID; a conflicting one settles
`errored` with `WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT`, never runs, and leaves
the original unchanged. `202` therefore confirms staging only. Staged work
can be lost on abrupt process death before acknowledgment and then never
becomes visible; observable losses are counted, and nothing is silently
resubmitted. `VerificationExecution.wait` polls with capped backoff, tolerates
not-found and delayed results within its deadline, and returns only a real
result, the terminal error, or `WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT`.

Execution status includes the existing Operator `dispatches` projection:
dispatch ID, exact Operator, delivery status, and optional error. Delivery
statuses are `pending`, `running`, `retrying`, `delivered`, or `failed`;
delivery completion is independent of execution completion and never changes
the verdict. Reuse the existing Operator queue/worker and tenant-authorized
status assembly rather than a separate dispatch resource. Binding inspection
uses `last_execution_id` for its latest admitted execution, not an application
Run or an unaccepted staged submission.

MCP projects the HTTP contracts as `verification.evidence`,
`verification.execute`, `verification.enqueue`, and
`verification.get_execution`, with retained `verification.get_binding`.
The old `verification.start_run` and `verification.get_run` have no aliases.
Read tools remain discoverable; write scopes supplement domain authorization.

Execution status is `pending`, `running`, `retrying`, `completed`,
`cancelled`, `timed_out`, or `errored`, separate from the verdict. The runner
drains `wyrd.verifier_executions` in claim rounds. Each round considers every
tenant with claimable work, oldest first, and claims at most one execution
per tenant. The shared resources that already admit work bound it: the
Postgres pool, Oracle admission and memory, the Bifrost memory budget, and
verification-runtime capacity. Work refused by one of them returns to the
queue without consuming an attempt. The claim transaction also returns the
Verifier Card's status, its spec on a cache miss, and any fitted drift
baselines. Parsed Verifiers are cached per process by tenant and Card UID in
a fixed 64 MiB least-recently-used cache that is never shared across tenants.
A deleted Verifier terminates the execution `errored`.

When execution completes, the runner builds the result once and stages it on
the process Scribe outbox, then settles at once: settlement never waits for
Scribe, so a completed execution can lack a retained result. Leases renew on
the PostgreSQL clock once a third of the lease has passed; an expired lease is
never revived, and a renewal that no longer finds the token cancels that work.

A `VerificationResult` carries `execution_id`, optional `run_id`, the exact
`verifier` and `subject`, and an `outcome` of `verdict`, `requirements`
(`id`, `step`, `statement`, `outcome`, `reason`, `actual`, `citations`,
`detail`, `check` provenance), `steps` (`id`, `status`, `output`, `attempts`,
`duration_ms`, `reason`, `provenance`, `tool_calls`), and pinned `evidence`.
Provenance records code hash and runtime, extraction version, Agent and Prompt
versions, effective routes, drift baselines and windows, and input digests.
`bifrost.verification.results` is the canonical payload-free result summary,
`bifrost.verification.execution_outputs` holds each result's `requirements`
and `steps`, and `bifrost.verification.execution_inputs` holds each
execution's one input record: declarations for direct and submitted work, a
selector for group and schedule work, and queued activation context. A
returned `VerificationResult` is assembled from the three by one reader:
verdict from the summary, breakdown from outputs by result ID, evidence from
inputs by execution ID (a group's resolved slot lists; an empty map for
scheduled Drift). PostgreSQL stores no input values; a queued execution's
selected `result_id` stays authoritative. A direct execution has no queue
row: its status is read from records whose input origin is `direct`, and is
`completed` once they are visible. Unknown IDs remain not found.

Every work record names who the work was for (`sub_principal_id`) and who
performed it (`act_principal_id`). A direct caller A records A/A and a
delegated caller B acting for A records A/B. A queued execution stores the
accepted authority and delegation, without credentials, through the
acknowledgment handoff and its row, and keeps it across claim, retry, and
restart; caller submission never switches to binding-owner or SYSTEM
authority. A scheduled execution runs with its binding owner's accepted
authority. Wyrd performs model calls, tool calls, input reads, and result
writes as the tenant SYSTEM executor, recording the subject with SYSTEM as
actor; SYSTEM is never a subject and authorizes nothing extra. The runtime's
own reads that prepare an execution never depend on owner credentials. These
are scheduled Drift observation reads and a group's selection of its recorded
verification inputs, outputs, and results. They run as the tenant's credentialless,
role-free internal read principal, which holds `bifrost_query:read` only on
the tables that read needs, with owner and source filters in the statement.
The owner's activity and authority are checked before such work starts. Gateway spend
is charged to the subject.

#### Agents, tools, and workspaces

Agent owns Prompt, tools, run configuration, and `execution_route`;
server-hosted calls require `wyrd_gateway`. Overrides are keyed by step or
requirement ID and replace that call's route whole. Step and check Agents
follow the same tool rules; span data a judge reads live stays outside the
pinned snapshot. An Agent may declare a `workspace` mapping read-only `/inputs`
paths to evidence: an execution-scope reference such as `inputs.policy`, or
`{artifact: {card, path?}}`, which selects an exact inventory file, a
directory prefix matched on `/` boundaries, or the whole collection of an
Artifact Card that registration pins to an exact version and UID. Fixed
selections are part of the Verifier version, so execution input cannot
replace them and a newer Artifact version never retargets them; overlapping
destinations are refused at declaration, and a selection matching no file is
refused before any model call. Only selected files are staged, unparsed;
each is a citable attachment slot counted with the other inputs. `/work` is
bounded scratch for that invocation only.

`AgentSpec.tool_names` selects server-owned built-ins: `cards.list`,
`cards.get`, `bifrost.list_tables`,
`bifrost.describe_table`, `bifrost.query`, `fs.list`, `fs.read`, `fs.search`,
`fs.extract`, and `code.run`. `fs.read` returns a UTF-8 file's actual lines
with `lines` locators and a resumable `next`, refusing directories and
non-UTF-8 files. `fs.search` returns one bounded hit per matching line across
visible inputs and scratch, in path order, skipping non-text files: a
case-insensitive literal by default, with `regex`, `case_sensitive`, a `path`
file or directory scope, and a filename `glob` (the basename, or the path
relative to the scope when it contains `/`); an invalid pattern or scope is
refused and a cut result says `truncated`. `fs.extract` takes one input
`path` and a new `/work` `output_path`, writes the shared extractor's JSON
without overwriting, and returns `{output_path, sources, extractor_version}`. The Invoker resolves them before the first model call through
the existing `ToolResolver`/`ToolRegistry`; an unknown name fails, and
filesystem tools and a workspace require each other. There is no fetch tool.
Every call first needs `{"resource":"tool","action":"invoke"}` with scope
`all` or `{ "tool": <exact name> }`, then the called capability's own
permission (for `bifrost.query`, read on every table it touches). Neither
grant implies the other, and neither is inferred from `verifier:run` or the
Agent declaration. Calls run under the execution's accepted authority; each
decision is audited non-blocking; tool names, arguments, output digests, and
byte counts are recorded.

`code` and `code.run` run in one per-process pool of sandboxed Python worker
processes with a separate unprivileged identity, empty credential
environment, OS-enforced network denial, parent-serviced file access, no
subprocesses or host packages, and a fresh interpreter session per operation.
Pool, memory, file, archive, extraction, scratch, tool-result, and deadline
limits are startup environment settings; slot limits only lower them.

#### Drift checks
A `drift` check flattens the existing `DriftSpec` next to `on:
inputs.<samples slot>`. No scheduling, no dispatch: scheduling is a
`Trigger`; dispatch is an `Operator`.
```yaml
check:
  kind: drift
  on: inputs.<samples slot>
  method: DriftMethod            # Spc | Psi | Custom
  signal: DriftSignal            # how the measurement enters the check
  condition: DriftCondition      # when a sample is judged drifted
  profile: DriftProfile          # matching PSI, SPC, or Custom config
```

**`Agent`, `External`, and judge-score methods/signals are deliberately absent.**
They have no complete input, fitting, or scoring contract here.

`DriftSignal` is a closed enum:

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

Fitted state is owned by `vala-drift`, keyed by tenant, exact Verifier, and
requirement ID. Existing threshold, minimum-sample, baseline-readiness, and
insufficient-data rules apply; per-feature statistics, thresholds, counts,
flags, and window identity are in the requirement `detail`.

### Source
Read-side reference to an external data system. **Wyrd reads, never writes.**

`source` is a **read-shape bucket**, not a vendor. The top-level discriminator is
the shape of data a consuming Verifier check sees — a row set, a time
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

The bucket set lines up with what Verifier checks already read:
`object_store`/`sql_warehouse` feed `DriftSignal::Distribution` (rows),
`metrics` feeds `DriftSignal::Metric`, `traces` feeds `trace` checks.

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
way it chooses a drift check's evaluation strategy.

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
automatically from the one DataFusion physical plan. A successful query returns
each row its selected published and online live sources hold exactly once,
including rows published while it runs. A Scribe that cannot be listed yields a
degraded terminal; losing a Scribe whose capture was accepted fails the query,
even before its first row. `bifrost-design.md` owns the source-selection and failure details.

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
  kind: schedule | group_end              # closed activation discriminator
  cron: string                            # schedule only
  tz?: string                             # schedule only — IANA tz name, default UTC
  after: duration                         # group_end only — elapsed since the newest acknowledged source evidence
```

`group_end` binds only a Verifier with source inputs and judges each caller
group once it ends (see Grouped verification under Verifier).

For `schedule`, each due occurrence creates one
execution per active binding over that binding's subject window
`[previous boundary, due)`, filling each `samples` slot from its own
`features`. The scheduler freezes the window's observation `record_id`s for
the subject when it prepares the occurrence and records them, with the
window, as the execution's `schedule` input record; every requirement and
retry reads exactly that population, an empty window is a real empty
selector, and later observations never change a prepared run. Missed
occurrences are skipped, not backfilled. A Trigger never
fires on its own: nothing runs merely because a Trigger Card is declared,
and one schedule occurrence shared by two subjects creates two subject-scoped
executions, never one mixed-subject execution.

Queued-execution creation follows Scribe's acknowledgement and never delays
it. After the ack, the server puts one request per submission into an
in-process outbox. One writer drains it in multi-row inserts, several per
tenant at once, each deciding idempotent replay and conflict in its
transaction. Requests sharing an idempotency key are written in arrival
order, so a repeated request inserts nothing. The outbox is bounded, but each submission reserves
its slot before its inputs are staged, so the hand-off is never refused; a
`submit` that finds no room is refused with
`WYRD_VERIFICATION_429_STAGING_FULL` before anything is staged. The outbox
never drops a request because PostgreSQL is slow or down: a failed write
keeps its batch and retries with backoff. Verification owns this outbox; graceful shutdown keeps it
open through Bifrost's final Scribe flush, so a late acknowledgement still creates its row, and then
flushes it before its dependencies close. A hard kill loses
only unwritten requests, and every loss the process observes is counted and
logged.

External pushes are deliberately not an activation — Rule 7 ("Wyrd reads, it
does not push") means external data enters through a `Source`.

### Operator
A failure reaction. Performs exactly one action — a typed notification or a
generic HTTP call. Operator is a pure side-effect
template with no `pre_invoke` / `post_invoke` hooks and no subscription of its
own: it runs only as an `on_failure` or `on_verdict_change` entry of a
verification binding. A
referenced Operator Card and an inline `on_failure` mapping use the same
flattened `OperatorSpec` shape.
```yaml
spec:
  description?: string
  kind: notify | http             # flattened OperatorAction discriminator
  # ...variant fields below
  budget?: { max_wall_seconds?: u32, max_tool_calls?: u32 }
```

`OperatorAction` is a closed tagged union (snake_case `kind` discriminator):

| Variant    | Variant-specific carries                                                              | Server does                                                                   |
|------------|---------------------------------------------------------------------------------------|-------------------------------------------------------------------------------|
| `Notify`   | `channel: NotifyChannel` (closed tagged union — typed vendor shape)                  | Sends the notification through the vendor-specific adapter the server owns.   |
| `Http`     | `method`, `url`, `headers?`, `body?`, `auth?`, `timeout_seconds?`, `expect_status?` | Builds and sends the HTTP request.                                            |

**Dispatch.** Only a completed, binding-created Verifier run with a `failed`
verdict creates Operator work: one durable dispatch per configured
`on_failure` Operator. Passed, inconclusive, cancelled, timed-out, errored,
and direct analysis-only runs create none. A `group_end` binding instead
dispatches each `on_verdict_change` Operator once when a completed group
execution's verdict differs from the group's previous one, carrying the group and
the result it changed from. Each dispatch has its own status,
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
logs, and audit carry only redacted metadata, never key locations. Declaration
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
the failed Verification Result: dispatch, execution, result, and binding IDs; the
exact Verifier and subject Card identities; the `failed` verdict; completion
time; a bounded result summary; and one count block for the failed execution's
Verifier's drift checks when it has any. It never
carries raw input context, media, feature rows, feature names, step output,
or secret material, and declaration rejects unknown template fields.
Exact field schema lives in OpenAPI.

| Applies to | Template fields |
|---|---|
| any | `dispatch_id`, `execution_id`, `result_id`, `binding_id`, `verifier_ref`, `verifier_card_uid`, `subject_ref`, `subject_uid`, `verdict`, `completed_at`, `summary` |
| Verifiers with a drift check | `drift.drifted_features`, `drift.total_features` |

The old verification `run_id` template field has no alias; `run_id` remains
reserved for the optional application Run and is not a template field.
Fields outside this table are rejected. Freeze the bounded generic result
summary and completion time with the existing failure context before delivery;
all retries retain that context. Adapt existing dispatch foreign keys, claim
joins, and deduplication to execution identity alongside this projection.

Counts render as base-10 digits and come from the same result as `summary`.
Multiple drift checks must not silently introduce an aggregate count meaning;
an ambiguous count projection requires an explicit semantic decision.
A binding refuses an attached Operator (inline or referenced) that uses
`drift.*` fields for a Verifier without a drift check, naming the binding
field path.

```yaml
# on_failure of a Verifier with a drift check
kind: notify
channel: { kind: slack, connection: ops-slack, channel_id: C0123456789,
           text: "{{drift.drifted_features}}/{{drift.total_features}} features drifted on {{subject_ref}} ({{verifier_ref}})" }
# on_failure of any Verifier
kind: notify
channel: { kind: slack, connection: ops-slack, channel_id: C0123456789,
           text: "{{verifier_ref}} failed on {{subject_ref}}: {{summary}}" }
```

---

## Registry lifecycle

How authored specs become durable cards. This section pins the wire
shape of composite declaration, the card blob state machine, and the
idempotency contract. Implementation lives in `wyrd_client::cards`; active change
specifications and task packets follow the repository specification-first
workflow and do not override this contract.

### Composite declaration

`POST /v1/cards` is a **composite** endpoint. One request may declare
one card or many, and the server derives the root from the DAG of
submissions.

```rust
struct DeclareCardRequest {
    submissions: Vec<CardSubmission>,
}

struct CardSubmission {
    api_version: ApiVersion,
    kind: CardKind,
    metadata: Metadata,
    spec: Value,                                       // kind-specific spec body
    artifacts: Vec<ArtifactManifestEntry>,             // heavy uploads; default empty
}

struct DeclareCardResponse {
    root: CardRef,                                     // server-derived, single no-in-degree node
    outcomes: Vec<CardDeclarationOutcome>,           // one per submission
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

PostgreSQL coordinates manifests and upload sessions; it keeps no inventory
of completed objects. Every completed artifact, envelope, and deletion is a
row in `bifrost.storage.objects` (see the Bifrost authority), and Card
listing, hydration, downloads, and drift baseline fitting all resolve a
Card's files there. Activation re-verifies each declared upload and records
it before the Card turns `Active`. A manifest entry that is complete in
PostgreSQL but not yet visible in the inventory answers the retryable
`WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE`.

### Idempotency

Idempotency is a client responsibility. The `Idempotency-Key` header is
required on `POST /v1/cards`. Missing header →
`WYRD_REGISTRY_400_IDEMPOTENCY_KEY_REQUIRED`.

Key derivation (client-side):

1. Canonicalize submissions by `(kind, name, version)`.
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

Idempotency state lives in `wyrd.card_declaration_operations` with a
24h TTL. Expired records outside the retry window surface as
`WYRD_REGISTRY_410_OPERATION_EXPIRED`.

### Error catalog

The public error catalog covering declaration is fixed and versioned.
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

Two complementary mechanisms — `wyrd declare file.yaml` reads + declares in
one move, and reference slots accept `Ref` or `InlineableRef<T>`. A path string is
loader-only authoring syntax projected to a Ref/Sibling before send; inline
is available only at `InlineableRef<T>` slots.

### Pre-declaration matrix (Rule 16)

| Kind | Must pre-declare? | Why |
|------|---------------------|-----|
| `Model`        | **Yes** | Carries weight artifacts; lineage anchor. |
| `Data`         | **Yes** | Carries dataset bytes; lineage anchor. |
| `Experiment`   | **Yes** | Carries run history. |
| `Artifact`     | **Yes** (typically derived from heavy cards) | Pointer to durable bytes. |
| `Prompt`       | Optional | Light. Inlineable as `InlineableRef<Prompt>` inside `Agent.prompt`; declaration materializes it as `<agent-name>-prompt`. |
| `Agent`        | Optional | Light. Spec-only; `wyrd declare` declares it. Verifier `agent` actions and checks take `InlineableRef<AgentSpec>`. |
| `Verifier`, `Policy`, `Trigger`, `Operator`, `Source`, `Mcp`, `Audit`, `Service` | Optional | Light. Spec-only; `wyrd declare` declares each card as it's read. |

A single YAML file may contain many `---`-separated card documents — `wyrd
declare verification-suite.yaml` declares all of them in dependency order. The Agent
under test, its judge Agent, and the Verifier that judges it can all
ship in one file.

### Reference forms

`Ref` and `InlineableRef<T>` are untagged values. A CardRef mapping is an
external reference, a string is a loader-local path, and an inlineable slot may
contain its native child body directly. There is no additional keyed
`ref`/`path`/`inline` discriminator wrapper around a slot value.

```yaml
# Registered Prompt reference at Agent.prompt.
prompt: { kind: Prompt, name: triage, version: "1.0.0" }

# Local full Card envelope, resolved relative to the containing file.
prompt: ./prompts/triage.yaml

# Inline portable Prompt: registers as the Agent's `<agent-name>-prompt` Card.
prompt:
  provider: openai
  model: example-model
  messages: [{role: user, content: "Hello"}]
```

The owning spec may itself name a field `ref`; this does not introduce a
reference discriminator. Service components retain that field for every form:

```yaml
components:
  - alias: agent
    ref: { kind: Agent, name: support-triage, version: "1.0.0" }
  - alias: model
    ref: { kind: Model, name: churn-classifier, version: "1.0.0" }
  - alias: prompt
    ref: ./prompts/triage-system.yaml
```

`CardRef` carries `kind`, `name`, one `version`, and optional `uid`; never
labels, annotations, a version requirement, or `space`. Its canonical text is
`Kind/name@version[#uid]`. Card identity is tenant, kind, name, and version;
the tenant comes from the authenticated caller, never from the reference. A
`space` field or a `space/Kind/name@version` text is refused as a validation
error, not mapped. Local loading never declares. Declare
declares path-loaded Cards in the same composite request, projects paths to
Sibling references, and preserves external refs even at matching identities.
Inline bodies remain embedded, except a declared Agent's inline Prompt,
which declaration materializes as its own Prompt Card and replaces with that
Card's exact reference (doctrine 17). Only resolved Ref/Sibling and inline
bodies cross declaration; durable persistence contains no Path or Sibling.

### Reference-slot inventory

The same two reference types apply at **every** reference slot, light or
heavy. There is one canonical slot inventory, and the loader, diagnostics, and
relationship tests all project from it:

- `Service.components[].ref` (light and heavy targets)
- `Service.components[].verified_by`, `Agent.verified_by`,
  `Service.verified_by` — each binding's `verifier`, `runs_on`, and
  `on_failure[]`
- `Agent.prompt`
- Verifier refs: `agent` action and check `agent`, `code` Artifact `source`,
  and drift check `signal.baseline_ref`
- Heavy anchors: `Model`/`Data`/`Experiment` `*_refs`, `Artifact` refs

 A new reference-bearing field is added to this inventory in one place; it then
 participates in path rewrite and relationship derivation without a second
 edit. This is the single-source rule for reference slots.

### Path resolution rules (loader contract)

A path string is **client-side authoring sugar**, not a durable wire variant.
It targets a full Card envelope on disk; the loader reads it without declaring.
Apply submits it as an independent Card and rewrites the referring slot to an explicit
declaration reference. When the target is part of the same composite
submission, the declaration reference is `Sibling { sibling: CardRef }`;
an authored external `ref: CardRef` remains `Ref(CardRef)`. This preserves
binding intent instead of inferring it from identity. The server validates
both forms and rewrites them to UID-bearing `Ref(CardRef)` before hashing,
relationship derivation, or durable persistence. The server, registry, and
`vala` never see a `path:` value or a durable `Sibling` value.

- Resolved **relative to the file containing the `path:` reference** — not
  CWD, not apply-root.
- Absolute paths are allowed but discouraged (breaks portability across
  machines and CI).
- The referenced file is a **full card envelope** — `kind` + `metadata`
  (`name`, `version`) + `spec`, plus `apiVersion`, which authored
  files may omit and which then defaults to `wyrd/v1`; the declaration wire
  always carries it explicitly. Declaration derives the
  child's identity from that `metadata`, and the parent slot becomes a `ref:`
  to it. This is what makes a `path:` target reusable and lineage-anchoring,
  unlike `inline:`.
- Path imports **are declared as independent cards**, in dependency order,
  ahead of the parent that references them. A heavy-card envelope reached by
  `path:` still follows the normal pre-declaration + blob-upload flow before
  the parent declares.
- Transitive: a `path:`-loaded envelope may itself contain `ref`/`path`/`inline`
  slots. The loader resolves transitively with a hard depth
  limit (≤8) to catch cycles.
- `ref`, `path`, and `inline` are mutually exclusive on any single slot. Any
  combination is a validation error.

This keeps the wire contract typed (`ref | inline`, with an explicit
declaration-only sibling projection where a composite write needs it), keeps
`path` resolution client-side, prevents filesystem-on-server, and gives authors
the file-splitting ergonomic they expect from JSON-Schema `$ref` / OpenAPI
external-file imports. Durable and read contracts remain the simpler direct
`ref` / `inline` forms.

---

## Workspace config (`wyrd.toml`)

Wyrd's wire contract identifies every card by `kind`, `name`, and one version
field, with an optional `uid`. Authors writing many cards in one bundle
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
[defaults.labels]
team   = "churn-ml"
domain = "customer"

[defaults.annotations]
"acme.com/owner" = "data-platform"

# Per-kind override; PascalCase keys match CardKind serialization.
[kind.Model.annotations]
"acme.com/owner" = "ml-platform"

[kind.Policy.labels]
tier = "governance"
```

- Defaultable fields: `labels`, `annotations`. `space` is not a field and
  is refused as an unknown key.
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
4. System fallback: empty `labels` / `annotations`.

### Merge rules

- **Maps** (`labels`, `annotations`): per-key merge. For each key
  present in the config, insert into the card's map only if absent.
  Per-card values always win for their own keys; other config keys
  still apply.
- **`version`, `name`, `uid`, `bump`, `spec_hash`, `artifact_hash`
  are never touched** by the loader. The first three are author
  identity or intent; the last three
  are server-derived.

The wire payload still arrives at the server fully populated; the server never
reads `wyrd.toml`.

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
| Data    | `card_refs`, `splits`                 | Verifier drift check `signal.baseline_ref`, `Experiment.target_refs` |
| Model   | `card_refs`                           | `Service.components.ref`, `Experiment.target_refs` |
| Agent   | `prompt`, `tool_names`, `verified_by` | `Service.components.ref`, Agent prompts (sub-agent calls), Verifier `agent` actions and checks |
| Mcp     | `server_name`, `transport`, `scopes` | `Service.components.ref` |
| Verifier| `agent` actions and checks, `code` Artifact sources, drift `signal.baseline_ref` | `*.verified_by[].verifier` |
| Audit   | `subject_refs`, `query` (roots), `lineage` (nodes), `investigator` (Agent variant) | — |
| Service | `components[].ref`, `components[].verified_by`, `verified_by` | — |
| Policy  | `rules`                              | `Service.components.ref` |
| Trigger | `schedule` activation | `*.verified_by[].runs_on` |
| Operator| `action` (typed `channel` shape \| tenant connection) | `*.verified_by[].on_failure` |
| Source  | `kind` (bucket), `connection` (vendor + `*_env`) | — |

`Service.components` accepts: Agent, Prompt, Model, Mcp, Policy. No
other kinds are runtime-aliased into a Service.

---

## Worked directory layout

A deployment is a folder. `wyrd declare <dir>` declares every card.

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
│   ├── triage-verifier.yaml       # agent steps and checks
│   ├── runbook-verifier.yaml
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

- **One declarative Verifier.** A Verifier is inputs, steps, requirements,
  and decide; drift and agent judging are check kinds, not Verifier kinds.
  Evidence mode (Run or Window) follows from the slot and check types, so
  `verify` and `submit` route without an authored discriminator.
