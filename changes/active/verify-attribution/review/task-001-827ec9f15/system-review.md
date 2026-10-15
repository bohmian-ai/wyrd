# TASK-001 system-resilience review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/verify-attribution`
- Base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`
- Approved authority: `changes/active/verify-attribution/spec.md` revision 3
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`
- Review scope: the complete base-to-candidate range, with emphasis on crash, outage, retry, timeout, cancellation, shutdown, authentication refusal, malformed producer context, and multiple-server process behavior.

The working tree's recorded evidence commit has the same tree as the candidate. Source locations below therefore describe the immutable candidate tree.

## Deployed paths

| Path | Runtime ownership and propagation | Downstream effect |
|---|---|---|
| HTTP/MCP request | `http/router.rs::request_span` calls `wyrd_telemetry::serve_request` before the span first enters. Authentication binds the live route in `AuthenticatedPrincipal::from_verified`. MCP remains under the HTTP request span and its tracked task lifecycle. | A sampled remote parent opens a route on the serving server's sink; allowlisted descendants inherit it and stage tenant copies as they end. Authentication refusal leaves the route unbound, so no tenant row is written. |
| Public gRPC request | `wyrd-tonic::server::request_span` applies the same `serve_request` edge to the public router. Gate, query, and OTLP authentication all converge on `gate::auth::AuthContext::from_verified`, which binds the verified tenant and principal. | Insert, query, and OTLP spans continue valid caller context. Missing or malformed context starts an operator-only trace. |
| Queued Bifrost write | Queue admission captures a row's producer context; `SealedBatch` owns the first 32 distinct contexts and the stable batch identity. `BifrostGrpcTransport` reconstructs every attempt from that immutable sealed batch. Gate parses at most 32 valid entries and adds links before `bifrost.gate.write` first starts. | Retry sends the same batch id, bytes, request id, and producer contexts. A sampled producer link makes the CONSUMER span recordable and opens its tenant route after authentication; malformed/excess entries are ignored without refusing data. |
| Telemetry decision and copy | The process provider uses `ServerSampler`; operator acceptance comes from the configured server policy, using a server-generated draw for a remote parent, while tenant eligibility comes from a sampled remote parent or sampled remote link. `OperatorExport` forwards only sampled spans and strips trace state. `TenantExport` uses the process live-route registry, but each route carries the particular server sink supplied by its request edge. | Operator queue membership is independent of client flags and trace id. Tenant copies retain operator ids, parents, and links but reduce attributes/events before they enter Scribe. Multiple in-process servers do not redirect one another's rows. |
| Tenant persistence | `ServerSpanSink` uses `try_acquire_owned` from a fixed 2,048-per-server pool and puts the permit inside `ScribeWrite::ServerSpans`. The shared `ScribeOutbox` groups and writes tenant slices through local or peer Scribe under content-derived batch ids. | Request completion never waits for Scribe. Retry keeps the item and permit; ACK, terminal consumption, projection loss, or shutdown abandonment drops the item and releases the permit exactly once. Audit, capture, and result items use no span permit. |
| Deferred verifier run | Manual enqueue persists the cause trace/span in the existing unshipped verifier-run schema. Claim returns it after restart and the runner adds it as an execution link. Observation execution retains the stored observation link. | Process restart loses no deferred cause metadata. Deferred execution remains a new trace and does not open a tenant-copy route merely because it links to its cause. |
| Timeline read | Rust, Python, and TypeScript call the same `Bifrost::trace_timeline` query through authenticated Oracle. The query returns the seed trace and traces with a direct inbound link; the client decodes canonical `SpanRecord`, sorts, and deduplicates. | RLS/query authority remains the tenant boundary. Oracle's existing admission, deadline, row, and encoded-result limits bound the operation; cancellation drops the response stream and its query guards. |

## Failure and recovery assessment

| Failure or interruption | Observed behavior | Recovery and affected siblings | Assessment |
|---|---|---|---|
| Operator OTLP endpoint unavailable | `BatchSpanProcessor` remains behind `OperatorExport`; request processing and tenant staging do not await it. The operator processor's existing bounded queue/export timeout governs loss. | Tenant copies continue through Scribe. Operator export recovers with the existing exporter behavior; no new retry loop reaches a request. | PASS |
| Scribe unavailable or peer request times out | A span end stages once and returns. Its item remains in the generic outbox's exact failed tenant slice with exponential backoff; the span permit stays attached. At 2,048 held copies, later copies are dropped and counted immediately. | Requests, permission checks, and sibling staging remain callable. Other tenants can use the remaining outbox writer slots; same-tenant writes remain ordered behind the failed slice. Recovery resubmits content-derived ids, so ambiguous acknowledgement is absorbed by Scribe. | PASS |
| Terminal Scribe rejection or local projection failure | `ScribeSink` logs and counts the affected item, completes the slice rather than retrying permanently, and item destruction releases its permit. | Later writes can progress; the originating request is unchanged, as required by the non-blocking loss contract. | PASS |
| Abrupt process crash | Live routes, queued tenant copies, and operator buffers are in memory and may be lost. | The route registry and permits are reconstructed on restart. Durable verifier-run cause fields and already acknowledged Scribe rows survive. This matches the repository's explicit in-memory Scribe-outbox loss boundary. | PASS |
| Graceful shutdown with reachable Scribe | Public admission and tracked request/gateway/MCP work drain before `scribe_outbox.shutdown`; completed spans have staged before the outbox fence. The outbox then drains while Scribe is still accepting. | Bifrost owners shut down after the shared outbox. No task-specific alternate writer or detached retry task remains. | PASS |
| Graceful shutdown with stalled Scribe | The outbox receives half of the remaining server deadline; on expiry it cancels in-flight writes, counts and logs the remaining loss, drops items, and releases span permits. | Bifrost still retains budget to withdraw and drain its owners. Late staging is refused and counted rather than reopening the queue. | PASS |
| Request timeout or caller cancellation | The request/consumer span closes through normal tracing ownership; live route removal happens in `TenantExport::on_end`. Oracle's streamed query owner drops its retained query on client disconnect. | Any completed tenant copy is staged non-blockingly; incomplete work is cancelled by its existing owner. No route or query lease is intentionally detached. | PASS |
| Invalid or repeated W3C context | Shared parsing returns no caller. `serve_request` still carries the server sink, but no sampled-parent route opens unless an authenticated Gate consumer has a valid sampled producer link. | Request behavior is otherwise unchanged. Invalid client context neither fails the request nor enters the operator exporter as trace state. | PASS |
| Malformed, excessive, or spoofed producer context | Gate examines at most 32 entries, requires nonzero 16-byte/8-byte ids and one-byte flags, and ignores invalid entries. Authentication supplies the destination tenant; producer ids never select it. | Valid rows are still admitted. An authenticated caller can cause only its own allowlisted server behavior to be copied, under the fixed permit cap; it cannot select another tenant or expose non-allowlisted fields. | PASS |
| Multiple servers in one process | The global route registry stores the server-specific sink inside each route rather than consulting mutable process-global sink state. Keys are the server-created `(TraceId, SpanId)` pair and are removed on end. | Concurrent servers stage only into their own Scribe outboxes. Restarting or stopping one server cannot redirect a later request on another server. | PASS |
| Authentication/authorization refusal | Tenant identity is installed only by verified-token constructors. Failed authentication never binds the route; authorization remains blocking and its audit staging keeps the existing non-blocking outbox semantics. | The operator may observe the refusal span according to server sampling, but no tenant copy is emitted for an unauthenticated route. | PASS |

## Proof assessment

The recorded evidence is proportionate to the changed system behavior:

- `wyrd-telemetry` unit coverage exercises all four operator/client sampling combinations, with and without an operator, sampled-link recording/routing, catalog filtering, route cleanup, and per-server sink isolation.
- The Scribe-outbox outage test holds Scribe unavailable past the 2,048-copy cap, confirms sibling audit/capture staging, confirms abandonment releases every permit, and confirms an acknowledged copy releases its permit.
- Server integration covers continued and malformed HTTP/gRPC context, operator-rejected but tenant-eligible requests, same inbound trace id across tenants, unrouted operator-only spans, deferred-run links, and real Scribe persistence.
- Rust, Python, TypeScript, MCP, gateway, drift, SQL, shared, Vala, code-generation, dependency, and tenant-isolation lanes are recorded green. The three SDK journeys exercise the public action timeline; the Rust journey additionally compares tenant and operator ids.
- Existing generic outbox tests cover retry retention, backoff, panic containment, shutdown abandonment, and idempotent ownership; the implementation reuses that owner rather than adding a second retry lifecycle.

I did not rerun verification during this parallel review. The remaining proof limit is operational performance rather than correctness: the route processor clones span data for an unrouted candidate in order to inspect links. It is bounded per span, has no retained growth, and does not change failure propagation, so it is not a material resilience finding.

## Material findings

None.

## Overall result

**PASS**

The candidate preserves request availability during telemetry and Scribe failures, bounds the only new high-rate retained producer, keeps retry identities stable, drains in the established lifecycle order, and recovers without cross-server or cross-tenant routing state.
