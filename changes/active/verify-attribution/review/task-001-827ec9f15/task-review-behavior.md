# TASK-001 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/verify-attribution`
- Base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`
- Approved authority: `changes/active/verify-attribution/spec.md`, revision 3
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`
- Review result: **FAIL**

The cumulative candidate implements the requested trace-continuity path and the
three SDK surfaces, but two observable data-contract defects and one required
recovery-proof gap remain.

## Navigation map and caller coverage

| Changed workflow | Producer / owner | Consumers followed | Proof inspected |
|---|---|---|---|
| Inbound continuation | `wyrd_telemetry::serve_request`; HTTP and tonic `request_span` | authenticated HTTP extractors, Gate auth interceptor, MCP-over-HTTP, request descendants | telemetry context tests; server and SDK journey source |
| Dual export and tenant routing | `ServerSampler`, `TenantRoutes`, `TenantExport` | operator processor, `ServerSpanSink`, Scribe outbox, tenant span table | telemetry unit tests; `pg_verification_runtime`; action timeline journeys |
| Deferred verification context | `VerificationControl::enqueue`, verifier-run SQL, `VerifierRunner::claim` | attempt span, Eval engine, gateway judge calls, result attribution | verifier runtime and `eval_verification` tests |
| Queued producer links | SDK enqueue span, `wyrd-queue::SealedBatch`, gRPC request | Gate `producer_links`, `bifrost.gate.write`, stored `SpanRecord.links` | queue tests; `pg_grpc_ingest_smoke`; three action timeline journeys |
| Timeline read | `Bifrost::trace_timeline`, shared row decoder | blocking Rust, Python sync/async, TypeScript | `pg_bifrost_e2e::trace_timeline`; SDK journeys |
| Gateway attempts | gateway invocation and capture | canonical attempt span rows, tenant timeline, operator capture | Rust/Python/TypeScript gateway journeys |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Scenario 1 / REQ-005: valid HTTP context parents the SERVER request span; malformed input is ignored | `wyrd-telemetry/src/context.rs:104-129`; `wyrd-server/src/http/router.rs:170-192` | context unit tests and gateway malformed-context journeys recorded in task evidence | PASS |
| Scenario 2 / REQ-005: gRPC and MCP continue the caller | `wyrd-tonic/src/server/mod.rs:218-249`; MCP replay uses the shared authenticated transport | `pg_grpc_smoke`; `verification_tool_continues_trace_context` recorded green | PASS |
| Scenario 3 / AC-004: verifier, judge, and gateway form one descendant chain | `vala-eval/src/executor.rs:421-430` instruments spawned tasks; verification/gateway owners use the active span | traced verify journeys in Rust, Python, and TypeScript | PASS |
| Scenario 4 / D8: deferred runs link rather than parent and preserve attribution | verifier-run cause columns and decode; `VerifierRunner` adds the cause link; observation path keeps its stored link | `manual_run_links_to_its_enqueuing_request`; observation Eval journey | PASS |
| Scenario 5 / D7 / INV-004: operator sampling is independent of client fields while sampled clients remain recordable for tenants | `wyrd-telemetry/src/export.rs:83-240` | telemetry sampling matrix and server dual-export tests | PASS |
| Scenario 6 / INV-001: authenticated route identity is non-exported and keyed by server span identity | `wyrd-telemetry/src/export.rs:242-459`; HTTP and Gate bind only verified identities | same-trace-id two-tenant and unrouted-span tests | PASS |
| Scenario 7 / INV-005: 2,048-copy cap, sibling availability, retry retention, and recovery are proved | `ServerSpanSink::stage` uses `try_acquire_owned`; the permit lives in `ScribeWrite::ServerSpans` | `scribe_outbox.rs:1395-1430` proves cap, direct sibling staging, abandonment, and a separate healthy outbox, but not retry/recovery of the same outbox or verifier-result staging | **FAIL — BR-003** |
| Scenario 8: canonical attempts are children of the exported gateway call and are not duplicated | gateway invocation captures `continued_context`; capture parents attempts on that span; tenant catalog excludes `gateway.attempt` | gateway journeys and action timeline assertions | PASS |
| Scenario 9 / REQ-006: Rust requests propagate active context and presigned storage does not | shared authenticated HTTP and gRPC trace injection; external storage path remains separate | transport tests and local-development journey | PASS |
| Scenario 10 / D4: queued contexts are sealed stably and become complete OTel links | queue and wire retain `TraceContext.trace_state`, but Gate discards it while constructing links | ingest test asserts only link ids and supplies empty trace state | **FAIL — BR-002** |
| Scenario 11: Python and TypeScript propagate active context and install W3C propagation | Python `trace::block_on`; Node `trace::spawn`; both telemetry initializers install W3C propagation | Python and TypeScript traced-request journeys | PASS |
| Scenario 12 / REQ-008: timeline is tenant-scoped, one hop, deduplicated, and totally ordered | authenticated parameterized query and canonical decoder; SDK projections reuse it | focused timeline integration tests, but the duplicate fixture gives both copies the same start time | **FAIL — BR-001** |
| Scenario 13 / AC-002: all three SDKs prove the action timeline; Rust also proves operator identity parity | three public SDK journeys drive real server behavior | recorded Rust 2/2, Python, and TypeScript journey results; source asserts producer/consumer links | PASS |
| REQ-004: every origin records the requester and judge calls retain Card/Run attribution | verifier-run SQL resolves authenticated or tenant SYSTEM actor; verification result and gateway attribution paths updated | SQL and verification integration evidence | PASS |
| REQ-009: request identity and causal trace context are documented as complementary | both architecture documents contain the approved model and linked-ingest rule | static source inspection | PASS |
| Prohibited and non-goal boundaries | one Scribe outbox remains; no trace route, auth/scope change, compatibility alias, sub/act columns, or mesh propagation was added | cumulative diff inspection | PASS |

## Proposed findings

### BR-001 — INCORRECT: timeline deduplication only removes adjacent duplicates

- Violated obligation: REQ-008 and Scenario 12 require deduplication by
  `(trace_id, span_id)` before returning the ordered timeline.
- Location: `crates/shared/wyrd-client/src/bifrost/facade.rs:860-874` and
  `crates/shared/wyrd-client/tests/integration/pg_bifrost_e2e/trace_timeline.rs:142-166`.
- Evidence: the implementation first sorts by
  `(start_time, trace_id, span_id)` and then calls `Vec::dedup_by` on only the
  ids. Equal ids are adjacent only when their stored start times are also
  equal. The test duplicates the row with the same start time, so it cannot
  expose the reachable case where two stored copies of one span disagree on
  start time and another span sorts between them.
- Observable consequence: `trace_timeline` can return the same span identity
  twice, contrary to the public Rust/Python/TypeScript contract.
- Required correction: keep the existing final sort, then retain only the
  first occurrence of each `(TraceId, SpanId)` with a local set. Because the
  vector is already in the required order, this deterministically keeps the
  earliest copy without adding a new type or query path. Extend the existing
  focused test with a duplicate identity at a different start time and an
  intervening span.
- Closure proof:
  `mise exec -- cargo nextest run --locked -p wyrd-client --test integration -E 'test(=trace_timeline::deduplicates_and_orders_ties_by_trace_and_span)'`
  under the task's repository-managed Postgres wrapper.

### BR-002 — INCORRECT: the queued producer's valid `tracestate` is discarded

- Violated obligation: REQ-006/D4 and Scenario 10 carry a
  `ProducerTraceContext.trace_state` so Gate can attach the producer context as
  an OTel link; the canonical `SpanLink` and timeline expose that field.
- Location: `crates/vala/vala-bifrost-redux/src/gate/mod.rs:1279-1291`,
  `crates/shared/wyrd-telemetry/src/lib.rs:274-301`, and
  `crates/wyrd/wyrd-server/tests/integration/pg_grpc_ingest_smoke.rs:693-741`.
- Evidence: the client encodes `trace.trace_state` in
  `wyrd-client/src/bifrost/grpc.rs:483-491`, but `producer_links` returns only
  ids and a sampled boolean. `link_remote_span` always constructs the link
  with `TraceState::default()`. The integration test supplies an empty state
  and compares only id pairs.
- Observable consequence: a valid vendor trace state present at enqueue is
  absent from the CONSUMER link returned by `trace_timeline` and from the
  operator copy, so the transmitted producer context is not preserved.
- Required correction: extend the existing `link_remote_span` input with the
  trace-state string, parse it with the installed OTel `TraceState` parser,
  and use the parsed value in the link. Gate passes each wire value; existing
  deferred-link callers pass an empty value. As with inbound W3C handling, a
  malformed state becomes empty without rejecting an otherwise valid link or
  data batch. Extend the existing Gate/ingest test with one valid non-empty
  state and one malformed state, asserting preservation and fail-open removal.
- Closure proof:
  `mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(=pg_grpc_ingest_smoke::batch_ingest_span_links_each_producer_context)'`
  under the task's Postgres wrapper, plus the exact Gate unit test selector.

### BR-003 — MISSING: the bounded-staging test does not exercise same-outbox recovery

- Violated obligation: Scenario 7 and AC-001 require proof that a sustained
  Scribe outage holds the 2,048 cap, does not starve sibling writes, and returns
  permits after recovery.
- Location: `crates/wyrd/wyrd-server/src/scribe_outbox.rs:1385-1430` and the
  task evidence row that attributes this proof to `pg_verification_runtime`.
- Evidence: the test fills an outbox backed by an unreachable peer, then calls
  immediate shutdown and checks abandonment. It creates a different healthy
  outbox for the success assertion. It stages audit and capture directly, but
  does not stage a verifier result. Therefore no assertion proves that the
  retained items on the failed outbox drain after that sink recovers and
  release their permits.
- Observable consequence: the exact retry/recovery path that owns permits can
  regress to permanent span loss after a transient outage while this test and
  the recorded evidence remain green.
- Required correction: strengthen the existing in-owner test, using the
  existing controllable Scribe test seam, so one outbox is held unavailable,
  reaches the cap, accepts audit/capture/verifier-result siblings, is released,
  drains, restores all 2,048 permits, and accepts one subsequent span. This is
  the smallest proof boundary; no production option or second harness is
  needed.
- Closure proof:
  `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::sustained_scribe_outage_caps_tenant_spans)'`.

## Negative-flow and regression assessment

- Malformed `traceparent`/`tracestate`, no context, unsampled client context,
  foreign-tenant timeline reads, catalog/attribute filtering, repeated batch
  retries, and no-operator-endpoint operation have source and test coverage.
- Authentication and authorization remain the existing owners. Producer links
  are attributed only after the gRPC token verifies, and timeline reads stay
  on the tenant-authenticated Oracle path.
- Presigned storage requests remain on the external transport and do not gain
  Wyrd or trace headers.
- The cumulative diff does not add a second durable writer, route, public span
  DTO, compatibility layer, or configuration option.

## Verification limits

This discovery pass inspected source and the task's recorded green commands. It
did not run Cargo, mise, Postgres, Python, or TypeScript commands because the
parallel review was source-only. The three closure commands above remain for
remediation verification.

