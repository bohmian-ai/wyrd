# Persistent trace data domain review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/verify-attribution`
- Base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`
- Approved authority: `changes/active/verify-attribution/spec.md`, revision 3
- Task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`
- Domain: persistent trace attribution, Bifrost producer metadata, Scribe span rows, timeline query/decoding, and Rust/Python/TypeScript public projections

## Authority and source coverage

| Boundary | Authority applied | Source and consumer coverage | Result |
|---|---|---|---|
| Queued producer metadata | Spec REQ-006/D4, task Scenario 10, `AGENTS.md` contract and bounded-queue rules, telemetry reference | `wyrd-queue::{Producer, Row, SealedBatch, producer_traces}`, `BifrostGrpcTransport`, `InsertBatchRequest`, Gate `producer_links`; queue and Gate tests | FAIL: PD-002 |
| Persistent tenant span attribution | Spec REQ-007, INV-001/INV-005, Bifrost one-write-path authority | `TenantExport`, `ServerSpanSink`, `ScribeWrite::ServerSpans`, `ScribeSink::{frames, identify}`, local and peer `ScribeBatch::into_frame`, `ScribeCapturePeerGrpc`, `project_resource_spans` | PASS |
| Deferred-run cause persistence | Spec REQ-005/D8, task Scenario 4 | verifier-run migration, `RunRequest::cause`, `INSERT_RUN_SQL`, `CLAIM_RUN_SQL`, `ClaimedRunRow`, runner link attachment | PASS for the task's required causal link |
| Timeline query semantics | Spec REQ-008/INV-001, task Scenario 12, Bifrost Oracle/query authority | `Bifrost::trace_timeline`, typed `QueryParam`, authenticated `QueryClient`, one-hop link SQL, table-not-created mapping, sorting/deduplication; timeline integration tests | FAIL: PD-001 |
| Arrow/Variant row decoding | Spec REQ-008, Arrow interop reference, canonical `SpanRecord` | `SPAN_COLUMNS`, `span_record`, nested event/link decoding, Variant attribute decoding, timestamp and ID conversion, Eval reader reuse | PASS |
| Public language projections | First-class SDK/PyO3/TypeScript rules | Rust async/blocking facade, Python native wrapper plus sync/async package methods and generated `SpanRecord`, TypeScript N-API facade plus generated `SpanRecord` declaration; three action-timeline journeys | PASS |
| Tenant read isolation and empty/error behavior | Spec INV-001/REQ-008/AC-003 | authenticated query path, tenant physical table binding, local typed ID parsing, table-not-created empty result, cross-tenant Rust journey | PASS |

## Deployed data path assessment

The write path keeps one bounded durable owner. JSON row admission captures one optional producer context, sealing retains the first 32 distinct `(trace_id, span_id)` values in admission order, and `SealedBatch` owns that vector across ambiguous retries. The gRPC transport rebuilds each attempt from the sealed vector. Gate adds links before entering `bifrost.gate.write`, which is the required OpenTelemetry span-creation point.

Tenant span copies stay on the process Scribe outbox. The requesting principal participates in frame grouping and the content-derived batch identity, and reaches both local `ScribeIngressFrame` construction and the mTLS peer request. The peer adapter confines the principal field to `vala.traces.spans`. Scribe therefore stamps the authenticated tenant and routed principal without trusting a tenant field carried by the public ingest request.

The read path uses one parameterized Oracle query over the caller's tenant-bound `vala.traces.spans`, selects the seed plus exactly one inbound-link hop, decodes the stored Arrow/Variant shape through the shared Rust decoder, and projects that result rather than reimplementing query semantics in Python or TypeScript.

## Material findings

### PD-001 — INCORRECT: timeline deduplication only removes adjacent duplicate identities

- Violated obligation: Spec REQ-008 and task Scenario 12 require deduplication by `(trace_id, span_id)` followed by total ordering by `(start_time, trace_id, span_id)`.
- Location: `crates/shared/wyrd-client/src/bifrost/facade.rs:854-874`; incomplete proof at `crates/shared/wyrd-client/tests/integration/pg_bifrost_e2e/trace_timeline.rs:142-166`.
- Evidence: the implementation sorts first by `start_time`, then IDs, and calls `Vec::dedup_by`. `dedup_by` only compares adjacent elements. Two stored rows with the same `(trace_id, span_id)` but different start times are separated by any span whose start lies between them and both remain in the public timeline. The existing test duplicates an otherwise identical row at the same start time, so it cannot expose the reachable failure.
- Observable consequence: Rust, Python, and TypeScript callers can receive duplicate span identities, violating the public helper's documented contract and making ancestry/timeline consumers count or traverse one logical span twice.
- Required correction: keep the existing total-order sort, then retain only the first occurrence of each `(TraceId, SpanId)` with one local set. Do not change the SQL, add pagination, or introduce another timeline type.
- Focused closure proof: change the existing `deduplicates_and_orders_ties_by_trace_and_span` fixture so the duplicate identity has a different start time with another span between the two ordered rows, and assert the identity appears once in the same total order. Run its existing exact `wyrd-client` integration selector through the repository Postgres setup.

### PD-002 — INCORRECT: Gate discards part of each sealed producer trace context

- Violated obligation: Spec REQ-006/D4 and task Scenario 10 define `ProducerTraceContext` with `trace_id`, `span_id`, `trace_flags`, and `trace_state`, require the sealed contexts to reach Gate unchanged across retries, and require Gate to add each valid context as the CONSUMER span link.
- Location: `crates/vala/vala-bifrost-redux/src/gate/mod.rs:1151-1155,1272-1291`; `crates/shared/wyrd-telemetry/src/lib.rs:274-301`.
- Evidence: `producer_links` reduces every wire context to `(TraceId, SpanId, bool sampled)`. `link_remote_span` reconstructs the link with either `TraceFlags::SAMPLED` or zero and always `TraceState::default()`. Thus a valid non-empty `trace_state` and every received W3C flag bit other than sampled are discarded before the link is stored in `vala.traces.spans`. The queue and wire correctly retain those fields, so the loss occurs only at the Gate consumer. The Gate test covers malformed IDs, an over-wide flag value, ordering, and the bound, but never asserts the resulting link's state or complete one-byte flags.
- Observable consequence: `trace_timeline` returns a CONSUMER link that is not the producer context sent by the SDK: its `trace_state` is empty and its flags may be truncated. This breaks canonical `SpanLink` parity at the persistent attribution seam.
- Required correction: preserve the validated one-byte flags and parse the received `trace_state` into the OpenTelemetry `SpanContext` used for the link, defaulting only an invalid state in the same fail-open manner as inbound W3C context. Extend the existing telemetry link mechanism rather than adding a second link representation; existing deferred callers may supply their currently known empty state/flags. Do not reject the data batch for malformed optional producer metadata.
- Focused closure proof: extend the existing Gate producer-link test (or its closest telemetry link unit test) with a valid non-empty trace state and a valid non-sampled low-byte flag value, then assert the ended CONSUMER span's persisted/captured link preserves the state and complete low byte. Retain the existing malformed/excess acceptance assertion.

## Verification evidence and limits

Recorded task evidence reports green owner lanes for Bifrost server integration, Rust/Python/TypeScript action timelines, shared and Vala tests, SQL, code generation, dependency boundaries, tenant isolation, formatting, and lints. Source inspection confirms those tests exercise the main write/read path, cross-tenant emptiness, one-hop closure, sealed retry metadata, and cross-language shape projection.

Per orchestrator coordination, this reviewer ran no Cargo, mise, codegen, database, Python, or TypeScript command while parallel discovery was active. The two focused closure proofs above are the only additional commands needed for these findings; no broad aggregate is warranted at remediation time.

The candidate commit remained the immutable review subject. The working branch contains later task-evidence commits, so all implementation conclusions above were checked against the explicit base-to-candidate range or source unchanged from candidate; later prose was used only as recorded verification evidence.

## Overall result

**FAIL**

The persistent ownership, tenancy, retry stability, decoding, and cross-language projection are sound, but PD-001 violates the timeline's public deduplication contract and PD-002 drops canonical producer-context fields before durable link projection. Both are bounded implementation corrections under the approved specification.
