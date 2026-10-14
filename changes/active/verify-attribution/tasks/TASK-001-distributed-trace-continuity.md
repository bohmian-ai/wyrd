---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-verify-attribution
spec_revision: 2
requirements: [REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-002, AC-003, AC-004, AC-005]
depends_on: []
parent_task:
remediates: []
---

# TASK-001 Distributed trace continuity, dual export, and the action timeline

## Outcome and Value

A Service instrumented with any Wyrd SDK runs one client trace through
observations, Bifrost writes, a gateway model call, and real-time verification
with an Eval `llm_judge`. One SDK read returns the whole connected, time-ordered
client → server → client tree from the tenant's `vala.traces.spans`, and the
operator's trace backend receives the same server spans with the same ids. The
revision 1 closeout lands with it. This delivers every obligation of
SPEC-verify-attribution r2: REQ-004 closeout, REQ-005 through REQ-009,
INV-001 through INV-005, and AC-001 through AC-005.

## Owners, Scope, Consumers, and Prohibited Changes

- **Server continuity:** `crates/wyrd/wyrd-server` owns this: the HTTP router
  request span, the trace-context middleware, gRPC services, MCP,
  verification, and the gateway invocation and capture.
- **Parent-span loss:** the `vala-eval` executor fan-out loses the parent span
  today and must carry it.
- **Telemetry pipeline:** `crates/shared/wyrd-telemetry` owns the one process
  tracer provider, its sampler, the operator exporter, the tenant span
  processor, the non-exported tenant routing registry (Scenario 6), and test
  capture. The tenant processor stages through a sink the server binds after
  the outbox is built. `wyrd-telemetry` must not depend on `wyrd-server`; the
  routing types it needs (`DataTenantId`, `PrincipalId`) already live in
  `wyrd-spec`, which it depends on.
- **Tenant writes:** these stay on the one Scribe write path: the process
  `ScribeOutbox`, a `ScribeWrite` variant for server spans, and
  `ScribeTable::Spans`. Rows are projected with the existing
  `vala_bifrost_redux::tables::traces::project_resource_spans`.
- **Client propagation:** `crates/shared/wyrd-client` owns client propagation:
  the HTTP transport builder sites, MCP replay, and Bifrost gRPC metadata.
  The three SDK packages project it: the Rust SDK `otel.rs`, the Python
  binding entry and `otel.py`, and the TypeScript native entry and `otel.ts`.
- **Ingest contract:** `crates/wyrd/wyrd-tonic` owns `InsertBatchRequest` in
  `proto/wyrd.v1.proto` and gains the D4 producer contexts (Scenario 10).
  `crates/shared/wyrd-queue` owns `SealedBatch`, the stable per-batch retry
  state, and carries the sealed producer-context vector. The client gRPC
  transport builds the request from that sealed batch; Gate consumes it.
- **Timeline read:** `wyrd_client::Bifrost` owns the trace timeline read over
  the existing authenticated query path, returning the canonical
  `wyrd_spec::vala::trace::SpanRecord`. Python and TypeScript project the same
  operation and the same type (Scenario 12).
- **Deferred-run context:** persisted per D8 in this change's existing
  unshipped verifier-run migration.
- **Architecture:** `architecture/wyrd-design.md` (the `Wyrd-Request-Id`
  section near lines 735–753 and the telemetry and correlation sections) and
  `architecture/bifrost-design.md` (the Telemetry section and the
  `vala.traces.spans` writers).

**Invariants:**
- Tenant isolation.
- Credentials are never logged, exported, or returned.
- Audit and tenant export never block or fail a request.
- The operator export's content and sampling are independent of every
  client-supplied field, including the inbound trace id, sampled flag, and
  `tracestate` (INV-004, Scenario 5).
- The tenant id never becomes a span attribute. `pg_verification_runtime`
  already pins that for `verification.*` spans.

**Prohibited:**
- A second Scribe write path, writer, or staging table.
- A new server route for reading traces.
- Client-supplied trace context on presigned external storage requests.
- Operator export sampling that is parent-based on client flags.
- Exporting `gateway.attempt` tracing spans to the tenant in addition to the
  captured attempt rows.
- Compatibility aliases.
- Any change to authentication, authorization, or permission scopes.

**Non-goals:**
- sub/act principal columns (SPEC-sub-act-attribution).
- Run and Card correlation for Workflow `wyrd_gateway` steps.
- Mesh or service-to-service propagation outside Wyrd SDK requests.

## Reuse Map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Inbound context | `http/middleware/trace_context.rs` `attach_trace_context`, `active`, `within` | `invocation.rs:489`, `mcp/mod.rs:248`, `mcp/probe.rs:272`; `malformed_traceparent_is_ignored_by_the_gateway` | Task-local only. No span parent. Request span is `debug` and opened before the middleware (`router.rs:146,155`). Not read on gRPC. | Set the request span's OTel parent where the span is made from the request headers. Make it info and kind SERVER. Read the same context from gRPC metadata in the shared gRPC router. | none |
| Request-scoped propagation | tracing span context; `oracle/telemetry.rs` `install_query_span_propagation` | `vala-eval` `executor.rs:421` `JoinSet::spawn` (loses span and task-local) | Eval judge and gateway lose the caller trace | Carry the current span (and the context the gateway reads) across the executor spawn | none |
| Deferred links | `wyrd_telemetry::link_remote_span`; `eval.rs:184` | `eval_verification::continuous_eval_reads_ordered_bounded_trace_evidence` | Manual enqueue context not persisted | Persist the enqueuing context on the run (D8) and link at execution | none |
| Sampling and operator export | `wyrd_telemetry::init` (`lib.rs:326-367`), `telemetry_sampler` (`lib.rs:433-438`, `AlwaysOn` or `TraceIdRatioBased`), `init_test_capture`, `CapturedSpan` | `pg_verification_runtime::attempts_record_queue_wait_phases_terminal_latency_and_one_trace`, `gateway/native.rs` | One provider makes one recording decision before any processor runs, so a server `Drop` leaves nothing for the tenant. The ratio sampler keys on the client-chosen trace id. No provider without an endpoint. | Wrap the existing sampler (Scenario 5). Filter the existing operator batch processor to `RecordAndSample`. Build the provider with or without an endpoint. | The sampler wrapper and operator filter are the minimum that let one provider produce two independent decisions; OTel 0.31 offers no per-processor sampling. |
| Tenant span routing | none: `trace_context::INBOUND` carries only `TraceContext`; `Caller` is resolved after the request span starts; `SpanProcessor::on_end` sees only `SpanData` | `pg_verification_runtime.rs:865-877` (no tenant id in span attributes) | No non-exported carrier of authenticated tenant and principal from the request to the processor | Telemetry-owned routing registry keyed by server-generated `(TraceId, SpanId)` (Scenario 6) | Nothing existing carries authenticated identity to a span processor without exporting it as an attribute; a trace-id key is client-controlled and not tenant-unique. |
| Tenant span rows | `ScribeOutbox`, `ScribeWrite`, `ScribeTable::Spans`, `project_resource_spans`; gateway `CallCapture::spans_batch` | gateway capture staging `invocation.rs:942`; `ScribeBatch::into_frame` principal choice | No server-span write. Frames are stamped with `GATEWAY_CAPTURE_PRINCIPAL`. | New `ScribeWrite::ServerSpans` variant stamped with the routed principal (D6). Reuse the projector. Reparent capture attempts under the exported gateway call span. | none |
| Bounded span admission | `wyrd_runtime::Outbox` (`mpsc::unbounded_channel`, retries retained slices) shared by audit, capture, and verifier results | Scribe-outbox loss counter and its tests | A high-rate new producer on an unbounded shared queue grows the heap without limit during a sustained Scribe outage | Cap only the server-span producer with a fixed per-process permit pool held by the staged item (Scenario 7) | Existing audit, capture, and result semantics stay unchanged; one outbox and one writer remain. |
| SDK propagation | `wyrd-client` `observe/verify.rs` `current_trace_context`; transport builder sites in `transport/http.rs`; `bifrost/grpc.rs` `add_auth_metadata`; Python `active_trace_context`; TS `activeTraceContext` | `workflow_transport::public_gateway_call_does_not_inject_ambient_trace_context`; verify-only journeys | Only verify propagates (OD-3) | One authenticated-request builder injects the context on every request. Add gRPC metadata. Bindings capture the context at each entry. Restore the removed `d39fced02` pattern. | none |
| Queued write linking | `wyrd_queue::SealedBatch` (`sink.rs:96`, stable batch id, frame, rows, request id across ambiguous retry); `wyrd-tonic` `InsertBatchRequest` (fields 1-3); `BifrostGrpcTransport`; Gate `insert_batch` span `bifrost.gate.write` (`gate/mod.rs:1129`) | `gate/mod.rs:1128`; queue seal and retry tests | No per-row context, no PRODUCER span, no wire field, no CONSUMER links | Per-row optional context at admission, a sealed bounded vector on `SealedBatch`, proto field 4, and Gate links (Scenario 10) | none |
| Timeline read | `Bifrost::sql_as` (`facade.rs:801`) and `BifrostClientError` (`bifrost/query.rs:36`); `wyrd_spec::vala::trace::SpanRecord`; Python sync and async `Bifrost.sql` (`stubs/bifrost.pyi:639,800`); TS `Bifrost.sql` (`index.ts:1171`); journey helpers `judge_calls`, `gateway_spans` | `verify_in_real_time.rs:184` (cross-join defect), `gateway_inference.rs:229` | No standard read; no SDK projection of `SpanRecord` | `Bifrost::trace_timeline` returning `Vec<SpanRecord>`, projected to Python and TS with the same type (Scenario 12) | none; reuses the canonical span record and the query path |
| MCP continuity proof | `wyrd-mcp` `mcp` journey target (`tests/bifrost/mcp/main.rs`, `verification.rs`); lane `test:bifrost:journey:mcp` | `mcp/mod.rs:248` re-scopes the spawned task with `within` | No test that an MCP tool call continues the caller's trace | `verification::verification_tool_continues_trace_context` (Scenario 2) | none |

## Approach

1. Make the server continue inbound context as the parent of its request span on
   HTTP, MCP, and gRPC. Carry it through every request-scoped task boundary,
   including the Eval executor fan-out. Give deferred work links, persisting
   the manual-enqueue context.
2. Always build the one tracer provider. Wrap its sampler so the operator and
   tenant decisions are independent (D7, INV-004). Route authenticated
   requests to their tenant through a non-exported registry (D6), filter by
   the D5 allowlist, and stage on the Scribe outbox under a fixed span-permit
   cap (INV-005). The operator export is unchanged in content.
3. Reparent the gateway's captured attempt rows under the exported gateway call
   span, so the tree is consistent and nothing is written twice.
4. Inject trace context from every SDK request: HTTP, MCP, and gRPC. Add
   PRODUCER spans, sealed producer contexts, and proto field 4 on queued
   writes. Install the W3C propagator in each SDK's `start_telemetry`.
5. Ship `trace_timeline` in Rust, Python, and TypeScript over the canonical
   `SpanRecord`, with generated stubs and declarations.
6. Close out the revision 1 obligations. Prove everything with the
   action-timeline journey in each SDK.
7. Update the two architecture documents (REQ-009).

## Ordered Implementation Scenarios

### Scenario 1 — An inbound `traceparent` parents the server request span

**Behavior.**
- An HTTP request with a valid `traceparent` produces a SERVER-kind request
  span whose trace id and parent are the caller's.
- That span is exported at the production filter level.
- A malformed or repeated `traceparent` starts a new trace and the request
  succeeds.
- Covers REQ-005 and AC-003.

**RED.** Add `pg_router_smoke::inbound_traceparent_parents_the_request_span`
and `pg_router_smoke::malformed_traceparent_starts_a_new_trace`. They fail
because the request span is a debug-level root.

```
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(=pg_router_smoke::inbound_traceparent_parents_the_request_span) | test(=pg_router_smoke::malformed_traceparent_starts_a_new_trace)'"
```

**GREEN.** Set the OTel parent where the request span is made from the
request headers. Make the span info level and kind SERVER. Keep the existing
task-local for current consumers until the later scenarios replace them.

**REFACTOR.** Parse the context in one place, used by HTTP, MCP, and gRPC.

### Scenario 2 — gRPC and MCP requests continue the caller's trace

**Behavior.**
- A gRPC `insert_batch`, an OTLP export, a gRPC query carrying `traceparent`
  metadata, and an MCP tool call each produce server spans that descend from
  the caller's span.
- Malformed metadata is ignored.
- Covers REQ-005 and AC-001 (MCP).

**RED.**
- Add `pg_grpc_smoke::grpc_traceparent_parents_the_server_span`. It fails
  because no gRPC code reads trace metadata.
- Add `verification::verification_tool_continues_trace_context` to the
  existing `wyrd-mcp` `mcp` journey target. Through a real `rmcp` HTTP client,
  call the verification tool once with a valid sampled `traceparent` and once
  with malformed `traceparent` and `tracestate`. Assert that the valid call's
  SERVER request span is a child of the supplied parent and that its
  verification, judge, and gateway spans descend from it. Assert that the
  malformed call succeeds on a new trace. It fails because the request span is
  not yet parented; its judge and gateway descent clause goes green with
  Scenario 3.

```
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(=pg_grpc_smoke::grpc_traceparent_parents_the_server_span)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::verification_tool_continues_trace_context)'"
```

**GREEN.** Extract the context in the shared gRPC router so every public
service gets it. Keep MCP's per-task re-scope consistent with Scenario 1.
Rerun Scenario 1.

**REFACTOR.** Remove any per-service extraction that duplicates the shared
router.

### Scenario 3 — Verifier → judge → gateway is one subtree for Task and Eval verifiers

**Behavior.**
- A direct verify with a Task `llm_judge` produces this chain, each span a
  descendant of the request span: `verification.execute` →
  `verification.score` → `verification.judge` → `dispatch` →
  `workflow.gateway.call` → `gateway.call` → `gateway.execute`.
- An Eval `llm_judge` produces the same chain.
- The judge call records `card_uid` and `run_id`.
- Covers REQ-005, REQ-004 closeout, and AC-004.

**RED.** Add
`pg_verification_runtime::eval_judge_gateway_call_descends_from_the_request_span`.
It fails at the Eval executor spawn.

```
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(=pg_verification_runtime::eval_judge_gateway_call_descends_from_the_request_span)'"
```

**GREEN.** Carry the current span, and the context the gateway reads, across
`executor.rs` fan-out. Rerun Scenarios 1–2 and
`pg_verification_runtime::attempts_record_queue_wait_phases_terminal_latency_and_one_trace`.

**REFACTOR.** Have the gateway take its trace context from the current span
instead of the task-local wherever the span now carries it. Delete the
task-local if no consumer remains.

### Scenario 4 — Deferred verifier runs link to their cause

**Behavior.**
- A manually enqueued run stores its enqueuing context and its execution span
  links to that context.
- An observation-triggered run keeps its link to the observation.
- Neither run is parented by the enqueuing request.
- The observation run's judge call records `card_uid` and `run_id`, and its
  result joins its judge calls on `subject_card_uid` = `card_uid`.
- Covers REQ-005, D8, REQ-004, AC-003, and AC-004.

**RED.** Add `pg_verification_runtime::manual_run_links_to_its_enqueuing_request`
(fails: no stored context) and
`eval_verification::observation_judge_call_records_card_and_run_and_joins_its_result`
(fails if any clause is unmet). Confirm the selectors with
`mise exec -- cargo nextest list --locked -p wyrd-testing --test server`.

```
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(=pg_verification_runtime::manual_run_links_to_its_enqueuing_request)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=eval_verification::observation_judge_call_records_card_and_run_and_joins_its_result)'"
```

**GREEN.** Persist the context in the existing verifier-run migration and link
from it at execution. Rerun
`eval_verification::continuous_eval_reads_ordered_bounded_trace_evidence`.

**REFACTOR.** Share one link helper between the manual and observation origins.

### Scenario 5 — Operator and tenant sampling are independent

**Behavior.** One provider produces two independent decisions (D7, INV-004).
- **Operator decision:** the existing server policy (`AlwaysOn`, or the
  configured ratio). For a span whose parent is remote, the ratio is applied
  to a server-drawn random value, not to the inbound trace id, so no
  client-supplied field (trace id, sampled flag, `tracestate`) changes it.
  A span whose parent is local follows its local root's operator decision.
- **Tenant decision:** the request continued a client context whose sampled
  flag was set.
- **Recording:** operator accept → `RecordAndSample`. Operator reject with a
  sampled continued client context → `RecordOnly`. Otherwise → `Drop`.
- **Operator processor:** the existing batch processor sits behind a filter
  that admits only `RecordAndSample` spans, so operator queue content and
  volume are unchanged.
- **Matrix, with and without an operator endpoint configured:**

  | Server sampler | Client sampled | Operator | Tenant |
  |---|---|---|---|
  | accept | yes | exported | staged |
  | accept | no | exported | none |
  | reject | yes | none | staged |
  | reject | no | none | none |

- Both copies keep the same trace id, span id, and parent.
- Covers REQ-007, D7, INV-004, and AC-002.

**RED.**
- Unit tests in `wyrd-telemetry` for the sampler wrapper (all four rows, and a
  remote parent whose trace id would fall under the ratio does not change the
  operator decision) and for the operator filter. Confirm selectors with
  `mise exec -- cargo nextest list --locked -p wyrd-telemetry --lib`.
- Add `pg_verification_runtime::continued_request_spans_reach_operator_and_tenant`,
  driving the four matrix rows through a real client-traced verify with the
  test capture as the operator exporter. It fails because no tenant export
  exists.

```
mise exec -- cargo nextest run --locked -p wyrd-telemetry --lib
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(=pg_verification_runtime::continued_request_spans_reach_operator_and_tenant)'"
```

**GREEN.**
- Build the provider whether or not an endpoint is configured; with no
  endpoint there is no operator processor.
- Add the sampler wrapper, the operator filter, and the tenant processor with
  a sink bound after the outbox is built.
- Add `ScribeWrite::ServerSpans`, projected through `project_resource_spans`.

Rerun Scenarios 1–4.

**REFACTOR.** Fold the gateway capture's span projection and the server-span
projection onto the same projector call.

### Scenario 6 — Tenant rows are routed by authenticated identity, never by trace id

**Behavior.**
- After authentication succeeds, the server binds the request span's
  server-generated `(TraceId, SpanId)` to `TenantSpanRoute { tenant:
  DataTenantId, principal: PrincipalId, continued_sampled: bool }` in a
  routing registry owned by `wyrd-telemetry`. The registry is never exported
  and never becomes a span attribute.
- The tenant processor's `on_start` copies a parent's route to each new
  child. `on_end` reads and removes the ended span's route before
  non-blocking staging, so the registry holds only live spans.
- The request root stays bindable after its pre-authentication start. Spans
  started before binding, spans of a refused authentication, deferred roots,
  and any span without a route are operator-only.
- Only D5-catalog spans are staged, carrying only D5-allowlisted attributes.
  Disallowed attributes are dropped from the tenant copy only.
- Tenant rows are stamped with the routed principal (D6), never with the
  tenant as an attribute.
- Covers REQ-007, D5, D6, INV-001, INV-002, and AC-003.

**RED.**
- Unit tests in `wyrd-telemetry` for the D5 catalog and attribute filter, and
  for route inheritance and removal.
- Add `pg_verification_runtime::same_inbound_trace_id_routes_each_tenant_separately`:
  two tenants concurrently send requests carrying the same inbound trace id;
  each root and descendant row lands only under its own tenant and principal.
- Add `pg_verification_runtime::unrouted_spans_export_only_to_the_operator`:
  a request with no context, a refused authentication, deferred work, and a
  non-catalog span (storage IO or SQL) leave zero tenant rows but reach the
  operator.

```
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(=pg_verification_runtime::same_inbound_trace_id_routes_each_tenant_separately) | test(=pg_verification_runtime::unrouted_spans_export_only_to_the_operator)'"
```

**GREEN.** Add the registry, bind after authentication on HTTP, MCP, and gRPC,
and apply the D5 filter. Rerun Scenario 5.

**REFACTOR.** Keep one declarative catalog and allowlist, documented in the
architecture text.

### Scenario 7 — Tenant span staging is bounded and never starves sibling writes

**Behavior.**
- After the D5 projection, each tenant span takes one permit from a fixed
  pool of 2,048 per process via `try_acquire_owned`. No configuration option
  is added.
- A failed acquire drops that tenant span at once and increments the existing
  Scribe-outbox loss counter. The request never waits.
- The permit lives inside the `ScribeWrite::ServerSpans` item, so retries
  keep holding it, and terminal success, terminal loss, or shutdown
  abandonment releases it exactly once.
- Audit, gateway capture, and verifier results take no span permits and keep
  their current semantics; one outbox and one writer remain.
- Covers INV-003, INV-005, and REQ-007.

**RED.** Add
`pg_verification_runtime::sustained_scribe_outage_caps_tenant_spans`: with
Scribe unavailable, stage more than 2,048 tenant spans. Assert that the cap
holds, the loss count rises, permits return after recovery, and HTTP, gRPC,
and MCP requests plus audit and verifier-result staging stay available
throughout. It fails because staging is unbounded.

```
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(=pg_verification_runtime::sustained_scribe_outage_caps_tenant_spans)'"
```

**GREEN.** Add the permit pool and carry the permit on the staged item. Rerun
Scenarios 5–6.

**REFACTOR.** Remove any staging path that bypasses the permit.

### Scenario 8 — Gateway attempts sit under the exported gateway call, never twice

**Behavior.**
- Under a continued trace, captured attempt rows have the exported
  `gateway.call` span as parent. Their trace descends from the caller.
- `gateway.attempt` tracing spans are not tenant-exported.
- An untraced call keeps today's `call_id` trace.
- Covers REQ-007.

**RED.** Change `gateway_inference::gateway_span_joins_the_callers_trace` to
assert the chain from caller span to request span to `gateway.call` to the
attempt, with exactly one attempt row. It fails because the attempt's parent
is the caller span today.

```
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=gateway_inference::gateway_span_joins_the_callers_trace) | test(=gateway_inference::malformed_traceparent_is_ignored_by_the_gateway)'"
```

**GREEN.** Parent captured attempts on the exported gateway call span's id,
carried on the admitted call. Rerun Scenarios 5–7.

**REFACTOR.** Remove the caller-parent special case in capture once every path
supplies the gateway call span.

### Scenario 9 — Every Rust client request carries the caller's context

**Behavior.**
- Every authenticated HTTP request carries the active `traceparent` and a
  non-empty `tracestate`: cards, observe, Bifrost HTTP, gateway, workflow,
  principals, and MCP replay.
- Bifrost gRPC requests carry it in metadata.
- Presigned external requests carry none.
- No active context means no header.
- Covers REQ-006 and D3.

**RED.** Replace
`workflow_transport::public_gateway_call_does_not_inject_ambient_trace_context`
with `workflow_transport::public_gateway_call_carries_the_ambient_trace_context`.
Add a transport test asserting that a presigned request carries none. Both
fail under OD-3.

```
mise exec -- cargo nextest run --locked -p wyrd-client --test integration -E 'test(=workflow_transport::public_gateway_call_carries_the_ambient_trace_context)'
```

**GREEN.** Restore one authenticated-request builder that all builder sites
use, as `d39fced02` did. Add the gRPC metadata and the MCP replay headers.

**REFACTOR.** Remove the verify-only header path now that the builder covers
it.

### Scenario 10 — Queued Bifrost writes link producer and consumer

**Behavior.**
- `record`, `observe`, and table `insert` record a PRODUCER span in the
  caller's trace at enqueue.
- **Wire:** `wyrd-tonic` adds `message ProducerTraceContext { bytes trace_id
  = 1; bytes span_id = 2; uint32 trace_flags = 3; string trace_state = 4; }`
  and `repeated ProducerTraceContext producer_trace_contexts = 4;` on
  `InsertBatchRequest`.
- **Admission:** each admitted queue row records an optional `TraceContext`;
  a row with none adds nothing.
- **Seal:** the sealed batch keeps the first 32 distinct `(trace_id,
  span_id)` identities in row-admission order and ignores the rest. The
  vector is stored on `SealedBatch`, so every retry of one batch id sends
  byte-identical contexts.
- **Gate:** validates each received context, ignores malformed or excess
  entries without refusing the data batch, and adds valid links to the
  CONSUMER ingest span in vector order.
- No drop-count field, configuration option, or second metadata
  representation is added.
- Covers REQ-006 and D4.

**RED.**
- `wyrd-queue` unit tests for seal ordering, deduplication, the 32 bound, a
  context-free row, and an ambiguous retry that compares both attempts'
  context vectors byte for byte. Confirm selectors with
  `mise exec -- cargo nextest list --locked -p wyrd-queue`.
- Gate tests: malformed and excess entries are ignored while the batch is
  accepted.
- Add `pg_grpc_ingest_smoke::batch_ingest_span_links_each_producer_context`.
  It fails because the ingest request has no producer contexts.

```
mise exec -- cargo nextest run --locked -p wyrd-queue --lib
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(=pg_grpc_ingest_smoke::batch_ingest_span_links_each_producer_context)'"
```

**GREEN.** Extend the proto in `wyrd-tonic`, capture context at row
admission, seal the vector onto `SealedBatch`, build the request from it in
the gRPC transport, and link on the Gate write span. Regenerate contracts.

**REFACTOR.** Reuse the deferred-link helper from Scenario 4 for the consumer
links.

### Scenario 11 — Python and TypeScript requests carry the active context, and `start_telemetry` installs the propagator

**Behavior.**
- Every Python and TypeScript SDK call made under an active OTel span reaches
  the server with that span's context.
- `start_telemetry` installs the W3C trace-context propagator in each SDK.
- Covers REQ-006.

**RED.** Add Python `test_sdk_requests_carry_the_active_trace` in
`tests/integration/test_verify_in_real_time.py`. Add TypeScript "sdk requests
carry the active trace" in `tests/integration/verify-in-real-time.test.ts`.
Each asserts that a non-verify call's gateway or server span descends from
the caller. They fail under OD-3.

```
mise run py:setup && scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_sdk_requests_carry_the_active_trace'
mise run ts:build ts:build:testing && scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t 'sdk requests carry the active trace'"
```

**GREEN.**
- Capture the host context at each binding entry. Scope it for the Rust call,
  the same way `d39fced02` did, rather than editing every call site's
  arguments.
- Install the propagator in each SDK's `start_telemetry`.
- Rerun the existing traced-verify journeys in both SDKs.

**REFACTOR.** Delete the per-method `traceparent` and `tracestate` parameters
that the scoped capture replaces.

### Scenario 12 — The trace timeline read returns one tenant-scoped tree

**Behavior.**
- **Contract:** `Bifrost::trace_timeline(TraceId) -> Result<Vec<SpanRecord>,
  BifrostClientError>` plus its blocking equivalent, returning the canonical
  `wyrd_spec::vala::trace::SpanRecord`. No second span type.
- **Python:** `trace_timeline(trace_id: str) -> list[SpanRecord]` on both the
  sync and async `Bifrost`, with the same calling convention as their `sql`.
- **TypeScript:** `traceTimeline(traceId: string): Promise<SpanRecord[]>`.
- `SpanRecord` is projected into Python and TypeScript from the canonical
  type through each SDK's existing generated-type mechanism, with the same
  fields.
- **Closure:** the seed trace plus every trace with a span holding a direct
  link to the seed trace: one inbound-link hop only, never transitive.
- **Order and dedup:** deduplicate by `(trace_id, span_id)`; order by
  `(start_time, trace_id, span_id)`.
- **Errors:** an invalid id fails local typed validation before any IO. A
  valid unseen trace, or another tenant's trace, returns an empty list.
- **Bounds:** no pagination or helper-specific limit. The read inherits the
  existing Oracle deadline and result limits and returns their existing error
  with no partial result.
- Queries use typed parameters, never interpolated ids.
- Covers REQ-008, INV-001, and AC-003.

**RED.**
- `wyrd-client` integration tests, each failing because no read exists:
  `trace_timeline::returns_seed_and_directly_linked_traces`,
  `trace_timeline::ignores_second_hop_links`,
  `trace_timeline::deduplicates_and_orders_ties_by_trace_and_span`,
  `trace_timeline::invalid_trace_id_fails_locally`,
  `trace_timeline::unseen_trace_returns_empty`. Confirm selectors with
  `mise exec -- cargo nextest list --locked -p wyrd-client --test integration`.
- `action_timeline::another_tenant_cannot_read_the_timeline` in the Rust SDK
  journeys (new module registered in `tests/integration/main.rs`).
- Cross-language shape parity is asserted by the Scenario 13 journeys.

```
mise exec -- cargo nextest run --locked -p wyrd-client --test integration -E 'test(=trace_timeline::returns_seed_and_directly_linked_traces) | test(=trace_timeline::ignores_second_hop_links) | test(=trace_timeline::deduplicates_and_orders_ties_by_trace_and_span) | test(=trace_timeline::invalid_trace_id_fails_locally) | test(=trace_timeline::unseen_trace_returns_empty)'
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=action_timeline::another_tenant_cannot_read_the_timeline)'"
```

**GREEN.** Implement the read over the existing authenticated query path.
Project the operation and `SpanRecord` into the Python and TypeScript SDKs
with regenerated stubs and declarations. Replace the cross-joining `judge_calls` helper in
`verify_in_real_time.rs` with a call-id join.

**REFACTOR.** Retire ad-hoc span SQL in journeys that the read now covers.

### Scenario 13 — The action-timeline journey proves the whole chain in every SDK

**Behavior.** A Service started with the SDK's telemetry does the following in
one client trace:
- records client spans;
- writes Run observations and a Bifrost table row;
- makes a gateway model call;
- verifies in real time with an Eval `llm_judge` Verifier.

The timeline read then returns one connected, time-ordered tree. That tree
contains the client spans, the PRODUCER/CONSUMER-linked writes, the gateway
call and attempt, and verification → judge → gateway. The Python and
TypeScript journeys assert the same `SpanRecord` fields as Rust. The Rust journey also
asserts that the operator's captured export holds the same server spans with
the same ids. That is the dual-export proof; the operator export is server
behavior and independent of SDK language. Covers AC-002 and REQ-005 through
REQ-008.

**RED.** Add `action_timeline::service_action_timeline_is_one_connected_tree`
(Rust),
`tests/integration/test_action_timeline.py::test_service_action_timeline_is_one_connected_tree`
(Python), and "service action timeline is one connected tree" in
`tests/integration/action-timeline.test.ts` (TypeScript). Each fails until
Scenarios 1–12 are green.

```
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=action_timeline::service_action_timeline_is_one_connected_tree)'"
mise run py:setup && scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_action_timeline.py::test_service_action_timeline_is_one_connected_tree'
mise run ts:build ts:build:testing && scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/action-timeline.test.ts -t 'service action timeline is one connected tree'"
```

**GREEN.** Fix whatever the journeys expose at its owner. No journey-only
production branches.

**REFACTOR.** Share fixture cards with the existing verify and gateway
journeys where they already fit.

## Acceptance Criteria

- The request, gRPC, and MCP server spans continue a valid inbound context as
  SERVER children. Malformed context starts a new trace, and the request
  succeeds.
- Verifier → judge → gateway is one descendant chain for both Task and Eval
  verifiers.
- Deferred runs link and are not parented. Manual-enqueue context is
  persisted.
- Observation judge calls carry `card_uid` and `run_id`, and results join
  their judge calls.
- The operator export is unchanged in content and sampling; no
  client-supplied field changes the operator decision.
- The four-row sampling matrix holds with and without an operator endpoint.
- Tenant rows exist only for allowlisted spans of sampled, continued,
  authenticated requests, with the same ids and the routed principal. Two
  tenants sharing an inbound trace id never see each other's rows. No
  disallowed attribute appears, and no attempt is written twice.
- Tenant staging is capped at 2,048 in-flight spans per process; overflow is
  counted, and a sustained Scribe outage never blocks a request or sibling
  outbox writes.
- Every SDK request propagates context, presigned requests do not, and no
  context means no header. Queued writes link producer and consumer through
  at most 32 sealed contexts per batch, identical on every retry.
- `trace_timeline` returns the seed plus one-hop linked traces, deduplicated
  and ordered by `(start_time, trace_id, span_id)`, as `SpanRecord` in all
  three SDKs; invalid ids fail locally and unseen or foreign traces are
  empty.
- The action-timeline journeys pass in all three SDKs. Rust also proves the
  operator view.
- Both architecture documents state the REQ-009 model.

## Expected Write Set and Consumer Closure

- **Server:**
  - `crates/wyrd/wyrd-server/src/http/{router.rs,middleware/trace_context.rs}`
  - `grpc/`, `mcp/`
  - `verification/{direct,eval,runner}.rs`
  - `components/gateway/{invocation,capture,workflow}.rs`
  - `scribe_outbox.rs`, `app/{mod,metrics}.rs`, `boot/mod.rs`
- **Vala:** `crates/vala/vala-eval/src/executor.rs`;
  `crates/vala/vala-bifrost-redux` (Gate write span links and producer-context
  validation).
- **Telemetry:** `crates/shared/wyrd-telemetry/src/lib.rs` (sampler wrapper,
  operator filter, tenant processor, routing registry).
- **Queue and wire:** `crates/shared/wyrd-queue` (`SealedBatch`, row
  admission, seal) and its tests; `crates/wyrd/wyrd-tonic/proto/wyrd.v1.proto`
  and generated code; the client `BifrostGrpcTransport`; Gate consumers.
- **Client:** `crates/shared/wyrd-client/src/{transport,bifrost,observe,workflow}`.
- **SQL:** the change's existing verifier-run migration and
  `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs`.
- **SDKs:** `otel.rs`; Python `src/observe`, `src/client.rs`,
  `python/wyrd/otel.py`, and the Bifrost export and stubs; TypeScript native
  entry, `wyrd/src/{index,otel}.ts`, and declarations.
- **Tests:**
  - `wyrd-server` integration tests;
  - `wyrd-testing` server tests;
  - `wyrd-client` integration tests;
  - `wyrd-telemetry` and `wyrd-queue` unit tests;
  - `wyrd-mcp` `mcp` journeys (`verification`);
  - `wyrd-ai` journeys (`action_timeline`, `gateway_inference`,
    `verify_in_real_time`);
  - Python and TypeScript journeys.
- **Docs:** `architecture/wyrd-design.md`, `architecture/bifrost-design.md`.

## Verification and Evidence

- Every scenario's exact command above.
- Owner lanes:
  - `mise run test:bifrost:integration:server`
  - `mise run test:bifrost:journey:server`
  - `mise run test:bifrost:journey:drift`
  - `mise run test:bifrost:journey:mcp`
  - `mise run test:gateway:journey`
  - `mise run test:shared`
  - `mise run test:sql`
- Static and boundary checks:
  - `mise run fmt`, `mise run lints`
  - `mise run py:format`, `mise run py:lints`, `mise run py:typecheck`
  - `mise run ts:format`, `mise run ts:typecheck`
  - `mise run codegen:check`
  - `mise run check:deps`
  - `mise run check:tenant-isolation`
- **REQ-009 has no executable behavior.** Proof is a review of the two
  architecture diffs against D1–D8. The retired "sole correlator" sentence
  must be gone:
  `git grep -n "sole correlator" architecture/` returns nothing.

## Material Stop Conditions

- The tenant span sink cannot be attached without `wyrd-telemetry` depending
  on a server crate.
- The ingest proto change cannot stay backward-neutral within the change.
- OTel 0.31's `ShouldSample` or `SpanProcessor` contract cannot express the
  Scenario 5 recording decisions or the Scenario 6 route inheritance.
- `SpanRecord` cannot be projected into Python or TypeScript through the
  existing generated-type mechanism.
- Stamping the requesting principal on server-span frames cannot use the
  existing managed `principal_id`. SPEC-sub-act-attribution lands after this
  change and replaces that column with the `sub`/`act` pair; this task must not
  pre-empt it.
- The D5 catalog would expose a field the security posture classifies as
  sensitive.
- `check:tenant-isolation` or `check:deps` requires a boundary change.

## Authority Links

- `changes/active/verify-attribution/spec.md` (revision 2)
- `AGENTS.md`, `architecture/agent-rules.md`
- `architecture/wyrd-design.md`, `architecture/bifrost-design.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/implementation-execution.md`
- `architecture/references/languages/testing-workflows.md`
