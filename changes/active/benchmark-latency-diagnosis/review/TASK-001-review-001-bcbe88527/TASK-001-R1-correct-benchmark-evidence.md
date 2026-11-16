---
id: TASK-001-R1
kind: remediation
status: proposed
spec: SPEC-benchmark-latency-diagnosis
spec_revision: 2
original_task: TASK-001
review_candidate: bcbe88527eccd7be4768cc39e1ba55b855300512
review_base: 8d25b13a1ededa23b9eb91973a2b489fe9224557
findings: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5]
---

# Preserve legitimate benchmark values and incomplete-evidence signals

## Authority and outcome

Implement this bounded remediation through `$wyrd-implement` against the
[approved spec revision 2](../../spec.md),
[original TASK-001](../../tasks/TASK-001-sole-benchmark-recorder.md), and
[validated ledger](findings-validation.md). The original review subject is base
`8d25b13a1ededa23b9eb91973a2b489fe9224557` through candidate
`bcbe88527eccd7be4768cc39e1ba55b855300512`. Preserve those identities and this
verdict for cumulative remediation review.

The sole recorder/backend architecture remains. Correct two evidence-handling
defects, supply one explicitly required capacity proof, and close the changed-item
rustdoc and independent-diagnosis gates. This is neither a new design plan nor
Gateway performance work. No approved behavior needs revision.

## Diagnoses and selected corrections

### FIND-TASK-001-1 — Valid fractional diagnostics are rejected

REQ-001/004 and scenario 2 require preserving real histogram units and populations.
`wyrd-benchmark/src/recorder.rs:99–112` assumes non-time observations are integers.
But Scribe/Forge's shared `VariantWriter::record` emits real residual/total ratios;
final Scribe artifact sealing and Forge output processing call it. The declared
OTLP fixture contains both shredded and residual attributes, so this is supported
Bifrost publication behavior. Decimal Gateway call cost is a sibling producer.
A ratio such as 0.25 is rejected, omitted from the histogram and then invalidates
a measured Population. Reporting the rejection does not preserve a valid sample.
The present shared-observation test embeds the incorrect integer-only assumption.

Correct the sole recorder/snapshot numeric representation with bounded fixed-point
HDR encoding and an explicit scale/unit declaration interpreted in original units.
Select and document precision/range against the actual producer values. Close
snapshot, interval selection/reconstruction and diagnostic conversion together.
Preserve integer count-family semantics, duration nanoseconds, counts and genuine
non-finite/out-of-range refusal. Use the existing HDR and owner machinery; do not
copy the tooling recorder's non-time truncation, add a dependency or collector,
delete producers, round ratios to whole units, or relax Population invalidity.
This shared-owner correction is necessary because downstream guards cannot
recover a value the recorder refused.

### FIND-TASK-001-2 — Required declared-workload capacity proof is absent

AC-002 and original scenario 3 require complete declared coverage within the
256 timing / 64 scan / 1024 standard-metric series limits. The recorder retains
series for its entire process lifetime. Existing overflow tests, a small consumer
snapshot and a two-call Gateway journey do not establish the complete footprint.
No actual overflow is claimed; this is an explicit missing acceptance proof.

Extend the existing in-process proof with a finite, source-backed footprint tied
to the workload declarations and actual producers. Cover reachable cases,
phases/outcomes/parts, scan series and standard families, including setup/drain,
for each actual engine/level server lifetime. Repeated keys do not multiply
cardinality, and different server lifetimes must not be combined. Exercise the
distinct reachable series through existing interfaces and assert retained counts
and zero capacity refusals against each bound. A documented finite bound may
cover unchanged standard families where running server work would require an
unnecessary harness. Preserve overflow-negative proof; increase a bound only if
this supported footprint demonstrates necessity.

Do not enumerate arbitrary external labels or every conceivable error product.
Do not introduce another harness or require a release build, synthetic host load
or benchmark run. The missing proof can be supplied without them.

### FIND-TASK-001-3 — Projection drops known refusal evidence

REQ-003/004 and scenarios 3–4 require truthful setup/poll/drain observations.
`LocalServer::metrics` and `Metrics::of_snapshot` at
`wyrd-testing/src/release_server.rs:358–359,568–594` discard dropped/rejected
metadata. Registration overflow produces no-op metric handles; invalid gauge
updates preserve a stale value. Projection then exposes absent gauges through
`sum()` as zero. Gateway settle, Verification drain/backlog, and Bifrost Residence
can report healthy/settled state from a snapshot already known to be incomplete.
Later measured Population validation does not protect every such raw read or
recover a gauge refused before its opening baseline. This finding asserts false
setup/drain evidence, not actual loss of accepted server work or a universal
false whole-run PASS.

Make the existing snapshot-to-Metrics boundary fallible and reject cumulative
`dropped > 0` or any metric `rejected > 0` before losing that metadata. Propagate
through the existing `LocalServer::metrics` Result and adapt all direct conversion
callers: capacity consumer proof, the ignored calibration snapshot helper and the
benchmark Gateway journey. Preserve healthy never-emitted-family behavior and
production `Metrics::parse`. This single projection guard owns the lost validity
information and is smaller than per-engine guards, required-family inventories,
retries or lifecycle changes. Fail the collection operation; do not fail server
requests, change accepted-work drain ownership or introduce a process-crash policy.

### FIND-TASK-001-4 — Changed-item rustdoc gate is incomplete

The root Code rules require rustdoc for changed fields/private items and
`# Errors` for fallible functions. New `verification/run.rs:1113–1122`
`acknowledgements` returns selection/merge errors without documenting them.
Retyped backend ownership fields at `app/server.rs:95,393` lack docs. Activation
docs at `wyrd-benchmark/src/lib.rs:200,245,271,1139` link to removed `enable`.
These are mandatory gate violations, not a new behavior requirement.

Add accurate error behavior, backend ownership/absence semantics, and current
install/enabled references. Keep this correction comment-only, batched with the
functional fixes. Do not reintroduce an alias or document unrelated old items.

### FIND-TASK-001-5 — Independent test-repair diagnosis record is missing

Candidate task evidence reports repairing a failed component-order assertion by
changing two equal 100 us fixtures to an outbox fixture of 50 us. Current
`verification/run.rs:1248` and the descending component aggregation/assertions
support the stated tie mechanism. No independent diagnostician report for that
failure was supplied. The earlier Gateway-series diagnosis concerns another
failure. A green test and implementation narrative do not satisfy the required
independent test-repair record. No weakened assertion or product defect is inferred.

Obtain one fresh, focused read-only diagnosis from the original failing selector,
available failure record/trace and original-to-corrected fixture diff. Preserve
its cause, fix-site, caller and ranking assessment beside the original diagnosis.
Explicitly date/disclose it as validation of an already-applied repair; never
claim it preceded the fix or invent missing historical trace. Preserve the fixture
and assertions if independently justified. The discovery/validation reports in
this review do not substitute for that separately required diagnosis. No production
change or broad audit is necessary for this finding.

## Preserved behavior and non-goals

- Exactly one benchmark backend; ordinary production Prometheus behavior unchanged.
- Per-request elapsed/excluded/residual semantics, cohort classification and
  duplicate suppression unchanged; units/scales remain truthful.
- Security, tenancy, audit, capture reservation before dispatch, deadline/refusal,
  accepted ownership, settlement, retries and durable acknowledgement unchanged.
- History remains immutable/readable; declare representation precision without
  presenting it as a measured speedup.
- No optimization, new dependency/feature, transport/scheduler/cache, baseline
  rerun, live benchmark, release benchmark build or synthetic load.
- No optional import/layout cleanup or additional reviewer findings are bundled.

## Acceptance and focused proof

Apply RED/GREEN to executable corrections using existing interfaces; passing
regressions need no manufactured RED. The implementer owns private code and test
fixture shape. Planned selectors below must be confirmed after adding the test,
before RED; adjust only to an equivalent focused selector if placement changes.

| Finding | Required observation |
|---|---|
| FIND-TASK-001-1 | A real residual-ratio producer/interface and decimal-cost interface retain one fractional observation each, with original units, declared scale/precision and containing bounds; integer and duration histograms still behave correctly. Snapshot differencing yields zero rejections and no Population invalidity. Invalid values remain rejected. |
| FIND-TASK-001-2 | Every counted distinct series maps to an actual declared producer/lifetime; all three finite caps fit the derived footprint with retained-count/no-drop assertions. Reducing a cap below that reachable footprint makes the proof fail. Existing refusal tests remain. |
| FIND-TASK-001-3 | Dropped registration with absent backlog gauges and a rejected update with a stale-zero gauge both fail conversion, rather than appear settled. Healthy snapshots preserve counters, gauges and histogram counts, including valid unused-family semantics. Every direct conversion caller handles failure. |
| FIND-TASK-001-4 | Targeted source/link inspection verifies the actual errors, ownership and activation contract; no behavior changed for docs. |
| FIND-TASK-001-5 | A separately recorded fresh diagnosis independently supports the original tie and corrected ranking, records historical evidence limits and accurately describes its retrospective timing. |

Run Cargo checks sequentially through mise. These commands are for remediation,
not evidence that this review executed them:

```bash
mise exec -- cargo nextest run --locked -p wyrd-benchmark --lib -E 'test(=tests::standard_metrics_and_request_timings_share_one_snapshot) | test(=recording_tests::benchmark_histograms_preserve_bounded_intervals) | test(=tests::series_beyond_the_bound_are_dropped)'
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=release_server::tests::snapshot_conversion_rejects_incomplete_metric_evidence) | test(=capacity::tests::snapshot_windows_keep_classes_and_scans_apart)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::declared_workloads_fit_recording_bounds) | test(=tests::all_engine_consumers_read_benchmark_snapshots_without_prometheus) | test(=verification::run::tests::diagnostics_use_benchmark_distributions)'
```

The two new names above are **planned**: the fallible-conversion proof belongs to
the existing `release_server::tests` library owner; the declared-footprint proof
belongs to the existing capacity binary/declaration test owner. Keep source-backed
coverage for the unchanged six-consumer paths and accepted production/capture
boundaries. A broad journey suite is not required to close these gaps.

For comment/link closure and the changed write set:

```bash
RUSTDOCFLAGS='-D rustdoc::broken_intra_doc_links' mise exec -- cargo doc --locked --no-deps --document-private-items -p wyrd-benchmark
mise run fmt:check
mise exec -- cargo clippy --locked -p wyrd-benchmark -p wyrd-testing --all-features --all-targets -- -D warnings
mise exec -- git diff --check
```

If the actual correction touches another owning package's executable behavior,
include only its focused tests/lint coverage; do not silently widen into a gate
or benchmark suite. Record exact final commands, new diagnostics and genuine
execution limits in remediation evidence. Do not weaken acceptance, overflow or
invalid-evidence checks to obtain green results.

## Handoff and stop conditions

Route directly to `$wyrd-implement`. A later immutable review checks these five
closures, the prior-to-new candidate diff and affected producer/consumer paths;
unchanged accepted evidence carries forward. It does not repeat comprehensive
discovery merely because this is a new round.

Stop for authority if correction truly needs changed approved behavior, a public
product/persistence/security/concurrency/resource-ownership decision or a new
external dependency. None is selected by this remediation. Bounded fixed-point
representation and a fallible existing projection remain within the approved
benchmark instrumentation contract; private implementation choices remain local.

## Implementation evidence

Commits `ef43d6c44..7ac1e99de` on top of candidate `bcbe88527`.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-1 | Any non-`_seconds` histogram records fixed-point billionths (`HistogramUnit::Billionths`, `per_unit()`), range `[1, HIGHEST_METRIC = 2^62]` billionths; `_seconds` stays nanoseconds. Negative, non-finite and out-of-range values are still rejected. `Method.highest_metric` and the `metric_histograms` timing declaration state the scale (`wyrd-benchmark/src/recorder.rs`, `lib.rs`, `wyrd-testing/src/capacity.rs`). | `tests::standard_metrics_and_request_timings_share_one_snapshot`: 0.25 residual share, 0.000123 cost, whole count 4 kept in original units with zero rejections; -1 and NaN still rejected. `parquet::variant_residual::tests::benchmark_recorder_keeps_fractional_residual_shares` goes through `VariantWriter::record`. RED: re-adding a fractional refusal fails the shared test. | PASS |
| FIND-TASK-001-2 | `tests::declared_workloads_fit_recording_bounds` (capacity `main.rs`) lists every series each engine/level server lifetime can open: cases, phases, outcomes, parts, and unlabelled setup/background work, for 8 lifetimes. It asserts each lifetime is within `MAX_SERIES`/`MAX_SCAN_SERIES`, then records the largest timing (218, `l1 bifrost writes-parquet`) and scan (33, L1 read) footprints with `dropped == 0` and exact retained counts. The first RED showed L1 reads reaching 99 scan series under failed/client_drop outcomes. Consumers read only successful scans (`scanned_per_query(SUCCESS)`), so Oracle now records scan facts only for answered queries, and the 64 cap holds. | Test passes; with `MAX_SERIES` lowered to 200 it fails. `oracle::tests::benchmark_scans_record_answered_queries_only` fails when the guard is removed. The overflow-negative `tests::series_beyond_the_bound_are_dropped` still passes. | PASS for timing and scan; see limits for standard metrics |
| FIND-TASK-001-3 | `Metrics::of_snapshot` returns `Err` when `dropped > 0` or any metric has `rejected > 0`. `LocalServer::metrics` propagates it; the consumer matrix test, the Gateway journey and the ignored calibration `scrape()` handle it (`release_server.rs`, `main.rs`, `tests/gateway/resilience.rs`, `tests/pg_tests.rs`). | `release_server::tests::snapshot_conversion_rejects_incomplete_metric_evidence`: a dropped registration and a stale refused gauge both fail; a healthy snapshot keeps counters, gauges, histogram counts, and never-emitted families read as zero. RED: removing either check fails it. The Gateway benchmark journey passes under Postgres. | PASS |
| FIND-TASK-001-4 | `# Errors` on `acknowledgements`; ownership docs on both `metrics: Option<MetricsBackend>` fields; `enable` links replaced with `install`. Comment-only. | `RUSTDOCFLAGS='-D rustdoc::broken_intra_doc_links' cargo doc -p wyrd-benchmark --document-private-items` | PASS |
| FIND-TASK-001-5 | Fresh read-only diagnosis below. | Recorded below. | PASS |

### FIND-TASK-001-5 retrospective diagnosis

Obtained 2026-11-09, after the repair: the fixture fix was applied before `8387f3a05` was committed, and this diagnosis validates it after the fact. The diagnostician was given only the failing selector, the single retained panic line (`assertion failed: components[0].starts_with("component verification.direct.engine:")`) and the one-line fixture diff (`outbox_write_seconds` `0.000_1` → `0.000_05`).

- **Cause:** both components were exactly 100 000 ns in the same HDR bucket, so `total_bounds()` matched. `components()` sorts stably by `Reverse(high)` over `component_totals`' `BTreeMap` order, so `outbox_write{…}` sorted before `verification.direct.engine` on every run. It was a deterministic tie, not flakiness.
- **Fix site:** the fixture. Tied components cannot be told apart at histogram resolution, so alphabetical tie order in production is acceptable.
- **Callers:** `components()` orders the diagnostic rows in `run.rs` (top `COMPONENT_ROWS`); `observed_components` checks only for emptiness; the matrix test asserts only `len() == 2`. No production behavior is wrong.
- **Ranking:** 50 000 ns versus at least 100 000 ns is a strict ~2× gap. The larger component sorts later alphabetically, so a ranking that fell back to key order would fail. The assertions are not weakened.
- **Limits:** no trace was kept, and the failing draft was never committed. The diagnostician did not re-run the original value; the conclusion comes from source reading.

### Commands

- `cargo nextest run --locked -p wyrd-benchmark --lib` (5 passed), including the three planned wyrd-benchmark selectors.
- `cargo nextest run --locked -p wyrd-testing --lib -E 'test(=release_server::tests::snapshot_conversion_rejects_incomplete_metric_evidence) | test(=capacity::tests::snapshot_windows_keep_classes_and_scans_apart)'`
- `cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::declared_workloads_fit_recording_bounds) | test(=tests::all_engine_consumers_read_benchmark_snapshots_without_prometheus) | test(=verification::run::tests::diagnostics_use_benchmark_distributions)'`
- `cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=parquet::variant_residual::tests::benchmark_recorder_keeps_fractional_residual_shares) | test(=oracle::tests::benchmark_scans_record_answered_queries_only)'`
- Gateway journey `resilience::benchmark_mode_keeps_capture_and_snapshot_counts_without_prometheus` under `with-test-postgres.sh`, `-P journey --run-ignored=all`.
- Owner suites under Postgres: `cargo nextest run --locked --no-fail-fast -p wyrd-benchmark -p wyrd-testing -p vala-bifrost-redux --lib --bin capacity`. Result: 1050 passed, 2 skipped.
- `cargo clippy --locked -p wyrd-benchmark -p wyrd-testing -p vala-bifrost-redux --all-features --all-targets -- -D warnings`.
- `RUSTDOCFLAGS='-D rustdoc::broken_intra_doc_links' cargo doc --locked --no-deps --document-private-items -p wyrd-benchmark`.
- `mise run fmt:check`; `git diff --check`.

### Limits

- **Standard-metric cap (1024) is not proven.** A rough source count of ~170 families gave an upper bound well above 1024, so an exact per-family count was not done. The two Oracle query-profile counters (`oracle_query_profile_{nanoseconds,count}_total`) take `event` labels from executed DataFusion plans, which source cannot bound. If the cap overflows, `Metrics::of_snapshot` and Population validation now make the collection fail rather than report false values. Recorded with the user's agreement.
- Footprint lifetimes count each lifetime's closed outcome sets; arbitrary external labels and every error combination are not enumerated, per the remediation.
- Failed and client-dropped queries no longer record scan facts in benchmark mode; their timings are unchanged. Production servers are unaffected because `record_scan` is a no-op outside benchmark mode.
- Non-goals stayed excluded: no benchmark run, no release benchmark build, no synthetic load, and no new dependency.
