---
id: BIFROST-OTEL-T04
title: Prove canonical signals through every public journey owner
kind: implementation
mode: DECOMPOSE
status: proposed
spec: SPEC-bifrost-canonical-otel-signals
spec_revision: 11
depends_on: [BIFROST-OTEL-T02, BIFROST-OTEL-T03, BIFROST-OTEL-T03A]
requirements: [REQ-002, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-020, REQ-021, REQ-023]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011]
---

# Existing-harness public fidelity, failure, and topology closure

## Outcome and value

Real Rust, Python, and TypeScript applications write trace/GenAI, log, and
metric data through stock OpenTelemetry exporters and through canonical Arrow.
Every path crosses Scribe ACK, publication, and Oracle before canonical SQL
readback. The raw OTLP suite separately pins protobuf, JSON, and gRPC fidelity;
MCP and existing topology owners prove the agent and distributed boundaries.

Required execution skill: `$wyrd-implement`.

## Owners, scope, consumers, and prohibited changes

- Earn back the focused `wyrd-testing` `otlp` target and
  `test:bifrost:journey:otlp` lane with their first real tests. The target owns
  the cohesive public OTLP trace, log, metric, mixed-batch, and negative
  protocol journeys across HTTP protobuf, HTTP JSON, and gRPC; it is not empty
  scaffolding. Reuse the ordinary capability-directory layout and documented
  inner `mod pg_tests` organization; do not create another topology fixture.
- Reuse `WyrdTestServer`, current Postgres lifecycle, public SDKs, Scribe flush,
  Oracle query, publication/restart controls, auth fixtures, and telemetry
  checkpoints.
- Extend existing Rust SDK, server, MCP, Python, TypeScript, standalone,
  distributed, single-tenant, and multi-tenant owners only for the boundary
  each uniquely proves.
- Extend the existing Python `WyrdTestServer` projection with only the
  access-token exchange needed by unmodified OTLP exporters; reuse the bound
  gRPC URL it already publishes as `WYRD_GRPC_URL`. Add upstream OpenTelemetry
  packages only as Rust, Python, or TypeScript test/development dependencies;
  do not add a Wyrd telemetry wrapper, exporter adapter, collector process, or
  second server fixture.
- One canonical expected dataset in OTLP `support.rs` supplies all signal
  values. Matching Arrow batches are built from the public describe response
  and T03A's Rust/Python/TypeScript `writable_schema` helpers, then sent through
  existing insert APIs. Expected DTOs are literal assertions over those fixture
  values, not another projector. Tests must not contain a second semantic
  mapper or column-order oracle.
- Do not weaken test sizes, durability assertions, permissions, terminal
  frames, timeouts, topology, or ignored-test policy to make the lane pass.

## Integrated baseline

The Forge/Oracle integration is already present on `oracle-distributed` and is
the baseline for this task. Extend its current Forge publication, Scribe, and
distributed Oracle owners without restoring deleted tables, direct projectors,
or retired query DTOs. This packet does not edit the separate change's
specification or task files.

BIFROST-OTEL-T02, BIFROST-OTEL-T03, and BIFROST-OTEL-T03A are accepted
prerequisites. Their historical review packets do not block this task.

## Restart checkpoint — read before editing

This task is mid-implementation. Preserve the committed work and the current
Python worktree edits; do not restart Scenario 1 or discard the Python exporter
work.

### Completed and committed

- `a79a231d0` restored the `wyrd-testing` `otlp` target and
  `test:bifrost:journey:otlp` lane with a real maximal gRPC trace test.
- `969df61a0` added trace parity across OTLP gRPC, HTTP protobuf, and HTTP JSON.
- `18eaf9b04` added the raw OTLP log and all-kind metric route tests. All four
  Scenario 1 tests are green.
- `02e953b8e` enforced declared sensitive payload columns on Oracle's optimized
  canonical SQL plan and reconstructed `WYRD_VALA_403_PAYLOAD_FORBIDDEN` in
  clients instead of exposing it as a retryable 502.

`vala.metrics.points` declares sensitive columns, but the approved doctrine
defines no metric-payload permission. Keep metrics ungated; this task must not
invent a permission.

### In progress — preserve these worktree files

- `crates/wyrd/wyrd-testing/src/python.rs`
- `python/py-wyrd/pyproject.toml`
- `python/py-wyrd/uv.lock`
- `python/py-wyrd/tests/integration/test_bifrost_e2e.py`

These edits add `WyrdTestServer.access_token()` plus stock Python trace, stdlib
log, and metric exporters. Finish them in place. The trace still needs the
required GenAI attributes and structured messages.

### Required journey matrix

| Boundary | Exact owner/test | Signals | Restart status |
|---|---|---|---|
| Raw OTLP gRPC | `trace_export::pg_tests::otlp_grpc_maximal_trace_round_trips_every_field` | trace/GenAI | Done; extend structured GenAI messages only |
| Raw OTLP HTTP | `trace_export_http::pg_tests::otlp_http_protobuf_and_json_match_grpc_trace_rows` | trace/GenAI | Done; inherits the shared GenAI fixture extension |
| Raw OTLP logs | `logs_export::pg_tests::otlp_log_routes_round_trip_body_context_and_redaction` | logs | Done |
| Raw OTLP metrics | `metrics_export::pg_tests::otlp_metric_routes_round_trip_every_supported_point_kind` | every supported metric kind | Done |
| Stock Python OTel | `test_standard_otel_tracer_exports_to_bifrost`; `test_stdlib_logging_exports_to_bifrost`; `test_standard_otel_metrics_export_to_bifrost` | trace/GenAI, stdlib log, representative metrics | In progress in the four files above |
| Stock Rust OTel | `stock_rust_otel_tracer_exports_genai_span_to_bifrost`; `stock_rust_otel_logger_exports_correlated_log_to_bifrost`; `stock_rust_otel_meter_exports_representative_metrics_to_bifrost` | trace/GenAI, log, representative metrics | Not started |
| Stock TypeScript OTel | `stock OpenTelemetry tracer exports GenAI span to Bifrost`; `stock OpenTelemetry logger exports correlated log to Bifrost`; `stock OpenTelemetry meter exports representative metrics to Bifrost` | trace/GenAI, log, representative metrics | Not started |
| Canonical Arrow Rust | `pg_tests::canonical_signal_arrow_write_and_sql_read_round_trip` | trace/GenAI, log, representative metrics | Not started |
| Canonical Arrow Python | `test_canonical_signal_arrow_write_and_sql_read_round_trip` | trace/GenAI, log, representative metrics | Not started |
| Canonical Arrow TypeScript | `canonical signal Arrow write and SQL read round-trip` | trace/GenAI, log, representative metrics | Not started |
| Public API reads | every stock-exporter and Arrow journey above through `Bifrost.sql`/the public query client | all three canonical tables | Not started except raw-suite readback |
| MCP reads | `query::pg_tests::agent_reads_canonical_trace_genai_logs_and_metrics_through_sql` | trace/GenAI, log, metric | Not started |
| Public failures | `mixed_otlp_requests_commit_only_complete_siblings_and_exact_partial_success`; `all_invalid_and_request_wide_failures_leave_no_queryable_rows` | mixed-validity and request-wide failure | Not started |
| Existing topology | `scribe_write_flush_read_user_journey`; `stage_graph_executes_representative_query_styles` | one complete GenAI span | Not started |

The matrix is complete when every non-done row is green or records a verified
limitation in the pinned upstream OpenTelemetry SDK. Hand-built OTLP does not
substitute for a missing stock-exporter journey.

### Concrete data and read questions

All stock-exporter tests use IDs generated by the upstream SDK rather than
injecting fixed IDs. Canonical Arrow tests create fresh valid IDs with the
language's existing public authoring/ID facilities. Every test uses fixed
service, scope, span, log, metric, and marker values so SQL assertions are
deterministic. Every trace-capable journey must include:

- a parent and child span, one event, one link, error status, resource, and
  instrumentation scope;
- `gen_ai.operation.name`, `gen_ai.provider.name`, `gen_ai.request.model`,
  `gen_ai.conversation.id`, `gen_ai.usage.input_tokens`, and
  `gen_ai.usage.output_tokens`;
- structured `gen_ai.input.messages` and `gen_ai.output.messages` payloads.

Every all-signal journey writes and then reads:

- `vala.traces.spans`: reconstruct the generated parent/child trace, filter by
  service/model, and calculate or select input/output token usage;
- `vala.logs.records`: find the error log and prove its trace/span correlation;
- `vala.metrics.points`: aggregate at least one counter and inspect one gauge
  or histogram point.

The raw metric protocol test remains the only exhaustive all-kind metric test.
If a pinned stock SDK cannot author a supported signal or instrument, record
the exact upstream limitation and leave its exhaustive proof in Scenario 1;
never replace the stock SDK call with hand-built OTLP.

## Ordered implementation scenarios

### Scenario 1 — COMPLETED: raw OTLP protocol fidelity

**Behavior.** Each exposed HTTP protobuf/JSON and gRPC route accepts the shared
maximal trace, log, and all-kind metrics dataset, acknowledges only after the
Scribe boundary, flushes/publishes, and reads every value exactly. Maps
REQ-002, REQ-004, REQ-006–REQ-018, INV-001–INV-009, AC-001, AC-003–AC-005.

**Implemented.** These four named tests exist and are green:

- `trace_export::pg_tests::otlp_grpc_maximal_trace_round_trips_every_field`
- `trace_export_http::pg_tests::otlp_http_protobuf_and_json_match_grpc_trace_rows`
- `logs_export::pg_tests::otlp_log_routes_round_trip_body_context_and_redaction`
- `metrics_export::pg_tests::otlp_metric_routes_round_trip_every_supported_point_kind`

Each uses the same support dataset, real server and public reads. Logs assert
authorized and unauthorized payload behavior. Metrics assert integer/double
distinction, all kind-specific values, metadata, and exemplars.

**Remaining amendment.** Extend the shared maximal trace fixture and its two
trace-route assertions with both structured GenAI attributes:

- `gen_ai.input.messages`
- `gen_ai.output.messages`

Keep the already-covered promoted GenAI operation, provider, model,
conversation, input-token, and output-token fields. Assert the structured
message JSON is queryable when payload access is authorized and rejected by
`WYRD_VALA_403_PAYLOAD_FORBIDDEN` when it is projected without payload access.
Do not create another raw OTLP test.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=trace_export::pg_tests::otlp_grpc_maximal_trace_round_trips_every_field)' --run-ignored=all"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=trace_export_http::pg_tests::otlp_http_protobuf_and_json_match_grpc_trace_rows)' --run-ignored=all"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=logs_export::pg_tests::otlp_log_routes_round_trip_body_context_and_redaction)' --run-ignored=all"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=metrics_export::pg_tests::otlp_metric_routes_round_trip_every_supported_point_kind)' --run-ignored=all"
```

**GREEN.** Preserve the completed implementation. Add only the shared GenAI
fixture values/assertions and any production fix those assertions expose. Use
explicit flush/publication and Oracle terminal success; HTTP JSON/protobuf
fixtures encode the same pinned request.

**REFACTOR.** Shared support constructs data; each test owns its boundary and
assertions. No test calls another test or reproduces table projection logic.

### Scenario 2 — Stock OpenTelemetry emitters reach Bifrost from every language

**Behavior.** Ordinary Rust, Python, and TypeScript applications configure the
pinned upstream OpenTelemetry SDKs and OTLP exporters, emit traces, logs, and
metrics to the bound Wyrd server, force exporter completion, and read the exact
emitted values from the canonical Bifrost tables through their public Wyrd
query clients. This is the primary OTLP user journey; Scenario 1 is its
protocol-fidelity support. Maps REQ-002, REQ-004, REQ-006–REQ-012,
REQ-015–REQ-018, INV-003, INV-006–INV-009, AC-001, AC-003–AC-005, AC-008,
AC-011.

#### Python — in progress

Finish these three integration tests already present in the existing gated Python journey
owner `python/py-wyrd/tests/integration/test_bifrost_e2e.py`:

- `test_standard_otel_tracer_exports_to_bifrost`
- `test_stdlib_logging_exports_to_bifrost`
- `test_standard_otel_metrics_export_to_bifrost`

Each test uses the session-scoped `wyrd_server` fixture, its real bound gRPC
endpoint, a bearer obtained by exchanging its existing admin API key, and the
upstream `opentelemetry-sdk` plus
`opentelemetry-exporter-otlp-proto-grpc`. Give each test a unique fixed
instrumentation-scope/name value that Oracle can predicate directly and a
marker attribute whose decoded value must survive readback. After the provider's
`force_flush()` succeeds, call the existing `wyrd_server.flush_bifrost()`
publication boundary, query through `BifrostQueryClient`, require its success
terminal, and compare the emitted values without fixture-side normalization.
Do not use sleeps, caller-supplied random fixture values, private extension
imports, hand-built OTLP requests, or an OpenTelemetry Collector. The upstream
SDK still generates the trace/span IDs.

The trace emits fixed-name parent and child spans, captures their SDK-generated
identifiers and asserts their relationship, and includes scalar/array
attributes, an event, a link, status, resource attributes, an explicit
instrumentation scope, the promoted GenAI operation/provider/model/conversation
and input/output-token attributes, and structured `gen_ai.input.messages` and
`gen_ai.output.messages`. The stdlib log uses a dedicated `logging.Logger` with OTel's
`LoggingHandler` while the trace span is current, then asserts body, severity,
custom attributes, resource/scope, and trace/span correlation. Metrics emit a
counter, up/down counter, gauge, and histogram with integer and floating-point
measurements and attributes; assert the SDK-produced aggregation, points, and
histogram buckets. Scenario 1 remains authoritative for supported protocol
fields and metric kinds the Python SDK cannot directly author, including raw
wire presence distinctions, Summary, and explicitly constructed exponential
histograms.

The proposed trace setup is:

```python
import os

from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from opentelemetry.trace import Link, SpanContext, Status, StatusCode, TraceFlags

headers = (("x-wyrd-access-token", f"Bearer {wyrd_server.access_token()}"),)
endpoint = os.environ["WYRD_GRPC_URL"]
resource = Resource.create({"service.name": "wyrd-python-journey"})
traces = TracerProvider(resource=resource)
traces.add_span_processor(
    BatchSpanProcessor(
        OTLPSpanExporter(endpoint=endpoint, insecure=True, headers=headers)
    )
)
tracer = traces.get_tracer("wyrd.tests.otel.trace", "1.0.0")
linked = SpanContext(
    trace_id=1,
    span_id=2,
    is_remote=True,
    trace_flags=TraceFlags.SAMPLED,
)
with tracer.start_as_current_span("python-parent", links=[Link(linked)]) as parent:
    parent.set_attribute("wyrd.test.marker", "python-trace")
    parent.set_attribute("test.values", [1, 2, 3])
    parent.add_event("checkpoint", {"step": 1})
    parent.set_status(Status(StatusCode.ERROR, "expected test status"))
    with tracer.start_as_current_span("python-child") as child:
        child.set_attribute("answer", 42)
assert traces.force_flush()
traces.shutdown()
```

The proposed standard-library logging setup is:

```python
import logging

from opentelemetry.exporter.otlp.proto.grpc._log_exporter import OTLPLogExporter
from opentelemetry.sdk._logs import LoggerProvider, LoggingHandler
from opentelemetry.sdk._logs.export import BatchLogRecordProcessor
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import TracerProvider

logs = LoggerProvider(resource=Resource.create({"service.name": "wyrd-python-journey"}))
logs.add_log_record_processor(
    BatchLogRecordProcessor(
        OTLPLogExporter(endpoint=endpoint, insecure=True, headers=headers)
    )
)
logger = logging.getLogger("wyrd.tests.otel.log")
logger.setLevel(logging.INFO)
logger.propagate = False
handler = LoggingHandler(logger_provider=logs)
logger.addHandler(handler)
context_tracer = TracerProvider().get_tracer("wyrd.tests.otel.log-context", "1.0.0")
try:
    with context_tracer.start_as_current_span("python-log-context") as current:
        expected_context = current.get_span_context()
        logger.warning("order delayed", extra={"wyrd.test.marker": "python-log"})
    assert logs.force_flush()
finally:
    logger.removeHandler(handler)
    logs.shutdown()
```

The queried log must carry `expected_context.trace_id` and
`expected_context.span_id`; the context-only trace provider intentionally has
no exporter because this test owns log transport rather than a second trace
export.

The proposed metrics setup is:

```python
from math import inf

from opentelemetry.exporter.otlp.proto.grpc.metric_exporter import OTLPMetricExporter
from opentelemetry.sdk.metrics import MeterProvider
from opentelemetry.sdk.metrics.export import PeriodicExportingMetricReader
from opentelemetry.sdk.resources import Resource

exporter = OTLPMetricExporter(
    endpoint=endpoint,
    insecure=True,
    headers=headers,
)
reader = PeriodicExportingMetricReader(exporter, export_interval_millis=inf)
metrics = MeterProvider(
    resource=Resource.create({"service.name": "wyrd-python-journey"}),
    metric_readers=[reader],
)
meter = metrics.get_meter("wyrd.tests.otel.metric", "1.0.0")
attributes = {"wyrd.test.marker": "python-metric"}
meter.create_counter("orders.created").add(7, attributes)
meter.create_up_down_counter("orders.active").add(-2, attributes)
meter.create_gauge("queue.depth").set(3.5, attributes)
meter.create_histogram("request.duration", unit="ms").record(12.5, attributes)
assert metrics.force_flush()
metrics.shutdown()
```

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd python/py-wyrd && mise run py:setup:testing && uv run pytest -q -m integration tests/integration/test_bifrost_e2e.py::test_standard_otel_tracer_exports_to_bifrost"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd python/py-wyrd && mise run py:setup:testing && uv run pytest -q -m integration tests/integration/test_bifrost_e2e.py::test_stdlib_logging_exports_to_bifrost"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd python/py-wyrd && mise run py:setup:testing && uv run pytest -q -m integration tests/integration/test_bifrost_e2e.py::test_standard_otel_metrics_export_to_bifrost"
```

#### Rust — not started

Add these tests to the existing `wyrd-testing` OTLP modules; do not create a
second Rust target:

- `trace_export::pg_tests::stock_rust_otel_tracer_exports_genai_span_to_bifrost`
- `logs_export::pg_tests::stock_rust_otel_logger_exports_correlated_log_to_bifrost`
- `metrics_export::pg_tests::stock_rust_otel_meter_exports_representative_metrics_to_bifrost`

Use the pinned upstream Rust OpenTelemetry SDK/exporter APIs, not `prost`
request construction. The trace test emits a parent/child trace with an event,
link, error status, resource/scope metadata, promoted GenAI fields, and
structured input/output messages. Capture the SDK-generated IDs and query the
same trace through the public Rust query client. The log test emits one
trace-correlated error log with body, severity, resource/scope, and a marker.
The metric test emits the representative instruments supported by the pinned
SDK—counter, up/down counter, gauge, and histogram—and asserts their canonical
points and aggregation metadata. Add upstream crates only as test/dev
dependencies in `crates/wyrd/wyrd-testing/Cargo.toml`.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=trace_export::pg_tests::stock_rust_otel_tracer_exports_genai_span_to_bifrost)' --run-ignored=all"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=logs_export::pg_tests::stock_rust_otel_logger_exports_correlated_log_to_bifrost)' --run-ignored=all"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=metrics_export::pg_tests::stock_rust_otel_meter_exports_representative_metrics_to_bifrost)' --run-ignored=all"
```

#### TypeScript — not started

Add `typescript/wyrd/tests/integration/otel-export.test.ts` with exactly these
gated tests and register that file in the existing TypeScript Bifrost journey
lane:

- `stock OpenTelemetry tracer exports GenAI span to Bifrost`
- `stock OpenTelemetry logger exports correlated log to Bifrost`
- `stock OpenTelemetry meter exports representative metrics to Bifrost`

Use the pinned upstream OpenTelemetry Node SDK and OTLP gRPC exporters. Apply
the same observable requirements as Rust: SDK-generated trace/span IDs,
parent/child relationship, event/link/status/resource/scope, promoted and
structured GenAI attributes, one correlated error log, and representative
counter/up-down-counter/gauge/histogram metrics. Query all results through the
public TypeScript Bifrost client. Add only upstream test/dev dependencies and
the owning lockfile update; do not introduce a Wyrd exporter wrapper.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd typescript/wyrd && mise run ts:build && mise run ts:build:testing && pnpm exec vitest run tests/integration/otel-export.test.ts"
```

**GREEN.** For Python, add `opentelemetry-exporter-otlp-proto-grpc` beside the existing
OpenTelemetry Python development dependencies and regenerate `uv.lock`. On the
existing Python `WyrdTestServer`, add one `access_token()` method that exchanges
its retained API key through the Rust harness's existing `exchange_api_key`
owner; continue reading its already-published `WYRD_GRPC_URL` rather than
adding another endpoint projection. Fix only production OTLP, table projection,
publication, or public-query owners exposed by the failing end-to-end
assertions. For Rust and TypeScript, add only the upstream test/dev packages
needed by the three tests above and use the existing OTLP target, test server,
auth exchange, flush/publication boundary, and public query clients.

**REFACTOR.** Keep all telemetry construction on upstream OpenTelemetry types,
keep credentials out of assertion output, remove logging handlers and shut down
all exporting providers deterministically, tolerate the pinned SDK's
`LoggingHandler` deprecation warning without adding an instrumentation package,
and share only small Python setup/read helpers in `test_bifrost_e2e.py` when
endpoint/header/query code actually repeats. The exhaustive raw OTLP dataset
and canonical Arrow journeys remain separate owners.

### Scenario 3 — RETIRED: separate OTLP/Arrow comparison test

Do not implement the formerly planned
`mixed_batch::pg_tests::otlp_and_canonical_arrow_share_user_rows_and_public_results`
test or a `mixed_batch.rs` module. It duplicated the real user journeys while
adding a fixture-side row comparison.

Instead, Scenario 2's stock exporters and Scenario 5's canonical Arrow writers
use the same fixed logical values for each signal and assert those values by
canonical field name through public SQL. Those independently prove that both
write paths converge on the same public schema. They need not compare
server-managed batch, request, or ingest identities. Maps REQ-004, INV-003,
AC-002, AC-008.

### Scenario 4 — Partial success and whole-request failures keep exact authority

**Behavior.** From the caller's boundary, a mixed-validity trace/log/metric
request reports exactly which records were rejected, leaves every accepted
record queryable, and leaves every rejected record absent. Retrying the same
request does not double-write accepted records. An all-invalid request or a
request-wide auth/size/schema/tenant/admission/durability failure makes no data
queryable. Maps REQ-005, INV-007–INV-009, AC-006–AC-007.

**RED.** Populate `negative.rs` with
`pg_tests::mixed_otlp_requests_commit_only_complete_siblings_and_exact_partial_success`
and
`pg_tests::all_invalid_and_request_wide_failures_leave_no_queryable_rows` across protobuf,
JSON, and gRPC. Use current fault injection for durability failure and current
auth/size/schema/tenant/admission fixtures. Assert the standard partial-success
response's rejected count and stable first reason; query the accepted siblings
and prove rejected siblings absent; retry and prove no duplicate rows. For
whole-request errors, assert the public error, no `partial_success`, and no
queryable rows.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=negative::pg_tests::mixed_otlp_requests_commit_only_complete_siblings_and_exact_partial_success)' --run-ignored=all"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=negative::pg_tests::all_invalid_and_request_wide_failures_leave_no_queryable_rows)' --run-ignored=all"
```

**GREEN.** Correct only the production Gate/Scribe/outcome owner implicated by
the failing assertion. Reuse the existing signal encoders and recovery probes.
WAL fences, payload digests, and contiguous ordinals belong in existing
lower-tier integration tests; add or extend one only if inspection shows that
mechanical invariant is otherwise uncovered. They are not journey assertions.

**REFACTOR.** No sleeps, best-effort observation, test-only projection oracle,
or durability bypass.

### Scenario 5 — Canonical Arrow writes and public SQL reads in every language

**Behavior.** Rust, Python, and TypeScript each build canonical Arrow batches
from the public table contract, write trace/GenAI, log, and representative
metric records through their public SDK, flush/publish, and answer useful
questions through canonical SQL. MCP reads the same three canonical tables and
observes the same payload gate. Maps REQ-004, REQ-009, REQ-017, REQ-023,
INV-006–INV-007, AC-002, AC-005, AC-011.

**RED.** Extend existing owners with:

- Rust `pg_tests::canonical_signal_arrow_write_and_sql_read_round_trip`
- Python `test_canonical_signal_arrow_write_and_sql_read_round_trip`
- TypeScript `canonical signal Arrow write and SQL read round-trip`
- MCP `query::pg_tests::agent_reads_canonical_trace_genai_logs_and_metrics_through_sql`

Each language test performs one complete public journey:

1. Obtain the three public canonical table descriptions and use their
   `writable_schema` projections; do not hard-code a second Arrow schema.
2. Write a parent/child trace containing an event, a link, error status,
   resource/scope metadata, promoted GenAI operation/provider/model/
   conversation/input-token/output-token fields, and structured
   `gen_ai.input.messages` / `gen_ai.output.messages`.
3. Write a trace-correlated error log with body, severity, attributes,
   resource, and scope.
4. Write representative counter, gauge, and histogram points. Scenario 1—not
   these tests—owns exhaustive metric-kind fidelity.
5. Flush/publish and run public SQL that proves: trace hierarchy; GenAI model
   and token filtering/aggregation; the correlated error log; and one metric
   aggregate. Require successful terminal metadata and literal expected values.
6. Query a structured GenAI payload column without payload permission and
   assert `WYRD_VALA_403_PAYLOAD_FORBIDDEN`; repeat with authorized access and
   assert the structured messages.

The SDK SQL calls are the required public API read proof. Do not add a separate
direct-HTTP read suite that repeats them. The MCP test uses the public MCP query
tool after seeding the shared canonical dataset through the existing public
Rust Arrow writer. It reads all three tables, asks at least one GenAI/token
question, and asserts both authorized payload readback and the same forbidden
projection.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p vala-sdk --test pg_bifrost_e2e -P journey -E 'test(=pg_tests::canonical_signal_arrow_write_and_sql_read_round_trip)' --run-ignored=all"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd python/py-wyrd && mise run py:setup:testing && uv run pytest -q -m integration tests/integration/test_bifrost_query.py::test_canonical_signal_arrow_write_and_sql_read_round_trip"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd typescript/wyrd && mise run ts:build && mise run ts:build:testing && pnpm exec vitest run tests/integration/oracle-query.test.ts -t 'canonical signal Arrow write and SQL read round-trip'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey -E 'test(=query::pg_tests::agent_reads_canonical_trace_genai_logs_and_metrics_through_sql)' --run-ignored=all"
```

**GREEN.** Update the existing language/MCP conversions and fixtures only where
T03A's generated/public contract does not already close them.

**REFACTOR.** Language runtimes test their own lifetime; no Python/Node runtime
is embedded in Rust and no SDK reimplements server mapping.

### Scenario 6 — Existing topology owners retain representative complete signals

**Behavior.** The existing standalone Scribe and distributed Oracle journeys
each carry one complete GenAI span through their existing topology while their
Forge/Oracle authority and tenant tripwires remain. Maps INV-007, INV-010,
AC-007, AC-009, AC-011.

**RED.** Extend the existing Scribe `write_read` and Oracle peer-network
journeys, not the exhaustive OTLP dataset, with one representative complete
GenAI span: parent/child identity, event, link, error status, resource/scope,
promoted GenAI fields, and structured input/output messages. Assert exact
tenant-isolated readback. Keep their existing assertions and
names; add the canonical signal assertion inside
`write_read::scribe_write_flush_read_user_journey` and
`peer_network::analytical::stage_graph_executes_representative_query_styles`.
Run the exact focused stage-graph command plus the owning Scribe lane.

```bash
mise run test:bifrost:journey:scribe
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=peer_network::analytical::stage_graph_executes_representative_query_styles)' --run-ignored=all"
```

**GREEN.** Adapt only shared fixtures/schema consumers exposed by the new
built-ins. Do not change planner selection, Forge publication, or topology.

**REFACTOR.** One GenAI span per topology is enough; logs and metrics are
already proven through the public ingest/read journeys and do not need to be
copied into topology tests.

## Expected write set and consumer closure

- Preserve the restored `crates/wyrd/wyrd-testing/tests/bifrost/otlp/` target,
  its lane, and its four completed raw tests. Extend its shared trace fixture;
  add the three stock Rust exporter tests to the existing trace/log/metric
  modules and test/dev dependencies to `crates/wyrd/wyrd-testing/Cargo.toml`.
- Finish the existing edits in
  `crates/wyrd/wyrd-testing/src/python.rs`,
  `python/py-wyrd/pyproject.toml`, `python/py-wyrd/uv.lock`, and
  `python/py-wyrd/tests/integration/test_bifrost_e2e.py`.
- Add `typescript/wyrd/tests/integration/otel-export.test.ts`, the upstream
  test/dev dependencies and lockfile update, and include the file in the
  existing `test:bifrost:journey:typescript` enumeration.
- Add canonical Arrow all-signal journeys to the existing
  `crates/vala/vala-sdk/tests/pg_bifrost_e2e.rs`,
  `python/py-wyrd/tests/integration/test_bifrost_query.py`, and
  `typescript/wyrd/tests/integration/oracle-query.test.ts` owners.
- Extend `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs` with the named
  all-signal SQL read journey.
- Extend the existing Scribe `write_read` and Oracle
  `peer_network::analytical` tests named in Scenario 6.
- Touch production owners only when one of these public journeys exposes a
  defect; record that defect and fix in the implementation report.

Other than the earned OTLP journey target and lane, no new Rust target, harness,
topology abstraction, production dependency, migration, branch controller,
collector process, telemetry wrapper, or duplicated exhaustive dataset belongs
here.

## Complete verification and evidence

Run every named scenario command, then the canonical lanes required by revision
11:

```bash
mise run test:bifrost:journey:otlp
mise run test:bifrost:journey:sdk
mise run test:bifrost:journey:mcp
mise run test:bifrost:journey:python
mise run test:bifrost:journey:typescript
mise run test:bifrost:journey:scribe
mise run test:bifrost:journey:oracle
mise run codegen:check
mise run fmt
mise run lints
mise run py:format
mise run py:lints
mise run py:typecheck
mise run ts:typecheck
mise run verify:bifrost
mise run gate
git diff --check
```

The implementation report must map each AC to the existing owner above, record
the four completed Scenario 1 tests/commits separately from new evidence, and
record every remaining named test result, partial-success/retry result,
authorized/redacted result, public language result, topology result, upstream
SDK limitation, generated drift result, and final aggregate gate.

## Material stop conditions

- Any acceptance obligation requires another new test target or topology rather
  than the earned OTLP target and declared existing owners.
- A public language cannot construct the canonical Arrow schema from the
  generated/table-owned contract without copying server semantics.
- A pinned upstream OpenTelemetry SDK cannot export one of its supported
  trace, log, or metric paths directly to the Wyrd OTLP endpoint. Stop and
  report the exact package/API limitation; do not silently substitute a raw
  request or Wyrd wrapper.
- A later change materially alters the reconciled Forge, Scribe, or Oracle
  owners before this task begins.
- A required failure cannot be induced through an existing production fault
  seam without weakening or adding test-only durable behavior.

## Authority links

- `AGENTS.md`
- `TESTING.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/wyrd-security-posture.md`
- `architecture/references/domain/telemetry-observations.md`
- `architecture/references/domain/analytical-operations-reliability.md`
- `architecture/references/languages/agent-harness.md`
- `architecture/references/languages/testing-workflows.md`
- `crates/wyrd/wyrd-testing/tests/README.md`
- `crates/vala/vala-bifrost-redux/tests/README.md`
- `changes/active/bifrost-canonical-otel-signals/spec.md`

## Implementation report

### Scenario 1 — completed and committed before this report

| Test | Owner | Commit |
|---|---|---|
| `trace_export::pg_tests::otlp_grpc_maximal_trace_round_trips_every_field` | `crates/wyrd/wyrd-testing/tests/bifrost/otlp/trace_export.rs` | `a79a231d0` |
| `trace_export_http::pg_tests::otlp_http_protobuf_and_json_match_grpc_trace_rows` | `.../otlp/trace_export_http.rs` | `969df61a0` |
| `logs_export::pg_tests::otlp_log_routes_round_trip_body_context_and_redaction` | `.../otlp/logs_export.rs` | `18eaf9b04` |
| `metrics_export::pg_tests::otlp_metric_routes_round_trip_every_supported_point_kind` | `.../otlp/metrics_export.rs` | `18eaf9b04` |

`02e953b8e` is the production fix those four tests demanded: Oracle's optimized
canonical SQL plan now enforces the declared sensitive payload columns, and
clients reconstruct `WYRD_VALA_403_PAYLOAD_FORBIDDEN` instead of surfacing it as
a retryable 502.

### Named test results

| Named test | Owner | Result |
|---|---|---|
| `stock_rust_otel_tracer_exports_genai_span_to_bifrost` | `.../otlp/trace_export.rs:415` | PASS (`6d270df00`) |
| `stock_rust_otel_logger_exports_correlated_log_to_bifrost` | `.../otlp/logs_export.rs:277` | PASS (`6d270df00`) |
| `stock_rust_otel_meter_exports_representative_metrics_to_bifrost` | `.../otlp/metrics_export.rs:507` | PASS (`6d270df00`) |
| `test_standard_otel_tracer_exports_to_bifrost` | `python/py-wyrd/tests/integration/test_bifrost_e2e.py:735` | PASS (`137d3c7dd`) |
| `test_stdlib_logging_exports_to_bifrost` | same file `:830` | PASS |
| `test_standard_otel_metrics_export_to_bifrost` | same file `:890` | PASS |
| `stock OpenTelemetry tracer exports GenAI span to Bifrost` | `typescript/wyrd/tests/integration/otel-export.test.ts:97` | PASS (`27aed248e`) |
| `stock OpenTelemetry logger exports correlated log to Bifrost` | same file `:216` | PASS |
| `stock OpenTelemetry meter exports representative metrics to Bifrost` | same file `:279` | PASS |
| `mixed_otlp_requests_commit_only_complete_siblings_and_exact_partial_success` | `.../otlp/negative.rs:209` | PASS (replayed exports, exactly-once rows) |
| `gate::otlp_batch_id_tests::derived_identity_is_stable_per_tenant_table_and_payload` | `crates/vala/vala-bifrost-redux/src/gate/mod.rs` | PASS |
| `tables::logs::tests::maximal_log_projection_preserves_body_context_and_presence` | `crates/vala/vala-bifrost-redux/src/tables/logs/mod.rs` | PASS (was failing on `integration:redux` before this change) |
| `all_invalid_and_request_wide_failures_leave_no_queryable_rows` | `.../otlp/negative.rs:366` | PASS |
| `pg_tests::canonical_signal_arrow_write_and_sql_read_round_trip` | `crates/vala/vala-sdk/tests/pg_bifrost_e2e.rs:2369` | PASS (`ff7995823`, `fbcefe607`) |
| `test_canonical_signal_arrow_write_and_sql_read_round_trip` | `python/py-wyrd/tests/integration/test_bifrost_query.py` | PASS |
| `canonical signal Arrow write and SQL read round-trip` | `typescript/wyrd/tests/integration/oracle-query.test.ts` | PASS (`b5838a649`) |
| `query::pg_tests::agent_reads_canonical_trace_genai_logs_and_metrics_through_sql` | `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs` | PASS (`8c635c224`) |
| `write_read::scribe_write_flush_read_user_journey` | `crates/wyrd/wyrd-testing/tests/bifrost/scribe/write_read.rs` | PASS (`22accd60e`) |
| `peer_network::analytical::stage_graph_executes_representative_query_styles` | `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/analytical.rs` | PASS (`97d09307c`) |

Partial-success and whole-request refusal results, authorized and redacted
payload results, and every metric kind are recorded in the owning tests above;
they are not restated per topology.

### Production defects these public journeys exposed

1. **No public Arrow batch write door.** The canonical signal tables declare
   binary, fixed-size-binary and nested list/struct columns, which the JSON row
   path cannot express, so no public caller could write one. Fixed by
   `3d88a3a14` (Rust SDK) and `81aa8eaef` (Python and TypeScript projections).
2. **Canonical ingress demanded server-owned field identity back from the
   writer.** A batch built from `describe_table`'s published schema was refused
   because the comparison included the stable field id and sensitivity tag the
   server assigns and `writable_schema` deliberately drops. Fixed by
   `fac83e240`: the user block is compared by shape, and the server re-stamps
   identity.
3. **Sensitive payload columns were unenforced on Oracle's optimized canonical
   plan.** Fixed by `02e953b8e` before this report.
4. **The OTLP ingress minted a fresh batch id per request, so an at-least-once
   exporter retry double-wrote every accepted row.** Fixed by deriving the
   batch identity at the Gate — see the finding below.
5. **`tables::logs::tests::maximal_log_projection_preserves_body_context_and_presence`
   still asserted the contract defect 2 replaced.** `fac83e240` made
   `validate_canonical_user_batch` compare the user block by shape and restamp
   server-owned identity, so the test's assertion that a drifted
   `parquet_field_id` is *rejected* had been failing on `integration:redux`
   since that fix. Rather than delete the coverage, the assertion now proves the
   live invariant: the drifted batch validates, and the validated schema carries
   the ledger's stable id and sensitivity, so a caller cannot relabel a
   sensitive column.

### Deviations and limitations

- Scenario 4's retry-dedup leg now replays each mixed-validity OTLP export
  byte-identically over gRPC, OTLP/HTTP JSON, and OTLP/HTTP protobuf, and
  asserts each accepted sibling is queryable exactly once. It exposed a
  production defect, fixed below.
- Scenario 6 adds a `Flush` control request and a foreign-tenant public
  credential to `BifrostProcessCluster`. Neither carries data: the first is the
  publication step a deployment reaches on its own timer, taken explicitly so a
  journey that wrote through a public ingest door can read deterministically;
  the second is an ordinary API key for a second data tenant. The canonical span
  itself enters through the Scribe pod's public OTLP route and leaves through
  the leader Oracle's public query listener.

### Fixed — the OTLP ingress had no client-retry idempotency

The durable batch fence (`vala.scribe_batch_commits`, keyed
`(data_tenant_id, logical_table_fqn, batch_id)`) is consulted on the live commit
path in `scribe/shards.rs::commit_batch_control_fence`, not only during WAL
replay. A second arrival of the same `batch_id` with a matching logical digest
resolves to `AlreadyCommitted` and writes nothing, so batch identity is
genuinely idempotent. The SDK gRPC path already supplied that identity:
`vala-sdk/src/grpc.rs::send_owned_bytes` reuses one client-minted
`wyrd_batch_id` across every retry attempt.

The OTLP path did not. `gate/mod.rs` minted `uuid::Uuid::now_v7()` server-side
per request, so a retried OTLP export was a new batch the fence could not match
and its rows were written a second time — ordinary exporter behavior (the server
commits, the response is lost, the upstream OTel SDK retries) corrupting exactly
the GenAI aggregates these tables exist to serve.

Fixed in `gate/mod.rs::otlp_batch_id`: `dispatch_canonical` now derives the
`wyrd_batch_id` deterministically as SHA-256 over the domain separator
`wyrd.otlp.batch-id.v1`, the authenticated `DataTenantId`, the logical table
FQN, and the canonical batch's digest from
`scribe::preprocess::logical_data_identity`, stamped with the RFC 4122 variant
and UUID version 7 bits the native path validates. The logical digest excludes
the request-scoped managed columns (request id, ingest time), so a retry with
new request metadata converges; tenant and table scoping keeps identity from
correlating across either boundary. The existing WAL and Postgres fence owns the
duplicate decision, and a digest collision with a different durable identity
stays fail-closed through the existing fence contradiction. Native SDK ingestion
is unchanged. Applies uniformly to traces, logs, and metrics on OTLP/HTTP
protobuf, OTLP/HTTP JSON, and OTLP/gRPC.

Proven by `gate::otlp_batch_id_tests::derived_identity_is_stable_per_tenant_table_and_payload`
(same input converges; different tenant, table, row order, or payload diverges;
value is a UUIDv7) and by Scenario 4's replayed journey above.

### Verification actually run

| Command | Result |
|---|---|
| `mise run test:bifrost:journey:otlp` | PASS |
| `mise run test:bifrost:journey:sdk` | PASS |
| `mise run test:bifrost:journey:mcp` | PASS |
| `mise run test:bifrost:journey:python` | PASS |
| `mise run test:bifrost:journey:typescript` | PASS |
| `mise run test:bifrost:journey:scribe` | PASS |
| `mise run test:bifrost:journey:oracle` | PASS |
| `mise run codegen:check` | PASS (no generated drift) |
| `mise run fmt` / `lints` | PASS; `lints` first failed on two `needless_pass_by_value` N-API arguments, fixed in `76e32a6c3` |
| `mise run py:format` / `py:lints` / `py:typecheck` | PASS |
| `mise run ts:typecheck` | PASS |
| `git diff --check` | PASS |

The Gate batch-identity fix is production code in `vala-bifrost-redux`, so
`mise run verify:bifrost` was run for this change and passed. Commands run for
the OTLP retry-idempotency fix:

| Command | Result |
|---|---|
| `mise run fmt` | PASS |
| `mise run lints` | PASS (first run flagged two `doc_markdown` items; fixed) |
| `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::otlp_batch_id_tests::derived_identity_is_stable_per_tenant_table_and_payload)'` | PASS |
| `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=negative::pg_tests::mixed_otlp_requests_commit_only_complete_siblings_and_exact_partial_success)' --run-ignored=all"` | PASS |
| `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=negative::pg_tests::all_invalid_and_request_wide_failures_leave_no_queryable_rows)' --run-ignored=all"` | PASS |
| `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all"` | PASS (9/9) |
| `mise run verify:bifrost` | 8/9 lanes PASS. `journey:oracle` failed once on `analytical_activation::selected_peer_failure_is_terminal` (`pod 2 activated 1 leases and still holds 1`) while a second verification run was competing for CPU on the same 4-core host. |
| `mise run test:bifrost:journey:oracle` (rerun, uncontended) | PASS 28/28 |
| `git diff --check` | PASS |

`mise run gate` was not run: the write set is one Gate function, its unit test,
and one journey test's assertions, all inside the Bifrost capability the lane
above gates at full scope.
