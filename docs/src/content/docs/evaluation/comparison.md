---
title: "Comparison"
description: "How Wyrd evaluation compares with adjacent agent-evaluation tools."
---

# Comparison

*Comparison current as of June 2026. Vendor features change frequently; verify
against current docs before making product decisions.*

## Predecessor citation

The predecessor scouter `AgentEvaluator` established the four-task evaluation
shape, dependency DAGs, conditional cost control, and offline/online framing.
Wyrd supersedes that surface with the locked `EvalCard`, `EvalSpec`, `Scenario`,
`EvalRecord`, `EvalResults`, and `EvalComparison` contracts covered by the
stage 17B plan and its `01-parity-audit.md` evidence.

## Comparison table

| Capability | Wyrd | LangSmith | Langfuse | MLflow | Datadog LLM Observability | Google ADK |
|---|---|---|---|---|---|---|
| Offline evaluation | Yes: `--records` and local scenario runs | Yes: datasets and experiments | Yes: datasets and evaluation runs | Yes: GenAI evaluation | Experiments and submitted evaluations | Yes: eval sets and CLI |
| Online or server-side evaluation | Yes: `vala-http::eval` on `wyrd-server` | Yes: online evaluators | Yes: online scoring on traces | Yes: production trace evaluation | Yes: primary workflow | Limited to development evaluation |
| Same task definitions across modes | Yes: one `EvalSpec` DAG | Partial: evaluator code can be reused, wiring differs | Partial: scores and evaluators attach in several workflows | Partial: scorers can be reused | No: primarily observability-configured | No: eval-set oriented |
| Deterministic assertions | Yes: `AssertionTask`, `TraceAssertionTask`, `AgentAssertionTask` | Yes, through custom evaluators and trajectory checks | Custom code evaluators | Code-based scorers | Custom or managed evaluations | Criteria for response and tool trajectory |
| LLM-as-judge | Yes: `LlmJudgeTask` | Yes | Yes | Yes | Yes | Evaluation criteria and platform integrations |
| Trace/span assertions | Yes: trace tasks in the same DAG | Trace and run evaluators | Scores attach to traces and observations | Scorers can read traces | Native trace evaluation | Test-time trajectory, not production trace storage |
| Agent tool-call assertions | Yes: vendor-agnostic agent assertions | Trajectory evaluation | Custom evaluator required | Scorers over traces | Managed and custom tool evaluations | Tool trajectory criteria |
| Dependency DAGs and gates | Yes | No first-class task DAG | No first-class task DAG | No first-class task DAG | No first-class task DAG | No first-class task DAG |
| Deployment model | Self-hosted Wyrd services | Managed and self-hosted options | Managed and self-hosted options | OSS and managed platform use | Managed service | OSS development framework |

## Per-platform notes

### Wyrd

Wyrd evaluation is a declared platform capability. `EvalSpec` is the task
definition language, `Scenario` is the runtime test-data unit, and results are
versioned artifacts that compare against a baseline. Offline record replay and
server-hosted protocol runs use the same task DAG. Trace assertions and
vendor-agnostic agent assertions are not separate systems.

### LangSmith

LangSmith covers development and production evaluation with datasets,
experiments, online evaluators, and trajectory-style agent checks. It is strong
for teams already using the LangChain ecosystem. Wyrd differs by keeping task
definitions in a Wyrd Card contract and by putting trace assertions,
agent assertions, and LLM judges in one DAG with explicit pass-gate behavior.

### Langfuse

Langfuse centers evaluation around scores attached to traces, observations,
sessions, and dataset runs. It is strong for observability, human review, and
self-hosted trace workflows. Wyrd differs by making the task DAG and baseline
comparison first-class Wyrd contracts instead of treating scoring methods as
separate attachments.

### MLflow

MLflow GenAI evaluation uses scorers over datasets and traces, including
production monitoring workflows. It is strong when a team already uses MLflow
for experiments, traces, and model lifecycle. Wyrd differs by using Card
identity, `EvalSpec` DAG validation, pass gates, and four-quadrant comparison
as the native evaluation contract.

### Datadog LLM Observability

Datadog starts from production observability. Evaluations attach to traces and
fit into the existing Datadog monitoring, dashboarding, and alerting model.
Wyrd differs by being self-hosted and by using the same declared task
definitions for local replay, local scenario runs, and server protocol runs.

### Google ADK

Google ADK evaluation is oriented around agent development: eval sets, CLI
runs, user simulation, and criteria such as response quality and tool
trajectory. Wyrd differs by making evaluation a registered Wyrd capability
with server protocol routes, result artifacts, and baseline comparison rather
than only a development-framework test surface.

## Where Wyrd is different

- Same task definitions across offline `--records`, local scenario runs, and
  server-hosted protocol runs.
- Trace-based assertions run in the same DAG as assertions, agent assertions,
  and LLM judges.
- Agent assertions stay vendor-agnostic at the Wyrd task layer.
- Wyrd is self-hosted by design; there is no managed-service tier in this
  primitive.
- Card identity and four-quadrant comparison are first-class concepts.
