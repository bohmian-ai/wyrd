# Wyrd spec examples

> Superseded sandbox examples. `architecture/wyrd-design.md` is the active
> design authority. These examples still contain older 18-kind exploration and
> are not public doctrine for Tool, Skill, or SubAgent card kinds.

This folder is the iteration sandbox for the **declarative Wyrd envelope**.
Every YAML here is a complete, end-to-end scenario expressed in cards. The goal
is not to ship these files — it is to pressure-test the spec until the envelope
reads naturally and covers real-world ML/AI workflows without surface-specific
vocabulary.

## How to read these files

Each `.yaml` is a **multi-document YAML** (`---` separators). One document =
one card. The cards within a file describe a single deployable scenario:
data → model/agent → service → governance → observability.

The reading order inside every file is the same:

1. **Sources** — `Data`, `Prompt`, `Artifact`
2. **Behavior** — `Model`, `Tool`, `Agent`, `Workflow`
3. **Composition** — `Service` (binds the above by alias)
4. **Governance** — `Policy`, `Audit`
5. **Observation** — `Drift`, `Eval`, `Trigger`, observation `Service`s

## The envelope

Every card uses the same outer shape. From `crates/wyrd-spec/src/envelope.rs`:

```yaml
apiVersion: wyrd/v1
kind: <one of 18 native kinds>
metadata:
  name: <CardName>
  version: "<semver>"
  space: <optional space>
  labels: {}
  annotations: {}
spec: <kind-specific payload>
# server-derived; omit when authoring:
# relationships: { outbound: [], inbound: [] }
# status: null
```

`kind` is **flat** on the envelope. `spec` is **flat** at the same level. There
is no outer `kind: Card` wrapper, no nested `body`. Authors write `CardRef`s
inside specs; the server derives `relationships` from them.

The 18 native kinds:

```
Data, Model, Experiment, Prompt, Tool, Agent, Workflow,
Eval, Drift, Service, Policy, Mcp, Skill, SubAgent,
Audit, Artifact, Trigger, Operator
```

## What's in this folder

| File | Scenario |
|---|---|
| `01-ml-prediction-service.yaml`   | Sklearn churn classifier + DataCard + ServiceCard, PSI drift watching the model, Datadog observability routing, retrain trigger on drift |
| `02-llm-agent-service.yaml`       | Customer-support agent with two prompts, judge-based Eval running an evaluation workflow, PII policy gate |
| `03-rag-workflow-service.yaml`    | Corpus DataCard, retrieval ToolCard, WorkflowCard chaining retrieval → answer → eval, observation hooks |
| `04-external-mlflow.yaml`         | MLflow-registered model exposed to Wyrd via ArtifactCard.external_uri + FrameworkAdapterRef, ModelCard with custom interface |
| `05-observability-datadog.yaml`   | Datadog forwarder declared as a ServiceCard, ObservationHooks across other cards routing to it |
| `06-multi-agent-service.yaml`     | ServiceCard composing two agents and an MCP tool catalog, runtime PolicyCard, AuditCard, reusable SkillCard |

## Conventions used in these examples

- **Spaces**: `prod`, `staging`, `eval`. Drop the field to mean `default`.
- **Versions**: strict semver `MAJOR.MINOR.PATCH`. No `latest`, no range syntax.
- **Annotations**: user/vendor keys use a DNS-style prefix
  (`acme.com/cost-center`). The `wyrd.io/*` prefix is reserved.
- **CardRefs**: written as full structs in YAML (`kind/name/version`). The
  Python SDK and CLI may offer string shortcuts; the durable contract is
  always the structured form.
- **Library-agnostic**: nothing in these specs assumes a particular framework
  beyond what the typed interface declares (e.g. `ModelInterface::Sklearn`).
  External systems (MLflow, Datadog, Snowflake) come in through `Artifact`,
  external `Service`s, and `FrameworkAdapterRef`.

## What these examples are testing

The exercise is "if a careful, literal reader picked up only this YAML, would
they understand what the user wants?" If the answer is "they'd need to read
runtime documentation," the spec is leaking implementation. If the answer is
"they'd understand the contract but not how to execute it," the spec is right
and the runtime is correctly the user's problem.

When something feels redundant, awkward, or surprising in these examples —
that's the signal to revisit the envelope.
