# Verifier attribution and distributed trace continuity

- Change: `verify-attribution`
- Specification: `SPEC-verify-attribution`, approved revision 3
- Completed: 2026-10-14
- Reviewed target: `490e3fae0`, approved by the human owner in place of a final change review
- Delivery reference: not supplied

Every Verifier execution and gateway model call records who asked and which
Run and Card it belongs to. A client's trace now continues through every server
action taken on its behalf, so one read reconstructs a Service's whole
client → server → client timeline: its own spans, its observations and Bifrost
writes, gateway model calls, and real-time verification down to the judge's
gateway call. The operator keeps an unchanged view of the same server spans.

## Shipped behavior

- Gateway calls record `caller_principal_id` plus whichever of `run_id` and
  `card_uid` the call carried; the two are independent. A Verifier's judge
  calls carry its subject Card and Run. Verifier runs and results record the
  actor as `requested_by_principal_id`: the caller for manual and direct
  runs, the tenant SYSTEM principal for scheduled and observation-triggered
  ones.
- HTTP, MCP, and gRPC requests with a valid `traceparent` continue the
  caller's trace: the request span is a SERVER child, and verification →
  judge → gateway, Bifrost ingest, and Bifrost query descend from it. A
  missing or malformed context starts a new trace and never fails the
  request. Deferred work (queued and observation-triggered runs, Scribe,
  Forge) starts its own trace and links to its cause; a manually enqueued
  run persists its enqueuing context.
- The Rust, Python, and TypeScript SDKs send the active trace context on
  every request to the server, never on presigned storage URLs, and
  `start_telemetry` installs the W3C propagator. A queued Bifrost write
  records a PRODUCER span at enqueue; its sealed batch carries up to 32
  distinct producer contexts, identical across retries, and the server's
  CONSUMER ingest span links to them.
- Server spans export to the operator exactly as before. A span of a request
  continuing a sampled client trace, or of an ingest linking a sampled
  producer, is also written to the requesting tenant's `vala.traces.spans`
  through the process Scribe outbox, with the operator's ids, restricted to
  the behavior-span catalog and safe attributes.
- `trace_timeline(trace_id)` in all three SDKs returns a trace's spans plus
  those of traces linking to it, deduplicated by span identity and ordered
  by start time, trace id, then span id, through the tenant-scoped Bifrost
  query path.

## Invariants and constraints

- Tenant isolation holds on export and read: tenant rows are routed by the
  authenticated request identity, never by trace id.
- Client trace context never changes the operator export's content or
  sampling (operator sampling follows the server; tenant export follows the
  client's sampled flag) and never fails a request.
- Tenant span staging is non-blocking and bounded at 2,048 in-flight copies
  per server; overflow is counted as outbox loss, and the same outbox
  recovers once Scribe returns.
- Each queued producer context is one allocation with one client-byte
  charge, shared by its rows and the sealed batch that selects it, from
  admission until ACK, terminal loss, or shutdown. Sealing reserves
  headroom only for the frame, so a traced record that fills the admission
  ceiling still seals.
- Gateway attempt rows stay canonical and are never written twice; producer
  `tracestate` and reserved flag bits are not forwarded to Gate links.

## Decisions and deviations

- Trace context orders behavior causally; `Wyrd-Request-Id` keeps request
  identity, audit, and replay. Synchronous work is parented, deferred work
  linked. Queued writes follow the OTel messaging batch convention.
- Revision 3 made a linked ingest tenant-eligible, because queued writes
  reach the server with producer links and no `traceparent`.
- The tenant span sink is carried per request in the request span's context
  rather than set process-wide, so several servers in one process stage on
  their own outboxes.
- Python and Node open the PRODUCER span in the host tracer; Python returns
  timeline spans as `SpanRecord` TypedDicts matching the canonical record's
  required keys and tagged status, after the SDK moved to authored stubs.
- The Scribe-outage proof uses the multi-pod cluster harness, since a single
  pod's in-process Scribe has no reversible outage without a production
  fault switch.

## Acceptance closure

All AC-001 through AC-005 obligations closed through Rust, Python, and
TypeScript action-timeline journeys (one connected tree including the
CONSUMER span per producer, plus the operator view in Rust), gateway and
verify trace journeys, negative flows (malformed context, unsampled trace,
cross-tenant read, disallowed attributes, deferred links, no-context
requests), the sampling matrix, the outage cap and recovery test, queue
admission-to-settlement accounting tests, Python static contract checks, and
the format, lint, codegen, dependency, and tenant-isolation lanes. Two task
reviews returned `FIX_REQUIRED`; both remediations were implemented.

## Current owners

- [Wyrd design](../../../architecture/wyrd-design.md) and
  [Bifrost design](../../../architecture/bifrost-design.md).
- [Server span export](../../../crates/shared/wyrd-telemetry/src/export.rs),
  [client trace propagation](../../../crates/shared/wyrd-client/src/transport/trace.rs),
  [queue sealing](../../../crates/shared/wyrd-queue/src/queue.rs), and the
  [timeline read](../../../crates/shared/wyrd-client/src/bifrost/facade.rs).
- [Verifier run persistence](../../../crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs)
  and [Python timeline types](../../../sdks/wyrd-sdk-python/python/wyrd/bifrost/_client.py).
- Journeys and integration proof:
  [server runtime](../../../crates/wyrd/wyrd-server/tests/integration/pg_verification_runtime.rs),
  [Rust action timeline](../../../sdks/wyrd-sdk-rust/tests/integration/action_timeline.rs),
  [Python action timeline](../../../sdks/wyrd-sdk-python/tests/integration/test_action_timeline.py),
  and [TypeScript verify journey](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts).
