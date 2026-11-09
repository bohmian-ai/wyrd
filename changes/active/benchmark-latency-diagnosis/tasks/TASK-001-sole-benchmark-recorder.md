---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-benchmark-latency-diagnosis
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-007, REQ-009, INV-001, INV-002, INV-003, INV-004, AC-001, AC-002, AC-005]
depends_on: []
---

# Make the benchmark recorder the sole benchmark metrics backend

## Outcome and Value

A benchmarked server installs one benchmark recorder for timings, diagnostic
histograms, counters and gauges. All three engines' L1/L2 consumers use its
snapshots. Ordinary deployments keep the production Prometheus backend.

For example, Gateway capture reconciliation and latency percentiles must work
without any Prometheus recorder, exporter or exposition parser installed. A
Prometheus backend that discards duration observations does not satisfy this
outcome. The unfinished candidate currently does exactly that and needs rework.

## What Changes and What Must Stay True

Select the backend before registering observations. Consolidate the existing
standard-metrics and precise HDR recording capabilities in one narrow shared
benchmark owner outside `wyrd_telemetry`; do not import a test harness into
production. Client clocks and external OS CPU/RSS sampling remain distinct
measurement sources. Reuse existing instrumentation, not a second collection
pipeline under a new name.

Preserve metric units, labels, diagnostic coverage, capture/acknowledgement
populations and failure detection. Recording one duration through existing metric
and explicit timing calls must not create duplicate semantic observations.
Diagnostic means must retain their precision bounds. Missing evidence is not zero.
Keep phase population and per-request overhead semantics; overlapping phases do
not form an additive latency budget.

Preserve production metric families and exporter behavior outside benchmark mode.
Keep loopback-only benchmark access, bounded labels and bounded recording storage.
Security, tenancy, capture reservation, accepted-work ownership, response behavior,
settlement and durable acknowledgement do not change. Existing history remains
readable and immutable; report changed instrumentation settings explicitly.

Recording, locking and cardinality have costs. This task establishes collection
correctness; it does not claim to reduce latency or qualify a performance target.
No new scheduler, cache, transport, admission policy or optimization is in scope.

Before implementation, inventory the current tracked and untracked candidate and
record each meaningful change as retained, reworked or removed. In particular,
review the filtered Prometheus backend, consumer deletions, new counters and
Gateway phase additions against the approved requirements. Do not preserve an
unsupported change merely because it is already present. Preserve pre-existing
work and do not reset the worktree wholesale.

## Approach

1. Inventory the unfinished candidate, instrumentation routes and all L1/L2
   consumers, including health, backlog, drain and resource bookkeeping.
2. Establish one benchmark recorder using the existing standard `metrics`
   approach and precise HDR distributions; retain full interval evidence.
3. Select the production or benchmark backend once at startup and serve the
   benchmark owner's snapshots through the existing local measurement surface.
4. Move every engine consumer to those snapshots, with coherent classification,
   late-cohort reconciliation and no duplicate duration observations.
5. Close the focused functional and regression proofs below and update benchmark
   documentation and candidate disposition evidence.

## Ordered Implementation Scenarios

### Scenario 1 — Startup selects exactly one backend

**Behavior.** An ordinary startup records production metrics and accumulates no
benchmark state. A benchmark startup installs only the benchmark backend before
registration. Its counters, gauges and histograms reach its snapshots without
constructing or installing a Prometheus backend. Invalid non-loopback benchmark
configuration is refused. There is no live switch or mixed fallback.

**RED.** Add the [planned exclusive-startup test](#exclusive-startup), and retain
[local-access regression](#local-access). It must reject the current filtered
Prometheus candidate as well as a benchmark backend installed after observations
were already registered. Use process-isolated tests for process-global state.

**GREEN.** Implement exclusive selection and benchmark snapshot access while
preserving production startup and invalid-configuration behavior.

**REFACTOR.** Remove the filtered-backend path and obsolete startup plumbing;
reuse existing process lifecycle ownership. Keep all preceding proofs green.

### Scenario 2 — One observation has one coherent snapshot

**Behavior.** Known standard-metric counter/gauge updates, duration and non-time
histograms, and per-request elapsed/excluded/residual observations retain their
units and populations in the same benchmark owner. Existing dual instrumentation
must not double-count a semantic duration. Gauge decrements and peaks remain
meaningful; non-time histograms are not scaled as seconds.

**RED.** Add the [planned shared-observation test](#shared-observations). It must
fail for parallel registries, duplicate observations, duration-unit mistakes or
lost counters/gauges. Exercise the existing instrumentation interfaces used by
real producers rather than only private registry mutation.

**GREEN.** Make those interfaces produce the required single-owner observations
and preserve per-request residual computation. Rerun scenario 1.

**REFACTOR.** Reuse the existing recorder and HDR primitives; remove duplicate
aggregation and conversion paths without erasing useful diagnostic classes.

### Scenario 3 — Windows preserve evidence and reject invalid inputs

**Behavior.** Warmup and measured cohorts, cases and outcomes stay separate.
Late completions remain with their admitted cohort; integer histogram bounds
remain visible. Replaced processes, resets, overflow, invalid labels, missing
parts and negative residuals are explicit invalid evidence. The complete declared
engine/level workload fits the bounded registry without dropping observations.
Historical records still read and regenerate their reports.

**RED.** Extend [interval conservation](#interval-conservation) and
[population validation](#population-validation), adding focused cases that fail
for any missing counter/gauge interval behavior or insufficient coverage bounds.
Keep [historical record regression](#historical-records). Existing passing cases
are regression proof and need no manufactured RED; only changed behavior needs
an initial failing case.

**GREEN.** Preserve distributions, bookkeeping and invalidity across snapshots;
finish every necessary consumer update. Rerun scenarios 1–2.

**REFACTOR.** Delete obsolete benchmark Prometheus parsing only after its consumers
have moved. Production contract-test utilities may remain with their production
owner. Never suppress overflow or relax population validation to pass.

### Scenario 4 — All consumers work without Prometheus

**Behavior.** Bifrost, Verification and Gateway reporting at both levels reads
latency, diagnostics, health, backlog, resources and integrity from benchmark-owned
snapshots. Correct capture and acknowledgement counts remain truthful through
post-response work and drain. The production Gateway metrics journey remains valid.

**RED.** Add the [planned consumer matrix](#consumer-matrix) for all six engine/level
combinations, and the [planned real-server journey](#benchmark-journey) proving
returned results, captured evidence and snapshot counts without Prometheus. The
current Prometheus-dependent `LocalServer::metrics` path and any missing
bookkeeping must fail. These are functional proofs, not load experiments.

**GREEN.** Migrate the complete consumer workflow, including setup/polling/drain,
not just report percentile calculations. Preserve the existing diagnostic and
population [consumer regressions](#consumer-regressions). Rerun scenarios 1–3.

**REFACTOR.** Remove superseded benchmark-only readers and wrappers; retain the
existing server, driver, snapshot and report owners. Do not replace the harness.

## Acceptance Criteria

| Spec obligations | Proof and expected result |
|---|---|
| REQ-001–002; AC-001 | Scenarios 1–2 prove exclusive startup and one owner for all metric kinds; any installed Prometheus benchmark backend or parallel independent timing registry fails. |
| REQ-003,007; INV-004; AC-002 | Scenario 3 proves interval conservation, declared bounds, complete workload capacity and historical readability; drops, invalid populations or altered history fail. |
| REQ-004; INV-001–003; AC-001–002 | Scenario 4 covers every engine/level consumer and functional capture/acknowledgement closure; missing/zero-substituted diagnostics, changed responses or lost evidence fail. |
| REQ-009; AC-005 | The candidate inventory explains retained/reworked/removed changes and final checks; interrupted runs are explicitly incomplete. |

## Implementation Reference

Inspected owners and gaps; paths guide reuse rather than constrain private layout:

- `wyrd-bench::recorder::BenchmarkRecorder::{install,register,snapshot}` implements
  standard `metrics::Recorder`, counters, gauges, peaks and HDR histograms. Its
  percentile-only snapshots and interval reset do not satisfy precise cumulative
  evidence. Reuse appropriate mechanics, not the entire test-tooling dependency.
- `wyrd-benchmark::{record_timing,record_timing_parts,TimingSnapshot::since,Distribution}`
  is the unfinished move of the former `wyrd_telemetry::timing` owner. It preserves
  precise request populations but has no standard counter/gauge recorder. Unify
  the capabilities; do not keep two independent collectors. Its location is an
  implementation choice. No new external dependency or feature is selected.
- `wyrd-server::app::metrics::{WyrdTelemetryRuntime,install_recorder,metrics_router}`,
  `app::run`, and `WyrdServer`'s metrics-handle startup own backend installation
  and local access. They currently construct a Prometheus handle even in the
  unfinished benchmark mode; startup, shared handles and test setup must close.
- `wyrd-testing::release_server::LocalServer::{metrics,timings}` and
  `capacity::{Window,Population}` feed the driver. `metrics()` still parses
  Prometheus text. Inspect both methods' callers, not only latency reports.
- `src/bin/capacity/query/{isolated,competing}.rs`, `verification/run.rs`, and
  `gateway/{run,capacity}.rs` own L1/L2 consumers. Inspected write-phase,
  Verification diagnostic, Gateway capture and capacity population tests are
  listed below. Verification diagnostics, outbox counts and Gateway admission
  counts have already been edited; revalidate their populations and coverage.
- `experiment::Record` owns reading, history and reports. `profile::{Profiler,
  HeapProfiler}` and `release_server::os` remain separate diagnostic/resource
  owners. The measurement calibration tests that deliberately spin the host are
  not part of this task's proof.
- `WyrdTestServerBuilder`, the Gateway journey harness and `resilience` module
  provide real-server/Postgres functional proof. A narrow test configuration
  adaptation may be needed for explicit backend selection; reuse this harness.
- Manifests, benchmark docs, tracing classification callers in HTTP/gRPC, Scribe,
  Oracle and verification belong to the necessary dependency/import closure.
  No SDK, schema, migration or public product contract change is intended.

## Verification and Evidence

Do not execute these during planning. Once implementation is requested, use
nextest's process-per-test isolation and run Cargo commands sequentially. No
benchmark run, release benchmark build or synthetic host load is authorized by
these checks. Planned names are proposals: confirm the actual selector after
adding each test, before RED. If placement changes, record its equivalent focused
selector; do not weaken the proof.

### Exclusive startup

Planned, existing `wyrd-server` library test owner `app::metrics::tests`:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=app::metrics::tests::benchmark_mode_installs_only_benchmark_recorder)'
```

### Local access

Existing production/local-access regression:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=app::metrics::tests::benchmark_snapshots_are_local_read_only_and_opt_in) | test(=app::metrics::tests::recorder_installs_and_renders)'
```

### Shared observations

Planned, current `wyrd-benchmark` library test owner `tests`:

```bash
mise exec -- cargo nextest run --locked -p wyrd-benchmark --lib -E 'test(=tests::standard_metrics_and_request_timings_share_one_snapshot)'
```

### Interval conservation

Existing concurrent observation and HDR evidence regression:

```bash
mise exec -- cargo nextest run --locked -p wyrd-benchmark --lib -E 'test(=recording_tests::benchmark_histograms_preserve_bounded_intervals) | test(=tests::series_beyond_the_bound_are_dropped) | test(=tests::counts_above_a_threshold_bound_its_bucket)'
```

### Population validation

Existing population/scan classification regression; extend for changed snapshot
behavior and full declared coverage:

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=capacity::tests::snapshot_windows_keep_classes_and_scans_apart)'
```

### Historical records

Existing cohort, persistence and historical-record regression:

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::histogram_cohorts_preserve_measurement_and_drain_windows) | test(=experiment::tests::kept_timing_evidence_rebuilds_judged_readings) | test(=experiment::tests::histogram_records_preserve_method_and_refuse_older_versions) | test(=experiment::tests::reports_regenerate_from_persisted_records)'
```

### Consumer matrix

Planned, existing `wyrd-testing` `capacity` binary test owner `tests`:

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::all_engine_consumers_read_benchmark_snapshots_without_prometheus)'
```

### Consumer regressions

Existing pure/in-process consumers; no Postgres, release server or load:

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=query::isolated::tests::write_phases_read_back_with_populations_and_labelled_gaps) | test(=verification::run::tests::diagnostics_use_benchmark_distributions) | test(=gateway::run::tests::overhead_repetition_subtracts_waits_per_call_and_counts_capture) | test(=gateway::capacity::tests::capacity_repetition_classifies_phases_and_capture)'
```

### Benchmark journey

Planned `resilience::benchmark_mode_keeps_capture_and_snapshot_counts_without_prometheus`
in the existing `wyrd-testing` Gateway target; use the real SDK/server/Postgres
path and its existing local provider. Keep the existing production metric and
post-response capture journeys as regressions. This is functional traffic with
no throughput or latency target, not a benchmark experiment:

```bash
WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=resilience::benchmark_mode_keeps_capture_and_snapshot_counts_without_prometheus) | test(=resilience::gateway_capacity_metrics_cover_buffered_and_streamed_calls) | test(=resilience::capacity_capture_accounting_waits_for_post_response_work)"'
```

### Static closure

After focused proofs: `mise run fmt:check`, then targeted Clippy for the actual
changed packages/all affected targets and features, and `mise run check:deps`.
For the currently inspected write set, the lint command is:

```bash
mise exec -- cargo clippy --locked -p wyrd-benchmark -p wyrd-telemetry -p wyrd-runtime -p wyrd-tonic -p vala-bifrost-redux -p wyrd-server -p wyrd-testing --all-features --all-targets -- -D warnings
```

Inspect the final diff and candidate inventory. Confirm no live benchmark consumer
uses Prometheus exposition, no test-tool dependency reaches production, no
useful diagnostic was silently dropped, and no product contract/generated output
needs updating. Record any required additions to the dependency closure. Full
journey suites and broad aggregates belong to integrated change review, not here.

## Material Stop Conditions

Stop for specification authority if completion needs changed security, capture,
admission, concurrency or durability semantics; a new public workflow; incompatible
history; or an optimization. New external dependencies/features or new ownership
boundaries need the repository's authority process. Internal decomposition and
placement inside the approved shared instrumentation boundary remain local choices.
Do not run a benchmark or release benchmark build to make a test pass. Present a
bounded experiment separately if evidence requires one.

## Authority Links

- [Approved spec revision 2](../spec.md)
- [Workflow](../../../spec-driven-development.md),
  [implementation execution](../../../implementation-execution.md),
  [testing](../../../../TESTING.md)
- [Capture contract](../../../../architecture/wyrd-design.md#principal-model),
  [Bifrost](../../../../architecture/bifrost-design.md),
  [operational indicators](../../../../architecture/operations/reliability-and-recovery.md)

## Implementation Evidence

Verdict: `IMPLEMENTED`, pending `$wyrd-task-review`.

### Candidate disposition

| Candidate change | Disposition | Reason |
|---|---|---|
| `wyrd_telemetry::timing` moved to `crates/shared/wyrd-benchmark` | Retained, reworked | Now the one benchmark owner: `BenchmarkRecorder` (`recorder.rs`) implements `metrics::Recorder` for counters, gauges (value, peak) and HDR histograms (`_seconds` as integer ns, other histograms as exact integers, refused values counted), beside the existing timing/scan series under one lock, bounded by `MAX_METRIC_SERIES`. `TimingSnapshot`/`TimingInterval` renamed `BenchmarkSnapshot`/`BenchmarkInterval`. |
| `enable()` | Reworked | Replaced by `install()`, which installs the recorder globally before enabling; it fails if any recorder already exists. |
| Filtered Prometheus `BenchmarkMetrics` backend | Removed | REQ-001 forbids it. `MetricsBackend::install(benchmark)` selects one backend before registration, with no fallback. |
| `/benchmark/timings` beside `/metrics` | Reworked | Benchmark mode serves `/benchmark/snapshot` (plus `/benchmark/heap` with `bench`) and no `/metrics`. Production serves `/metrics` only. Loopback validation is unchanged. |
| `outbox_writes_total` counter | Removed | New production family. Gateway `Integrity` reads `outbox_write_seconds_count` again, from the snapshot. |
| Gateway `CaptureSlot`/`AnswerResume` phases, `Answer::sent`, per-phase `record_timing` | Removed | REQ-005/TASK-002 scope, and they added production label values. `admitted` is back to the `phase="account"` histogram count. |
| Verification `record_timing` for `verification.submit`/`queue_wait`/`bifrost.scribe.ack`, plus the duplicate-retry ack histogram | Removed | Each was a second observation of a duration that an existing histogram already records. Consumers read those histograms from the snapshot. |
| `capacity::{Selection,Readings,Telemetry,Histogram::read}` deleted | Retained, reworked | Superseded by `Population::{metrics,counter,histogram}`. The Verification `BREAKDOWN` families, submission/queue-wait means and ack count are restored over snapshot metrics. |
| `Histogram`/`deltas` moved to `release_server` | Retained | Production metrics contract-test utilities (`tests/gateway/resilience.rs`). |
| Existing dual sites (Gate write, direct Verification, Gateway handoffs, Scribe ack) | Reworked | The production histogram is recorded only when `wyrd_benchmark::enabled()` is false. In benchmark mode, `bifrost_scribe_ack_seconds` holds only deduplicated retries and `wyrd_verification_*` phase/run/verifier/overhead histograms hold only queued executions. Consumers merge the timing series for the rest. |
| `timing_declaration`, `benchmarks/README.md` | Reworked | Both now declare the sole recorder and the dedupe rule, and the declaration adds `max_metric_series`. The README text about Gateway phases was dropped (TASK-002). |
| Manifests (`wyrd-tonic`, `vala-bifrost-redux`, `wyrd-telemetry`) | Retained | Incidental reorderings reverted. `wyrd-telemetry` keeps `metrics`/`ulid`/`http`, which it still uses. |
| Calibration `pg_tests` scrape | Reworked | Reads `/benchmark/snapshot`. Still `#[ignore]` and was not run. |
| Untracked `changes/active/gateway-latency-capture/` | Untouched | Not part of this task. |

### Acceptance

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001–002; AC-001: exclusive startup, one owner for all kinds | `wyrd-server/src/app/metrics.rs` `MetricsBackend::install`, `metrics_router`; `app/mod.rs`, `app/server.rs`; `wyrd-benchmark/src/{lib,recorder}.rs` | `app::metrics::tests::{benchmark_mode_installs_only_benchmark_recorder, benchmark_snapshots_are_local_read_only_and_opt_in, recorder_installs_and_renders}`; `wyrd-benchmark tests::standard_metrics_and_request_timings_share_one_snapshot`. RED: initializing Gate metrics before install fails the startup test. | PASS |
| No double-counting at dual sites | `gate/mod.rs record_gate_request`; `scribe/ingress.rs`; `verification/telemetry.rs emit`; `gateway/telemetry.rs point` | `gate::tests::histogram_write_boundary_includes_durable_append`, `scribe::ingress::tests::acknowledged_append_records_each_write_phase_once`, `components::gateway::telemetry::tests::histograms_preserve_head_and_final_overhead`, `verification::hosted::blocks::tests::histogram_overhead_excludes_real_overlapping_blocks` (each asserts the production histogram is absent) | PASS |
| REQ-003,007; INV-004; AC-002: intervals, bounds, invalid evidence, history | `capacity.rs Population::{from_interval,metrics,counter,histogram}`; `IntervalMetric::{between_all,counter,histogram}` | Interval conservation trio; `capacity::tests::snapshot_windows_keep_classes_and_scans_apart` (extended: refused metric updates are invalid, an unregistered counter is `None`); the four `experiment::tests` history cases | PASS |
| REQ-004; INV-001–003: every engine/level consumer, no Prometheus | `release_server.rs Metrics::of_snapshot`; `verification/run.rs`; `gateway/{run,capacity}.rs` | `tests::all_engine_consumers_read_benchmark_snapshots_without_prometheus` (RED: zeroing histogram counts in `of_snapshot` fails `Integrity.written`); consumer regressions quartet including `verification::run::tests::diagnostics_use_benchmark_distributions` | PASS |
| Functional capture/acknowledgement closure | `tests/gateway/resilience.rs` | `resilience::benchmark_mode_keeps_capture_and_snapshot_counts_without_prometheus`, plus production journeys `gateway_capacity_metrics_cover_buffered_and_streamed_calls` and `capacity_capture_accounting_waits_for_post_response_work` | PASS |
| REQ-009; AC-005 | Disposition table above | Static closure below | PASS |

### Commands

All ran sequentially and passed:

- Every selector listed under Verification and Evidence, run as written.
- The benchmark journey through `scripts/postgres/with-test-postgres.sh`.
- Owner suites under Postgres: `cargo nextest run --locked --no-fail-fast -p wyrd-benchmark -p wyrd-testing -p wyrd-server -p vala-bifrost-redux -p wyrd-runtime --lib --bin capacity`. Result: 1708 passed, 2 skipped (the ignored calibration tests).
- `mise run fmt:check`.
- The listed Clippy command with `--all-features --all-targets -D warnings`.
- `mise run check:deps`.
- `git diff --check`.

### Diagnoses

- **Symptom:** `diagnostics_use_benchmark_distributions` failed on first run, at the component-order assertion. **Evidence:** the outbox and engine fixtures were both 100 µs. **Cause:** the test fixture produced a tie in the sort key. **Fix site:** the test fixture only (outbox set to 50 µs). Product code was unchanged.
- **Symptom:** a Postgres-less ad-hoc run failed cluster, teardown and catalog pin tests. **Evidence:** `test DB env unset (WYRD_TEST_DATABASE_ADMIN_URL)`. **Cause:** those tests need the fixture database, and the ad-hoc filter did not exclude them. **Fix site:** none needed. The same suites passed under `with-test-postgres.sh`.

### Limits

- `MAX_METRIC_SERIES` coverage is proven for the Gateway journey (`dropped == 0`) and the bounded-registry tests. It is not proven by a full declared L1/L2 run, because no benchmark run was authorized.
- Component means are bucket bounds, not exact Prometheus sums.
- Non-goals stayed excluded: no scheduler, cache, transport, admission or optimization change; no SDK, schema, migration or generated-artifact change; no benchmark run or release benchmark build.
