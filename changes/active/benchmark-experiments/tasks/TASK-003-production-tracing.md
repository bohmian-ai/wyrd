---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-benchmark-experiments
spec_revision: 8
requirements: [REQ-017, REQ-018, REQ-019, INV-001, INV-003, INV-006, INV-010, AC-010, AC-011]
depends_on: []
---

# Production tracing that preserves client timelines

## Outcome and Value

Operators can change log verbosity and diagnostic trace sampling without losing
client-visible server behavior. Normal operator tracing samples 1% of traces by
default instead of implicitly exporting everything. Export loss becomes visible
without making requests fail. Tracing remains useful for optimization.

For example, a sampled client gateway call remains part of the tenant's connected
timeline when the operator ratio is zero, the endpoint is absent or log verbosity
is reduced. Client spans must themselves be exported to Wyrd to appear there.

## What Changes and What Must Stay True

Apply revision 8 only after approval. Keep the existing single provider, W3C
parent/link propagation, remote-client/operator sampling isolation, authenticated
tenant catalog copies, scrubbing and nonblocking outbox. Do not replace the
tracer or route all internal server spans into the tenant table.

Separate log filtering from operator filtering with the specified trace-filter
setting; normal/debug operator verbosity cannot gate required tenant copies.
Use the specified 0.01 default only for operator sampling; validate explicit
ratios. Preserve explicit 0 and 1. With no endpoint, no operator export occurs.

Review the actual span trees for Bifrost, Verification and Gateway and retain
meaningful operation boundaries, including queue/admission, dependencies,
retries, streaming and background jobs. Move function-detail diagnostics to
debug where justified; preserve handler instrumentation and tenant catalog
behavior. No arbitrary spans/request threshold, second provider, tail sampler,
new tracing backend or dynamic capture service is part of this task.

Keep the stock SDK batch processor and its finite nonblocking queue. Use its
first queue-overflow warning and orderly-shutdown lifetime drop total for queue
loss visibility. There is no exact live dropped-span metric requirement. Keep
SDK warning logs enabled in the documented normal configuration; abrupt process
termination cannot guarantee a shutdown total.

Expose failed export batches through a small exporter wrapper, counting one per
failed export call, not queue drops or permanently lost spans. Preserve the
export result and delegate existing flush/shutdown behavior. Keep tenant shedding
counters separate. Do not add a custom queue, worker, retry policy, log-message
parser, SDK fork or replacement processor. Keep correlated,
scrubbed error logs even for unsampled failures. Head sampling cannot promise an
error trace for every failure. Operator settings must be documented at their
owning architecture/operations sections and in the existing configuration guide;
no new configuration hierarchy is needed. Explain how to alert on SDK overflow
warnings and that failed-batch counts do not measure permanent span loss.

## Approach

1. Trace production subscriber construction and both export destinations; compare
   the production and test subscriber paths before changing filters.
2. Separate log, operator and tenant decisions and apply the explicit defaults.
3. Review representative engine span trees and remove only unjustified normal
   diagnostic detail while preserving the tenant catalog and handler rules.
4. Add bounded export-loss observability and failure-path proofs.
5. Exercise real client/server timeline and tenant-negative journeys, update
   owning documentation, and record representative trace inventories.

## Ordered Implementation Scenarios

### Scenario 1 — Log verbosity cannot remove tenant behavior

**Behavior.** Production initialization with quiet logging still records the
same required sampled-client catalog tree. Operator trace verbosity controls
only operator diagnostic selection. The production and test policies agree.

**RED.** Add the planned production-filter test in V1. Initialize the real
subscriber and compare tenant output across log/operator filter settings;
the current global filter should remove required spans in the quiet case.
A test using only init_test_capture is insufficient.

**GREEN.** Separate destination policies in the existing owner. Preserve remote
parent/link identity and safe fields. Run V1 and V4 after this change.

**REFACTOR.** Reuse existing subscriber and catalog composition; remove redundant
filter logic without adding a parallel subscriber implementation.

### Scenario 2 — Operator sampling cannot be forced by a caller

**Behavior.** Unset ratio selects 0.01; explicit 0/1 and intermediate valid ratios
are honored; invalid/NaN ratios are rejected. Sampled client input cannot raise
operator export, but required tenant copies remain available at operator zero
and without an endpoint. Children retain the selected trace decision.

**RED.** Add planned V2 against the production configuration/default path; the
current unset ratio is AlwaysOn. Extend existing sampling coverage as needed.

**GREEN.** Change the default at its shared configuration owner and preserve the
existing four-way operator/client decision matrix. Rerun V1, V2 and V4.

**REFACTOR.** Keep one default and one validation path; avoid benchmark-specific
sampler branches and duplicate provider configuration.

### Scenario 3 — SDK loss signals and failed-batch counts remain truthful

**Behavior.** Queue saturation retains the SDK overflow warning and orderly-
shutdown lifetime drop total. Failed exports increment the failed-batch counter
once per failed call; successful calls do not. No live drop count is inferred
from warnings. Export failure/stall, flush, shutdown and interruption remain
bounded and preserve originating request results and independent tenant copying.
Error logs remain available for unsampled requests, subject to declared logging
filters; tenant shedding retains its existing counters.

**RED.** Planned V3 injects failing/successful exporter calls and asserts accurate
failed-batch counts and unmodified results; the counter is currently absent.
Use the actual stock processor with controlled stalls to exercise overflow,
flush/shutdown and interruption without host load or timing sleeps. Existing
SDK overflow reporting may already pass: record verification-only proof for
that part rather than manufacture a failing runtime change. Assert the SDK
warning and shutdown total separately; never scrape them into a metric.

**GREEN.** Wrap only the installed exporter's export result for failed-batch
counting. Delegate the processor's admission/batching/lifecycle and exporter
flush/shutdown. Preserve bounded nonblocking behavior, request success and
tenant policy; run V3–V6. Capture SDK internal logs without replacing the
production pipeline or treating a shutdown-only count as a live reading.

**REFACTOR.** Remove duplicate failure reporting and unnecessary wrappers. Keep
one narrow exporter adapter and the stock processor; no custom queue, scheduler,
log parser or generic exporter framework is justified by this requirement.

### Scenario 4 — The real client sees its connected server timeline

**Behavior.** An authenticated client exports its spans to Wyrd, calls Gateway,
then queries its timeline: client and catalog server spans retain IDs, parents
and links under operator zero/no endpoint and quiet logs. A different tenant
cannot read it. Exporting client spans solely elsewhere does not promise those
client spans appear in Wyrd.

**RED.** Extend the existing real SDK journey and server rejection-path journeys
(V5/V6) with missing policy cases; assert actual readback rather than internal
sampler decisions. If existing behavior passes unchanged, record verification
rather than manufacture a failure or production churn.

**GREEN.** Close only policy/continuation regressions revealed by those journeys;
keep public SDKs thin. Run V1–V6 with repository Postgres setup.

**REFACTOR.** Reuse the existing action-timeline fixture and gateway capture
paths; do not add another timeline API or emulate client runtimes in Rust.

## Acceptance Criteria

| Obligations | Observable proof |
|---|---|
| REQ-017, INV-010, AC-011 | V1/V2/V4 prove production filter independence, explicit defaults and caller isolation; V5/V6 prove tenant readback under the same policies. |
| REQ-018, INV-001, INV-003, AC-010 | V3 proves exact failed-batch counting, SDK overflow/shutdown reporting and bounded lifecycle; V6 preserves independent tenant caps and request results without weakening authorization/audit. |
| REQ-019, INV-006 | Reviewed retained/debug span inventory and configuration instructions explain normal operation, temporary detailed capture, restoring policy, head-sampling limits and safe correlation. No secrets or payloads enter logs/artifacts. |

## Implementation Reference

- `wyrd-telemetry/src/lib.rs::init` installs the production global filter;
  `init_test_capture` currently filters formatting differently. Existing
  `capture_pipeline_preserves_span_semantics` cannot alone prove production
  independence. Extend this owner rather than add a subscriber.
- `wyrd-telemetry/src/export.rs::ServerSampler`, operator processor and catalog
  own remote-parent decisions, RecordOnly tenant spans and scrubbing. Existing
  export tests and real server verification journeys consume this policy;
  preserve them while separating filtering.
- `wyrd-server/src/config.rs` parses/validates operator settings, and
  `app/metrics.rs::init_telemetry` supplies them. Extend those consumers together;
  do not encode a different benchmark-only production policy.
- `wyrd-server/src/scribe_outbox.rs::ServerSpanSink` already caps tenant copies
  and counts shedding. Installed opentelemetry_sdk 0.31.0 keeps
  BatchSpanProcessor::dropped_spans_count private and on_end returns no admission
  result. Its first-drop warning and exact orderly-shutdown total remain SDK
  logs, not live counters. Preserve that whole queue/worker/flush/shutdown owner.
  The public SpanExporter::export result is the supported seam for counting
  failed calls through a narrow delegating wrapper. This does not observe queue
  loss. The source-backed limitation and selected correction address PR-001 in
  .dev/review/benchmark-experiments-r7-plan-readiness/review.md; that report is
  preserved as the historical revision-7 review, not rewritten as a new verdict.
- `sdks/wyrd-sdk-rust/tests/integration/action_timeline.rs` drives the public SDK
  exporter, Gateway and timeline. `pg_verification_runtime` covers continuing and
  queued tenant spans after operator rejection. Source-validated commands below
  preserve these existing owners and setup.
- Owning documentation: `architecture/wyrd-design.md` trace-context section,
  security telemetry/privacy rules and existing operations/configuration docs.
  Document one current policy without adding a parallel authority.

## Verification and Evidence

All commands are sequential. Planned names below are new, not existing tests:
confirm the actual selector after adding each test and before RED. Record RED
failure, GREEN and final verification; run no benchmark concurrently.

### V1 — Production filter isolation (planned)

```bash
mise exec -- cargo nextest run --locked -p wyrd-telemetry --features test-support --lib -E 'test(=tests::production_filters_preserve_tenant_traces)'
```

### V2 — Sampling configuration/defaults (planned)

```bash
mise exec -- cargo nextest run --locked -p wyrd-telemetry --features test-support --lib -E 'test(=tests::operator_sampling_defaults_are_explicit)'
```

### V3 — Failed-batch counts and SDK loss/lifecycle reporting (planned)

```bash
mise exec -- cargo nextest run --locked -p wyrd-telemetry --features test-support --lib -E 'test(=tests::operator_export_loss_is_observable)'
```

### V4 — Existing export and capture regression owner

```bash
mise exec -- cargo nextest run --locked -p wyrd-telemetry --features test-support --lib -E 'test(/^export::tests::/) | test(=tests::capture_pipeline_preserves_span_semantics)'
```

### V5 — Existing real SDK timeline and tenant-negative journey

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=action_timeline::service_action_timeline_is_one_connected_tree) | test(=action_timeline::service_action_timeline_survives_a_quiet_operator_policy) | test(=action_timeline::another_tenant_cannot_read_the_timeline)'"
```

The existing tests are lifecycle-ignored, hence run-ignored; add no new ignore.

### V6 — Existing server isolation and tenant cap seams

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -P journey -E 'test(=pg_verification_runtime::operator_rejected_requests_still_reach_a_continuing_tenant) | test(=pg_verification_runtime::operator_rejected_queued_attempt_still_reaches_a_sampled_caller)'"
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::sustained_scribe_outage_caps_tenant_spans)'
```

### V7 — Changed-owner checks and non-TDD proof

```bash
mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
mise exec -- git diff --check
```

Inspect the span inventory and architecture/configuration updates against the
real engine traces. Documentation needs no manufactured RED. Language-surface
regression journeys run once on the integrated candidate, not as full SDK
suites at each scenario. Any directly changed SDK projection gets its focused
owner test/type checks before handoff.

## Material Stop Conditions

Stop for authority if preserving the catalog requires changing tenant consent,
authentication, scrubbed fields or W3C identity; if a second provider, new
runtime dependency/feature or tail-sampling service is required; or if a filter
cannot remain independent of log settings. A hidden SDK drop counter does not
authorize replacing its processor: the selected contract deliberately uses SDK
loss logs and a failed-export-batch counter instead. Do not weaken the catalog to reduce
operator span volume. Revision 8 must be approved before implementation.

## Authority Links

- [Specification](../spec.md), revision 8.
- [Repository rules](../../../../AGENTS.md).
- [Workflow](../../../spec-driven-development.md).
- [Execution](../../../implementation-execution.md).
- [Testing](../../../../TESTING.md).
- [Trace-context authority](../../../../architecture/wyrd-design.md#trace-context--w3c-traceparent-and-tracestate).
- [Security posture](../../../../architecture/wyrd-security-posture.md).

## Evidence

Implemented on the maintainer's direct instruction while spec revision 8 is
still `draft`; approval of revision 8 remains outstanding. Commits
`93dcbd390`, `927a7f1f4`, `d6aa03aa4`, `50ec497f4`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-017 / INV-010 / AC-011: log and operator filters independent; tenant copies survive both | `wyrd-telemetry/src/lib.rs` `subscriber`: per-layer filters (fmt ← `WYRD_LOG`/`RUST_LOG`; OTel ← `WYRD_TRACE_FILTER` or catalog spans/events at `INFO`+); `init` and `init_test_capture` share it; `TelemetryConfig::trace_filter` | V1 `tests::production_filters_preserve_tenant_traces`: RED at `93dcbd390`'s parent shape (quiet logs → `[]` tenant copies), GREEN after; also asserts an unsampled error is logged inside its request span | PASS |
| REQ-017: 1% default, explicit 0/1/intermediate honored, invalid/NaN refused, one validation path | `TelemetryConfig::operator_sample_ratio`; `telemetry_sampler`; `wyrd-server/src/config.rs` check 15 now calls it | V2 `tests::operator_sampling_defaults_are_explicit`; `wyrd-server --lib config::` 82/82. RED is source evidence: unset ratio was `Sampler::AlwaysOn` | PASS |
| REQ-018 / AC-010: failed-batch counter, SDK overflow/shutdown loss, bounded lifecycle | `export.rs` `FailedBatches` + `TRACE_EXPORT_FAILED_BATCHES_TOTAL` (`wyrd_trace_export_failed_batches_total`) around the stock `BatchSpanProcessor` exporter | V3 `tests::operator_export_loss_is_observable`: result preserved, 1 count per failed call only; stalled export never blocks `on_end`; `SpanDroppingStarted` warning; `dropped_span_count=2` at shutdown; shutdown bounded at 100 ms; exporter shutdown delegated | PASS |
| Existing export/capture owner preserved | unchanged `ServerSampler`, `OperatorExport`, catalog, scrubbing | V4 `export::tests::*` + `capture_pipeline_preserves_span_semantics`: 27/27 lib tests | PASS |
| AC-011: real client → Gateway → tenant timeline at ratio 0, no endpoint, quiet logs/traces; tenant-negative | `action_timeline.rs` shares `act_and_read`; new `service_action_timeline_survives_a_quiet_operator_policy` installs production `wyrd_telemetry::init` (`filter`/`trace_filter` `error`, ratio 0, no endpoint) | V5: 3/3 incl. `another_tenant_cannot_read_the_timeline` | PASS |
| INV-001/003: tenant caps and rejection paths | no change | V6: both `pg_verification_runtime::operator_rejected_*`, `scribe_outbox::tests::sustained_scribe_outage_caps_tenant_spans` | PASS |
| REQ-019 / INV-006: inventory and operator documentation | `architecture/wyrd-design.md` trace context; `architecture/operations/reliability-and-recovery.md` indicators + Operator tracing (normal policy, temporary detailed capture on one instance, restore, alerting, head-sampling limit, scrubbed fields); deploy guide settings table | `mise run docs:check` | PASS |
| Changed-owner checks | — | `mise run fmt`, `lints`, `check:deps`, `check:tenant-isolation`; `git diff --check 7f422c53a..HEAD` | PASS |

Span inventory (source review of normal-verbosity spans; the V5/V6
timelines exercise the catalog subset). Retained at `info`, each an
operation boundary: request (`request`, OTLP `otlp.http.*.export`, gateway
route handlers); queue/admission (`verification.attempt`, `claim`,
`verification.submissions`, `complete_many`, `gateway.admit`,
`bifrost.oracle.graph_lease`); engine stages (`verification.engine`,
`.prepare`, `.load`, `.score`, `.settle`, `.group.snapshot`,
`gateway.route`, `.authorize`, `.execute`, `.account`,
`bifrost.oracle.analytical.stage`/`.attempt`); dependencies
(`verification.evidence_read`, `gateway.call`, `agent.gateway.call`,
`dispatch`); streaming (`bifrost.gate.query.stream`, `dispatch_stream`);
capture (`gateway.capture`); background (`verification.schedule.pass`,
`operator.dispatch`, `bifrost.forge.*.pass`, `bifrost.forge.catalog.commit`,
`run_orphan_gc_for_table`); Agent loop (`invoke_agent`, `chat`,
`execute_tool`). Already `debug`: `verification.block`,
`bifrost.scribe.wal.append`/`.fsync`, `scribe_logical_batch_identity`, auth
internals. No span was moved: none at `info` is function-level detail.

Decisions: catalog spans at `info`+ always reach the OTel layer, so an
operator filter quieter than `info` still receives sampled catalog spans;
`WYRD_TRACE_FILTER` selects the additional diagnostic spans. Test capture
keeps an unset ratio at 1.0 and its trace filter at `trace`, so captures stay
complete and independent of `WYRD_LOG`/`WYRD_TRACE_FILTER`. Non-goals held:
no new provider, processor, queue, retry, tail sampler, log parser, or
dynamic capture control; no new package (`metrics` and `wyrd-telemetry` are
existing workspace crates); no unrelated file changed.
