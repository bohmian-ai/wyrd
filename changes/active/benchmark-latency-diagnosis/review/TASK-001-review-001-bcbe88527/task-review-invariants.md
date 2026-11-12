# TASK-001 invariant acceptance review

Verdict: **FAIL**.

Subject: base `8d25b13a1ededa23b9eb91973a2b489fe9224557` to candidate
`bcbe88527eccd7be4768cc39e1ba55b855300512`. First review; no carried-forward
acceptance evidence. Approved authority is specification revision 2 and the
original TASK-001 contract at `8387f3a05`. Candidate implementation evidence
was read separately as claims. HEAD matched candidate and tracked source was
clean when inspected. No other reviewer report was read.

## Scope and method

Reviewed the shared value-conservation boundary: standard producers and explicit
timers → `Registry` / `BenchmarkRecorder` → cumulative snapshots →
`BenchmarkSnapshot::since` / `IntervalMetric` → `Metrics::of_snapshot` /
`Population` → Bifrost, Verification and Gateway L1/L2 consumers. Expanded the
starting map to unchanged sibling histogram producers, Scribe publication,
Forge output measurement, query profiles, and runtime outbox accounting.

Read the supplied repository rules, task-review skill and scope first, then
the original task/specification, relevant diff/source, workflow, maintainer
guide, testing map, architecture authority map and operational-indicator
section. No `.codegraph/` is present. No nested AGENTS.md was found in the
reviewed changed source trees. This specialist does not repeat startup,
concurrency, repository-wide standards or production-resilience acceptance
assigned to other reviewers.

No source edits, builds, test executions, benchmarks, dependency installation
or services were performed. Existing test bodies and candidate evidence are
verification evidence by inspection, not independently rerun results.

## Scoped acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: counters, gauges and timing/diagnostic histograms share one owner | `wyrd-benchmark/src/lib.rs` Registry fields, `record`, `snapshot`; `recorder.rs` registration/Handle updates; standard and explicit observations use the same registry lock | `tests::standard_metrics_and_request_timings_share_one_snapshot`; recorder installation evidence in original task's candidate appendix | PASS for ownership; fractional values fail below |
| REQ-001/004: preserve existing non-time histogram units and useful observations | `recorder.rs:99–112` accepts only exact integers for every non-`_seconds` family; real ratio/cost producers do not satisfy that constraint | Shared-observation test deliberately expects `0.5` to be refused; no real ratio producer is exercised | FAIL — I-1 |
| REQ-001: one semantic duration at existing dual sites | Gate `record_gate_request`; Scribe ingress normal append versus duplicate retry; direct Verification `ExecutionTelemetry::emit`; Gateway `CallTelemetry::point` | Focused Gate/Scribe/direct-Verification/Gateway tests assert production histogram absence in benchmark mode | PASS |
| REQ-003: per-request residual and independent outcomes/classes/phases | `Registry::record` subtracts before aggregation and increments negative/missing/out-of-range evidence; closed label propagation; `IntervalSeries::problems` | `recording_tests::benchmark_histograms_preserve_bounded_intervals`; `capacity::tests::snapshot_windows_keep_classes_and_scans_apart` | PASS |
| AC-002: cumulative interval conservation, replaced process/reset detection | `BenchmarkSnapshot::since` checks process/pid/method/time, disappeared series, decreasing counters/buckets; `IntervalMetric::between_all` checks disappeared/kind-changed metrics; `between` checks units and rejected deltas | Interval conservation test and shared-observation vanished-metric assertion, inspected | PASS |
| REQ-003/004: missing/refused evidence reaches result validity | `Population::from_interval` propagates drops, invalid labels, timing and scan failures and rejected metric updates; all six workflows retain population validity; `histogram` returns missing rather than an empty default | Population validation test; engine source call paths listed below | PASS for refusal propagation; this makes I-1 invalidate valid work |
| REQ-004: count, backlog, health and resources use snapshots | `LocalServer::metrics` calls `Metrics::of_snapshot`; counter totals, gauge values and histogram observation counts map into existing consumers; CPU/RSS remain external; no benchmark exposition reader remains | All-engine consumer matrix, Gateway Integrity and Verification diagnostic tests by inspection | PASS within inspected invariant boundary |
| REQ-004: Scribe ACK population preserves normal appends and deduplicated retries | Normal admitted append emits `bifrost.scribe.write`; duplicate branch retains `bifrost_scribe_ack_seconds`; Verification `acknowledgements` merges those distributions once; Gateway writer proof still reads `outbox_write_seconds_count`, rather than items | `acknowledged_append_records_each_write_phase_once`, diagnostic/consumer matrix tests | PASS |
| AC-002 / original Scenario 3: complete declared workload fits bounded process lifetime | Caps are 256 timing / 64 scan / 1024 standard series; process accumulates across warmup, steps and repetitions; no complete cardinality assertion/derivation supplied | Timing overflow test only proves refusal; startup assertions only prove `len <= cap`; Gateway journey observes one supported workflow | FAIL — I-2 (required proof missing; actual overflow is not established) |
| INV-004: historical readability and declared method changes | New metrics vector and method max-metric-series field have serde defaults; `TimingEvidence` retains before/after distributions; timing declaration identifies sole recorder and metric cap | Four original historical/cohort tests retained with owner/type migration | PASS by source/test inspection |
| Reuse / no unearned collection abstraction | New narrow recorder reuses existing standard `metrics` mechanics and HDR interval storage; existing `wyrd-bench` recorder cannot supply cumulative bucket evidence and cannot enter production dependency cone | Compared its registration, handles and scale with new owner; all live benchmark consumers reuse LocalServer/Population/experiment owners | PASS; I-1 correction belongs in this owner |

## Producer-to-consumer tracing

- Standard counters use monotonic total/absolute semantics, gauges retain current
  values and lifetime peaks, and histogram snapshots retain count plus buckets.
  `Metrics::of_snapshot` intentionally projects histogram **observation counts**,
  preserving Gateway's distinction between writer invocations and written items.
- `Population::collect` waits for exact declared boundary/outcome/case/phase
  cohorts; completed work after the traffic stop retains its admission labels.
  Query isolated (`isolated.rs:380–381`), query competing
  (`competing.rs:398–420`), Verification L1/L2 (`run.rs:326–338`, `471–495`),
  Gateway L1 (`run.rs:556`, `336`) and Gateway L2 (`capacity.rs:1056`, `755`)
  propagate the final population into persisted repetition validity/evidence.
- Verification L2 stage diagnostic intervals are completion-window observations,
  while admission-cohort direct timings remain distinct. Diagnostic means use
  `Distribution::total_bounds` divided by sample count, preserving HDR bounds
  rather than averaging percentiles. Direct phase distributions are merged with
  queued standard component histograms; components are explicitly overlapping.
- Scribe's deduplicated branch (`ingress.rs:789–802`) still contributes one ACK
  histogram observation and skips the regular write timer. The normal branch
  (`813–820`) records one write distribution instead of a second ACK histogram.
  A refused/accepted-under append does not fabricate a durable success sample.
- Snapshot projections keep existing zero-if-never-emitted counter bookkeeping
  for loss families, while histogram selections and population evidence have
  explicit absence/refusal handling. No additional speculative change to those
  established sparse-counter semantics is proposed here.

## Proposed findings

### I-1 — INCORRECT: valid fractional diagnostic observations are refused

**Violated obligation:** REQ-001's diagnostic histograms, REQ-004's preservation
of useful diagnostic coverage, and TASK-001 Scenario 2's unit/population
preservation. `metrics::Histogram` observations are `f64`; the existing producer
contract is not restricted to integers for non-time families.

**Exact changed defect:**
`crates/shared/wyrd-benchmark/src/recorder.rs:99–112`, especially the
`HistogramUnit::Value => observed` and `integer.fract() == 0.0` combination.

**Source-proven supported path:**

1. `parquet/variant_residual.rs:49–70` computes residual uncompressed bytes
   divided by all non-metadata Variant bytes. A partially shredded file has a
   legitimate fractional share between zero and one. `VariantWriter::record`
   (`33–36`) emits that value as `bifrost_variant_residual_share`.
2. Final Scribe publication calls this producer from
   `scribe/parquet_writer.rs:926–927`; Forge uses the same producer at
   `forge/managed/executor.rs:475`. These are production writers, not dormant
   test helpers. The existing residual-share test itself writes shredded files
   and checks their share is below 0.5.
3. The declared Bifrost WritesParquet workload pays for staging/publication
   during its windows (`query/mod.rs:155–160`); PARQUET_ENV retains only 30 s
   (`136–140`). L2 competing also publishes/compacts while traffic runs. A
   normal partially shredded span/log file therefore produces this observation
   during supported measured work.
4. This family lacks `_seconds`, so the new recorder refuses its fraction and
   increments `Metric.rejected` instead of recording it. Interval conversion
   preserves the rejected delta. `capacity.rs:521–527` makes it invalid evidence;
   `Population::validate` (`669–680`) carries the reason into the result.

**Observable consequence/materiality:** valid normal Bifrost publication loses
its diagnostic samples and can invalidate an otherwise correct benchmark
repetition. This is an actual supported producer mismatch, not hypothetical
NaN/negative-input hardening. The new shared-observation test expecting a
fraction to be rejected codifies the incompatible assumption. The sibling
`wyrd_gateway_call_cost` producer (`gateway/invocation.rs:1183–1187`) can also
emit valid fractional values when pricing exists; it shares the cause rather
than requiring a separate downstream fix.

**Smallest correction boundary:** fix representation of valid finite fractional
non-time histogram values in `wyrd-benchmark` and keep their original unit and
declared precision through snapshots/intervals. Existing `wyrd-bench` uses an
integer scaling approach (`HISTOGRAM_SCALE`), so the necessary primitive is
already conventional in the repository; reuse the approach within the sole
owner, without importing the test-tool crate. Preserve integer counts and
duration precision. Do not delete the diagnostic producer, round shares/costs
to whole units, or suppress rejected-update invalidity at `Population`.
This is a bounded recorder/snapshot conversion correction, not a second
collector or a product/security/capture change.

**Focused proof:** record a real valid fractional ratio through
`VariantWriter::Scribe.record(Some(0.25))` under the benchmark recorder, plus an
integer non-time histogram and a seconds histogram; round-trip before/after
snapshots and require correct counts/unit/precision bounds and no refused
update or population invalidity. Cover the Forge sibling via the same existing
producer interface. Keep genuine non-finite/out-of-range refusal proof.

### I-2 — MISSING: complete declared workload capacity proof was omitted

**Violated obligation:** AC-002 explicitly requires maximum declared workload
coverage to fit bounded recording capacity. Original Scenario 3 requires
focused cases that fail for insufficient coverage bounds, including complete
engine/level coverage. A live benchmark is not required or authorized for this.

**Exact evidence:** `lib.rs:75–78` and `recorder.rs:28` set process-lifetime
caps. `Registry::record`, `scan`, and `metric` refuse new series at their caps.
The existing `series_beyond_the_bound_are_dropped` test only overfills the
timing registry using synthetic unique series; it does not represent supported
workloads. `all_engine_consumers_read_benchmark_snapshots_without_prometheus`
(`capacity/main.rs:1687` onward) creates a small representative set in one
snapshot pair; it does not expand declared cases/phases/outcomes or accumulate
all steps/repetitions for any process. No complete capacity derivation or
full-workload bound test is present among all MAX_* references. Candidate
task evidence's Limits explicitly restricts capacity proof to the Gateway
journey and bounded-registry tests.

**Materiality:** this is a missing explicitly requested acceptance proof,
not a demonstrated assertion that 256/64/1024 is too small. New standard
series include existing HTTP/RPC, SQL, outbox, Oracle plan-profile and resource
families, beyond the explicit request timers. Process-wide accumulation means
one passing Gateway journey or a `len <= cap` assertion cannot establish zero
drops over a complete declared Bifrost/Verification/Gateway workload.

**Smallest testable correction:** provide the required bounded in-process
coverage proof, derived from actual declared workloads, producer families and
process lifetimes, including warmup, repeated phases, incidental/setup work,
supported terminal outcomes, and standard diagnostic/resource series. Assert
no refused registration/observation caused by the capacity caps and retain a
separate overflow-negative test. A justified source-derived cardinality bound
can complement the fixture. Reuse existing recorder/producer tests and workload
declarations; do not run load or a release build, invent an exhaustive Cartesian
product of impossible combinations, add a new harness, or enlarge caps without
evidence. The proof should expose I-1 as well if it uses actual fractional
producers rather than integer-only invented observations.

## Limits and disposition

No separate drift finding: the narrow shared owner and existing consumer owners
are sufficient. Proposed corrections preserve sole-recorder selection,
production telemetry, sparse zero-loss counters, durable ACK/capture ownership,
and immutable historical records.

Source and named tests support the passing scoped rows, subject to the explicit
no-execution limit. I-1 is a functional supported-producer defect; I-2 is the
specific missing required capacity proof. These proposals require independent
validation, not reviewer agreement. Overall **FAIL** for this invariant scope.

Skipped runtime verification under the review's read-only/static boundary.
Risk: current valid fractional diagnostics can discard samples and invalidate
benchmark results; full declared workload capacity remains unproven.
