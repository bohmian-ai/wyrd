# Telemetry and observations

Load for traces, metrics, logs, OpenTelemetry integration, observation
identity, correlation, sampling, or payload sensitivity.

## Protocol identity and physical storage

OpenTelemetry signals describe runtime activity. Wyrd turns durable measured
facts into typed Observations attached to declared Cards and client Runs.
Signal transport may batch, sample, retry, or re-encode data without changing
Wyrd identity.

The public correlation spine is:

```text
authenticated tenant
  + authorized card_ref
  + opaque run_id
  + Wyrd-Request-Id
```

`card_ref` and `run_id` are observation-grain values. A buffered request may
contain rows for several Cards and Runs, so authorize every distinct asserted
Card against the principal's card scope. The server derives tenant and request
identity from verified authority and stamps physical storage columns such as
`data_tenant_id`, `card_uid`, `principal_id`, `wyrd_batch_id`,
`wyrd_row_ordinal`, and `wyrd_ingested_at`. Those physical columns support
isolation and efficient joins; they do not replace the public Card/Run contract.

`Wyrd-Request-Id` joins Wyrd hops and audit ancestry. W3C trace/span IDs join
telemetry causality. Preserve both; never overload one as the other.

## Instrumentation contract

- Follow OpenTelemetry semantic conventions for standard service, HTTP, RPC,
  database, messaging, GenAI, and exception attributes. Add Wyrd attributes
  only where Card, Run, tenant, request, table, or Bifrost stage identity has no
  standard equivalent.
- Bound metric and resource-attribute cardinality. Tenant, Card, Run, request,
  prompt, tool payload, query text, object path, batch, task, and attempt IDs
  belong in protected records or scrubbed traces, not metric labels.
- Record start and terminal timestamps, status, error type, resource ownership,
  queue delay, retry count, cancellation, and sampling decision consistently.
- Preserve parent/child relationships and links across async queues and
  distributed stages. Propagate context through typed carriers and authenticated
  peer assignments, not process globals.
- Emit lifecycle metrics from the concrete owner that performs the effect.
  Metric descriptors or tests that emit surrogate observations are not proof
  that the production path is instrumented.
- Every active gauge has a decrement on success, refusal, failure, retry,
  uncertainty, cancellation, and shutdown.

## Sampling and payload safety

Sampling policy is explicit, versioned, and observable. Retain unsampled
aggregate counters so operators can measure sampling bias and dropped volume.
Tail sampling may retain rare failures but cannot recreate spans dropped before
the decision point. Head sampling is repeatable only when its inputs and policy
are stable. Never let trace sampling erase required audit, policy-decision,
evaluation, or durability evidence.

Treat prompt text, completion text, tool arguments/results, logs, span events,
headers, query text, and user identifiers as sensitive payloads. Metadata read
permission does not imply payload permission. Redaction is allowlist-oriented,
occurs before durable export where possible, and records which policy/version
was applied. Redaction cannot guarantee removal from opaque encrypted or
encoded blobs; such payloads require separate access and retention controls.

## Ingest and query invariants

Ingest validates schema, event-time acceptance, card scope, batch replay
identity, and bounded envelope size before Scribe admission. Query surfaces
require a tenant-qualified table, bounded time range, explicit projection, and
the permission associated with every sensitive column. Filter and project
before collecting or decoding protected payloads.

At-least-once delivery is made safe with deterministic batch identity and row
ordinal. Do not claim end-to-end exactly-once when object-store and Postgres
effects cannot share a transaction. Duplicate transport attempts must converge
to one logical row set or a typed identity conflict.

## Failure states

Missing identity, denied Card scope, malformed context, schema conflict,
out-of-range event time, sampling/export loss, redaction failure, and incomplete
ingest are explicit degraded or failed states. They never become a synthetic
"no signal" observation. Reject trusting client tenant fields, deriving a Card
from principal identity alone, metric labels with unbounded values, and payload
collection before authorization/projection.

## Stable Wyrd anchors

- Observation identity: `architecture/wyrd-design.md` §Observation identity.
- Bifrost storage: `architecture/bifrost-design.md`.
- Wire types: `crates/wyrd-spec/src/vala/`.
- Ingest and query owners: `crates/vala/` and `crates/wyrd/wyrd-server/`.

## Primary grounding

- [OpenTelemetry signals](https://opentelemetry.io/docs/concepts/signals/)
- [OpenTelemetry semantic conventions](https://opentelemetry.io/docs/specs/semconv/)
- [OpenTelemetry sampling](https://opentelemetry.io/docs/concepts/sampling/)
- [OpenTelemetry security guidance](https://opentelemetry.io/docs/security/)
- [W3C Trace Context](https://www.w3.org/TR/trace-context/)
