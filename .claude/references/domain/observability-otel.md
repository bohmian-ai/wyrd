# Observability And OTel Review

Use this reference when a Wyrd proposal touches Vala, traces, spans, metrics,
logs, observations, eval/drift results, OTel ingest/export, trace context,
runtime telemetry, or agent/provider observability.

## Wyrd Model

- `Run` records one execution of declared behavior.
- `Observation` records measured facts attached to a run, card, service, or
  workflow.
- Vala owns observability ingestion, archival query, drift, eval execution,
  trace/span storage, alerts, and background data-plane behavior.
- Wyrd control-plane writes still require request ID, trace context, actor
  identity, policy decision where relevant, redacted payload summary, and audit.

Do not model trace/span/eval/drift facts as ad hoc service-private objects when
they need to be linked, queried, governed, retained, or exposed to agents. Use
Wyrd `Run`, `Observation`, card refs, relationships, and service-owned status.

## OTel Semantic Checks

Review whether the design:

- Propagates trace context across HTTP, Python SDK, CLI, MCP, workers, Skald
  provider calls, Vala ingest, and storage/query paths.
- Links traces/spans to Wyrd identities: tenant/space, card ref, run ID,
  observation ID, service, actor/request ID, provider/model where appropriate.
- Distinguishes spans, span events, logs, metrics, runs, observations, audit
  records, eval results, drift findings, and status updates.
- Uses stable attribute keys and typed values for fields used in filters,
  grouping, retention, policy, and dashboards.
- Defines sampling, retention, redaction, and cardinality controls.
- Avoids storing provider response blobs, prompts, secrets, tokens, or PII in
  trace attributes unless explicitly redacted and governed.

Blocking patterns:

- Metrics or trace attributes with unbounded cardinality such as raw prompt,
  full payload, user text, stack trace as label, request body, or random IDs as
  high-volume dimensions.
- Trace/log data that bypasses tenant/space scoping.
- OTel ingest path that writes high-volume facts into Postgres by default.
- Provider-specific wire payloads becoming durable public Wyrd contracts.

## Trace Storage And Query Checks

For high-volume trace/span storage:

- Keep ingestion on a batched columnar path and avoid JSON row roundtrips.
- Use bounded buffers, flush-by-size/time, backpressure, and graceful shutdown.
- Preserve time-first and tenant/space-visible query shape.
- Store common filters as typed columns: start/end time, service, span kind,
  status, model/provider, card/run IDs, root trace/span IDs where relevant.
- Precompute query helpers only when the storage cost is justified by hot query
  paths.
- Separate hot ingest, archival query, compaction, and alert/eval workers so one
  path cannot starve the others.
- Emit metrics for queue depth, dropped/retried spans, batch size, flush age,
  commit latency, query bytes scanned, and compaction backlog.

## Evaluation, Drift, And Alerts

Review whether:

- Eval and drift declarations are cards/specs; executions become runs and
  measured results become observations.
- Alert state is derived from observations/eval/drift results and service-owned
  status, not an independent public ontology unless locked by architecture.
- Triggered work has idempotency, cooldown, policy checks, audit records, and
  clear failure status.
- Human review/escalation paths are modeled when policy, safety, compliance, or
  business impact requires them.

## Agent And Developer Observability

Agent-facing observability should support:

- Discovering relevant cards, runs, observations, evals, drift findings, and
  audit history without raw database access.
- Explaining why a policy/eval/drift decision happened.
- Replaying or comparing runs when inputs, artifacts, prompts, model versions,
  and policies are available.
- Stable MCP/CLI/API filters for time, card ref, run, status, service, tenant,
  and observation kind.

## Review Questions

- Which facts are spans/events/metrics/logs, and which are Wyrd runs or
  observations?
- What are the cardinality limits and redaction rules?
- How are traces correlated with cards, runs, policy decisions, audit records,
  artifacts, and generated status?
- What is the retention path from hot ingest to archival query?
- What proves the design works under ingest spikes, broad queries, and provider
  error storms?
