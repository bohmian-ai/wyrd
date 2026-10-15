# TASK-001 findings validation

## Immutable subject

- Base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`
- Candidate tree: `9df6ae0e34c646e6ef2c32a1c030e2a800683644`
- Approved authority: `changes/active/verify-attribution/spec.md`, revision 3
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`

The candidate object and the current source tree have the same tree. This pass
validated every discovery claim against that source, the complete task, the
approved specification, and the applicable repository authority. No Cargo,
mise, database, code-generation, Python, or TypeScript command was run.

## Deduplicated finding ledger

### FIND-TASK-001-1 — CONFIRMED — REGRESSION: producer contexts bypass the queue byte owner

- Discovery source: `CONDUR-001`.
- Violated obligation: `QueueConfig::client_byte_limit_bytes` and
  `ClientByteBudget` are explicitly the sole bound for every client-owned row,
  sealed batch, in-flight send, and retained retry. Bifrost's resource
  authority also requires every Wyrd-owned queue and retained buffer to remain
  bounded. Scenario 10 added producer contexts to all of those ownership
  states without relaxing either invariant.
- Exact locations:
  - `crates/shared/wyrd-queue/src/producer.rs:541-560`
  - `crates/shared/wyrd-queue/src/queue.rs:33-48,557-597,667-683`
  - `crates/shared/wyrd-queue/src/sink.rs:95-115`
  - `crates/shared/wyrd-queue/src/config.rs:7-17`
  - `crates/wyrd-spec/src/trace.rs:7-17`
- Evidence and reachability: `Producer::enqueue_prepared` reserves only the
  prepared Arrow charge, then retains an `Arc<TraceContext>` for every logical
  record on an intentionally unbounded handoff channel. `TraceContext` owns
  three strings and places no bound on `trace_state`. Sealing clones up to 32
  distinct contexts into each `SealedBatch`, while the sealed guard is resized
  to the IPC frame alone; the batch then retains those strings through every
  transport attempt and ambiguous retry. Normal traced Rust, Python, and
  TypeScript writes all reach this path, and the public Rust queue surface can
  also supply a context directly. The 32-link limit bounds one batch's count,
  not the bytes retained across staged records and retained batches.
- Observable consequence: `used_bytes()` can remain at or below the configured
  ceiling while the queue owns additional, potentially unbounded context
  memory. A burst of traced writes or an unavailable/ambiguous sink can exceed
  the configured client memory ceiling and expose the process to resource
  exhaustion.
- Decision-complete smallest correction: keep `ClientByteBudget` as the only
  owner and add no limit, option, or second counter. Charge each context's
  owned bytes at admission before it enters the unbounded handoff. Carry that
  same charged ownership with the context while rows are staged, and transfer
  ownership only for the first 32 distinct contexts selected for each sealed
  batch; discarded contexts release with their rows. The selected ownership
  remains charged beside the existing sealed frame through send, ambiguous
  retry, ACK, terminal refusal, and shutdown. This preserves the existing
  stable context vector, UUID, retry behavior, and 32-context rule without a
  downstream guard or a new resource owner.
- Focused closure proof: add one `wyrd-queue` test using a small
  `ClientByteBudget`, distinct non-empty trace states, and the existing
  ambiguous `MockSink`. It must show admission refuses before the configured
  ceiling is exceeded, the context charge remains in `used_bytes()` while the
  sealed batch is retained and retried, and terminal settlement returns the
  budget to zero. Name it
  `producer::tests::producer_context_bytes_stay_bounded_through_ambiguous_retry`
  and run `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E
  'test(=producer::tests::producer_context_bytes_stay_bounded_through_ambiguous_retry)'`.

### FIND-TASK-001-2 — CONFIRMED — INCORRECT: timeline identity deduplication is adjacency-dependent

- Discovery sources: `BR-001`, `PD-001`.
- Violated obligation: spec REQ-008 and task Scenario 12 require one result per
  `(trace_id, span_id)`, ordered by `(start_time, trace_id, span_id)`.
- Exact locations:
  - `crates/shared/wyrd-client/src/bifrost/facade.rs:854-874`
  - `crates/shared/wyrd-client/tests/integration/pg_bifrost_e2e/trace_timeline.rs:142-166`
- Evidence and reachability: `trace_timeline` sorts by start time before calling
  `Vec::dedup_by` on identity. `dedup_by` compares adjacent elements only. Two
  stored rows with the same identity but different start times remain when any
  other span sorts between them. The existing fixture duplicates the same
  start time, so it proves only the adjacent case. Every Rust, Python, and
  TypeScript timeline call consumes this shared result.
- Observable consequence: the public timeline can return the same logical span
  twice, which violates its documented contract and can double-count or
  mis-traverse a span in every SDK.
- Decision-complete smallest correction: retain the existing authenticated SQL,
  decoder, and total-order sort. After sorting, retain the first row for each
  `(TraceId, SpanId)` using one local standard-library set. Do not add a query,
  timeline type, pagination rule, or SDK-specific deduplication.
- Focused closure proof: modify the existing
  `deduplicates_and_orders_ties_by_trace_and_span` fixture so one duplicate has
  a different start time and another span sorts between the copies, then assert
  the identity appears once in total order. Under the task's repository
  Postgres wrapper, run `mise exec -- cargo nextest run --locked -p
  wyrd-client --test integration -E
  'test(=trace_timeline::deduplicates_and_orders_ties_by_trace_and_span)'`.

### FIND-TASK-001-3 — CONFIRMED — MISSING: Scenario 7 has no same-outbox recovery and sibling-availability proof

- Discovery source: `BR-003`; independently resolved by the focused follow-up.
- Violated obligation: task Scenario 7 explicitly requires
  `pg_verification_runtime::sustained_scribe_outage_caps_tenant_spans` to prove
  the 2,048 cap, loss accounting, permit return after recovery, and continued
  HTTP, gRPC, MCP, audit, and verifier-result availability. AGENTS.md and the
  testing authority state that a lower-tier test does not replace a required
  cross-boundary negative-flow proof.
- Exact locations:
  - `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md:339-363`
  - `crates/wyrd/wyrd-server/src/scribe_outbox.rs:1385-1430`
  - absent from `crates/wyrd/wyrd-server/tests/integration/pg_verification_runtime.rs`
- Evidence: the owner unit test fills an unreachable outbox, stages audit and
  capture directly, abandons that outbox, and then creates a different healthy
  outbox for its success assertion. It does not recover the failed outbox,
  stage a verifier result, or drive HTTP, gRPC, and MCP during the outage. The
  production ownership is credible and generic outbox tests cover retry, but
  neither proves this task's named cross-boundary recovery obligation. The task
  evidence nevertheless marks `pg_verification_runtime` as the proof even
  though the named test does not exist.
- Observable consequence: a regression in the seam between retained span
  permits, same-outbox recovery, or sibling request/result staging can ship
  while the recorded evidence stays green. This is a proof gap; no production
  defect was established.
- Decision-complete smallest correction: leave production code unchanged. Add
  the named focused server integration test using the existing test-server and
  Scribe/outbox test facilities. On one outbox, hold Scribe unavailable, drive
  more than 2,048 tenant copies, verify overflow/loss, exercise the existing
  HTTP, gRPC, and MCP probes and one verifier-result stage, then make that same
  Scribe route available and assert the retained copies drain, all permits
  return, and one subsequent copy is accepted. Audit and gateway-capture
  mechanics may remain in the existing owner unit test; do not add a second
  production path, limiter, or general fault framework.
- Focused closure proof: run the exact command already prescribed by Scenario
  7:
  `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(=pg_verification_runtime::sustained_scribe_outage_caps_tenant_spans)'"`.

### FIND-TASK-001-4 — CONFIRMED — VIOLATION: the modified `stored` decoder lost its rustdoc

- Discovery sources: `REPO-001`, `MAINT-001`.
- Violated obligation: `architecture/agent-rules.md` makes substantive rustdoc,
  including `# Errors` on fallible helpers, a hard acceptance rule for every
  new or materially modified Rust item.
- Exact location: `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:2143-2158`.
- Evidence: the pre-existing “Decode a stored value” documentation now sits
  immediately before the new `fixed_width` documentation, so Rust attaches
  both blocks to `fixed_width`. `stored` begins with no rustdoc. Its many decode
  callers remain unchanged, so this is one documentation-placement defect, not
  multiple caller findings.
- Observable consequence: `fixed_width` advertises an unrelated error contract
  and the shared stored-value decoder no longer states its required fallible
  behavior. Repository authority explicitly classifies this as blocking.
- Decision-complete smallest correction: move the existing “Decode a stored
  value” intent and `# Errors` block back immediately above `stored`; leave only
  the fixed-width intent and `# Errors` above `fixed_width`. Change no code and
  add no test.
- Focused closure proof: inspect the two adjacent items and run the remediation
  write set's existing Rust format and lint lanes. A separate documentation
  harness is unnecessary because `stored` is private.

## Rejected and nonmaterial proposals

- **REJECTED — `BR-002` / `PD-002` (preserve producer `trace_state` and all
  trace-flag bits in Gate links).** Approved INV-004 says no client-supplied
  trace field changes operator-export content or sampling; REQ-007 says the
  tenant uses the operator's links and the operator copy is unchanged.
  `architecture/wyrd-design.md` explicitly includes `tracestate` in that
  exclusion. Gate therefore validates the one-byte wire flag, keeps the real
  sampled bit needed by the approved eligibility rule, and constructs the
  operator/tenant link without vendor state or reserved bits. Preserving those
  fields would change approved operator content; a richer tenant-only link
  would break the required copy parity. That would need a future spec revision,
  not TASK-001 remediation.
- **REJECTED as nonblocking — task frontmatter still names spec revision 2.**
  The immutable candidate, approved authority, implementation, and evidence all
  use revision 3. This stale task metadata does not change runtime behavior or
  leave an approval decision open.
- **REJECTED as nonblocking — function-local test imports.** The placements
  violate a repository style rule but have no behavioral, public-contract,
  security, tenancy, durability, or maintenance consequence sufficient for a
  task finding. The validated rustdoc defect already captures the only hard
  documentation rule failure.
- **REJECTED — linked-route `SpanData` clone.** It is one localized,
  non-retained read needed because the OpenTelemetry processor exposes links
  through the exported span snapshot. It creates no second routing mechanism
  or configuration, has a documented profiling threshold, and the owner
  explicitly accepted this simple cost. No corrective machinery is justified.

## Validated empty claims

The security/tenancy and system-resilience reports' empty finding sets are
supported by source: tenant/principal routing derives only from verified
identity; timeline reads retain the authenticated Oracle boundary; tenant
copies remain allowlisted and non-blocking; retries keep stable identity; and
shutdown/terminal paths release the span permit. The invariant report's empty
set is valid except where the independent data and proof reviews exposed
FIND-TASK-001-1 through FIND-TASK-001-3. No additional abstraction, migration,
compatibility path, public option, dependency, or second writer is warranted.

## Specification impact

None of the four retained findings requires a specification revision. Each is
a bounded correction within approved revision 3: restore the existing queue
memory invariant, make the existing timeline contract true, add the task's
already-decided focused recovery proof, and repair required rustdoc placement.
