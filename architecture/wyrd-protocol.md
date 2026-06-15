# Wyrd Protocol — v1 (draft)

> Superseded working draft. `architecture/wyrd-design.md` is the active design
> authority. This document still reflects older 18-kind protocol exploration
> and must not be used to justify `Tool`, `Skill`, or `SubAgent` as v1 Card
> kinds.

**Status:** draft — not locked
**API group:** `wyrd/v1`
**Editors' working copy.** Will be published as the canonical Wyrd protocol
specification under the public documentation site once stabilized.
**Companion artifacts:**
- Worked YAML examples: `architecture/specs/*.yaml`
- Design filter: `docs/src/content/docs/concepts/core-doctrine.mdx`
- Rust reference implementation: `crates/wyrd-spec/`

> When this document and an implementation disagree, **this document is the
> authority for the wire and the data model**. The Rust reference
> implementation is normative for behavior not explicitly described here, and
> is **non-normative** wherever this document is explicit.

---

## Table of contents

- **Part 0** — About this protocol
- **Part I** — Resource model
- **Part II** — Card kind reference (18 native kinds)
- **Part III** — Composition flows
- **Part IV** — Resource operations
- **Part V** — Discovery and capabilities
- **Part VI** — Observation stream
- **Part VII** — Authorization
- **Part VIII** — Transport bindings
- **Part IX** — Lifecycle, lineage, versioning, conformance
- **Part X** — Authoring guide
- **Appendix A** — Conflicts catalog (open issues)
- **Appendix B** — Worked-example cross-reference
- **Appendix C** — Schema generation

---
---

# Part 0 — About this protocol

## 0.1 What Wyrd is

Wyrd is the **protocol for the AI control plane**. It is to AI systems what
Kubernetes is to compute, what MCP is to model-tool interfaces, and what A2A is
to agent peering.

Wyrd is **not** a runtime, a framework, an inference server, a training
platform, or a vendor product. It is a vendor-neutral specification for how
AI work is **declared, registered, governed, observed, and discovered**.

A system is "Wyrd-compatible" when it speaks the Wyrd Protocol: serves the
data model in Part I, presents the resource operations in Part IV, advertises
capabilities per Part V, and emits observations per Part VI.

## 0.2 Audience

This document has three audiences:

1. **Authors** — humans and agents writing YAML/JSON specs describing AI
   work (models, agents, prompts, workflows, services, monitors, policies).
   Authors read Parts I, II, III, and X.
2. **Surface builders** — those building SDKs (Python, TypeScript, Go, …),
   CLIs, UIs, MCP servers, and IDE integrations that project the protocol.
   Surface builders read everything.
3. **Implementers** — those building a Wyrd-compatible control plane
   (registry, lineage, policy, observation routing). Implementers read
   everything, plus the conformance matrix in Part IX.

## 0.3 Reference implementations

The current reference implementations of the Wyrd Protocol:

| Component                     | Status                  | Reference path                     |
|-------------------------------|-------------------------|------------------------------------|
| Rust contract crate (`wyrd-spec`) | normative source for types | `crates/wyrd-spec/`        |
| Wyrd control-plane server     | reference implementation | `crates/wyrd/`                    |
| Python SDK                    | reference implementation | `python/py-wyrd/`                 |
| Observability runtime (`vala`) | reference implementation | `crates/vala/`                    |
| LLM runtime (`skald`)         | reference implementation | `crates/skald/`                   |

Other implementations may exist. A surface is Wyrd-compatible if and only if
it conforms to this document.

## 0.4 Document conventions

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHALL NOT**,
**SHOULD**, **SHOULD NOT**, **RECOMMENDED**, **MAY**, and **OPTIONAL** are
interpreted as in **RFC 2119** and **RFC 8174** when, and only when, they
appear in all capitals.

Type annotations in this document use a TypeScript-like notation:

```
{ field: Type, optional?: Type }
[Type, ...]                // array
Type | Type | ...          // union
Type<T>                    // generic
```

The wire encoding is **JSON** unless a transport binding (Part VIII) specifies
otherwise. **YAML** is interchangeable with JSON for authoring; servers MUST
accept both.

`OPEN-...` markers identify unresolved design questions catalogued in
**Appendix A**. Implementations MUST treat current code behavior as the
de facto rule for any OPEN issue, and MUST track resolutions when the
appendix is updated.

## 0.5 Relation to other protocols

| Protocol | Wyrd's relation |
|----------|-----------------|
| **MCP** (Model Context Protocol) | Wyrd registers `Mcp` Cards declaring MCP servers, and Wyrd-compatible MCP servers MAY surface Wyrd Card operations as MCP tools (Part VIII §VIII.2). |
| **A2A** (Agent-to-Agent) | A Wyrd `Agent` or `Service` Card MAY publish an A2A AgentCard at its endpoint. The A2A AgentCard is **not** a Wyrd Card; it is the runtime peering descriptor. |
| **Kubernetes API** | Wyrd's resource model (envelope, kind, metadata, spec, status, watch, derived relationships) is patterned on the Kubernetes API. Authors familiar with `kubectl apply -f` will recognize the shape. Wyrd is **not** built on Kubernetes; it is its own protocol. |
| **OpenAPI** | Wyrd publishes OpenAPI for the HTTP+JSON binding (Part VIII §VIII.1) and JSON Schema for each Card kind (Appendix C). |
| **OpenTelemetry** | Wyrd observations carry OTel-compatible trace context (Part VI §VI.4). Wyrd does not replace OTel; it routes Wyrd-shaped observations alongside OTel signals. |

---
---

# Part I — Resource model

## I.1 The universal envelope

Every Card uses the same outer shape:

```yaml
apiVersion: wyrd/v1
kind: <CardKind>
metadata: <Metadata>
spec:     <kind-specific Spec>
relationships: { outbound: [], inbound: [] }   # server-derived; authors omit
status: null                                    # server-managed; authors omit
```

| Field           | Type                | Author writes? | Notes |
|-----------------|---------------------|----------------|-------|
| `apiVersion`    | `string`            | yes            | MUST be `wyrd/v1` for v1. |
| `kind`          | `CardKind`          | yes            | One of 18 native kinds or an External object. Flat on the envelope. |
| `metadata`      | `Metadata`          | yes            | §I.2. |
| `spec`          | kind-specific       | yes            | Discriminated by `kind`. Defined per kind in Part II. |
| `relationships` | `Relationships`     | no             | Authors MAY omit. Server overwrites on registration. |
| `status`        | `Status?`           | no             | Authors MUST NOT set. Server overwrites on registration. |

### I.1.1 Author/server boundary

- Authored payloads MAY include `relationships` and `status`. The server MUST
  ignore those values on write.
- Returned payloads MUST include `relationships` and SHOULD include `status`.

### I.1.2 What `kind` is not

- `kind` is **not** nested inside `spec`.
- There is **no** outer `kind: Card` wrapper.
- `spec` is **not** a generic JSON object — it is one of the 19 typed payloads
  defined in Part II (18 native + External).

### I.1.3 External kinds

For forward compatibility, `kind` MAY be the object form:

```yaml
kind:
  kind: <ExternalName>     # matches [A-Za-z][A-Za-z0-9_.-]*
  schema_hash: <hex64>     # 64 lowercase hex chars = sha256 of external schema
```

External kinds are opaque to the registry: stored, hash-verified, linked by
CardRef, but the registry MUST NOT interpret their spec. External kinds MUST
NOT be used to extend any native kind. See §I.7 and §IX.4.

## I.2 Identity and metadata

```yaml
metadata:
  name:        <CardName>
  version:     <VersionBlock>   # strict semver MAJOR.MINOR.PATCH
  space:       <SpaceName?>     # omit ⇒ "default"
  uid:         <CardUid?>       # server-assigned on first registration
  labels:      { <string>: <string> }
  annotations: { <string>: <string> }
  spec_hash:   <string?>        # server-derived
  artifact_hash: <string?>      # server-derived for ArtifactCards
```

### I.2.1 Identity rules

| Rule | Requirement |
|------|-------------|
| `name` | MUST match `^[A-Za-z][A-Za-z0-9_-]*$`. Leading digit prohibited. |
| `version` | MUST be strict semver `MAJOR.MINOR.PATCH`. Pre-release and build metadata are RESERVED for a future revision. |
| `space` | OPTIONAL. Omitted means `default`. `default` MAY be authored explicitly. |
| `uid` | Assigned by the server on first successful registration. Subsequent writes for the same `(space, kind, name, version)` MUST preserve the uid. |
| Uniqueness | The tuple `(space, kind, name, version)` MUST be unique within a Wyrd installation. |

### I.2.2 Labels

Labels are **queryable**. Keys SHOULD be short (`team`, `domain`, `env`),
values SHOULD be enumerable, and the set SHOULD be designed for selectors.

### I.2.3 Annotations

Annotations are **free-form metadata** for automation, UI hints, and integration
metadata.

- Keys under `wyrd.io/*` are RESERVED for Wyrd-owned metadata.
- User and vendor keys MUST use a DNS-style prefix (e.g.,
  `acme.com/cost-center`).
- Servers MUST preserve unrecognized annotations on read/write round-trips.

### I.2.4 What metadata does NOT carry

Metadata MUST NOT carry:

- runtime endpoint information (belongs in spec)
- secret material (use `CredentialRef`)
- composition refs (use spec fields)
- governance refs (use `Governance`)
- mutable status (server-managed)

## I.3 CardRef

A `CardRef` is the **only** authored pointer between Cards.

```yaml
ref:
  kind:    <CardKind>
  name:    <CardName>
  version: <VersionBlock>
  space:   <SpaceName>
  uid:     <CardUid?>       # optional resolved hint
```

### I.3.1 Resolution

- `space` is required. Card identity on the wire is
  `(kind, name, version, space)`; the registry also accepts `uid` as a
  resolved alternative. Authoring tools that infer `space` from the enclosing
  card MUST fill it in before serialization.
- A `CardRef` MUST resolve to exactly one registered Card identified by
  `(space, kind, name, version)`.
- If `uid` is present, the server MUST verify it matches the resolved Card
  and reject the write on mismatch.
- A `CardRef` to a non-existent Card MUST cause the containing Card's
  registration to fail unless explicitly permitted by the field's contract
  (none does in v1).

### I.3.2 Versioning posture

`version` is **always exact**. There is no range syntax, no `latest`, no
floating tag. "Latest stable" is achieved by pinning a version and bumping
via a service-owned promote action that produces a new spec revision.

### I.3.3 Display form

The canonical display form is `<space>/<kind>/<name>@<version>`.

### I.3.4 What CardRef is NOT

- Not a free-form string. Surfaces MAY accept a string shortcut that desugars
  to the typed struct.
- Not a URI scheme. The durable form is the typed struct.
- Not a runtime handle.

## I.4 Relationships (server-derived)

```yaml
relationships:
  outbound: [<display-string>, ...]
  inbound:  [<display-string>, ...]
```

### I.4.1 Derivation

- The server MUST scan every `CardRef` in a Card's spec (and nested
  structures) on registration.
- For each resolved ref, the server MUST add a display-string entry to the
  source Card's `outbound` and the target Card's `inbound`.
- The display string format is §I.3.3.
- Authors MAY supply manual relationship entries on write; the server MUST
  overwrite with its derivation.

### I.4.2 What relationships do NOT carry

- No edge "kind" tag. Edge type is implicit in the producing spec field.
- See OPEN-A1 (Appendix A).

## I.5 Status (server-managed)

```yaml
status:
  phase:      <string>          # e.g. draft | active | deprecated
  message:    <string?>
  updated_at: <RFC3339?>
```

### I.5.1 Status rules

- Authors MUST NOT set `status` on write. Servers MUST drop client-provided
  status.
- `phase` is a free string today. A closed enum is proposed (OPEN-A2).
- `updated_at` is RFC3339 UTC.

## I.6 Shared foundations

Foundations are spec building blocks reused by multiple kinds. They are
**not** Card kinds.

### I.6.1 `Governance`

```yaml
governance:
  policy_refs:           [<CardRef>, ...]   # Policy Cards governing this Card
  audit_ref:             <CardRef?>         # Audit Card scope this belongs to
  approval_requirements: [<string>, ...]    # named approval scopes
  metadata:              { <string>: NonSecretValue }
```

Currently appears on: `EvalSpec`, `WorkflowSpec`, `SubAgentSpec`.
**OPEN-A3:** placement is inconsistent — examples want governance on
`Service`, `Model`, `Agent`, `Drift`. See Appendix A.

### I.6.2 `ObservationHooks`

```yaml
observation_hooks:
  route_refs: [<CardRef>, ...]   # Cards that consume observations (today: Services)
  events:     [<string>, ...]    # event names
```

Currently appears on: `EvalSpec`, `WorkflowSpec`, `SubAgentSpec`.
**OPEN-A4 / OPEN-A5:** missing on `Service`, `Model`, `Agent`, `Drift`; the
event vocabulary is open. See Appendix A.

### I.6.3 `FrameworkAdapterRef`

```yaml
framework_adapter:
  name:    <string>            # adapter name (sklearn, mlflow, datadog, ...)
  version: <string>            # adapter version or range
  config:  { <string>: NonSecretValue }
```

The adapter bridges declared intent to executable code. Adapters live in user
code or published packages, not in this protocol.

### I.6.4 `CredentialRef`

```yaml
credential_refs:
  - { provider: <string>, name: <string> }
```

The protocol guarantees: the reference is durable, secret material MUST NOT
appear in any spec field, and the runtime resolves credentials at invocation
time.

### I.6.5 `NonSecretValue` / `ParameterValue`

```yaml
# NonSecretValue (config)
{ type: str,    value: "..." }
{ type: number, value: 1.5 }
{ type: bool,   value: true }
{ type: list,   value: [<NonSecretValue>, ...] }
{ type: object, value: { <string>: <NonSecretValue> } }

# ParameterValue (run inputs)
{ type: int,   value: 42 }
{ type: float, value: 3.14 }
{ type: str,   value: "..." }
{ type: bool,  value: true }
{ type: json,  value: <arbitrary JSON> }
```

**OPEN-A6:** unification proposed.

## I.7 External kinds and the extension model

The Wyrd Protocol provides two extension mechanisms:

1. **External kinds** (§I.1.3, §IX.4) — for entirely new card-shaped concepts
   that a non-Wyrd vendor wants to register and link.
2. **`FrameworkAdapterRef`** (§I.6.3) — for bridging to upstream registries
   (MLflow, SageMaker, Vertex) and external runtimes (Datadog, OTel,
   pytorch, sklearn).

Authors integrating an external registry SHOULD use pattern (2) — a
`ModelCard` with `ModelInterface::Custom` plus an `ArtifactCard` whose
`external_uri` points at the upstream and whose `framework_adapter` names
the loader. Worked example: `architecture/specs/04-external-mlflow.yaml`.

---
---

# Part II — Card kind reference

This part documents every native CardKind. Each section follows the same
format:

> **Purpose** — one sentence
> **YAML skeleton** — authoring template
> **Required fields** — table
> **Optional fields** — table
> **CardRefs carried** — what edges this kind emits to lineage
> **Validation invariants** — locked rules
> **Worked example** — pointer to `architecture/specs/`
> **Runtime emissions** — events the runtime SHOULD emit
> **Open issues** — pointers to Appendix A

The 18 native kinds:

| Kind        | Role group     |
|-------------|----------------|
| Data        | Source         |
| Artifact    | Source         |
| Prompt      | Source         |
| Model       | Behavior       |
| Tool        | Behavior       |
| Agent       | Behavior       |
| SubAgent    | Behavior       |
| Workflow    | Behavior       |
| Operator    | Behavior       |
| Service     | Composition    |
| Skill       | Composition    |
| Mcp         | Composition    |
| Policy      | Governance     |
| Audit       | Governance     |
| Drift       | Observation    |
| Eval        | Observation    |
| Trigger     | Observation    |
| Experiment  | Provenance     |

## II.1 Data

> **Purpose.** Declares a dataset: interface (parquet, vector store, sql, …),
> schema, splits, statistics, optional SQL bundle, artifact bytes.

**YAML skeleton:**
```yaml
apiVersion: wyrd/v1
kind: Data
metadata: { name: <CardName>, version: <semver>, space: <SpaceName?> }
spec:
  interface:
    kind: <DataInterfaceKind>
    meta: { ... }
  schema:
    columns: [ { name, dtype, nullable }, ... ]
  card_refs: [<CardRef→Artifact>, ...]
  splits: { <SplitName>: { kind, meta } }
  target_columns: [<ColumnName>, ...]
  sql: { ... }                              # optional SQL bundle
  stats: { kind: <StatsKind>, meta: { ... } }
```

**Required fields:** `interface`, `schema`, `stats`.
**Optional fields:** `card_refs`, `splits`, `target_columns`, `sql`.
**CardRefs:** `card_refs → Artifact`; `splits[*].strategy.value → Artifact` (Materialized splits only).
**Validation:** schema columns MUST be unique; split keys MUST be unique.
**Worked example:** `01-ml-prediction-service.yaml`, `03-rag-workflow-service.yaml`.
**Runtime emissions:** none directly; runs are emitted by Cards that consume Data.

## II.2 Artifact

> **Purpose.** Declares durable downloadable bytes (or an external pointer):
> kind, URIs, content-type, integrity, framework adapter.

**YAML skeleton:**
```yaml
apiVersion: wyrd/v1
kind: Artifact
metadata: { ... }
spec:
  artifact_kind: <string>             # e.g. sklearn_pickle | external_model
  artifact_uris: [<URI>, ...]         # storage URIs; empty for external
  content_type: <string?>
  size_bytes:   <integer?>
  integrity:    <string?>             # sha256:<hex64>
  schema_ref:   <CardRef?>
  framework_adapter:
    name: <string>
    version: <string>
    config: { ... }
  external_uri: <string?>             # for upstream-registry refs (MLflow, …)
  metadata: { ... }
```

**Required fields:** `artifact_kind`.
**CardRefs:** `schema_ref` (any kind, typically `Data`).
**Validation:** if `external_uri` is set, `artifact_uris` MAY be empty.
**Worked example:** `01-…` (sklearn pickle), `04-external-mlflow.yaml` (external pattern).
**Runtime emissions:** none.
**Open:** OPEN-A19 (`artifact_kind` closed taxonomy).

## II.3 Prompt

> **Purpose.** Declares an LLM prompt: messages, variables, model hint,
> response format. Accepts two input forms (declarative `PromptDraft` and
> native `Prompt`); both produce the same stored shape.

**YAML skeleton (declarative form):**
```yaml
apiVersion: wyrd/v1
kind: Prompt
metadata: { ... }
spec:
  provider: <string>                  # anthropic | openai | …
  model:    <string>
  messages:
    - { role: system | user | assistant, content: <string> }
  variables: [<string>, ...]
  response_format: text | { kind: json_schema, schema: { ... } }
```

**Required fields:** the native Skald `Prompt` shape, flattened into spec.
**CardRefs:** none in v1.
**Validation:** declared `variables` MUST appear in at least one message
template.
**Worked example:** `02-llm-agent-service.yaml`, `03-rag-workflow-service.yaml`.
**Runtime emissions:** none directly; Prompt is invoked via Agent or Workflow.

## II.4 Model

> **Purpose.** Declares a callable model: framework interface, task type,
> typed I/O signature, sample input, artifact refs.

**YAML skeleton:**
```yaml
apiVersion: wyrd/v1
kind: Model
metadata: { ... }
spec:
  interface:
    kind: Sklearn | Xgboost | Lightgbm | Catboost | Torch | Lightning
         | Tensorflow | Huggingface | Custom
    meta: { ... }
  task_type: <ClosedTaskTaxonomy>     # e.g. BinaryClassification | Generation
  signature:
    inputs:  [ { name, dtype, nullable }, ... ]
    outputs: [ { name, dtype, nullable }, ... ]
  sample_input: { rows: [ ... ] }     # optional
  card_refs: [<CardRef→Artifact>, ...]
```

**Required fields:** `interface`, `task_type`, `signature.inputs`, `signature.outputs`.
**CardRefs:** `card_refs → Artifact`.
**Validation:** every input and output MUST have a `name` and a valid `dtype`;
`card_refs` MUST be non-empty unless `interface.kind == Custom`.
**Worked example:** `01-ml-prediction-service.yaml`, `04-external-mlflow.yaml`.
**Runtime emissions:** `card.Model.predict.started | finished | failed` (proposed).
**Open:** OPEN-A3, OPEN-A4, OPEN-A5 (Governance / ObservationHooks placement).

## II.5 Tool

> **Purpose.** Declares an agent-visible capability: name, description, input
> schema, output schema, tool type (`script | api | mcp | builtin`).

**YAML skeleton:**
```yaml
apiVersion: wyrd/v1
kind: Tool
metadata: { ... }
spec:
  name:         <string>             # LLM-visible tool name
  description:  <string>
  tool_type:    script | api | mcp | builtin
  args_schema:  <JSONSchema>
  output_schema: <JSONSchema?>
  script_config: { ... }             # tool_type-specific
  api_config:    { ... }
  credential_refs: [<CredentialRef>, ...]
  mcp_server_name: <string?>         # OPEN-A11 → should be CardRef→Mcp
  allowed_tools:   [<string>, ...]
  requires_approval: <bool>
  hook_events:     [<string>, ...]
```

**Required fields:** `name`, `description`, `tool_type`, `args_schema`.
**CardRefs:** none today (see OPEN-A11).
**Validation:** `name` MUST match `^[a-z][a-z0-9_]*$` (LLM-visible).
**Worked example:** `02-llm-agent-service.yaml`, `06-multi-agent-service.yaml`.
**Runtime emissions:** `card.Tool.call.started | finished | failed` (proposed).
**Open:** OPEN-A10 (`tool_type` closed enum), OPEN-A11 (Tool→Mcp via CardRef).

## II.6 Agent

> **Purpose.** Declares a bounded tool loop: prompt, tool names, run config.

**YAML skeleton:**
```yaml
apiVersion: wyrd/v1
kind: Agent
metadata: { ... }
spec:
  prompt: <PromptRef>                 # inline Prompt or CardRef→Prompt
  tool_names: [<string>, ...]         # OPEN-A12 → should be tool_refs
  run_config:
    max_iterations: <int>
    tool_concurrency_cap: <int?>
    session_recent_limit: <int?>
    timeout_ms: <int?>
```

**Required fields:** `prompt`.
**CardRefs:** `prompt → Prompt` (when `prompt.kind == Card`).
**Validation:** `prompt` MUST resolve; if a tool name is referenced that the
runtime does not provide, registration MUST fail.
**Worked example:** `02-…`, `03-…`, `06-…`.
**Runtime emissions:** `card.Agent.run.started | iteration | tool.call |
run.finished | run.failed` (proposed).
**Open:** OPEN-A3, OPEN-A4, OPEN-A5, OPEN-A12.

## II.7 SubAgent

> **Purpose.** Declares a delegated agent (used by harnesses for sub-tasks).

**YAML skeleton:**
```yaml
apiVersion: wyrd/v1
kind: SubAgent
metadata: { ... }
spec:
  prompt: <string?>
  model:  <string?>
  tool_refs: [<CardRef→Tool>, ...]
  disallowed_tools: [<string>, ...]
  skill_refs: [<CardRef→Skill>, ...]
  max_turns: <int?>
  permission_mode: <string?>
  background: <bool>
  governance: <Governance?>
  observation_hooks: <ObservationHooks?>
```

**Required fields:** none beyond identity.
**CardRefs:** `tool_refs → Tool`, `skill_refs → Skill`.
**Worked example:** `05-observability-datadog.yaml`.

## II.8 Workflow

> **Purpose.** Declares a typed DAG over Agent / Tool / Prompt / Skill / Mcp
> actions, with inputs, outputs, governance, and observation hooks.

**YAML skeleton:**
```yaml
apiVersion: wyrd/v1
kind: Workflow
metadata: { ... }
spec:
  description: <string?>
  inputs: { <string>: <ParameterValue> }
  steps:
    - id: <string>
      action:
        type: skill | agent | mcp | tool | prompt
        target: <CardRef|AgentRef>
      depends_on: [<step-id>, ...]
      inputs: { <string>: <ParameterValue> }
      condition: <string?>
      timeout_seconds: <int?>
      retry: { max_retries: <int>, initial_backoff_ms: <int?> }
  outputs: { <string>: <expression> }
  governance: <Governance?>
  observation_hooks: <ObservationHooks?>
```

**Required fields:** `steps` (each with id + action).
**CardRefs:** every `step.action.target` → a Card of the matching kind.
**Validation:** step ids MUST be unique; `depends_on` MUST reference existing
step ids; the step graph MUST be a DAG.
**Worked example:** `03-rag-workflow-service.yaml`, `05-…`.
**Runtime emissions:** `card.Workflow.run.started | step.started | step.failed
| step.finished | run.finished | run.failed`.

## II.9 Operator

> **Purpose.** Declares a server-side, policy-gated compute step.

**YAML skeleton:**
```yaml
apiVersion: wyrd/v1
kind: Operator
metadata: { ... }
spec:
  adapter:
    name:    <string>                 # adapter name
    version: <string>
    config:  { ... }
  inputs:
    - { name: <string>, schema_ref: <string> }
  pre_invoke:  [<CardRef→Policy>, ...]
  post_invoke: [<CardRef→Policy>, ...]
  budget:
    max_wall_seconds: <int?>
    max_memory_mb:    <int?>
    max_tool_calls:   <int?>
```

**Required fields:** `adapter`.
**CardRefs:** `pre_invoke → Policy`, `post_invoke → Policy`.
**Worked example:** `01-ml-prediction-service.yaml`.

## II.10 Service

> **Purpose.** Declares the deployable composition of other Cards. One Service
> = one process boundary.

**YAML skeleton:**
```yaml
apiVersion: wyrd/v1
kind: Service
metadata: { ... }
spec:
  description: <string?>
  service_type: api | mcp | agent | workflow | observability | batch
  components:
    - alias: <string>
      ref: <CardRef>
      source: { path: <string>, kind: <string> }    # optional dev source
      config: { ... }
      credential_refs: [<CredentialRef>, ...]
  entry_point: <string?>                       # e.g. pkg.module:obj
  deployment: { ... }                          # image, replicas, …
  runtime:
    kind: api | mcp | agent | workflow
    framework: <string?>
    mode: in_process
    strict: <bool?>
    config: { ... }
    policy:
      runtime_hooks: <bool>
  service_config: { ... }
  credential_refs: [<CredentialRef>, ...]
```

**Required fields:** `components`.
**CardRefs:** `components[*].ref` → any Card kind permitted by §III.1.
**Validation:** component aliases MUST be unique within the spec.
**Worked example:** every spec in `architecture/specs/`.
**Runtime emissions:** `card.Service.run.started | finished | failed`.
**Open:** OPEN-A14, OPEN-A15.

## II.11 Skill

> **Purpose.** Declares a reusable instruction set referencing prompts and tools.

**YAML skeleton:**
```yaml
apiVersion: wyrd/v1
kind: Skill
metadata: { ... }
spec:
  description: <string?>
  prompt_refs: [<CardRef→Prompt>, ...]
  tool_refs:   [<CardRef→Tool>, ...]
  input_schema:  <JSONSchema?>
  output_schema: <JSONSchema?>
```

**Worked example:** `06-multi-agent-service.yaml`.

## II.12 Mcp

> **Purpose.** Declares an MCP server registration.

**YAML skeleton:**
```yaml
apiVersion: wyrd/v1
kind: Mcp
metadata: { ... }
spec:
  description: <string?>
  server_name: <string>
  transport:   stdio | http+sse | http+streamable
  tool_refs:   [<CardRef→Tool>, ...]
  scopes:      [<string>, ...]
  details:     { ... }
```

**Worked example:** `06-multi-agent-service.yaml`.

## II.13 Policy

> **Purpose.** Declares a rule set evaluated at Wyrd control points.

**YAML skeleton:**
```yaml
apiVersion: wyrd/v1
kind: Policy
metadata: { ... }
spec:
  description: <string?>
  rules:
    - name: <string>
      expression: <string>            # policy DSL (out of scope here)
      action: allow | warn | block | approval_required
      metadata: { ... }
  enforcement: pre_invoke | post_invoke | both
```

**Worked example:** `02-llm-agent-service.yaml`, `06-multi-agent-service.yaml`.
**Open:** OPEN-A16 (closed `enforcement` enum), OPEN-A17 (policy selectors).

## II.14 Audit

> **Purpose.** Declares an audit scope binding subjects, governing policies,
> and evidence.

**YAML skeleton:**
```yaml
apiVersion: wyrd/v1
kind: Audit
metadata: { ... }
spec:
  description:   <string?>
  subject_refs:  [<CardRef>, ...]
  policy_refs:   [<CardRef→Policy>, ...]
  evidence_refs: [<CardRef→{Eval|Drift|Artifact|Experiment}>, ...]
  details:       { ... }
```

**Worked example:** `02-…`, `06-…`.
**Open:** OPEN-A18 (constrain edge kinds).

## II.15 Drift

> **Purpose.** Declares a drift monitor: method, profile, baseline, targets,
> features, thresholds.

**YAML skeleton:**
```yaml
apiVersion: wyrd/v1
kind: Drift
metadata: { ... }
spec:
  method: Spc | Psi | Custom | Agent | External
  profile:                          # method-specific
    type: <same as method>
    config: { ... }
  baseline_ref: <CardRef→Data?>
  target_refs:  [<CardRef>, ...]    # what this monitor watches
  features:     [<string>, ...]
  thresholds:   { <metric>: <number> }
```

**Required fields:** `method`.
**Validation:** monitor declares targets (not vice-versa); `target_refs` MUST
be non-empty.
**Worked example:** `01-…`, `04-…`.
**Runtime emissions:** `card.Drift.observation.recorded | alert.fired`.

## II.16 Eval

> **Purpose.** Declares an evaluation harness: type, targets, judges,
> assertions, pass gates, datasets, profile, observation hooks, governance.

**YAML skeleton:**
```yaml
apiVersion: wyrd/v1
kind: Eval
metadata: { ... }
spec:
  description: <string?>
  eval_type:   Assertion | Judge | Benchmark | Agentic | Custom
  target_refs: [<CardRef>, ...]
  judge_refs:  [<CardRef→Prompt>, ...]
  assertions:  [ { name, rule, metadata } ]
  pass_gates:  [ { name, operator, threshold } ]
  dataset_refs: [<CardRef→Data>, ...]
  profile:                          # eval_type-specific
    type: <same as eval_type>
    config: { ... }
  default_parameters: { <string>: ParameterValue }
  governance:        <Governance?>
  observation_hooks: <ObservationHooks?>
  details: { ... }
```

**Worked example:** `02-…`, `03-…`.
**Open:** OPEN-A20 (flat vs profile), OPEN-A21 (eval-as-task-workflow).

## II.17 Trigger

> **Purpose.** Declares a source-driven trigger that fires a target Card on a
> cooldown.

**YAML skeleton:**
```yaml
apiVersion: wyrd/v1
kind: Trigger
metadata: { ... }
spec:
  source:
    kind: drift_observation | eval_observation | schedule
    card: <CardRef>                  # for drift_/eval_observation
    cron: <string>                   # for schedule
  target: <CardRef>                  # currently → Operator only
  cooldown_seconds: <int?>
  config: { ... }
```

**Worked example:** `01-ml-prediction-service.yaml`.
**Open:** OPEN-A22 (target kinds), OPEN-A23 (source taxonomy).

## II.18 Experiment

> **Purpose.** Declares a training or evaluation experiment: inputs (datasets,
> models, prompts), parameters, recorded metrics.

**YAML skeleton:**
```yaml
apiVersion: wyrd/v1
kind: Experiment
metadata: { ... }
spec:
  description: <string?>
  input_refs:  [<CardRef>, ...]
  output_refs: [<CardRef→{Model|Artifact}>, ...]
  parameters:  { <string>: ParameterValue }
  metrics:     [<MetricEntry>, ...]
```

---
---

# Part III — Composition flows

This part describes how Cards compose to produce the declarative behavior of
a Wyrd installation.

## III.1 Service composition

A `Service` is the only Card that composes other Cards.

| Component kind  | Role in service                                  |
|-----------------|--------------------------------------------------|
| Model           | Callable behind a runtime kind.                  |
| Agent           | Agent loop behind a runtime kind.                |
| Workflow        | Workflow behind a runtime kind.                  |
| Tool            | Adds a tool to the service's tool set.           |
| Prompt          | Available to the service's runtime.              |
| Mcp             | Adds MCP-sourced tools.                          |
| Skill           | Reusable instruction set.                        |
| Policy          | Runtime policy gate.                             |
| Service         | Observability sink or sub-service.               |
| Drift           | Service-scope monitor.                           |
| Eval            | Service-scope eval.                              |
| Audit           | Audit scope coverage.                            |

Component aliases are **service-local** and MUST NOT be presented as CardNames
in any surface.

### III.1.1 Service is itself a valid monitor target

A `Service` Card is a valid `target_refs` entry for `Drift`, `Eval`, and
`Audit`. Monitors targeting a service observe every observation produced by
any component of that service.

## III.2 Governance composition

Policy applies at three scopes:

| Scope | How declared |
|-------|--------------|
| Card  | `governance.policy_refs` on the governed Card's spec. |
| Kind  | (proposed — OPEN-A17) |
| Space | (proposed — OPEN-A17) |

### III.2.1 Effective policy set

For a run of Card `X` inside Service `S`, the effective set is the union of
Card-scope, Service-scope, Kind-scope, and Space-scope policies. A `block`
decision from any policy MUST abort the run.

### III.2.2 Runtime policy hooks

`ServiceSpec.runtime.policy.runtime_hooks: bool` toggles per-invocation
policy evaluation. When `false`, only registration-time policy applies.

## III.3 Monitor composition

The Wyrd Protocol locks the **monitor-declares-target** direction:

- A `Drift` or `Eval` Card declares `target_refs`.
- The monitored Card does **not** declare its monitors.
- A monitor targeting a `Service` observes every observation emitted by any
  component of that service.

## III.4 Trigger composition

A `Trigger` fires a `target` Card when its `source` emits an observation
matching its predicates. Triggers MUST honor `cooldown_seconds` across all
firings of the same trigger.

## III.5 Observation routing

Observation routing is declared via `ObservationHooks.route_refs` on Cards
that emit runs. Routing targets are `Service` Cards with
`service_type: observability` (OPEN-A8). Tracing context (OTel-compatible)
flows alongside observations; see Part VI.

---
---

# Part IV — Resource operations

This part describes the operations a Wyrd-compatible server presents on Card
resources. All operations are described in transport-independent form; the
HTTP+JSON binding (Part VIII) provides URL paths and verbs.

## IV.1 Operation taxonomy

The Wyrd Protocol defines the following operations on Card resources:

| Operation       | Description |
|-----------------|-------------|
| `cards.create`  | Register a new Card. Server assigns `uid`, derives `relationships`, sets initial `status`. |
| `cards.get`     | Retrieve one Card by `(space, kind, name, version)` or `uid`. |
| `cards.list`    | List Cards, filtered by kind, space, labels, and optional ref-graph predicates. |
| `cards.watch`   | Subscribe to a stream of Card create/update/delete events matching a filter. |
| `cards.update`  | Update the spec of an existing Card. By default, updates are **rejected** — versions are immutable (§I.3.2). A server MAY support spec-edit on draft Cards as an extension. |
| `cards.delete`  | Remove a Card. Servers MAY soft-delete (set `status.phase = archived`) or hard-delete; the choice is implementation-defined. |
| `cards.dry_run` | Validate a Card without registering it. Returns the same errors as `create` would. |

In addition, every server MUST support:

| Operation               | Description |
|-------------------------|-------------|
| `lineage.neighbors`     | Inbound and outbound relationships for one Card. |
| `lineage.between`       | Paths between two Cards. |
| `observations.publish`  | Emit an observation. Reserved for trusted runtime services. |
| `observations.subscribe`| Subscribe to the observation stream (Part VI). |

## IV.2 Idempotency

`cards.create` MUST be **conditionally idempotent**:

- A second create with the same `(space, kind, name, version)` and an
  identical canonicalized spec MUST be a no-op (returns the existing Card).
- A second create with the same key but a different spec MUST be rejected
  with a structured "conflict" error.

Servers MAY accept an `Idempotency-Key` header (or transport equivalent) so
clients can retry safely; if present, the server MUST replay the prior
response.

## IV.3 Optimistic concurrency

The server-assigned `uid` plus `metadata.spec_hash` form an etag for a Card.
Update operations MUST accept an "if-match" precondition; if the etag does
not match the server state, the update MUST be rejected.

## IV.4 Status subresource

`status` is **server-managed**. Authors MUST NOT write `status`. Servers MAY
expose a `cards.update_status` operation for trusted runtime services; if
present, it MUST require a scope distinct from `cards.update`.

## IV.5 Server-side filtering

`cards.list` MUST accept:

- `space`         — exact match
- `kind`          — exact match or array
- `labels`        — selector expressions (`team=ml`, `domain in (a,b)`)
- `name_prefix`   — prefix match
- `version_range` — semver range (RFC9421-style; protocol revision MAY tighten)
- `refers_to`     — Cards whose spec contains a CardRef to the given Card

Servers MAY support additional filters; clients MUST NOT depend on them.

## IV.6 Bulk operations

A Wyrd-compatible server MUST support bulk submission via
`cards.create_many` (atomic-per-Card; the bulk operation is **not**
transactional across Cards). The server MUST resolve and validate refs across
the bulk before committing, so that a bulk containing two Cards with a
cross-ref between them registers cleanly.

---
---

# Part V — Discovery and capabilities

## V.1 The capability descriptor

A Wyrd-compatible server MUST expose a **capability descriptor** advertising
the protocol version, supported kinds, supported operations, supported
extensions, and supported transport bindings.

```json
{
  "protocol":     "wyrd",
  "api_version":  "wyrd/v1",
  "server_version": "1.0.0",
  "capabilities": {
    "kinds":        ["Data", "Model", "Prompt", "Tool", "Agent", "Workflow",
                     "Eval", "Drift", "Service", "Policy", "Mcp", "Skill",
                     "SubAgent", "Audit", "Artifact", "Trigger", "Operator",
                     "Experiment"],
    "external_kinds": [],
    "operations":   ["cards.create", "cards.get", "cards.list", "cards.watch",
                     "cards.delete", "cards.dry_run",
                     "lineage.neighbors", "lineage.between",
                     "observations.publish", "observations.subscribe"],
    "extensions":   ["wyrd.io/spec-edit-draft",
                     "wyrd.io/bulk-create"],
    "transports":   ["http+json", "mcp", "a2a-bridge"]
  },
  "authn": {
    "schemes": ["bearer", "mtls"]
  }
}
```

### V.1.1 Discovery requirements

- The capability descriptor MUST be accessible via every supported transport.
- HTTP+JSON binding: `GET /v1/protocol` returns the descriptor.
- MCP binding: served as a resource `wyrd://protocol`.
- The descriptor MUST be a stable canonical JSON for a given server state.

### V.1.2 Extension naming

- Extensions MUST be named with a DNS-style prefix.
- `wyrd.io/*` is RESERVED for Wyrd-protocol extensions.
- Vendor extensions MUST use vendor DNS (`acme.com/feature-name`).

## V.2 Kind catalog

Clients MAY enumerate `kinds/{Kind}/schema` to retrieve the JSON Schema for
a kind's spec. The schemas returned MUST be equivalent to those produced by
the Rust reference implementation's schema generator (Appendix C).

## V.3 Negotiation

When a client connects, the client SHOULD fetch the capability descriptor
first and pin behavior to the advertised `api_version`. Clients MUST tolerate
unknown extensions and unknown kinds in returned payloads (per Postel's law);
clients MUST NOT send extensions or kinds the server has not advertised.

---
---

# Part VI — Observation stream

## VI.1 The observation envelope

> **Status:** the observation envelope is a placeholder until OPEN-A24 is
> resolved. This section captures the protocol-level requirements; the wire
> shape is a candidate, not locked.

```yaml
observation:
  schema:       wyrd/v1
  observation_id: <ULID>
  emitted_at:   <RFC3339>
  emitter:
    kind:       <CardKind>          # what kind produced this observation
    ref:        <CardRef>           # the producing Card
    service_ref: <CardRef?>          # service composition context
  run:
    run_id:     <ULID?>             # if attached to a run
    parent_run_id: <ULID?>
  event:        <EventName>         # from the closed taxonomy (OPEN-A7)
  payload:      <kind-specific>     # event-shape-defined
  trace:
    trace_id:   <hex32>             # OTel-compatible
    span_id:    <hex16>
    parent_span_id: <hex16?>
  attributes:   { <string>: <NonSecretValue> }
```

## VI.2 Subscription model

Clients subscribe via `observations.subscribe` with a filter:

```json
{
  "filter": {
    "kinds":        ["Service", "Model"],
    "card_refs":    [ ... ],
    "events":       ["card.Service.run.failed"],
    "trace_id":     "..."
  },
  "cursor": "<opaque cursor>"
}
```

- Servers MUST support cursor-based resumption.
- Cursors MUST be opaque to clients.
- A cursor MUST address a point-in-time such that the same cursor returns the
  same logical resume position (subject to retention).

## VI.3 Ordering and delivery

- The Wyrd Protocol does **not** guarantee global ordering across emitters.
- Within a single `run_id`, observations MUST be delivered in monotonically
  non-decreasing `emitted_at` order.
- Delivery is **at-least-once**. Subscribers MUST be idempotent on
  `observation_id`.

## VI.4 Tracing

Every observation MUST carry an OTel-compatible trace context. The Wyrd
Protocol does **not** specify how spans are constructed within a runtime
service; it specifies that every observation MUST be join-able with
OTel-collected spans via `trace.trace_id`.

## VI.5 Publishing observations

`observations.publish` is reserved for **trusted runtime services**:
implementations of `Service`, `Agent`, `Workflow`, `Eval`, `Drift`. The
operation is not directly exposed to user code; user code emits observations
indirectly by invoking a Wyrd-compatible runtime that publishes them.

---
---

# Part VII — Authorization

## VII.1 Actor model

Every operation is performed by an `Actor`:

```yaml
actor:
  kind:    user | service | agent | system
  id:      <string>                # identity within the actor's namespace
  display_name: <string?>
  attributes: { <string>: <NonSecretValue> }
```

Actors are conveyed in the request context (e.g., via bearer token claims).
The protocol does not specify token format; bindings do (Part VIII).

## VII.2 Scope catalog

The Wyrd Protocol defines a closed set of scopes:

| Scope                       | Permits                                        |
|-----------------------------|------------------------------------------------|
| `cards:read`                | get, list, watch                               |
| `cards:write`               | create, dry_run                                |
| `cards:write:status`        | update_status (trusted runtime)                |
| `cards:write:<Kind>`        | restrict write to a kind, e.g. `cards:write:Policy` |
| `lineage:read`              | lineage.neighbors, lineage.between             |
| `observations:read`         | observations.subscribe                         |
| `observations:write`        | observations.publish (trusted runtime)         |
| `policy:approve:<scope>`    | satisfy `approval_requirements` named `<scope>`|
| `protocol:read`             | capability descriptor and kind catalog         |

Custom (extension) scopes MUST be DNS-prefixed (`acme.com/feature:read`).

## VII.3 Space scoping

A grant MAY be scoped to a `space`. Operations on a Card MUST satisfy a grant
covering that Card's space.

## VII.4 Audit trail

Every operation that mutates Wyrd state MUST be recorded in **audit history**
(service-owned, not protocol-defined). The audit record MUST include actor,
operation, target, decision (incl. policy outcome), and trace context.

---
---

# Part VIII — Transport bindings

The Wyrd Protocol is transport-independent. This part defines bindings.

## VIII.1 HTTP+JSON (normative baseline)

Every Wyrd-compatible server MUST implement the HTTP+JSON binding.

### VIII.1.1 Base path and versioning

- Base path: `/v1/`
- All resources are under `/v1/spaces/<space>/cards/<kind>/<name>/<version>`.
- Default-space shortcut: `/v1/cards/<kind>/<name>/<version>`.
- The capability descriptor: `GET /v1/protocol`.

### VIII.1.2 Operation map

| Operation                | HTTP                                                      |
|--------------------------|-----------------------------------------------------------|
| cards.create             | `POST /v1/cards`                                          |
| cards.create_many        | `POST /v1/cards:bulk`                                     |
| cards.get                | `GET /v1/spaces/<space>/cards/<kind>/<name>/<version>`    |
| cards.list               | `GET /v1/cards?…filters…`                                 |
| cards.watch              | `GET /v1/cards:watch?…filters…` (SSE)                     |
| cards.update             | `PATCH /v1/spaces/<space>/cards/<kind>/<name>/<version>`  |
| cards.delete             | `DELETE /v1/spaces/<space>/cards/<kind>/<name>/<version>` |
| cards.dry_run            | `POST /v1/cards:dryRun`                                   |
| lineage.neighbors        | `GET /v1/lineage/neighbors?ref=…`                          |
| lineage.between          | `GET /v1/lineage/between?from=…&to=…`                      |
| observations.subscribe   | `GET /v1/observations:watch` (SSE)                        |

### VIII.1.3 Errors

Errors use the **RFC 7807 Problem Details** shape, with the Wyrd error
catalog providing `type`, `title`, and `code`. Servers MUST set:

```json
{
  "type":     "https://wyrd.io/errors/<code>",
  "title":    "<title>",
  "status":   <int>,
  "code":     "<code>",
  "detail":   "<human-readable>",
  "trace_id": "<hex32>",
  "remediation": "<string>",
  "details":  { ... }
}
```

The error code catalog is canonicalized via the Rust derive
`wyrd_spec::error::WyrdError`. New codes MUST be added through the catalog.

### VIII.1.4 Content negotiation

- `Content-Type: application/json` — JSON wire (default).
- `Content-Type: application/yaml` — YAML wire (RECOMMENDED accepted for
  authoring tools, OPTIONAL on responses).
- `Accept: text/event-stream` — required for watch endpoints.

### VIII.1.5 Authentication

The HTTP+JSON binding MUST accept `Authorization: Bearer <token>`. Token
shape is implementation-defined. mTLS is OPTIONAL.

## VIII.2 MCP bridge

A Wyrd-compatible MCP server MAY expose:

- **Resources** for each Card kind. URI pattern:
  `wyrd://<space>/<kind>/<name>@<version>`.
- **Tools** corresponding to Wyrd operations, namespaced `wyrd.<operation>`:
  `wyrd.cards.create`, `wyrd.cards.list`, `wyrd.lineage.neighbors`, etc.
- **Tool input schemas** derived from the operation contract.
- **Resource subscriptions** for `cards.watch` and `observations.subscribe`,
  bridged to MCP `notifications/resources/updated`.

### VIII.2.1 MCP capability negotiation

The Wyrd-as-MCP-server MUST advertise the capability `wyrd.io/control-plane:
v1` in its MCP `initialize` response.

## VIII.3 A2A interop

A Wyrd `Service` whose `runtime.kind == agent` MAY publish an **A2A
AgentCard** at its runtime endpoint. The A2A AgentCard is **runtime
metadata**, not a Wyrd Card; it carries A2A capabilities, skills, and
endpoints derived from the Wyrd `Service` spec.

The Wyrd Protocol does not define the A2A AgentCard shape; see the A2A spec.

## VIII.4 gRPC (informative)

A gRPC binding MAY be defined in a future revision of this protocol. The
existing HTTP+JSON binding is sufficient for v1 conformance.

---
---

# Part IX — Lifecycle, lineage, versioning, conformance

## IX.1 Registration lifecycle

`cards.create` on a Card MUST produce one of:

| Outcome              | Server behavior                                                          |
|----------------------|--------------------------------------------------------------------------|
| Accepted             | Card is stored, `uid` assigned, `relationships` derived, `status.phase` set. |
| Policy-blocked       | At least one registration-time policy returned `deny`. Card is not stored. |
| Validation-rejected  | Structural validation failed. Card is not stored.                       |
| Conflict             | A different spec exists for `(space, kind, name, version)`.             |

## IX.2 Promotion and deprecation

Promotion and deprecation are **service-owned**. The protocol guarantees:

- `metadata.version` is immutable after first registration.
- `status.phase` MAY be updated by service-side flows.
- Promotion MUST produce a new Card version with refreshed status; it MUST
  NOT mutate the prior version's spec.

## IX.3 Lineage

The server constructs the lineage graph as:

```
nodes = { Card C : C is registered }
edges = { (A, B, role)
        : A.spec contains a CardRef R, R resolves to B
        ; role = qualified spec-field path that contained R }
```

The lineage service MUST support `ancestors`, `descendants`, `between`, and
`by_role` queries. Lineage is **derived** — authors MUST NOT write edges.

## IX.4 External kinds

Servers MAY accept External kinds. The server:

- MUST store the External spec as opaque JSON.
- MUST verify `schema_hash` is canonical sha256 over the external schema.
- MUST NOT silently coerce External `<name>` to a native kind.
- MAY reject External names colliding with reserved native kind names.
- MUST NOT return External Cards as native kinds in lineage queries.

The RECOMMENDED integration pattern for external registries (MLflow,
SageMaker, etc.) is **not** External kinds; it is `Artifact.external_uri +
FrameworkAdapterRef` (§I.7).

## IX.5 Protocol versioning

- `apiVersion` is currently fixed at `wyrd/v1`.
- A new API version MUST be introduced when a breaking change is required.
- The protocol uses **API-group versioning** in the Kubernetes style: a single
  Wyrd installation MAY serve multiple API versions simultaneously, with
  per-Card conversion handled by the registry.

### IX.5.1 Deprecation policy

- A field, kind, scope, or operation marked DEPRECATED MUST remain in the
  protocol for at least one minor revision after deprecation.
- Deprecations MUST be reflected in the capability descriptor's
  `deprecations` field.
- Removal MUST be accompanied by a protocol-version bump.

## IX.6 Conformance tiers

A Wyrd-compatible implementation declares one of three conformance tiers:

### IX.6.1 Minimum conformance

A Minimum-conformant server:

- MUST serve the capability descriptor.
- MUST implement `cards.create`, `cards.get`, `cards.list`, `cards.dry_run`.
- MUST implement `lineage.neighbors`.
- MUST implement HTTP+JSON binding.
- MUST implement at least the **eight core kinds**: `Data`, `Model`, `Prompt`,
  `Tool`, `Agent`, `Service`, `Policy`, `Artifact`.
- MAY omit observation streams.

### IX.6.2 Standard conformance

A Standard-conformant server (RECOMMENDED for production):

- MUST satisfy Minimum.
- MUST implement all 18 native kinds.
- MUST implement `cards.watch`, `lineage.between`, `observations.subscribe`.
- MUST implement RFC 7807 errors with the Wyrd error catalog.
- MUST implement the closed scope catalog (Part VII §VII.2).

### IX.6.3 Full conformance

A Full-conformant server:

- MUST satisfy Standard.
- MUST implement `observations.publish`.
- MUST implement External kinds.
- MUST implement the MCP bridge (Part VIII §VIII.2).
- SHOULD implement `cards.create_many`.

## IX.7 Conformance invariants (data model)

Every Wyrd surface (SDK, CLI, UI, MCP, codegen) MUST preserve:

| # | Invariant |
|---|-----------|
| C1  | Envelope shape `apiVersion / kind / metadata / spec / relationships / status`. |
| C2  | `kind` flat on envelope; no nesting under `spec` or `body`. |
| C3  | Authors do not write `relationships` or `status`. Servers silently drop client-supplied values. |
| C4  | `CardRef` is the only authored pointer between Cards. |
| C5  | Lineage is derived from spec CardRefs. No authored-edge API. |
| C6  | Policy applies at the scopes in §III.2. |
| C7  | Observations route via `ObservationHooks.route_refs`. |
| C8  | Triggers fire on declared sources only. |
| C9  | 18 native kinds are closed for v1. |
| C10 | External kinds are opaque to the registry. |

---
---

# Part X — Authoring guide

This part is **non-normative**. It is a how-to for humans and agents writing
Wyrd YAML.

## X.1 Your first spec

The minimum viable Card:

```yaml
apiVersion: wyrd/v1
kind: Data
metadata:
  name: my-first-dataset
  version: "0.1.0"
spec:
  interface: { kind: parquet, meta: { uri: s3://bucket/dataset/ } }
  schema:
    columns:
      - { name: id, dtype: string, nullable: false }
  stats: { kind: local, meta: { rows: 0, bytes: 0 } }
```

Save as `my-dataset.yaml` and submit via your Wyrd-compatible client.

## X.2 Common patterns

### X.2.1 Linking artifacts to models

```yaml
# Artifact card
spec:
  artifact_kind: sklearn_pickle
  artifact_uris: [s3://acme-models/clf.pkl]
  integrity: sha256:…
---
# Model card
spec:
  interface: { kind: Sklearn, meta: { sklearn_estimator: …, sklearn_version: … } }
  card_refs:
    - { kind: Artifact, name: clf-bytes, version: "1.0.0" }
```

### X.2.2 Bridging an MLflow-registered model

Use `Artifact.external_uri` + `FrameworkAdapterRef`. See worked example
`04-external-mlflow.yaml`.

### X.2.3 Composing a service

```yaml
kind: Service
spec:
  components:
    - { alias: model, ref: { kind: Model, name: classifier, version: "1.0.0" } }
    - { alias: pii,   ref: { kind: Policy, name: pii, version: "1.0.0" } }
```

### X.2.4 Monitoring a service

Declare `Drift.target_refs` pointing at the `Service` Card. The monitor
catches every observation from any component.

### X.2.5 Routing observations to Datadog

Declare a `Service` Card with `service_type: observability` and
`runtime.framework: datadog`. Then reference it from the producer's
`observation_hooks.route_refs`. See worked example
`05-observability-datadog.yaml`.

## X.3 Anti-patterns

- **Hard-coding secrets in spec.** Use `CredentialRef`.
- **Re-declaring the same Card across spaces with version `0.0.1`.** Use
  semver; `space` is for isolation, not version overloading.
- **Using `External` kinds for MLflow/SageMaker.** Use Artifact pattern.
- **Inventing a "Card kind" for a tool.** It is a `Tool` Card.
- **Writing `relationships` on author submission.** Server overwrites.
- **Inventing event names ad hoc.** Use the (forthcoming) closed taxonomy.

## X.4 Where to look up fields

Every kind's anatomy is in **Part II**. Cross-cutting flows are in **Part
III**. Worked examples are linked from each kind and from Appendix B.

---
---

# Appendix A — Conflicts catalog (open issues)

Resolution of each issue MUST update both this catalog and the affected
sections. New issues append to the end with the next letter.

| ID | Topic | Affected sections | Recommendation |
|----|-------|-------------------|----------------|
| A1 | Edge type on derived relationships | §I.4.2 | Add `derived_edges: Vec<{from, to, role}>` alongside display strings. |
| A2 | Closed `status.phase` set | §I.5 | Enum `{ draft, active, deprecated, archived }`. |
| A3 | `Governance` placement | §I.6.1, §II.4, §II.6, §II.10, §II.15 | Promote to envelope-adjacent block. |
| A4 | `ObservationHooks` placement | §I.6.2, §II.4, §II.6, §II.10, §II.15 | Promote to envelope-adjacent block. |
| A5 | Closed event taxonomy | §I.6.2, §VI.1 | Lock the taxonomy. Draft in §VI.1. |
| A6 | Unify `NonSecretValue` and `ParameterValue` | §I.6.5 | Single `Value` type. |
| A7 | (reserved) | | |
| A8 | Constrain `route_refs` target kinds | §I.6.2, §III.5 | `Service` with `service_type: observability`. |
| A10 | Closed `tool_type` enum | §II.5 | `{ Script, Api, Mcp, Builtin, Workflow, Agent }`. |
| A11 | Tool → MCP via CardRef | §II.5 | Replace `mcp_server_name: String` with `mcp_ref: CardRef→Mcp`. |
| A12 | Agent → Tool via CardRef | §II.6 | Replace `tool_names: Vec<String>` with `tool_refs: Vec<CardRef→Tool|Mcp>`. |
| A14 | Closed `service_type` enum | §II.10, §III.5 | `{ Api, Mcp, Agent, Workflow, Observability, Batch }`. |
| A15 | Service `components` typed roles | §II.10, §III.1 | Split into typed slots `uses / monitored_by / governed_by / audited_by / routes_to` (preferred) or add closed `role` discriminator. |
| A16 | Closed `Policy.enforcement` enum | §II.13 | Tied to control-point taxonomy. |
| A17 | Policy scope (Kind, Space) | §II.13, §III.2 | Add `selector: PolicySelector` (space, kind, labels). |
| A18 | Audit edge constraints | §II.14 | Constrain `subject_refs`, `policy_refs`, `evidence_refs` to listed kinds. |
| A19 | Closed `Artifact.artifact_kind` taxonomy | §II.2 | Wyrd-owned taxonomy + extension via `framework_adapter.name`. |
| A20 | EvalSpec flat fields vs `profile` | §II.16 | Fold flat fields into per-variant `profile`. |
| A21 | Eval as workflow of tasks | §II.16 | Add `tasks: Map<TaskId, EvalTask>` with DAG inside `EvalSpec`. |
| A22 | Trigger target kinds | §II.17, §III.4 | `target.kind ∈ { Operator, Workflow, Eval }`. |
| A23 | Trigger source taxonomy | §II.17, §III.4 | Add `service_observation`, `policy_observation`, `external_signal`, `run_finished`. |
| A24 | Observation envelope lock | §VI.1 | Companion document `wyrd-observation-protocol.md`. |
| A25 | Service-level observations | §III.5, §VI.1 | Standard `service.run.*` events; SLO via Drift/Eval targeting service. |
| A26 | `ServiceCard.entry_point` is a Python symbol | §II.10 | Typed `EntryPoint { language, locator }`. |
| A27 | CardName vs ServiceComponent alias namespacing | §II.10, §III.1 | Aliases are service-local; surfaces MUST NOT present aliases as CardNames. |
| A28 | Drift + Eval unification (`Monitor` kind) | §II.15, §II.16 | Defer to v2; share a Targeting trait. |

---
---

# Appendix B — Worked-example cross-reference

| Spec file | Exercises |
|-----------|-----------|
| `01-ml-prediction-service.yaml` | §II.1 Data, §II.4 Model, §II.2 Artifact, §II.10 Service, §II.15 Drift, §II.17 Trigger, §II.9 Operator |
| `02-llm-agent-service.yaml` | §II.3 Prompt, §II.5 Tool, §II.6 Agent, §II.10 Service, §II.13 Policy, §II.16 Eval, §II.14 Audit |
| `03-rag-workflow-service.yaml` | §II.1 Data, §II.5 Tool, §II.3 Prompt, §II.6 Agent, §II.8 Workflow, §II.16 Eval, §II.10 Service |
| `04-external-mlflow.yaml` | §II.2 Artifact (external_uri pattern), §II.4 Model (Custom interface), §II.15 Drift, §II.10 Service |
| `05-observability-datadog.yaml` | §I.6.2 ObservationHooks, §II.10 Service (observability), §II.8 Workflow, §II.7 SubAgent |
| `06-multi-agent-service.yaml` | §II.11 Skill, §II.12 Mcp, §II.3 Prompt, §II.6 Agent, §II.13 Policy, §II.10 Service, §II.14 Audit |

---
---

# Appendix C — Schema generation

The Wyrd Protocol publishes machine-readable schemas:

- **JSON Schema per Card kind:** generated by the Rust reference
  implementation (`schemars::JsonSchema` derive in `wyrd-spec`). Available
  at the kind-catalog endpoint (§V.2).
- **OpenAPI document** for the HTTP+JSON binding: derived from the Rust
  reference implementation's request/response types (`utoipa` derive under
  the `server` feature).
- **Capability descriptor JSON Schema:** part of this protocol; published at
  `/v1/protocol/schema`.

Authors writing tooling (editor completion, linters, codegen) SHOULD use
these published schemas rather than parsing this document.
