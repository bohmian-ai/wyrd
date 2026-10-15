# Concurrency and durability domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/verify-attribution`
- Base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`
- Approved authority: `changes/active/verify-attribution/spec.md`, revision 3
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`

The candidate Git object was reviewed directly. The checked-out branch has
subsequent commits, so current-worktree source was not treated as the immutable
subject except where its code tree was confirmed identical to the candidate.

## Reviewed boundary

| Boundary | Source and authority inspected | Result |
|---|---|---|
| Live tenant-route registry | `wyrd-telemetry/src/export.rs`: `TenantRoutes::{start,bind,end}`, `Route`, `TenantExport`; spec REQ-007/INV-001/INV-005 | PASS. The process registry is keyed by the server span's `(TraceId, SpanId)`, the lock covers each transition, descendants retain the shared route independently, first authentication binding wins, and `on_end` removes the live entry before staging. Concurrent same-client-trace requests do not share a route. |
| Request sink ownership | `wyrd-telemetry/src/context.rs::serve_request`; HTTP/gRPC request edges; `state.rs::Bifrost::tenant_span_sink` | PASS. Each server supplies its own sink through the request context; the global route registry retains that exact sink, so multiple in-process test servers do not redirect a request to another server's outbox. |
| Tenant-span backpressure | `wyrd-server/src/scribe_outbox.rs`: `ServerSpanSink`, `ScribeWrite::ServerSpans`; shared `Outbox` retry/shutdown ownership | PASS. `try_acquire_owned` never waits; the owned permit travels in the staged item through retry and is released by the single item drop on ACK, terminal consumption, late-stage refusal, or shutdown abandonment. Audit, capture, and result variants take no permits. |
| Scribe outage, retry, terminal refusal, and shutdown | `ScribeSink::write`, peer timeout/rotation, content-derived frame identity, `OutboxWriter`, `Outbox::shutdown`, `BoundServer::run`; AGENTS one-write-path rule and Bifrost durability/shutdown authority | PASS. Retry keeps the same slice and permit, frame identities are content-derived, terminal rejection consumes and counts, abrupt/deadline loss matches the documented in-memory durability boundary, and request/gateway/verifier work drains before the shared outbox fence. |
| Queued producer-context lifetime | `wyrd-queue/src/producer.rs`, `queue.rs`, `sink.rs`; `BifrostGrpcTransport`; Gate producer-link parsing | FAIL. Context order and retry bytes are stable, but their owned memory bypasses the queue's sole byte admission owner. See CONDUR-001. |
| Deferred-run cause persistence | verifier-run SQL/migration, claim projection, `VerifierRunner::claim`; spec D8/REQ-005 | PASS. The cause pair is written in the enqueue transaction, constrained as a pair of exact-width ids, returned by the fenced claim, and attached as a link without becoming the execution parent. |
| Eval fan-out propagation and cancellation | `vala-eval/src/executor.rs::fan_out_bucket`; REQ-005 and repository async rules | PASS. Each `JoinSet` task is instrumented with the caller span; early error still aborts siblings through the existing structured `JoinSet` lifecycle. |

## Material finding

### CONDUR-001 — REGRESSION — producer trace contexts bypass the queue's only memory bound

- **Violated obligation:** `crates/shared/wyrd-queue/src/config.rs:9-16` and
  `producer.rs:34-39` define `client_byte_limit_bytes` as the only bound on
  every buffered client byte across rows, sealed batches, in-flight sends, and
  retained retries. Task Scenario 10 adds producer contexts to those ownership
  states without relaxing that invariant.
- **Locations:**
  - `crates/shared/wyrd-queue/src/producer.rs:546-560` reserves only the
    prepared Arrow `charge`, then allocates/retains an owned `TraceContext`
    outside that guard for every admitted logical record.
  - `crates/shared/wyrd-queue/src/queue.rs:586-594,665-683` clones as many as 32
    contexts into each `SealedBatch`, then drops the row owners while resizing
    the sealed guard only to the IPC frame size.
  - `crates/shared/wyrd-queue/src/sink.rs:95-115` retains that uncharged vector
    through every in-flight and ambiguous retry state.
- **Evidence and reachability:** All public queued JSON writes now pass a
  producer context when an SDK span is active. `Producer` uses an unbounded
  handoff channel, intentionally relying on `ClientByteBudget` for the hard
  memory ceiling. A context owns three `String` allocations, including up to a
  W3C-sized `tracestate`. A burst or unavailable sink can therefore retain
  context allocations that `used_bytes()` does not include; after sealing,
  every retained batch continues to own up to 32 such contexts while its guard
  accounts only for Arrow IPC.
- **Observable consequence:** The configured 256 MiB (or operator-overridden)
  client memory ceiling is no longer a ceiling. Under high-cardinality traced
  writes and transport ambiguity, process memory can exceed the admitted
  budget by the accumulated context allocations, and queue metrics understate
  the live ownership. This is a reachable resource-exhaustion regression, not
  an optional accounting refinement.
- **Smallest testable correction:** Keep the existing `ClientByteBudget` as the
  sole owner; add no option or second limiter. Charge the producer context's
  owned bytes with the logical record at admission, and transfer the charge for
  the first 32 retained contexts into the existing sealed-batch ownership when
  rows become a `SealedBatch`, so the charge remains live through send,
  ambiguous retry, ACK, terminal refusal, and shutdown. Release the charge for
  contexts discarded by the 32-context seal bound. Add one focused queue test
  using a small byte budget, distinct maximum-size trace states, and a retained
  ambiguous sink; it must prove admission backpressures before
  `used_bytes() > client_byte_limit_bytes`, the charge survives seal/retry, and
  terminal settlement returns `used_bytes()` to zero.

## Failure and recovery assessment

- A Scribe outage retains at most 2,048 tenant-span items per server and does
  not block span completion. Recovery reuses the same item and batch identity.
- Terminal Scribe rejection and projection failure release the associated span
  permit once when the outbox consumes the slice.
- Shutdown closes staging before awaiting the writer; late span staging is
  counted lost and drops its permit. The server drains request, MCP, gateway,
  and verifier work before closing the shared Scribe outbox.
- An ambiguous queued-ingest transport outcome retains the same sealed batch,
  producer-context ordering, and UUID. The remaining gap is its unbounded-by-
  policy context memory described in CONDUR-001.

## Verification limits

No Cargo, mise, database, or code-generation command was run during parallel
discovery. I inspected the recorded task evidence and the candidate source.
The focused closure proof needed for CONDUR-001 does not exist in the candidate;
the current producer-context tests prove ordering and retry identity, not the
client byte ceiling.

## Overall result

**FAIL** — CONDUR-001 is a material, bounded resource-ownership regression in
the changed queued-ingest path. The route registry, Scribe permits, retry
identity, terminal loss, and shutdown behavior otherwise satisfy the approved
task and durability authority.
