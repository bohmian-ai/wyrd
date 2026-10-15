# TASK-001 invariant review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/verify-attribution`
- Base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`
- Candidate tree: `9df6ae0e34c646e6ef2c32a1c030e2a800683644`
- Approved authority: `changes/active/verify-attribution/spec.md`, revision 3
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`

The current checkout has the same tree as the immutable candidate, so source
locations below describe the candidate exactly. I reviewed the cumulative
base-to-candidate change and traced values from their producers through shared
owners, lifecycle transitions, retries, and consumers. I did not use the
implementation summary as evidence.

## Navigation and invariant map

| Value or state | Producer | Owner and lifecycle | Consumers / sinks |
|---|---|---|---|
| Inbound W3C context | SDK HTTP/gRPC transport | `wyrd_telemetry::serve_request` sets the request span parent before first entry | HTTP, gRPC, MCP request descendants; outbound SDK requests |
| Tenant route | Verified HTTP or gRPC principal | `TenantRoutes`, keyed by the server span's `(TraceId, SpanId)`; inherited at start and removed at end | `TenantExport` and the serving server's `ServerSpanSink` |
| Operator/tenant sampling decisions | `ServerSampler` | One process tracer provider; server policy controls sampled flag, client sampled parent/link can only retain recording | Filtered operator processor and tenant processor |
| Tenant span capacity | `ServerSpanSink::stage` | One 2,048-permit pool per server; permit owned by `ScribeWrite::ServerSpans` through retry/abandonment | Existing process `ScribeOutbox` and Scribe spans table |
| Producer context | Active Rust/Python/Node span at row admission | Row context, first 32 distinct contexts fixed on `SealedBatch` | Identical gRPC retries; Gate CONSUMER links |
| Deferred cause | Enqueuing request or stored observation context | Verifier-run persisted cause / observation row | Independent attempt trace link, never parent |
| Timeline seed | Validated `TraceId` | Existing authenticated Bifrost query path and tenant SQL authority | Rust, blocking Rust, Python sync/async, TypeScript `SpanRecord` projections |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-004: direct, manual, scheduled, and observation executions retain the required actor and independent Run/Card attribution | `components/verification/service.rs:430-598`; `verification/results.rs:136-180,300-343`; `wyrd-sql/src/queries/verifier_runs.rs` inserts and claims `requested_by_principal_id`; Eval builds `GatewayCallSubject` from the claimed subject and observed Run | Recorded `pg_verification_runtime`, `eval_verification`, and SQL owner-lane evidence in the task | PASS |
| REQ-005: valid HTTP, gRPC, and MCP context parents the SERVER request span; malformed context is ignored | `wyrd-telemetry/src/context.rs:20-129`; `wyrd-server/src/http/router.rs:148-192`; `wyrd-tonic/src/server/mod.rs:220-249`; MCP reuses the HTTP edge | Recorded server integration and Rust/Python gateway journeys | PASS |
| REQ-005: synchronous verifier, judge, gateway, ingest, and query work stays below the request span across task fan-out | `vala-eval/src/executor.rs` instruments spawned bucket work with the current span; verification and gateway calls use the active span; Gate instruments `write_batch` with `bifrost.gate.write` | Recorded server integration, gateway, MCP, and three-language verify journeys | PASS |
| REQ-005 / D8: deferred execution links rather than parents | Manual cause is captured in `components/verification/service.rs:362-376`, persisted and claimed by `verifier_runs.rs`, then linked in `verification/runner.rs:527-578`; observation evidence links in `verification/eval.rs:180-182` | `manual_run_links_to_its_enqueuing_request`; observation Eval journey | PASS |
| REQ-006: every normal Wyrd HTTP/gRPC request carries the active context, while external presigned storage requests do not | Central `HttpTransport::authenticated` applies trace headers to authenticated helpers; `BifrostGrpcTransport::add_auth_metadata` does the same for gRPC; the external transport remains separate; Python `trace::block_on` and Node `trace::spawn` scope host contexts at binding entry | Shared transport tests and recorded Rust/Python/TypeScript journeys | PASS |
| REQ-006 / D4: queued rows retain producer context, batches keep a stable bounded vector, and retries resend it unchanged | `wyrd-client/src/transport/trace.rs:75-97`; `wyrd-queue/src/producer.rs:473-564`; `queue.rs:544-597,665-684`; `sink.rs:95-115`; `bifrost/grpc.rs:211-253,331-364` | Queue ordering/bound/retry tests and recorded shared lane | PASS |
| REQ-006 / r3: Gate creates CONSUMER links before first span entry, preserving real sampled flags and accepting malformed/excess metadata without rejecting data | `vala-bifrost-redux/src/gate/mod.rs:1139-1157,1276-1292`; tracing-opentelemetry keeps a builder until entry, so `add_link` here precedes SDK span creation and sampling | `producer_links_skip_malformed_and_excess_contexts`; telemetry sampled-link tests; three action-timeline journeys | PASS |
| REQ-007 / INV-004: operator sampling is independent of client trace id, flags, and tracestate | `ServerSampler::should_sample` uses a server-drawn id for a remote root, local children follow that root, and `OperatorExport` admits only `RecordAndSample` while clearing trace state (`wyrd-telemetry/src/export.rs:94-223`) | Four-row matrix, ratio test, operator filter test, and recorded integration matrix | PASS |
| REQ-007 r3: sampled remote parent or sampled producer link can open exactly one tenant route; catalog descendants inherit it | `TenantRoutes::start` joins a routed local parent or opens from a sampled remote parent/link (`export.rs:292-343`); Gate supplies links before start | `a_sampled_link_records_for_the_tenant`; `a_sampled_producer_link_routes_the_consumer`; action-timeline journeys | PASS |
| REQ-007 / INV-001 / D6: tenant and principal come only from verified identity and never from client trace identity | HTTP binding occurs only in `AuthenticatedPrincipal::from_verified` (`principal_extractor.rs:30-44`); gRPC binding occurs only in `AuthContext::from_verified` (`gate/auth.rs:53-72`); registry key is the server span `(TraceId, SpanId)` and each route owns the serving server's sink (`export.rs:270-380`) | Same-inbound-trace concurrent tenant test; per-server sink test; cross-tenant timeline test | PASS |
| REQ-007 / D5 / INV-002: tenant copy is catalog- and attribute-allowlisted; credentials, SQL, hosts, paths, and tenant identity do not enter copied fields | `in_catalog`, `safe_attribute`, and `tenant_copy` at `export.rs:433-493`; resource is reduced to `service.name`; status/event detail is scrubbed | `tenant_copy_keeps_ids_and_only_safe_fields`, `tenant_filter_drops_detail`, integration attribute assertions | PASS |
| REQ-007: captured gateway attempts remain the sole tenant attempt rows | `in_catalog` excludes `gateway.attempt` and takes its subtree out of the route; captured attempt remains the existing Scribe path | Sampling/catalog test plus action and gateway journeys | PASS |
| INV-003 / INV-005: tenant export does not wait, fail the request, or starve sibling write semantics; staged span memory is bounded | `ServerSpanSink::stage` uses `try_acquire_owned`; overflow calls the existing loss counter; permit is stored in `ScribeWrite::ServerSpans` and released on item settlement/drop (`scribe_outbox.rs:115-128,642-687`) | `sustained_scribe_outage_caps_tenant_spans` covers overflow, sibling staging, shutdown release, and success release | PASS |
| REQ-008 / INV-001: timeline reads the seed and one inbound-link hop through the authenticated tenant query path, with deterministic ordering and deduplication | `Bifrost::trace_timeline` at `bifrost/facade.rs:817-876`; typed parameter; existing SQL path supplies tenant authority; `rows.rs` decodes canonical `SpanRecord` by column name | Five focused timeline tests and cross-tenant action test | PASS |
| REQ-008: Rust, blocking Rust, Python sync/async, and TypeScript expose the same canonical record shape | `bifrost/blocking.rs`; Python native and public `bifrost` wrappers/stubs; TypeScript native `trace_timeline` and public `traceTimeline`; all project `SpanRecord` | Python and TypeScript type checks plus three-language action journeys recorded in task evidence | PASS |
| REQ-009: request identity and causal trace identity remain distinct, and dual export/catalog authority is documented | `architecture/wyrd-design.md`; `architecture/bifrost-design.md` | Recorded architecture grep/review | PASS |
| AC-001 through AC-004: public journeys cover three SDKs plus MCP and negative/edge flows at their required tiers | Rust/Python/TypeScript action and verification journeys, MCP verification journey, server integration tests, queue/telemetry units | Task evidence records all named focused and owner lanes green | PASS |
| AC-005: repository gates for the touched boundaries | No bypass, ignore, or weakened gate was found in the cumulative diff | Task evidence records format, lints, codegen, dependency, tenant-isolation, Python, and TypeScript checks green; `git diff --check base..candidate` is clean in this review | PASS |
| Prohibited: no second Scribe writer/path/table, trace-read route, compatibility alias, presigned trace headers, or auth/permission change | Tenant copies use `ScribeOutbox`; timeline uses existing Bifrost query; external HTTP path remains header-free; auth only supplies verified route identity | Source audit of cumulative diff | PASS |
| Non-goals: no sub/act storage change, Workflow Run/Card correlation, or general mesh propagation | No such contract or owner was added in the cumulative diff | Source audit of cumulative diff | PASS |

## Proposed findings

None.

The approved `export.rs` clone is not a material invariant violation. It is one
localized read of an unrouted span's start data so the existing sampler rule
and tenant route rule use the same parent/link eligibility. It creates no
second route opener, writer, durable representation, configuration surface, or
public contract. The existing `ponytail:` marker correctly records the measured
upgrade trigger instead of adding speculative machinery now.

## Non-blocking notes

- The task frontmatter still names specification revision 2 even though its
  implementation evidence and the approved specification describe revision 3.
  The reviewed behavior follows r3. This is change-packet metadata drift, not a
  runtime invariant failure, so it is not a material implementation finding in
  this report.
- `ProducerTraceContext.trace_state` is carried on the wire but Gate constructs
  producer links with an empty trace state. The acceptance contract and tests
  require producer identity, ordering, and the real sampled flag; preserving
  client vendor state in the operator copy would also conflict with the
  operator-independence constraint. No correction is required.

## Verification limits

- I inspected source and the task's recorded evidence. Per orchestrator
  coordination, I did not run Cargo, mise, codegen, database, Python, or
  TypeScript commands during parallel discovery.
- I independently ran the read-only `git diff --check` over the immutable
  base-to-candidate range; it passed.
- The candidate commit and current checkout are different histories with the
  same tree, so no source drift exists for this review. A later tree change
  requires a new immutable candidate review.

## Result

**PASS**

Source-local proposed findings: none.
