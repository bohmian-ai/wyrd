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
    `TriggerSource::Schedule`. Dispatch lives on `Operator`. There is no
    `Alert` kind — alerting is an Operator with a notification adapter.

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
  prompt: PromptRef              # CardRef or inline prompt
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
  content_hash?: string
  lock_hash?: string
  metadata: { string: NonSecretValue }
```

### Policy
Runtime / governance gate rules.
```yaml
spec:
  description?: string
  rules: [PolicyRule]            # { name, expression, action, metadata }
  enforcement?: string           # pre_invoke | post_invoke | both
  details: { string: NonSecretValue }
```

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
Behavioral assessment for a single subject; declares pass gates.
```yaml
spec:
  description?: string
  eval_type: EvalType            # Assertion | Judge | Benchmark | Agentic | Custom
  subject_ref: CardRef           # → Agent | Model | Workflow — singular
  judge_refs: [CardRef]          # → Prompt | Agent
  assertions: [EvalAssertion]
  pass_gates: [EvalPassGate]
  dataset_refs: [CardRef]        # → Data
  source_refs: [CardRef]         # → Source — production runs/traces
  profile?: EvalProfile
  default_parameters: { string: ParameterValue }
  governance?: Governance
  details: { string: NonSecretValue }
```

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
Wires an observation source to an Operator target.
```yaml
spec:
  source: TriggerSource          # { kind: drift_observation | eval_observation | schedule, ... }
  target: CardRef                # → Operator
  cooldown_seconds?: u32
  config: { string: NonSecretValue }
```

### Operator
Reaction primitive; performs the action when a Trigger fires.
```yaml
spec:
  adapter: FrameworkAdapterRef
  inputs: [OperatorInput]
  pre_invoke: [CardRef]          # → Policy
  post_invoke: [CardRef]         # → Policy
  budget?: OperatorBudget
```

---

## Foundations

Shared types embedded into specs. Owned by `wyrd-spec`.

| Foundation          | Purpose |
|---------------------|---------|
| `CardRef`           | `{ kind, name, version, space?, uid? }` — the only authored pointer between Cards |
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
- Any `schedule` / `cron` / `alert_config` / dispatch fields on Drift or Eval
  — scheduling is `TriggerSource::Schedule`; dispatch is `Operator`. No
  scheduling or notification leaks onto observation cards.

---

## Reference-direction quick reference

| Card    | Refs that authored on it             | Refs that point at it          |
|---------|--------------------------------------|--------------------------------|
| Data    | `artifact_refs`, `splits`            | `Drift.signal.baseline_ref`, `Eval.dataset_refs`, `Experiment.target_refs` |
| Model   | `artifact_refs`                      | `Drift.subject_ref`, `Eval.subject_ref`, `Service.components.ref`, `Experiment.target_refs` |
| Agent   | `prompt`, `tool_names`               | `Drift.subject_ref`, `Eval.subject_ref`, `Service.components.ref`, Agent prompts (sub-agent calls) |
| Workflow| `steps.*.target`                     | `Eval.subject_ref`, `Service.components.ref`, `Operator.workflow_ref` |
| Mcp     | `credential_refs`                    | `Service.components.ref` |
| Drift   | `subject_ref`, `signal.*` (`baseline_ref` \| `eval_ref` \| `source_ref`) | `Trigger.source.card`, `Drift.signal.eval_ref` (other Drifts watching an Eval indirectly) |
| Eval    | `subject_ref`, `judge_refs`, `dataset_refs`, `source_refs` | `Trigger.source.card`, `Drift.signal.eval_ref` |
| Audit   | `subject_refs`, `policy_refs`, `evidence_refs`, `source_refs` | — |
| Service | `components[].ref`                   | `Drift.subject_ref` (service-level), `Eval.subject_ref`, `Trigger.source.card` (via observations) |
| Policy  | `rules`                              | `Service.components.ref`, `Audit.policy_refs`, `Operator.pre_invoke`, `Operator.post_invoke` |
| Trigger | `source.card`, `target`              | — |
| Operator| `adapter`, `pre_invoke`, `post_invoke` | `Trigger.target` |
| Source  | `credential_ref`                     | `Drift.signal.source_ref` (External variant), `Eval.source_refs`, `Audit.source_refs` |

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
│   ├── triage-eval.yaml         # source_refs → run-archive
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
   over `source_refs`.
5. Service-level Drift subject semantics — what "drift on a Service" computes
   when Wyrd reads internal traces vs external Sources, given the subject is
   singular.
6. Default Source binding at the Service or Agent level to avoid repeating
   `source_refs` on every Drift/Eval.
7. Audit `source_refs` vs `evidence_refs` boundary — Source is queryable
   history; `evidence_refs` are concrete card pointers.
8. Whether tool hook phases need a closed enum on `Policy.rules` or can stay
   off the wire entirely (no consumer today).
9. Whether `Audit.subject_refs` stays plural. An audit may genuinely cover a
   Service plus its component Agents; collapse to singular if real audits
   don't span multiple cards in practice.
10. Whether `Eval` should carry its own `signal` decomposition symmetric with
    Drift (dataset vs production-trace input edges), or if `dataset_refs` +
    `source_refs` already does the job.
