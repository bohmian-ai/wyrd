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
3. **Monitors declare targets.** Drift, Eval, Audit reference what they
   observe. Targets do not list their observers.
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

---

## Kind catalog

19 native kinds + `External { name, schema_hash }` for forward-compat.

| Domain        | Kinds |
|---------------|-------|
| Data plane    | Data, Model, Artifact, Experiment |
| Agent plane   | Prompt, Tool, Agent, Workflow, SubAgent, Skill, Mcp |
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

### Tool
LLM-visible tool descriptor.
```yaml
spec:
  name: string
  description: string
  tool_type: string              # script | api | mcp | builtin
  args_schema: json
  output_schema?: json
  script_config: { string: NonSecretValue }
  api_config: { string: NonSecretValue }
  credential_refs: [CredentialRef]
  mcp_server_name?: string
  allowed_tools: [string]
  requires_approval: bool
  hook_events: [string]
  hook_matcher: { string: NonSecretValue }
  details: { string: NonSecretValue }
```

### Agent
Agent contract: prompt + tools + run config.
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

### SubAgent
Headless sub-agent definition for harness consumption.
```yaml
spec:
  description?: string
  prompt?: string
  model?: string
  tool_refs: [CardRef]
  disallowed_tools: [string]
  skill_refs: [CardRef]          # → Skill
  max_turns?: u32
  permission_mode?: string
  memory: { string: NonSecretValue }
  background: bool
  effort?: string
  isolation?: string
  compatible_clis: [string]
```

### Skill
Reusable instruction + tool bundle.
```yaml
spec:
  description?: string
  prompt_refs: [CardRef]
  tool_refs: [CardRef]
  input_schema?: json
  output_schema?: json
  details: { string: NonSecretValue }
```

### Mcp
MCP server registration.
```yaml
spec:
  description?: string
  server_name: string
  transport?: string             # stdio | http | sse
  tool_refs: [CardRef]
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
Distribution monitor; declares its targets.
```yaml
spec:
  description?: string
  method: DriftMethod            # Spc | Psi | Custom | Agent | External
  profile?: DriftProfile
  baseline_ref?: CardRef         # → Data
  target_refs: [CardRef]         # → Model | Agent | Service | Data
  source_refs: [CardRef]         # → Source — historical data to query
  features: [string]
  thresholds: { string: f64 }
  details: { string: NonSecretValue }
```

### Eval
Behavioral assessment; declares its targets and pass gates.
```yaml
spec:
  description?: string
  eval_type: EvalType            # Assertion | Judge | Benchmark | Agentic | Custom
  target_refs: [CardRef]         # → Agent | Model | Workflow
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

---

## Reference-direction quick reference

| Card    | Refs that authored on it             | Refs that point at it          |
|---------|--------------------------------------|--------------------------------|
| Data    | `artifact_refs`, `splits`            | `Drift.baseline_ref`, `Eval.dataset_refs`, `Experiment.target_refs` |
| Model   | `artifact_refs`                      | `Drift.target_refs`, `Eval.target_refs`, `Service.components.ref`, `Experiment.target_refs` |
| Agent   | `prompt`, `tool_names`               | `Drift.target_refs`, `Eval.target_refs`, `Service.components.ref` |
| Workflow| `steps.*.target`                     | `Service.components.ref`, `Operator.workflow_ref` |
| Drift   | `target_refs`, `baseline_ref`, `source_refs` | `Trigger.source.card` |
| Eval    | `target_refs`, `judge_refs`, `dataset_refs`, `source_refs` | `Trigger.source.card` |
| Audit   | `subject_refs`, `policy_refs`, `evidence_refs`, `source_refs` | — |
| Service | `components[].ref`                   | `Drift.target_refs` (service-level), `Trigger.source.card` (via observations) |
| Policy  | `rules`                              | `Service.components.ref`, `Audit.policy_refs`, `Operator.pre_invoke`, `Operator.post_invoke` |
| Trigger | `source.card`, `target`              | — |
| Operator| `adapter`, `pre_invoke`, `post_invoke` | `Trigger.target` |
| Source  | `credential_ref`                     | `Drift.source_refs`, `Eval.source_refs`, `Audit.source_refs` |

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
3. Closed event-name taxonomy for internal `vala` observations.
4. Format negotiation for `object_store` Source — schema-on-read vs registered
   schema reference.
5. Time-window semantics for how Drift/Eval cards describe the read range
   over `source_refs`.
6. Service-level Drift target semantics — what "drift on a Service" computes
   when Wyrd reads internal traces vs external Sources.
7. Default Source binding at the Service or Agent level to avoid repeating
   `source_refs` on every Drift/Eval.
8. Audit `source_refs` vs `evidence_refs` boundary — Source is queryable
   history; `evidence_refs` are concrete card pointers.
