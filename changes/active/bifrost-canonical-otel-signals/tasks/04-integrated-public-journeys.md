---
id: BIFROST-OTEL-T04
title: Prove canonical signals through every existing public journey owner
kind: implementation
mode: DECOMPOSE
status: proposed
spec: SPEC-bifrost-canonical-otel-signals
spec_revision: 7
depends_on: [BIFROST-OTEL-T02, BIFROST-OTEL-T03, BIFROST-OTEL-T03A]
requirements: [REQ-002, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-020, REQ-021]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011]
---

# Existing-harness public fidelity, failure, and topology closure

## Outcome and value

Real clients write one shared maximal dataset through OTLP HTTP protobuf/JSON
and canonical Arrow, while stock OpenTelemetry Python SDK exporters send
traces, standard-library logs, and metrics through OTLP/gRPC. Every path
crosses Scribe ACK, publication, and Oracle before exact canonical readback
through its owning public client. Existing topology owners prove recovery and
the representative distributed paths; no parallel harness or duplicate
exhaustive suite is created.

Required execution skill: `$wyrd-implement`.

## Owners, scope, consumers, and prohibited changes

- Populate the existing empty `wyrd-testing` `otlp` files. Add plain module
  declarations for `logs_export`, `metrics_export`, `mixed_batch`, `negative`,
  `support`, `trace_export`, and `trace_export_http` to its existing empty
  `main.rs`; do not create another target or topology fixture. Each test module
  uses the documented inner `mod pg_tests` organization.
- Reuse `WyrdTestServer`, current Postgres lifecycle, public SDKs, Scribe flush,
  Oracle query, publication/restart controls, auth fixtures, and telemetry
  checkpoints.
- Extend existing Rust SDK, server, MCP, Python, TypeScript, standalone,
  distributed, single-tenant, and multi-tenant owners only for the boundary
  each uniquely proves.
- Extend the existing Python `WyrdTestServer` projection with only the
  access-token exchange needed by an unmodified OTLP exporter; reuse the bound
  gRPC URL it already publishes as `WYRD_GRPC_URL`. Add the already-used
  OpenTelemetry gRPC exporter package to the Python development group and
  lockfile; do not add a Wyrd telemetry wrapper, exporter adapter, collector
  process, or second server fixture.
- One canonical expected dataset in OTLP `support.rs` supplies all signal
  values. Matching Arrow batches are built from the public describe response
  and T03's Rust/Python/TypeScript `writable_schema` helpers, then sent through
  existing insert APIs. Expected DTOs are literal assertions over those fixture
  values, not another projector. Tests must not contain a second semantic
  mapper or column-order oracle.
- Do not weaken test sizes, durability assertions, permissions, terminal
  frames, timeouts, topology, or ignored-test policy to make the lane pass.

## Fixed integration order

Complete and integrate BIFROST-OTEL-T01 through T04 on `oracle-distributed`
before the separate draft `bifrost-forge-oracle-integration` change is merged.
That later change must rebase/merge onto this canonical target and treat the
three-table registry and signal-neutral Scribe boundary as retained authority;
it may not restore deleted tables, direct projectors, or old query DTOs. This
packet does not depend on the unapproved draft change and does not edit its
spec/task files.

## Ordered implementation scenarios

### Scenario 1 — OTLP protobuf and JSON retain complete signals

**Behavior.** Each exposed HTTP protobuf/JSON and gRPC route accepts the shared
maximal trace, log, and all-kind metrics dataset, acknowledges only after the
Scribe boundary, flushes/publishes, and reads every value exactly. Maps
REQ-002, REQ-004, REQ-006–REQ-018, INV-001–INV-009, AC-001, AC-003–AC-005.

**RED.** Populate the existing modules with these named tests:

- `trace_export::pg_tests::otlp_grpc_maximal_trace_round_trips_every_field`
- `trace_export_http::pg_tests::otlp_http_protobuf_and_json_match_grpc_trace_rows`
- `logs_export::pg_tests::otlp_log_routes_round_trip_body_context_and_redaction`
- `metrics_export::pg_tests::otlp_metric_routes_round_trip_every_supported_point_kind`

Each uses the same support dataset, real server and public reads; trace asserts
ordered events/links and structured GenAI content, log asserts authorized and
unauthorized payload behavior, and metrics asserts integer/double distinction,
all kind-specific values, metadata and exemplars. They fail because the files
are empty and storage is incomplete.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=trace_export::pg_tests::otlp_grpc_maximal_trace_round_trips_every_field)' --run-ignored=all"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=trace_export_http::pg_tests::otlp_http_protobuf_and_json_match_grpc_trace_rows)' --run-ignored=all"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=logs_export::pg_tests::otlp_log_routes_round_trip_body_context_and_redaction)' --run-ignored=all"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=metrics_export::pg_tests::otlp_metric_routes_round_trip_every_supported_point_kind)' --run-ignored=all"
```

**GREEN.** Add only fixture/client assertions and any missing production wiring
revealed across the T01–T03 owners. Use explicit flush/publication and Oracle
terminal success; HTTP JSON/protobuf fixtures encode the same pinned request.

**REFACTOR.** Shared support constructs data; each test owns its boundary and
assertions. No test calls another test or reproduces table projection logic.

### Scenario 2 — Stock Python OpenTelemetry emitters reach Bifrost over gRPC

**Behavior.** An ordinary Python application can configure the upstream
OpenTelemetry SDK and gRPC exporters, emit traces, standard-library logs, and
metrics to the bound Wyrd server, force exporter completion, and read the exact
emitted values from the canonical Bifrost tables. This interoperability proof
is additive to Scenario 1's exhaustive pinned-wire coverage and Scenario 5's
canonical Arrow SDK coverage. Maps REQ-002, REQ-004, REQ-006–REQ-012,
REQ-015–REQ-018, INV-003, INV-006–INV-009, AC-001, AC-003–AC-005, AC-008,
AC-011.

**RED.** Add these three integration tests to the existing gated Python journey
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
Do not use sleeps, random data, private extension imports, hand-built OTLP
requests, or an OpenTelemetry Collector.

The trace emits fixed-name parent and child spans, captures their generated
identifiers and asserts their relationship, and includes scalar/array
attributes, an event, a link, status, resource attributes, and an explicit
instrumentation scope. The stdlib log uses a dedicated `logging.Logger` with OTel's
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

**GREEN.** Add `opentelemetry-exporter-otlp-proto-grpc` beside the existing
OpenTelemetry Python development dependencies and regenerate `uv.lock`. On the
existing Python `WyrdTestServer`, add one `access_token()` method that exchanges
its retained API key through the Rust harness's existing `exchange_api_key`
owner; continue reading its already-published `WYRD_GRPC_URL` rather than
adding another endpoint projection. Fix only production OTLP, table projection,
publication, or public-query owners exposed by the failing end-to-end
assertions.

**REFACTOR.** Keep all telemetry construction on upstream OpenTelemetry types,
keep credentials out of assertion output, remove logging handlers and shut down
all exporting providers deterministically, tolerate the pinned SDK's
`LoggingHandler` deprecation warning without adding an instrumentation package,
and share only small Python setup/read helpers in `test_bifrost_e2e.py` when
endpoint/header/query code actually repeats. The exhaustive raw OTLP dataset
and canonical Arrow journeys remain separate owners.

### Scenario 3 — OTLP and Arrow produce equivalent accepted rows

**Behavior.** Equivalent logical records written through OTLP and public Arrow
produce equal user columns and typed results, excluding trusted batch/request/
ingest identities. Maps REQ-004, INV-003, AC-002, AC-008.

**RED.** Add
`mixed_batch::pg_tests::otlp_and_canonical_arrow_share_user_rows_and_public_results` to
the current OTLP binary, using the public Rust SDK Arrow writer for the derived
canonical batches. Query both disjoint batch IDs after publication and compare
every user field by stable field name/ID plus typed trace/log/metric results.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=mixed_batch::pg_tests::otlp_and_canonical_arrow_share_user_rows_and_public_results)' --run-ignored=all"
```

**GREEN.** Fix only convergence/SDK schema generation defects in their owning
modules. Do not normalize results in the fixture.

**REFACTOR.** The comparison ignores only enumerated server-managed identity
and receipt-time fields, never a signal field.

### Scenario 4 — Partial success and whole-request failures keep exact authority

**Behavior.** Mixed trace/log/metric requests return exact standard partial
success after one accepted-subset fence; all-invalid creates none; request-wide
failures commit none. Maps REQ-005, INV-007–INV-009, AC-006–AC-007.

**RED.** Populate `negative.rs` with
`pg_tests::mixed_otlp_requests_commit_only_complete_siblings_and_exact_partial_success`
and
`pg_tests::all_invalid_and_request_wide_failures_leave_no_wal_or_fence` across protobuf,
JSON, and gRPC. Use current fault injection for durability failure and current
auth/size/schema/tenant/admission fixtures. Assert accepted relative order,
contiguous ordinals, stable first reason, invalid-row absence, exact fence and
payload digest, recovery/retry duplicate suppression, and no partial_success
for whole-request errors.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=negative::pg_tests::mixed_otlp_requests_commit_only_complete_siblings_and_exact_partial_success)' --run-ignored=all"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=negative::pg_tests::all_invalid_and_request_wide_failures_leave_no_wal_or_fence)' --run-ignored=all"
```

**GREEN.** Correct only the production Gate/Scribe/outcome owner implicated by
the failing assertion. Reuse the existing signal encoders and recovery probes.

**REFACTOR.** No sleeps, best-effort observation, test-only projection oracle,
or durability bypass.

### Scenario 5 — Existing language and agent surfaces close their own boundary

**Behavior.** Canonical Arrow write and typed trace/GenAI reads work through
the first-class SDK projections and MCP with the same payload gates. Maps
REQ-004, REQ-009, REQ-017, INV-006–INV-007, AC-002, AC-005, AC-011.

**RED.** Extend existing owners with:

- Rust `pg_tests::canonical_signal_arrow_write_and_typed_reads_round_trip`
- Python `test_canonical_signal_arrow_write_and_typed_reads_round_trip`
- TypeScript `canonical signal Arrow write and typed reads round-trip`
- MCP `query::pg_tests::agent_reads_canonical_trace_and_genai_without_stale_tables`

Each asserts its native public values and unauthorized payload behavior; it
does not repeat every metric kind already proved in Scenario 1.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p vala-sdk --test pg_bifrost_e2e -P journey -E 'test(=pg_tests::canonical_signal_arrow_write_and_typed_reads_round_trip)' --run-ignored=all"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd python/py-wyrd && mise run py:setup:testing && uv run pytest -q -m integration tests/integration/test_bifrost_query.py::test_canonical_signal_arrow_write_and_typed_reads_round_trip"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd typescript/wyrd && mise run ts:build && mise run ts:build:testing && pnpm exec vitest run tests/integration/oracle-query.test.ts -t 'canonical signal Arrow write and typed reads round-trip'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey -E 'test(=query::pg_tests::agent_reads_canonical_trace_and_genai_without_stale_tables)' --run-ignored=all"
```

**GREEN.** Update the existing language/MCP conversions and fixtures only where
T03's generated/public contract does not already close them.

**REFACTOR.** Language runtimes test their own lifetime; no Python/Node runtime
is embedded in Rust and no SDK reimplements server mapping.

### Scenario 6 — Existing topology owners retain representative complete signals

**Behavior.** Standalone/distributed and single-/multi-tenant paths each write
and read at least one complete canonical signal while existing Forge/Oracle
authority and tenant tripwires remain. Maps INV-007, INV-010, AC-007, AC-009,
AC-011.

**RED.** Extend the existing Scribe `write_read` and Oracle peer-network
journeys, not the exhaustive OTLP dataset, with one representative maximal span
fixture and exact tenant-isolated readback. Keep their existing assertions and
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

**REFACTOR.** One representative signal per topology is enough; exhaustive
fidelity remains in Scenario 1.

## Expected write set and consumer closure

- Existing empty `crates/wyrd/wyrd-testing/tests/bifrost/otlp/*.rs` files
- Existing `vala-sdk` `pg_bifrost_e2e` target
- Existing Python and TypeScript Bifrost integration files
- Existing Python `WyrdTestServer` binding, Python development dependency
  group, and `uv.lock`
- Existing `wyrd-mcp/tests/bifrost/mcp/query.rs`
- Existing Scribe/Oracle journey fixtures and only production owners found
  defective by these RED proofs

No new Rust target, harness, topology abstraction, production dependency,
migration, branch controller, collector process, telemetry wrapper, or
duplicated exhaustive dataset belongs here.

## Complete verification and evidence

Run every named scenario command, then the canonical lanes required by revision
7:

```bash
mise run test:bifrost:journey:otlp
mise run test:bifrost:journey:sdk
mise run test:bifrost:journey:mcp
mise run test:bifrost:journey:python
mise run test:bifrost:journey:typescript
mise run codegen:check
mise run fmt
mise run lints
mise run verify:bifrost
mise run gate
git diff --check
```

The implementation report must map each AC to the existing owner above, record
every RED/GREEN result, exact ACK/fence/recovery evidence, authorized/redacted
results, public language results, topology results, generated drift result, and
the final aggregate gates.

## Material stop conditions

- Any acceptance obligation requires a new test target or topology rather than
  the declared existing owners.
- A public language cannot construct the canonical Arrow schema from the
  generated/table-owned contract without copying server semantics.
- The separate Forge/Oracle integration lands first and restores or materially
  changes a shared table/Scribe/query owner; re-run `$wyrd-plan` reconciliation
  before implementation rather than merging both designs ad hoc.
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
