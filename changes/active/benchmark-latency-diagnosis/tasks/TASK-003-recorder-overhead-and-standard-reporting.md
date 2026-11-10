---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-benchmark-latency-diagnosis
spec_revision: 3
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-010, REQ-011, REQ-012, INV-001, INV-002, INV-003, INV-004, AC-001, AC-002, AC-003, AC-004, AC-006, AC-007, AC-008]
depends_on: [TASK-001]
---

# Repair benchmark measurement and implement four Gateway hot-path optimizations

## Outcome and Value

Remove recorder contention introduced by the single-recorder implementation,
demonstrate that the corrected recorder restores the saved pre-task Gateway
performance baseline, make phase diagnostics part of the canonical terminal
report, then implement all four identified Gateway hot-path optimizations in this
same task. A benchmark that collects correct counts but materially slows the measured
server is not qualified for latency diagnosis.

Today every metric update competes for one process-wide lock, and implementors
extract phase results into different handwritten tables. After this task,
independent observations can record without that serialization, recording overhead
has measured evidence, and fresh runs and saved-record reports present the same
diagnostics without scratch scripts or surviving raw artifact directories. The
task then removes avoidable work from capture draining, metadata response usage
extraction, post-answer payload materialization and foreground audit construction,
with correctness and performance evidence for the optimized candidate.

The maintainer explicitly corrected this scope on 2026-11-09: all four
optimizations belong to TASK-003. Recorder recovery is an intermediate milestone,
not completion or a handoff to future tasks. Do not insert another general
diagnosis exercise between recovery and the four implementations.

## What Changes and What Must Stay True

The existing benchmark recorder remains the sole backend for standard metrics and
explicit timing observations. Remove its common update bottleneck and unnecessary
registration work. Preserve coherent snapshots: a snapshot must not expose only
some components of one grouped timing observation. Keep cumulative distributions,
bounded storage, units, labels, process identity, rejected-observation accounting,
and interval reconciliation. Ordinary production monitoring remains unchanged.

One recorder does not require one lock around every observation. Internal
synchronization is implementation-owned, but unrelated series must not wait for
each other's updates or for full histogram snapshot materialization. Brief
coordination needed to establish a coherent snapshot boundary is distinct from
holding all producers through copying and allocation.

Persist bounded, declared diagnostic readings in the existing experiment record
before publication. Keep the current common report columns and extend the existing
diagnostic detail with per-repetition phase results, units, sample populations,
precision, and explicit missing evidence. Report phase p50/p95/p99 and bounded
means where the stored distribution supports them; distinguish elapsed wall time
from CPU/work. Queue delay includes admission and must be labelled as overlapping.
Do not add their percentiles or subtract independently aggregated percentiles.

The required diagnostic set follows existing observations, not new instrumentation:

| Observation | Population and report expectation |
|---|---|
| `authenticate`, `parse`, `snapshot`, `authorize`, `route`, `admit`, `credential`, `translate`, `account`, `capture` from `wyrd_gateway_phase_duration_seconds` | Report each phase separately for every new buffered case/repetition, using its measured-to-drained interval and actual count. In the successful, no-retry fixture, reconcile each once-per-call phase with the admitted cohort, including late completions; do not assume this equality for arbitrary workloads. `capture` is post-answer work. |
| `relay` | Stream-only diagnostic when that step is selected; explicitly inapplicable for buffered cases. Do not add streaming qualification runs just to populate it. |
| `wyrd_gateway_queue_delay_seconds` | Report its actual count and distribution, labelling the interval from receipt in `govern` to task start; this includes admission and is not exclusive scheduler delay. |
| Server elapsed, provider exclusion and per-request residual | Retain the existing request distributions, identify their handoff boundary and count, and expose available diagnostic quantiles/means per repetition. |
| `outbox_write_seconds` for the capture outbox, capture accounting and drain | Retain/report available interval diagnostics separately with batch counts or declared counter populations. A batch write is not a request phase or direct response latency. |

For each selected step, declare which observations are required and which are
inapplicable. An absent required observation in a new fully instrumented run is
missing/invalid evidence, not silently inapplicable. An old record without persisted
diagnostics remains readable and reports them as unavailable.

The canonical renderer owns terminal formatting for both fresh and saved results.
Document that implementors reproduce its output and append interpretation rather
than replace it with custom result tables. Retain every repetition; a median-only
summary must not conceal the slower first payload repetition. Existing schema-3
records remain readable; unsupported older schema versions remain refused under
the existing reader contract. Missing historical phase readings remain unavailable, never
zero or fabricated from another run.

Qualification compares corrected candidates against saved unprofiled pre-task
records. Do not rebuild or rerun the historical baseline. A `VALID` verdict proves
measurement/integrity rules, not acceptable recording overhead. Keep performance
qualification separate from the existing benchmark judgments and exit codes.
New experiments still require the spec's bounded proposal and explicit execution
authorization; writing or approving this task alone runs no benchmark.

No buffer/concurrency increase, weakened instrumentation, dropped capture,
changed admission policy, security change,
new recorder, scheduler, dependency, Cargo feature, or record-schema replacement
belongs here. Do not disable required observations to claim a restored baseline.

After confirming recorder recovery, implement the following within this task:

1. Improve payload-capture draining, beginning with avoidable staged-row/part copies.
2. Avoid full owned response JSON solely to extract usage in metadata mode.
3. Reduce repeated post-answer payload materialization.
4. Trim foreground audit construction; preserve authentication/security semantics.

Every item is required implementation scope. Preserve source-backed findings and
ordering without another general research prerequisite. The scope is removal of
avoidable work at existing owners, not a capture execution or retry redesign.
Keep response bytes and typed usage, redaction and binary-object rules, row content
and ordering, reservation weights and release boundaries, decision cardinality,
verified attribution and durable retry identities unchanged. Do not introduce
authentication/principal caching or defer audit projection into a new queue.

Distinguish proven costs from overstated ones: `StagedRow` already shares its
cells through `Arc`; its clones add shallow bookkeeping, not payload-sized copies.
Capture also already consumes Values and directly encodes Variant without a JSON
text round trip. Optimize the remaining actual work rather than recreate an
optimization already present or promise savings from nonexistent deep copies.

## Approach

1. Revalidate the recorder's update, registration and snapshot paths and establish
   deterministic concurrency regressions before changing synchronization.
2. Remove common update serialization and avoidable registration allocation while
   preserving grouped timing and snapshot semantics; close all existing consumers.
3. Project Gateway phase distributions into persisted repetition readings and
   render them through the existing common report workflow.
4. Prove fresh/saved report parity, including historical missing diagnostics and
   regeneration after removal of raw artifacts; document the reporting contract.
5. Present the bounded qualification experiment below, execute only when explicitly
   authorized, and retain all records, identities, repetitions and validity evidence.
6. After recovery, implement all four optimizations in order with focused
   correctness/ownership proofs and affected real-server regressions.
7. Verify the final candidate against the corrected recorder-only milestone with
   unchanged instrumentation, workload and reporting. Record actual gains, costs
   and remaining limits; none of the four may be deferred at completion.

## Ordered Implementation Scenarios

Scenarios 1–4 establish the measurement workflow. Complete the intermediate
[recorder qualification](#qualification-against-saved-pre-task-evidence) before
changing production paths in scenarios 5–8, so recovery cannot be credited to
unrelated optimizations. Then finish all four and their final performance proof.

### Scenario 1 — Independent observations do not serialize process-wide

**Behavior.** With one registered series held during an update, another registered
series can finish its update independently. Repeated registration preserves series
identity and does not create duplicate observations. Snapshot copying does not hold
all independent producers for the full histogram materialization walk.

**RED.** Add the [planned independent-series regression](#recorder-concurrency).
Use deterministic coordination to show the second update cannot finish before the
first is released under the current implementation. Timeouts may guard deadlocks,
not act as microsecond performance assertions. Select a focused extension of this
proof for producer progress during snapshot copying. Always release/join workers
before reporting a failure. Preserve the [shared-observation regression](#recorder-regressions).

**GREEN.** Remove the common serialization and unnecessary repeated registration
work at the existing owner. Preserve one recording workflow and exact final totals.

**REFACTOR.** Reuse established metric storage/concurrency patterns and remove
obsolete common-lock plumbing. Do not add a parallel registry or synchronization
framework. Keep the scenario and existing recorder proofs green.

### Scenario 2 — Concurrent snapshots retain complete evidence

**Behavior.** Snapshots racing grouped request observations expose complete
elapsed/excluded/residual and parent/part groups at a coherent boundary. Exact
counts are conserved after producers finish. Invalid values, overflow, resets,
process replacement and bounded-series refusal remain explicit evidence failures.

**RED.** Add the [planned concurrent-group regression](#grouped-snapshots) with
coordinated snapshots during recording, asserting complete populations at every
observed cut and exact final counts. This may initially pass under the original
global lock: record that as regression proof, not a fabricated failure. Any new
failing case must identify lost/torn evidence, not scheduling noise. Retain
[interval and consumer regressions](#interval-and-consumer-regressions).

**GREEN.** Preserve coherent cuts under scenario 1's corrected implementation.
Rerun scenario 1 and the existing interval/consumer proofs.

**REFACTOR.** Simplify synchronization without weakening group atomicity,
classification, bounds or consumer invalidity checks.

### Scenario 3 — Gateway phase evidence survives record publication

**Behavior.** Each declared phase's interval readings retain its own population,
unit and distribution bounds in the saved repetition. Missing/inapplicable phase
data is explicit; it is not zero. Rejected observations or inconsistent required
populations are not silently accepted. Existing capture reconciliation, residual
calculation and criterion meanings remain unchanged.

**RED.** Add the [planned diagnostic-projection test](#phase-projection), using
distinct phases and warmup/measured cohorts. It must fail because current Gateway
repetitions omit these diagnostic readings. Cover absent and invalid evidence as
well as successful projection; do not assert that all background metrics share
the request cohort.

**GREEN.** Extend the current metric declarations and repetition projection using
the existing differenced distributions and reading types. Rerun scenarios 1–2 and
the current Gateway population proof.

**REFACTOR.** Keep distribution handling with its existing owner. Remove any
duplicate calculations without introducing a separate parser or analysis program.

### Scenario 4 — One terminal report retains every repetition

**Behavior.** A fresh run and `capacity report <record>` produce the same canonical
result report. Diagnostic details identify each repetition and phase, show the
declared percentiles/counts/bounds and available means, and label overlapping
intervals. Regeneration succeeds after raw artifacts are deleted. Old records
remain readable and identify unavailable phase evidence without inventing it.

**RED.** Add the [planned persisted-diagnostics report test](#report-parity) with
three deliberately different repetitions. It must fail for missing phase output
or collapsed repetitions. Extend the existing readback/CLI parity proofs for
historical missing readings and unchanged common report fields.

**GREEN.** Extend the existing renderer and record projection so all required
output comes from the published record. Rerun scenarios 1–3 and report readback.

**REFACTOR.** Keep one renderer for fresh and saved reports; remove ad hoc
reporting paths introduced for this work. Document the exact reproduction command
and the implementor reporting rule in the benchmark authority.

### Scenario 5 — Capture draining avoids redundant projection bookkeeping

**Behavior.** Mixed audit/capture/result slices build the same ordered dense rows
and bounded frames without avoidable temporary staged-row/Part cloning. Local and
peer delivery preserve identical content and stable retry identities. Memory
accounting, reservation ownership, release timing, cancellation and acknowledgement
rules remain unchanged. Savings here are shallow bookkeeping unless separate
evidence establishes a larger copy.

**RED.** Use the [capture-drain proofs](#capture-drain-proofs) to pin mixed rows,
attribution, byte accounting and replay before editing. Add the planned focused
case only where existing tests leave a gap. An existing passing equivalence proof
is regression evidence; do not manufacture a semantic failure. For newly changed
logic, establish the smallest failing ownership/allocation or edge assertion before
GREEN. Record the existing extra Vec/refcount/Part work as the structural pre-change
cost and the corrected recorder-only run as performance comparison evidence.

**GREEN.** Remove the redundant work in existing projection/chunk owners while
preserving the original retryable slice and all accounted owners. Update affected
projection consumers and rerun scenarios 1–4 and the focused drain proofs.

**REFACTOR.** Reuse the existing shared staged-row and canonical/peer frame paths.
Do not add a new row store, early capacity release, chunk protocol or IPC path.

### Scenario 6 — Metadata usage extraction avoids owning the response tree

**Behavior.** Metadata-only RawV1 replies preserve exact native response bytes and
typed usage without materializing the whole response as owned JSON. Selected
payload capture still decodes once and reuses that Value. Missing, null, malformed,
oversized and overflowing usage retains current unknown/bounded behavior. Preserve
provider field precedence, duplicate-field last-value behavior, valid top-level
arrays/scalars and successful answers whose optional usage is unrepresentable.

**RED.** Add the [planned metadata usage proof](#metadata-response-proofs) and
retain current provider/capture regressions. Exercise selected/unselected capture
and the edge cases above. Pin existing output behavior first; any newly changed
branch requires its focused failing case before GREEN. For the allocation-only
part, record the current whole-tree materialization and corrected-milestone costs;
do not change successful-answer semantics merely to manufacture RED.

**GREEN.** Extend existing response/usage decoding to avoid content-sized ownership
when capture is unselected, using the installed JSON/typed usage machinery. Keep
full-body validation and existing typed accounting bounds. Rerun earlier scenarios
and the native-response/capture proofs.

**REFACTOR.** Reuse the same provider usage rules for both paths. Do not add a
competing parser or change provider wire behavior, capture selection or errors.

### Scenario 7 — Post-answer payload projection avoids transient materialization

**Behavior.** Capture redaction and Variant projection produce the same selected
content and object references with fewer transient allocations. Preserve normalized
secret-key matching, nested objects/arrays, Unicode keys, explicit nulls, exact
numeric values, binary forms, malformed-base64 handling, payload bounds and tenant
attribution. Capture failures still leave an already answered call unchanged.

**RED.** Add the [planned payload-materialization proof](#payload-materialization-proofs)
for uncovered normalization/encoding cases; retain existing policy, binary and
shared-buffer tests. Establish current output behavior before optimization. New
redaction/encoding branches require a focused failing case before GREEN; pure
allocation removal uses existing behavioral regression plus source/ownership and
the final measured performance proof, not an artificial semantic failure.

**GREEN.** Remove actual transient work at the existing capture/redaction owner,
including unnecessary key-normalization/media-type ownership where applicable.
Keep in-place redaction and direct Variant encoding already present. Rerun earlier
scenarios, capture regressions and the affected deadline/policy server seams.

**REFACTOR.** Keep the smallest owner-local reduction. No new payload cache,
allocator, object lifecycle or Variant framework; extend shared encoding only if
the actual change requires its existing consumer closure and regression proof.

### Scenario 8 — Foreground audit projection transfers existing owned content

**Behavior.** An already-owned audit decision reaches the same canonical staged
row without copying its text fields into a second owned representation. Preserve
the original decision time, verified tenant/principal pair, credential attribution,
delegation/detail, request lane and permission/outcome. Allowed and denied decisions
are still staged once at their current boundary; failures remain nonblocking and
observable. Keep projection before enqueue so weight/lane semantics do not change.

**RED.** Extend the [audit projection proofs](#audit-construction-proofs) or add the
planned ownership case for canonical content, optional/delegated detail and decided
time. Pin current semantics before editing. Any new projection/error branch gets
its focused failing assertion before GREEN; a pure move replacing clones uses
regression and explicit ownership proof, with no manufactured semantic failure.

**GREEN.** Remove redundant copies through the existing audit event/projection
workflow and close every affected caller. Rerun earlier scenarios, audit framing,
and the real-server attribution, permissions and capture regressions.

**REFACTOR.** Keep the one audit writer and decision boundary. No deferred raw-event
queue, audit sampling, removed permission check, principal cache, timestamp shift,
or weaker memory accounting is permitted.

## Acceptance Criteria

| Obligations | Owner, proof and failure observation |
|---|---|
| REQ-001–004; INV-001–003; AC-001 | Recorder scenarios 1–2 and consumer regressions prove one backend, independent progress and conserved/coherent observations. A common update bottleneck, torn group, duplicate count or production-monitoring change fails. |
| REQ-003–004,007; INV-004; AC-002 | Interval and report tests preserve precision, cohorts, missing/invalid evidence, old-record readback and record-only regeneration. Zero-filled missing phases or a raw-artifact dependency fails. |
| REQ-005–006,008; AC-003–004 | Scenarios 3–4 persist and report phase evidence for every repetition with declared boundaries and overlap. A CPU attribution from wall time or an additive overlapping-percentile budget fails. |
| REQ-003,007–008; INV-001–004; AC-002–004 | Qualification below supplies valid corrected-run evidence, measured instrumentation cost and comparison against the saved pre-task baseline. `VALID` alone, concealed repetition spread, weakened observations or a baseline rerun fails. Until recovery is demonstrated, qualification remains unverified. |
| REQ-010; AC-006 | Recorder/reporting work reaches the measured recovery milestone before any production optimization. Final optimization gains cannot conceal an unqualified recorder. |
| REQ-011; AC-007 | Scenarios 3–4 produce the same full repetition diagnostics through fresh and saved canonical reports, without raw-artifact dependencies or replacement prose tables. |
| REQ-012; INV-001–003; AC-008 | Scenarios 5–8 implement all four reductions at existing owners. Focused projection/usage/redaction/audit tests and real-server journeys preserve content, attribution, admission, retry, byte accounting and acknowledgements. A deferred item or a claimed reduction already present in current code fails. |
| REQ-007,010,012; INV-004; AC-006,008 | Final qualification compares the optimized candidate to the corrected recorder-only milestone under identical instrumentation/workload, retaining all repetitions and reporting both latency objectives independently. Preserve baseline recovery and show the measured effect of the implemented work; a production latency claim inferred solely from allocations or a changed recorder fails. |

## Implementation Reference

Inspected owner paths below are guidance, not a file/type allowlist.

- **Recorder:** `crates/shared/wyrd-benchmark/src/recorder.rs`,
  `BenchmarkRecorder::handle` and `Handle::update`, register and update every
  standard metric through the same registry lock and allocate a fresh handle on
  registration. `src/lib.rs::REGISTRY`, `Registry::snapshot`, `snapshot`, and
  `record_timing_parts` own storage, snapshot materialization and grouped timing.
  Extend this owner; do not restore the old dual-recorder path.
- **Reuse precedent:** `crates/shared/wyrd-bench/src/recorder.rs::BenchmarkRecorder`
  uses atomic values and narrower histogram synchronization. Its summary/reset
  contract cannot replace cumulative distributions and coherent intervals. Reuse
  applicable mechanics without importing test tooling into the shipped server.
- **Producers and consumers:** `wyrd-server/src/app/metrics.rs::MetricsBackend`
  selects startup and serves snapshots; Gateway telemetry and all-engine consumer
  tests exercise the actual observation interfaces. Preserve those boundaries.
  Existing recorder tests cover shared metric/timing snapshots and bounded
  intervals, but do not establish low overhead or snapshots racing timing groups.
- **Diagnostic projection:** `wyrd-testing/src/capacity.rs::Population::histogram`
  supplies differenced distributions; `src/bin/capacity/gateway/run.rs::Observed::repetition`
  builds readings through `Repetition::set`; `gateway/mod.rs` declares metrics.
  Extend these rather than parse JSON snapshots in a report script.
- **Persistence/report:** `wyrd-testing/src/experiment.rs::Repetition.metrics`
  persists readings, while `Repetition.timings` is deliberately skipped by serde.
  `keep_timings` writes raw artifacts separately. `Record::render` and `step_cells`
  own formatting. `src/bin/capacity/levels.rs` invokes the same renderer for fresh
  runs and `report`; `main.rs` prints it. Persist the diagnostic projection before
  publication, retaining schema-3 compatibility through existing readings.
- **Tests/docs:** existing inline recorder, `capacity`, `experiment`, Gateway run
  and CLI tests own the focused proofs below. `benchmarks/README.md` owns terminal
  output and record-only regeneration; extend it with the diagnostic layout and
  implementor reporting rule. No SDK, generated schema or product route changes
  are required for this internal measurement/reporting work.
- **Capture draining:** `wyrd-server/src/scribe_outbox.rs::Part::dense` collects
  cloned rows before projection; `ScribeSink::chunks` clones Parts. The original
  slice remains retryable. `vala-bifrost-redux/src/tables/staged.rs::StagedRow`
  already uses `Arc<[Cell]>`; `StagedRow::project` owns dense construction and
  schema checks. Extend those owners and inspected audit, verification result,
  Scribe and peer-fixture callers if their internal input shape changes. Existing
  dense/memory/replay/bounded-frame tests below pin the important contract.
- **Metadata response:** `wyrd-gateway/src/adapter/mod.rs::complete` owns RawV1
  body parsing; `usage` owns provider field precedence and typed bounds. Its
  production caller is `adapter/http.rs`. Unselected capture currently parses
  a full Value then discards it. Extend this branch using installed raw/borrowed
  JSON support; selected capture already reuses its decoded Value. Inspect the
  existing native-byte and bounded usage/capture tests, including duplicate keys.
- **Payload projection:** `wyrd-server/src/components/gateway/capture.rs::CallCapture::from_facts`
  consumes Values, `redacted_payload` mutates then directly encodes Variant, and
  `calls_batch` already shares a lone capture's buffers. `redact_object` still
  materializes normalized keys and media-type text. Target those actual transient
  costs and preserve `PayloadObjects` reference/persistence checks. The existing
  buffer-sharing proof must remain green. `wyrd-queue::variant` is an existing
  shared owner only if an encoder change is necessary, not a mandated rewrite.
- **Audit:** `wyrd-server/src/audit/mod.rs::audit_event` constructs owned fields;
  `ScribeWrite::from` borrows the event for
  `vala-bifrost-redux/src/tables/audit/projection.rs::stage_audit_event`, which
  copies fields into cells. `Cell::text` accepts owned content already. Reuse
  that projection owner and close its Scribe persistence/execution and peer-test
  callers if its private consuming workflow changes. `ScribeSink::{lane,weight}`
  depends on the staged row; keep that stage and accounting intact.

## Verification and Evidence

New names below are **planned**, not existing tests. Package/target/runner and
inline test owners have been checked. Confirm actual selectors after adding tests,
before RED. Recorder/report tests use default features without Postgres. Server
projection tests use the existing `test-support` feature; the explicitly marked
server seams and journeys below use repository-managed Postgres and mock providers.
Run Cargo commands sequentially.

### Recorder concurrency

`mise exec -- cargo nextest run --locked -p wyrd-benchmark --lib -E 'test(=recorder::tests::unrelated_series_record_while_one_series_is_held)'`

Planned independent-progress proof; extend its contested cases to cover snapshot
materialization without depending on a fixed internal locking design.

### Grouped snapshots

`mise exec -- cargo nextest run --locked -p wyrd-benchmark --lib -E 'test(=recording_tests::snapshots_preserve_complete_timing_groups_during_recording)'`

Planned coherent-cut and exact-count proof during concurrent recording.

### Recorder regressions

`mise exec -- cargo nextest run --locked -p wyrd-benchmark --lib -E 'test(=tests::standard_metrics_and_request_timings_share_one_snapshot) | test(=recording_tests::benchmark_histograms_preserve_bounded_intervals)'`

Existing instrumentation, units, populations and bounded intervals remain correct.

### Phase projection

`mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=gateway::run::tests::phase_diagnostics_preserve_window_populations_and_missing_evidence)'`

Planned persisted phase readings and absent/invalid evidence proof.

### Report parity

`mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::reports_keep_repetition_diagnostics_after_artifacts_are_removed) | test(=experiment::tests::reports_regenerate_from_persisted_records) | test(=experiment::tests::histogram_records_preserve_method_and_refuse_older_versions)'`

First test planned; remaining tests exist. Prove all repetitions, record-only
readback, stable report ownership and historical compatibility.

### Interval and consumer regressions

`mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::histogram_cohorts_preserve_measurement_and_drain_windows) | test(=capacity::tests::snapshot_windows_keep_classes_and_scans_apart)'`

`mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::level_commands_preserve_records_and_exit_precedence) | test(=tests::all_engine_consumers_read_benchmark_snapshots_without_prometheus) | test(=gateway::run::tests::overhead_repetition_subtracts_waits_per_call_and_counts_capture) | test(=gateway::tests::overhead_declares_every_case_at_every_client_count)'`

Existing interval, CLI fresh/saved parity, all-engine consumers and Gateway cohort
proofs. Extend assertions where needed; preserve exit/criterion semantics.

### Capture drain proofs

`mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=scribe_outbox::tests::borrowed_projection_preserves_mixed_rows_and_retry_frames) | test(=scribe_outbox::tests::dense_projection_preserves_records_and_attribution) | test(=scribe_outbox::tests::staged_writes_charge_their_owned_memory) | test(=scribe_outbox::tests::a_retry_resubmits_identical_frames) | test(=scribe_outbox::tests::local_canonical_and_peer_ipc_preserve_frames_and_replay) | test(=scribe_outbox::tests::large_captures_reach_bounded_frames_without_a_split_copy) | test(=scribe_outbox::tests::delivery_limits_preserve_per_writer_ack_overlap) | test(=scribe_outbox::tests::cancelled_encoding_retains_delivery_capacity)'`

First test planned if needed; all others exist. Prove equivalent dense projection,
owned-memory charges, bounded frames, local/peer replay, cancellation and delivery
limits. Confirm a planned case selects an actual test before using the combined
command; omit only an unnecessary planned test, never an existing required proof.

`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::staged::tests::staged_rows_project_into_one_dense_batch)'`

Existing shared projection proof; close all consumers if its input shape changes.

`mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(/^outbox::tests::/)'`

Reservation, retry, release and shutdown regressions for the shared outbox contract.

### Metadata response proofs

`mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::tests::raw_metadata_usage_matches_selected_capture_without_materializing_content) | test(=adapter::tests::provider_usage_is_typed_and_bounded) | test(=adapter::tests::native_answer_bytes_are_preserved) | test(=adapter::tests::native_dialects_pass_through_with_adapter_routes_and_auth) | test(=adapter::tests::selected_capture_is_scrubbed_bounded_and_leaves_answers_unchanged) | test(=adapter::tests::capture_scrubbing_rebuilds_only_secret_holding_content) | test(=adapter::tests::selected_capture_drops_valid_base64_credentials)'`

First test planned; others exist. Pin native bytes, both capture choices, typed
provider usage, credential scrubbing and optional-usage edge behavior. Verify the
owned-tree removal structurally or with a small existing-owner allocation/ownership
proof; do not add a general allocation-testing framework.

### Payload materialization proofs

`mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=components::gateway::capture::tests::payload_materialization_preserves_redaction_and_variant_values) | test(=components::gateway::capture::tests::capture_columns_avoid_intermediate_payload_copies)'`

First test planned; the existing test pins the buffer reuse already present.

`mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(/^components::gateway::capture::tests::/)'`

Complete narrow capture policy/redaction/binary/size/ownership regression module.

`mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(/^variant::tests::/)'`

Conditional owner proof only if the shared Variant encoder changes. Otherwise
record it as not applicable; no shared encoder rewrite is required.

### Audit construction proofs

`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::audit::projection::tests::owned_decision_preserves_canonical_row_and_attribution) | test(=tables::audit::projection::tests::projects_canonical_content_with_the_decision_time) | test(=tables::audit::projection::tests::detail_projects_as_variant_of_the_canonical_json)'`

First test planned if existing tests need extension; other tests exist. Verify
canonical fields/detail, original decision time and the owned-content transfer.

`mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=scribe_outbox::tests::audit_decisions_frame_under_the_platform_audit_principal) | test(=scribe_outbox::tests::a_slice_of_small_writes_travels_as_one_frame)'`

Existing framing, attribution and batching proofs after audit projection changes.

### Real server seams and journeys

These are existing regression proofs for preserved behavior, not new product
capabilities or new SDK workflows. Run after the four optimizations, with the
repository's isolated Postgres lifecycle and migration setup.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E "test(=components::gateway::pg_invocation_tests::gateway_capture_follows_policy_and_never_affects_the_call) | test(=components::gateway::pg_invocation_tests::capturing_call_waits_for_a_capture_slot_and_refuses_at_its_deadline) | test(=components::gateway::pg_invocation_tests::gateway_capture_work_ends_at_the_call_deadline) | test(=components::gateway::pg_invocation_tests::gateway_payload_objects_are_authorized_convergent_and_stable_when_expired) | test(=components::gateway::pg_administration_tests::gateway_administration_is_authorized_audited_and_tenant_isolated)"'
```

Prove capture policy, pre-dispatch slot/deadline behavior, binary-object tenancy
and expiry, and authorized/audited administration against the real database.

```bash
mise exec -- env WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=compatible::openai_compatible_streams_terminate_and_survive_a_capture_outage) | test(=compatible::configured_gateway_headers_are_authorized_and_captured) | test(=native::anthropic_messages_ingress_is_native_governed_and_fail_closed) | test(=native::gemini_generate_content_ingress_is_native_governed_and_fail_closed) | test(=native::gateway_api_key_exchange_is_scoped_cached_and_revocable) | test(=resilience::capacity_capture_accounting_waits_for_post_response_work) | test(=resilience::benchmark_mode_keeps_capture_and_snapshot_counts_without_prometheus)"'
```

Prove existing compatible/native public call paths against real servers and local
upstreams: native bytes/capture behavior, scoped/revocable credentials, exact
post-answer accounting and sole-recorder correctness. Preserve all existing
assertions; no live provider credentials or synthetic host load are needed.

### Owner and static closure

- `mise exec -- cargo nextest run --locked -p wyrd-benchmark --lib` — full narrow recorder owner.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(/^experiment::tests::/) | test(/^capacity::tests::/)'` — affected library owners only.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(/^gateway::/)'` — affected Gateway driver owner.
- `mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(/^adapter::tests::/)'` — affected adapter owner.
- `mise run fmt:check` — Rust format.
- `mise exec -- cargo clippy --locked -p wyrd-benchmark -p wyrd-testing -p wyrd-server -p wyrd-gateway -p vala-bifrost-redux --all-features --all-targets -- -D warnings` — affected-owner lint policy; include `wyrd-queue` only if its encoder changes.
- `mise run check:deps` — no test-tooling dependency leak or tier violation.
- `mise exec -- git diff --check` — patch hygiene, followed by link/diff inspection.

If the actual implementation extends startup or additional producer behavior,
include its existing exact focused regressions and repository-managed setup in the
execution record. Do not run unrelated full journey suites or `gate` for this task.

### Qualification against saved pre-task evidence

This is performance verification, not a manufactured RED unit test. Present the
bounded protocol for explicit experiment authorization before execution. The
maintainer's current request writes the task; it does not start measurements.

Use the unprofiled pre-task candidate
`benchmarks/history/gateway/overhead/20261109T153318Z-b1cf95f2e.json`
as the recovery reference. Retain the older
`benchmarks/history/gateway/overhead/20261108T234942Z-5508cf29f.json`
as historical context and the regressed
`target/benchmarks/gateway/overhead/20261109T222502Z-906882704.json`
as the TASK-001/002 comparison. Inventory each host/build/settings identity first;
do not substitute a profiled run or rebuild either historical baseline.

The minimum corrected-run proposal is the existing closed-loop c64 cases,
unprofiled, with the default 5-second warmup and three 5-second measured
repetitions per case:

```bash
mise run bench:l1:gateway -- --steps buffered-1kib-capture-metadata.c64,buffered-64kib-capture-metadata.c64,buffered-64kib-capture-payload.c64
```

The existing task supplies the release build and isolated Postgres lifecycle.
Do not build during measurement. Declare source/diff, binary identity, settings,
instrumentation and omitted steps; retain matching binaries, raw window snapshots,
server log, immutable record and canonical terminal report.

Before execution, extract the pre-task repetition ranges into the protocol. For
each case, recovery requires the corrected median of repetitions to be no worse
than the saved pre-task worst repetition for client p50, residual p50/p99 and
CPU/request, and no lower than its worst throughput. Show every repetition and
any outlier; do not describe range overlap as proof that instrumentation is free.
This is a task-local recovery check, not a new benchmark criterion or product SLO.
If host/settings are incompatible or the result misses that envelope, do not
claim recovery or silently widen the rule: resolve the measurement discrepancy.

Also propose the smallest paired candidate comparison that isolates added phase
instrumentation, keeping the corrected sole recorder, core observations and
workload unchanged. Declare the exact instrumentation difference, execution order,
cases, repeats and stopping rule before authorization. A temporary bench-only
source variant may serve this diagnostic; no new public configuration or second
backend is implied. Retain both source/binary identities and report the observed
latency/CPU/throughput deltas and repetition spread. The fully instrumented
candidate must satisfy recovery; a reduced-observation diagnostic cannot pass in
its place. Do not interpret this as a controlled reconstruction of every historical
source change or as a zero-overhead claim.

All qualification records must satisfy current validity and integrity rules:
matching cohorts, zero lost/shed/unstaged captures, complete drain, bounded registry
coverage and declared resource conditions. Stop after the bounded evidence answers
recovery and observer-cost questions; no full suite, profiler or baseline rerun is
required. This qualification is an intermediate milestone: do not begin production
optimizations before it passes, and do not mark TASK-003 complete when it passes.
An unresolved qualification remains unverified and prevents progression to those
optimizations, rather than allowing their gains to mask recorder interference.

### Final optimization qualification

After scenarios 5–8 and their correctness regressions pass, run the same declared
c64 command above for the optimized candidate, subject to the experiment boundary.
The corrected recorder-only milestone is the comparison reference; retain its
binary/source identity before applying optimizations. Keep recorder, phase
instrumentation, workload, host/settings, window lengths, repeats and profiling
mode identical. This separates production optimization effects from recorder
repair. No new general diagnosis pass is required.

The canonical report must retain each repetition's client/server/residual
quantiles, phase populations/distributions, CPU/request, throughput, outbox write
and drain evidence. Show effects in the relevant metadata and payload cases,
including tails, and assess both p50 objectives independently. Identify the actual
work removed by each of the four implementations and distinguish measured gains
from source-only attribution. Improvements in background work must not be reported
as equal response-latency savings.

The final candidate must retain the recovery envelope and all validity/integrity
properties. Investigate and correct any supported performance regression within
this task rather than weakening the rule. If an individual reduction is below
run-to-run resolution, report that limit with its concrete ownership/allocation
proof; do not invent an individual speedup. All four reductions, their caller/test
closure, real-server regressions and final comparison are mandatory for completion.
Do not stop after the recorder, publish only proposals, move an item to another
task, or claim that meeting either investigation target is proven without evidence.

Document/report edits use link inspection and renderer readback rather than a
manufactured TDD failure. Append evidence here and update the existing research
report's measurement qualification, implemented reductions, final comparisons and
standard-report references. Correct any source claim disproved during execution;
preserve established findings without launching another diagnosis exercise.

## Material Stop Conditions

- A correction would weaken snapshot coherence, measurement precision, count
  reconciliation, required diagnostic coverage or bounded-series failure detection.
- The only proposed solution changes a wire/persisted schema, backend ownership,
  security, capture/admission, accepted-work, retry or durability semantics.
- Recovery requires changing the workload, hiding instrumentation, changing
  benchmark judgments, rerunning historical baselines or crediting a production
  optimization before recorder qualification. None is an acceptable recorder fix.
- A benchmark experiment has not received the separately required authorization:
  finish independent implementation/tests and present the concrete protocol;
  leave qualification unverified rather than claiming completion.

Private synchronization/storage choices, focused test scaffolding, owner-local
copy/parse reductions and extension of the existing renderer are implementation
decisions, not reasons to stop or defer one of the four optimizations.

## Authority Links

- [Spec revision 3](../spec.md), recording the maintainer's explicit 2026-11-09
  scope instruction: recorder qualification, canonical reporting and all four
  hot-path optimizations belong to this task.
- [Recorder task](TASK-001-sole-benchmark-recorder.md) and
  [existing diagnosis](TASK-002-gateway-latency-diagnosis.md).
- [Workflow](../../../spec-driven-development.md),
  [implementation execution](../../../implementation-execution.md),
  [testing](../../../../TESTING.md), [repository rules](../../../../AGENTS.md).
- [Benchmark measurement and reports](../../../../benchmarks/README.md),
  [Rust](../../../../agent_docs/rust.md),
  [maintainer style](../../../../agent_docs/maintainer-style.md),
  [agent surfaces](../../../../agent_docs/agent-surfaces.md).
- [Capture reservation contract](../../../../architecture/wyrd-design.md#principal-model),
  [Scribe batching and identical-slice retries](../../../../architecture/bifrost-design.md),
  [audit integrity and privacy](../../../../architecture/wyrd-security-posture.md#audit-integrity-and-privacy).
