# Telemetry and observations

Load for traces, metrics, logs, OpenTelemetry integration, observation
identity, correlation, or signal-card design.

## Model

OpenTelemetry signals describe runtime activity; Wyrd turns durable facts into
typed Observations attached to declared Cards and client Runs. Traces explain a
request path, metrics provide aggregatable measurements, and logs retain event
records. Keep signal transport separate from Wyrd's Card/Run/Observation
identity so telemetry can be sampled or retried without changing the protocol.

The minimum identity tuple is:

```text
tenant_id (server) + card_ref (client asserted, server authorized)
           + run_id (opaque client execution) + Wyrd-Request-Id (request spine)
```

`card_ref` and `run_id` are row-level values. A buffered batch may contain
multiple cards and runs; authorization is therefore per row, not once per
request. The request ID joins hops and is not a replacement for trace IDs.

## Instrumentation guidance

- Use OpenTelemetry semantic conventions for service, span, metric, and log
  attributes. Add Wyrd-specific attributes only for Card, Run, tenant, and
  request correlation.
- Bound attribute cardinality. Put unbounded prompts, tool payloads, and user
  text in protected payload columns or linked artifacts, not metric labels.
- Record start/end times, status, error type, resource identity, and sampling
  decisions consistently. Preserve parent/child and links when a workflow
  crosses a service boundary.
- Keep payload sensitivity explicit. Trace, log, GenAI, and agent-trace payload
  reads require elevated permissions; metadata queries do not imply payload
  access.
- Prefer tail sampling or bounded head sampling that is observable and
  reproducible. Never let sampling erase the audit or failure evidence needed
  for a policy decision.

## Ingest and query invariants

Ingest stamps tenant and system timestamps server-side, validates schema and
batch replay keys, and preserves immutable row ordinals. Query surfaces require
a tenant-qualified table, a bounded time range, and an explicit projection.
Payload reads must pass the corresponding sensitive-column permission. Every
accepted or denied policy decision is itself observable and auditable.

## Failure modes and trade-offs

At-least-once delivery is usually the honest choice for telemetry. Make it safe
with idempotency keys and deterministic batch identity; do not claim exactly-once
when a process crash can occur between object-store and control-plane commits.
Sampling reduces cost but can bias rare failures, so retain unsampled counters
and make sampling policy visible. High-cardinality labels improve local
debugging while damaging index and scan cost; store them as bounded maps or
payloads instead.

Reject using a trace ID as the Wyrd request correlator, trusting a client-supplied
tenant, deriving a Card from a principal alone, or collecting all columns before
filtering. These patterns produce ambiguous lineage, isolation leaks, or
unbounded analytical cost.

## Stable Wyrd anchors

- Identity contract: `architecture/wyrd-design.md` §“Observation identity —
  Card → Run → Observation”.
- Observation wire and correlation types: `crates/wyrd-spec/src/vala/`.
- OTEL ingestion and projections: `crates/vala/` and `crates/wyrd/wyrd-server/`.

## Primary grounding

- [OpenTelemetry signals](https://opentelemetry.io/docs/concepts/signals/)
- [OpenTelemetry semantic conventions](https://opentelemetry.io/docs/concepts/semantic-conventions/)
- [OpenTelemetry context propagation](https://opentelemetry.io/docs/concepts/context-propagation/)
- [W3C Trace Context](https://www.w3.org/TR/trace-context/)
- [Apache Arrow columnar format](https://arrow.apache.org/docs/format/Columnar.html)
- Wyrd anchors: `architecture/wyrd-design.md` §Observation identity;
  `crates/wyrd-spec/src/vala/`; `crates/vala/`.
