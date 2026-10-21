---
id: TASK-004
kind: implementation
status: implemented
spec: SPEC-gateway-capacity
spec_revision: 5
requirements: [REQ-016, REQ-017, REQ-018, REQ-019, AC-015, AC-016, AC-017, AC-018, AC-019]
depends_on: [TASK-002]
---

# Capture loss under saturation

Implements [revision 5](../spec-revision-5.md). Code and verification are
complete. The release benchmark rerun is deferred at the maintainer's request
(2026-10-21) and remains open; see [Open evidence](#open-evidence).

## Implementation

- `wyrd-runtime::outbox`
  - `LossReason` (`shed|rejected|unreachable|shutdown`) labels every
    `outbox_events_lost_total` increment.
  - `OutboxLimits { items, bytes }` with `OutboxSink::weight`. The ¾/¼
    reserved split applies to bytes. Unreserved staging sheds on either
    limit. A spent reserved slot always keeps its item, and `reserve` refuses
    once the reserved items' bytes reach their share. `OutboxFull.unit`
    names which limit refused.
  - Admission happens under the queue read lock, so shutdown settles byte
    shares exactly.
  - Shed records are aggregated to at most one per outbox and tenant per
    `SHED_LOG_INTERVAL` (1 s), each carrying the count; the remainder is
    flushed when shutdown begins.
  - New counters: `outbox_items_written_total` and
    `outbox_bytes_written_total`.
- `ScribeSink`
  - `LANES = 4`. A capture and every audit decision get their lane from a
    hash of the request ID, so one call shares a lane. Verifier inputs and
    results, server spans, and unprojected audits take lane 0.
  - `weight` is a capture's payload texts plus its objects, a span's encoded
    length, or a row batch's Arrow memory.
  - Loss reasons: span-permit exhaustion is `shed`, no route is
    `unreachable`, and terminal refusal or projection failure is `rejected`.
  - `SCRIBE_WRITERS` is unchanged at 4.
- Config: `scribe.outbox_bytes` (default `SCRIBE_BYTES` = 1 GiB, at least 1)
  is wired at boot beside `scribe.outbox_capacity`. Submission outbox bytes
  are unlimited, because its items are bounded by verification input limits.
- Capacity report
  - `Integrity` gains `shed` and `written_items`; `Step.durable` holds the
    items and bytes written during the traffic window.
  - An overhead row that passes except for shedding is `CAPTURE-SATURATED`.
    It is not a failure, never `TARGET`, and is listed in the summary line
    of `report.md`. Its evidence gives the shed count, the shed fraction, and
    durable items/s and MB/s.
  - The drain row and the shutdown quiescence check forgive `shed`. Overload
    forgives `shed`, including that many missing durable rows. Rate,
    recovery and integrity rows still fail on any loss.
- Docs: security posture, Bifrost design, reliability and recovery,
  runbooks, and the docs-site verification limits row.

## Commit-order reader check (REQ-019)

An independent read-only survey of every reader of the six Scribe outbox
tables found one reader that relies on order: the Verifier group read
(`verification/groups.rs`). It orders executions and picks results by
`wyrd_ingested_at`, which follows commit order. Pinning all Verifier inputs
and results to lane 0 keeps today's arrival order for them; the
`pg_verification_groups` and MCP group journeys that assert call order are
therefore unaffected.

Every other reader is safe:
- spans, read by event time;
- results, by point lookup;
- trace timeline, unordered;
- MCP guide queries, by producer time;
- audit log readers, by `created_at` or counts;
- gateway harnesses, as sets, counts, or by `started_at`;
- capacity bins, counts.

No reader uses a cursor over commit order.

## Acceptance evidence

| Criterion | Implementation | Verification | Result |
|---|---|---|---|
| AC-015 reasons per path | `outbox.rs` `count_lost`, `lose_late`, `shed`, `shutdown`; `scribe_outbox.rs` reasons | `outbox::tests::bounded_audit_staging_remains_nonblocking` (shed), `shutdown_counts_items_unwritten_at_the_deadline_as_lost`, `concurrent_shutdown_releases_reserved_losses_once`, `shutdown_refuses_items_staged_after_it_begins` (shutdown); `scribe_outbox::tests::an_unreachable_route_drops`, `a_terminal_rejection_never_blocks_later_writes`; `pg_verification_runtime::sustained_scribe_outage_caps_tenant_spans` | PASS |
| AC-016 bytes before items | `Outbox::stage`, `reserve`, `send_reserved`, `Admission` bytes | `outbox::tests::large_items_shed_on_bytes_before_items` | PASS (release memory evidence deferred) |
| AC-017 report classification | `report.rs` `Verdict::CaptureSaturated`, `Report::saturated`; `envelope.rs` `capture_misses(.., allow_shed)` | `report::tests::shed_only_rows_are_capture_saturated_and_other_loss_fails`, `every_row_requires_latency_correctness_and_zero_loss`; whole capacity bin (20 tests) | PASS (release matrix deferred) |
| AC-018 lanes keep order | `ScribeSink::{LANES, lane}` | `scribe_outbox::tests::a_calls_writes_share_a_lane_and_calls_spread`; `outbox::tests::a_tenants_lanes_write_concurrently_and_each_keeps_order`; reader check above; `test:wyrd` | PASS (new ceiling deferred) |
| AC-019 aggregated shed log | `Outbox::shed`, `log_shed`, shutdown flush | `outbox::tests::shed_records_are_aggregated_per_tenant_and_interval` | PASS (release log deferred) |

## Verification

| Command | Result |
|---|---|
| `mise run fmt` (`cargo fmt --check`) | PASS |
| `mise run lints` | PASS |
| `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(/outbox::tests::/)'` | PASS (15) |
| `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/scribe_outbox::tests::|config::tests::/)'` | PASS (95) |
| capacity bin tests (`wyrd-testing --bin capacity`) | PASS (20) |
| `pg_verification_runtime::sustained_scribe_outage_caps_tenant_spans` (repository Postgres wrapper) | PASS |
| `mise run test:shared` | PASS (806) |
| `mise run test:wyrd` | PASS after the fix below (first run 2359/2360) |
| `pg_administration_tests::{gateway_administration_is_authorized_audited_and_tenant_isolated, gateway_apply_is_atomic_and_tenant_scoped}` exact selectors, three runs | PASS |
| `mise run check:tenant-isolation`, `check:deps`, `codegen:check` | PASS |
| `mise run docs:check` | PASS after committing regenerated `docs/public/llms-full.txt` |

### Diagnosis: cross-call audit order

- **Symptom:** `test:wyrd` failed
  `gateway_administration_is_authorized_audited_and_tenant_isolated`
  (`pg_administration_tests.rs:892`). The expected decisions arrived with
  `put/denied` ahead of two other callers' `list/denied`.
- **Evidence:** `ScribeSink::lane` hashes an audit decision's `request_id`, and
  `SCRIBE_WRITERS = 4` writes those lanes concurrently. The test's `caller()`
  gives every caller a fresh `RequestId`, and nothing settles until the
  assertion.
- **Cause:** the test asserted cross-call commit order, which REQ-019 no
  longer promises. Each caller's own decisions stayed ordered.
- **Fix site:** the test assertions, not production lane selection, which
  would contradict REQ-019. An independent diagnostician confirmed the cause,
  and found one other test with the same exposure:
  `gateway_apply_is_atomic_and_tenant_scoped` (`:1739`). Both now compare the
  decisions sorted. Every other ordered audit or row assertion either uses
  one request or settles between requests.

## Open evidence

The release benchmark matrix on the reference host was not run, by maintainer
instruction. It still has to show:
- only `shed` loss under saturation;
- chat and chat-stream at 64 clients with zero `shed`;
- the 64 KiB capture case keeping memory headroom at 16 and 64 clients;
- the shed log rate;
- the new ceiling against the revision-5 baseline.

Rerun it with the gated serial `bench.sh` before closing the change.
