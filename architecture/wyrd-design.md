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
3. **Monitors declare subjects.** Drift and Eval reference what they
   observe. Subjects do not list their observers. Each declares a single
   `subject_ref`.
4. **Reactions are Operators. Wiring is Triggers.** Drift/Eval/Policy never
   inline reaction logic.
5. **Service composes for deployment, not observation.** Drift/Eval/Trigger/
   Operator/Audit/Source are peer cards, not Service components.
6. **Enforcement is composed at the surface that enforces it.** Service
   composes Policy for runtime gates.
7. **Wyrd reads. It does not push.** Application code emits to its own
   observability stack with its own SDKs. Wyrd records internal observations
   in `vala`; external data is queried through `Source` cards.
8. **Lineage is server-derived from `*_refs`.** Never authored. Never edited.
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

    A deployed Service card runs under a Wyrd-owned service account derived
    deterministically from its `card_ref`. `wyrd apply` registers the card and
    creates the SA (idempotent on re-apply); no secret is returned. Credentials
    are issued out-of-band by an admin-authenticated `wyrd auth issue-key
    <card_ref>` call, which mints a card-bound API key and returns it to the
    caller. The caller uploads the key to the deploy environment's secret store
    (Vault, AWS Secrets Manager, GCP Secret Manager); deploy-time secret
    injection puts it into the pod as `WYRD_API_KEY`. The SDK exchanges it once
    at startup for a short-lived JWT carrying the card's `card_ref` claim. On
    cross-service calls the SDK adds `Wyrd-Caller-Identity: Bearer <jwt>` — the
    application's `Authorization` header is never touched. The mesh's ext_authz
    filter (or the SDK middleware) authenticates itself to `/v1/authz/check`
    with its own card-scoped JWT and forwards the caller's JWT plus the
    original method/path as headers; the body is empty. Both identities are
    server-verified from signed claims.
19. **Light-card reference slots accept `ref | path | inline`.** Wherever a
    card spec references a light card (Prompt, Agent, Workflow, Mcp, Policy,
    Eval, Trigger, Operator, Source, Audit, Service), the slot is a
    three-variant tagged-by-key union, mutually exclusive:
    - `ref:`    — `CardRef` to a registered card. Stable identity.
    - `path:`   — client-side authoring sugar. Loader splices the file's
                  spec body inline before send. Never on the wire. Never
                  auto-registered.
    - `inline:` — spec body embedded in the parent, prefixed with `kind:`.
                  No card identity; not addressable from outside the parent.

    Heavy cards (Model, Data, Experiment) accept `ref:` only — identity is
    required for lineage. See §"Light-card reference forms" for loader rules.

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
  components: [ServiceComponent] # { alias, ref | path | inline }
  entry_point?: string           # SDK AppState bootstrap module (e.g. `acme.copilot.app:app`).
                                 # Importing it materializes the service's locked card snapshot
                                 # at runtime. Wyrd doesn't import this; the deploy image does.
```

Identity is derived from the Service's `card_ref` and bound on first deploy
contact — no `service_account` field on the spec. See "Runtime identity".

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

A deployed Service card runs under a Wyrd-owned service account derived
deterministically from the card's `card_ref`. `wyrd apply` registers the card
and creates the SA (idempotent on re-apply); no secret is returned. The
declarative and credential operations are separated, matching the kubectl
pattern (`apply` then `create token`):

| Operation | Wyrd command | What it does |
|---|---|---|
| Register card + create SA | `wyrd apply -f service.yaml` | Idempotent. Writes card, creates SA. No secret. |
| Mint a card-bound API key | `wyrd auth issue-key <card_ref>` | Admin-authenticated. Returns the key to the caller. Caller uploads to the deploy environment's secret store. Re-issuable for rotation. |

Deploy-time secret injection (Vault Agent, External Secrets Operator, AWS
Secrets Manager CSI driver, etc.) puts the API key into the pod as
`WYRD_API_KEY`. The SDK exchanges it ONCE at startup at `POST /auth/token` for a
short-lived JWT (~15m) carrying the `card_ref` claim, and auto-refreshes before
expiry.

Env vars in deployed services:

| Env var | Required? | Source | Used for |
|---|---|---|---|
| `WYRD_API_KEY` | REQUIRED | Deploy environment's secret store (key minted by `wyrd auth issue-key <card_ref>`) | Exchanged ONCE at startup at `POST /auth/token` for short-lived JWT. SDK auto-refreshes. JWT carries `card_ref` claim. |
| `WYRD_API_URL` | REQUIRED | Static config | Wyrd server base URL. |
| `WYRD_GOV_TOKEN` | OPTIONAL | CI writes from `wyrd gov-token issue` response | Only if the app calls `wyrd.observe(...)`. |

The API key is exchanged at startup — never on the wire. The JWT — not the API
key — is what travels on cross-service
calls in the dedicated `Wyrd-Caller-Identity: Bearer <jwt>` header. The
application's own `Authorization` header is never touched by the SDK.

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
| `Prompt`    | `Agent.prompt`, `Eval.tasks[].Judge.prompt`                       |
| `Dataset`   | `Eval.dataset_ref`                                                |
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

### Light-card reference forms

A light-card reference slot accepts exactly one of three keys: `ref`, `path`,
or `inline`. The key IS the discriminator; there is no separate `kind:` tag
for the variant. The three forms in isolation:

```yaml
# 1. ref — points at a registered card.
ref: { kind: Policy, name: pii-redaction, version: "1.0.0", space: prod }

# 2. path — authoring sugar. Loader splices the file's spec body inline.
#    File begins with `kind:` then spec fields; no `apiVersion` or `metadata`.
path: ./policies/pii-redaction.yaml

# 3. inline — full spec body embedded in the parent. No card identity.
inline:
  kind: Policy
  rules:
    - name: redact-ssn
      expression: "message.contains_pii('ssn')"
      action: gate
  scope: service_local
```

In context — `Service.components[]` mixing all three plus a heavy-card ref:

```yaml
components:
  - alias: agent
    ref:  { kind: Agent, name: support-triage,   version: "1.0.0", space: prod }
  - alias: model
    ref:  { kind: Model, name: churn-classifier, version: "1.4.2", space: prod }  # heavy — ref only
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

Same three-key shape applies anywhere a light card is referenced —
`Service.components[].ref`, `Trigger.target`, `Workflow.steps[].target`,
`Agent.prompt`, `EvalTask::Judge.prompt`, etc.

### Path resolution rules (loader contract)

`path:` is **client-side authoring sugar**, not a wire variant. The loader
splices the referenced file's spec body into the parent at read time; the
payload that leaves the client contains only `ref` or `inline`. The server,
registry, and `vala` never see a `path:` value.

- Resolved **relative to the file containing the `path:` reference** — not
  CWD, not apply-root.
- Absolute paths are allowed but discouraged (breaks portability across
  machines and CI).
- The referenced file is a **bare spec body** — `kind:` plus spec fields,
  no `apiVersion` or `metadata` envelope. The `kind:` makes the file
  self-describing and parses identically to an inline body.
- Path imports are **never auto-registered as cards**. The result is inline.
  If you want a registered, reusable card, write a full card document (with
  `apiVersion` + `kind` + `metadata`) and apply it; then reference by `ref:`.
- Transitive: a `path:`-loaded fragment may itself contain `path:` refs.
  Loader resolves transitively with a hard depth limit (≤8) to catch cycles.
- `ref`, `path`, and `inline` are mutually exclusive on any single slot.
  Any combination is a validation error.

This keeps the wire contract tight (two-variant `LightRef` post-loader),
prevents filesystem-on-server, and gives authors the file-splitting
ergonomic they expect from JSON-Schema `$ref` / OpenAPI external-file imports.

---

## Reference-direction quick reference

| Card    | Refs that authored on it             | Refs that point at it          |
|---------|--------------------------------------|--------------------------------|
| Data    | `card_refs`, `splits`            | `Drift.signal.baseline_ref`, `Eval.dataset_ref`, `Experiment.target_refs` |
| Model   | `card_refs`                      | `Drift.subject_ref`, `Eval.subject_ref`, `Service.components.ref`, `Experiment.target_refs` |
| Agent   | `prompt`, `tool_names`               | `Drift.subject_ref`, `Eval.subject_ref`, `Service.components.ref`, Agent prompts (sub-agent calls) |
| Workflow| `steps.*.target`                     | `Eval.subject_ref`, `Service.components.ref`, `Operator.action.workflow_ref` |
| Mcp     | `server_name`, `transport`, `scopes` | `Service.components.ref` |
| Drift   | `subject_ref`, `signal.*` (`baseline_ref` \| `eval_ref` \| `source_ref`) | `Trigger.source.drift_ref`, `Drift.signal.eval_ref` (other Drifts watching an Eval indirectly) |
| Eval    | `subject_ref`, `dataset_ref`, `source_ref`, `tasks[].Judge.prompt` (PromptRef) | `Drift.signal.eval_ref`, `Trigger.source.eval_ref` |
| Audit   | `subject_refs`, `query` (roots), `lineage` (nodes), `investigator` (Agent variant) | — |
| Service | `components[].ref`                   | `Drift.subject_ref` (service-level), `Eval.subject_ref` |
| Policy  | `rules`                              | `Service.components.ref`, `Operator.pre_invoke`, `Operator.post_invoke` |
| Trigger | `schedule`, `source.drift_ref` \| `source.eval_ref`, `operator_ref` | — |
| Operator| `action` (`workflow_ref` \| typed `channel` shape \| `auth.env`), `pre_invoke`, `post_invoke` | `Trigger.operator_ref` |
| Source  | `kind`, `uri`, `format`              | `Drift.signal.source_ref` (External variant), `Eval.source_ref` |

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
7. Whether tool hook phases need a closed enum on `Policy.rules` or can stay
   off the wire entirely (no consumer today).
8. **Closed.** `Eval` does not carry its own `signal` decomposition. Eval IS
    the signal — its per-task pass/fail aggregates into a score stream
    consumed downstream by `Drift` with `DriftSignal::EvalScore`. The input
    edges (`dataset_ref` vs `source_ref`) are two optional `CardRef`s, not a
    tagged enum: presence is the mode (offline driver, online sink, both, or
    neither → vala default archive).
