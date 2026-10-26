---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-benchmark-experiments
spec_revision: 10
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-019, REQ-020, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011, AC-012, AC-013]
depends_on: [TASK-004, TASK-004-R2, TASK-004-R3]
---

# Complete and prove the L1/L2 benchmark measurements

## Outcome and Value

An engineer changing Bifrost, Verification or Gateway runs its L1 command and
gets a trustworthy comparison of server latency, CPU/operation and throughput
per core for the complete declared case matrix. A selected case gives quicker
optimization feedback but never claims the omitted cases passed. L2 preserves
sustained contention, overload and recovery evidence.

A Bifrost change now gets both hot and Parquet read evidence and both hot- and
Parquet-configured ingestion evidence. A throughput improvement cannot hide a
CPU-cost or tail-latency regression. Diagnostic traces and profiles explain
failures without becoming the baseline measurement configuration.

Completion also proves the measurements mean what their case says: they cover
the intended operation, server processes and interval, and remain associated
with that case in saved evidence. For example, a Bifrost query result must not
receive CPU from a separate Verification server or a previous case. An engineer
can follow a slow case from its record to server timing components, diagnostic
traces and a server CPU profile to locate waits or expensive functions.

## What Changes and What Must Stay True

Keep the existing shared experiment runner, production fixtures, public SDK/OTLP
workloads, correctness checks, clean-source rules, portability, profile option
and append-only evidence. No engine optimization, harness rewrite, new workload
DSL, optional dead-helper cleanup, baseline-format migration or CI dependency.

TASK-004-R2 owns the real-path smoke rewrite; TASK-004-R3 completes bounded
calibration and the separate normal-mode judging proof. Integrate both first;
reuse their tests and extend only missing case/metric coverage here. This task
does not create a second smoke or calibration runner. Revision 8's approval
persists; the smoke instructions follow the approved revision-10 contract.

The complete L1 matrix is:

| Experiment | Measured cases | Question answered |
|---|---|---|
| Bifrost writes-hot | Native Arrow, OTLP spans, OTLP logs, each c1/c64: 6 | Durable-ingest latency, rows/bytes per second and CPU cost with hot-retention settings. |
| Bifrost writes-parquet | Same six, production rotation/30 s retention | Durable-ingest performance with staging/publication settings and observed background CPU/backlog. Not publication latency or automatic proof of sustained throughput. |
| Bifrost hot reads | Point c1/c64; small aggregate c1; shredded span attribute c1: 4 | Unloaded/concurrent memory lookup, aggregation and telemetry filtering. |
| Bifrost Parquet reads | Point c1/c64; small aggregate c1; full scan c1; trace lookup c1/c64; shredded and long-tail span attribute c1: 8 | Storage-backed lookup/scaling, scan efficiency and telemetry layout behavior. Warmed cache, not cold-cache claims. |
| Verification direct-verify | Existing four assertion checks/2 KiB JSON at c1/c4/c16/c64: 4 | Server orchestration overhead and verification CPU/throughput scaling. Expected fail judgments are correct verified work. |
| Gateway overhead | Buffered metadata, streaming metadata and buffered payload capture, each c1/c4/c16/c64: 12 | Gateway header/end overhead, capture cost, CPU/call and scaling. |

Retain deterministic seeds/answer checks, 10M events + 1M spans read fixtures,
2M-row write generators, payload sizes, storage ratio samples, shredding/pruning
checks, existing peak/scaling derived rows and exact unit meanings. Check read
residence before measurement. Reuse prepared state only when equivalence and
ownership are proved. Smoke uses the real loader and required publication waits;
write-ack timing itself still ends at durable acknowledgement, not publication.
No shared mutable fixture across independently owned databases.

Every case reports server latency, CPU seconds, C/T busy cores, provisioned P,
W/T, (W/T)/P, W/C, C/W, peak memory and all outcome counts; write cases also report
rows and uncompressed/OTLP-wire bytes with explicit units. Query cases retain
scan facts and scaling; capture cases retain capture bytes/accounting. Phase
histograms guide diagnosis. Resources and work use matching windows; drain work
is separate. No favorable metric is inferred from an absent observation.

Keep a compact evidence mapping for every declared case: named operation and
timing boundary, outcome/cohort and phase, included server processes, resource
window, work/byte units, and corresponding record fields and artifact paths.
This is task evidence using the existing declarations/records, not a new
manifest format. Prove distinct measurement paths once with controlled inputs,
then use all-case real smoke to establish that each declaration is wired to the
proved path. A mapping or fabricated Repetition alone is not runtime proof.

CPU is actual user+system consumption of the declared server processes, with
driver, provider, profiler and Postgres processes excluded. Required server
replicas/workers are included once. Do not import counters from another process,
case or repetition. Work intentionally sharing that server, including required
background work, remains part of its CPU cost. Mixed L2 CPU is explicitly the
combined workload's CPU; never label it query-only or attribute CPU to individual
functions using elapsed timings. Profiles diagnose where that CPU went.

Latency and overhead retain the specification's server boundaries. Exclude only
the per-request union of declared Verifier/provider waits; do not subtract
percentiles. Keep timing groups, scan facts, work and bytes associated with the
correct case/admission phase. Incidental lanes, filler and visibility probes
cannot satisfy foreground populations: exactly reconcile required populations
and document the purpose, accounting scope and reason for any incidental group
that is not reconciled request by request. Do not claim unmeasured attribution.

Preserve 5 s + 3 x 10 s L1 windows, four Bifrost per-experiment budgets from the
spec, and the existing 15-minute budgets for other engines. Existing sample
floors still govern judged percentiles. Never reduce coverage, durations or
sample floors to fit a runtime budget; use recorded phase timings to fix setup
or lifecycle waste. Building and service prerequisites precede the experiment
clock. The isolated smoke prepass has its own declared budget/time; the full
clock starts afterward and includes full fixtures, pre/post host checks and
drain. Full-run windows/floors remain unchanged. Explicit smoke may shrink only
the controls allowed by REQ-008, with production criteria explicitly inapplicable
and their observed readings retained. Recorded production errors,
dropped/refused calls, shedding, wrong
answers and target misses do not fail smoke. Broken measurement/evidence
plumbing is INVALID. Smoke cannot qualify or supply a reference; normal judging
is proved separately with controlled results and exercised by full runs.

Keep three-repetition L2 workloads and exact declared rates/mixes, overloaded
phases, backlog/drain/recovery and capture accounting. Independent arrivals are
required when claiming offered rates; own/cancel every scheduler/request task.
A profiler's saving cannot delay drain or determine the workload budget.

Existing historical references keep their values/provenance but become advisory
when methodology differs. Establish clean compatible references as specified;
reference promotion is a reviewed TOML change, never automatic benchmark output.
Apply relative judgment to latency, CPU/work, work/CPU-s and throughput/P. Preserve
absolute objectives and noise-aware comparison; do not widen thresholds to pass.
Explicit --qualify-baseline L1 runs omit relative criteria, record
baseline-unavailable/regression-coverage=false and may PASS absolute/correctness
criteria. They require full clean ordinary configuration; partial/profiled/L2
use is rejected. Normal L1 with missing compatible references is INVALID.
L1 full-scan latency is descriptive; its throughput/core and CPU/query remain
judged, so short windows do not manufacture a scan-tail sample population.

## Approach

1. Expand existing declarations/dispatch and write configuration ownership to the
   full matrix, retaining existing read, Verification and Gateway cases.
2. Close metric/report and lifecycle consumers, including L2 independent arrivals,
   task cancellation, window boundaries and profiler/drain behavior.
3. Extend comparison identity, reference eligibility and relative judgment for
   all required dimensions with deterministic negative/noise cases; prove real
   measurement wiring and controlled regressions through the existing runner.
4. Update mise descriptions and the existing README/TESTING/configuration docs;
   prove direct and aggregate coverage, smoke and exit precedence.
5. On clean integrated source, run smoke, full L1 one engine at a time, repeat
   for replacement references and perform matched tracing overhead comparisons.
6. Supply real L2 evidence and a trace/profile investigation for each engine;
   final change review checks that evidence and owns broad production/language
   journeys once, rather than repeating them after every task scenario.

## Ordered Implementation Scenarios

### Scenario 1 — Complete matrix cannot silently omit a configuration

**Behavior.** Default Bifrost L1 executes 24 measured cases across four records;
Verification executes four and Gateway twelve. Writes aggregate runs both
configurations. Full/partial selection, params, storage state and derived rows
are explicit. A missing required case cannot produce a complete-suite PASS.

**RED.** Extend existing V1 matrix/dispatch tests to assert the exact matrix and
commands. Current declarations have one write configuration, so the added six
cases and aggregate dispatch should fail. Do not duplicate timing windows for
derived summaries.

**GREEN.** Extend existing engine declaration/setup owners and dispatch. Preserve
all existing correctness/SLO obligations. Rerun V1 before measured runs.

**REFACTOR.** Reuse existing generators, shared controls and levels dispatch;
remove only replaced declaration duplication, not workload coverage.

### Scenario 2 — Resource and latency reports explain each case

**Behavior.** Each case retains required metrics, per-repetition spread,
normalized denominators and timing precision. Hot/Parquet storage state and
write acknowledgement/publication distinctions are visible. Reports regenerate
from records; missing required facts are INVALID, never invented.

**RED.** Planned V2 exercises representative repetitions from all engine shapes,
including write bytes, scan/capture facts, separate CPU/P and late completions.
Current incomplete/new-mode conversions must fail explicit assertions.

**GREEN.** Complete the engine Repetition conversions and shared report fields
using TASK-004's histogram/population contract. Run V1/V2 and V6.

**REFACTOR.** Reuse common controls and one record/report path. Do not average
unlike query/storage classes or turn engine metrics into one generic score.

### Scenario 2a — Each measurement belongs to the work it claims to measure

**Behavior.** Controlled requests drive the actual engine measurement path,
through server instrumentation, snapshots, resource accounting, reconciliation
and saved records/reports. Cover every distinct boundary/transport/accounting
path; equivalent case variations share proof only when the case mapping shows
the same owner and conversion. Include native/OTLP spans/OTLP logs ingestion,
queries, direct/background Verification and buffered/streaming/capture Gateway
paths where declared. In particular, exercise the real Gateway ingress, not
only synthetic Observed values or a provider fixture in isolation.

| Measurement | Small controlled proof and expected observation |
|---|---|
| Latency and overhead | Compare controls with a known delay inside the named server boundary and a delay outside it. Only the included delay moves server latency; outside client delay does not. For Verification/provider waits, changing wait from zero to W moves elapsed and excluded wait together while residual overhead stays within a justified tolerance; zero-wait residual agrees with elapsed. Buffered/head/final streaming boundaries and overlapping waits retain their distinct meanings. Compare p50/max and HDR bounds against independently observed durations, allowing declared scheduling/timer error. Client duration to the corresponding outcome bounds the server interval; never compare different streaming endpoints. |
| Process and window attribution | A bounded known CPU operation in a declared server process moves server CPU, not driver CPU. A driver-only control moves driver CPU, not server CPU. Use independent process-counter observations or existing injected resource seams to prove an unrelated PID, a prior case and work before/after the window cannot enter the result, including summed worker scopes. Real controlled CPU work runs inside the benchmark process scope, never as synthetic host load; sleep duration is not a CPU reference. |
| Work, rows and bytes | A known successful population gives exact completed work and rows, with W/T, W/C, C/W and (W/T)/P recomputed from independent operands. Refused/failed/retried/late requests follow their declared accounting, not admitted=successful. Known payloads establish retained Arrow bytes and encoded OTLP protobuf bytes separately, plus required scan/capture accounting; source JSON size is not retained Arrow size. |

Record independent expected values, measured values, tolerances and reasons.
Tolerances are fixed before execution from clock/sampling precision and control
noise; they must still detect a wrong boundary, process, unit or window. A
green check must not derive its expected value from the result being checked.
Deterministic negative controls deliberately substitute a wrong case/process,
boundary or window at an existing test seam and must trip the assertions or
normal INVALID checks. Do not require fresh production switches or APIs for
test-only perturbations.

**RED.** Planned V2a extends TASK-004-R2's real-path tests where needed. Confirm
each new check fails for the diagnosed missing/wrong measurement, rather than
fixture setup. Already-correct paths use verification-only evidence; do not
manufacture source changes. Scripted resource negatives complement, and never
replace, real server/driver accounting and real engine requests.

**GREEN.** Repair only uncovered wiring at the existing owning instrument,
request context, resource window or record conversion. Run V2a, V1/V2 and
affected existing TASK-004-R2 proofs. V7 reads the persisted evidence from every
real case; V8/V9 establish ordinary full-size behavior.

**REFACTOR.** Reuse engine fixtures, process readers, timing populations and the
shared recorder. No parallel runner, general calibration framework, new
per-request production record or inferred per-function CPU accounting.

As part of this scenario, audit instrumentation on the benchmark measurement
paths only. For each timer or recording hook, identify its operation boundary,
existing owner and benchmark or production-monitoring consumer. Reuse existing
timing owners where their boundaries match; remove duplicate or unconsumed
instrumentation introduced by this benchmark change without losing required
observations. Keep necessary scheduling/deadline clocks distinct from operation
timers. Benchmark-only collection stays centralized and gated; verify its
disabled path avoids clock reads, histogram recording and label processing
performed solely for benchmarks. Use profiles for function-level investigation
rather than adding timers to every function. Preserve timers required by normal
production behavior, monitoring or correctness; unrelated instrumentation is
outside this task.

Record this bounded audit alongside the case mapping: source location, purpose,
consumer, gating where applicable, and retain/reuse/remove decision. This is
source/consumer inspection, so no manufactured RED is required. Any resulting
behavior change uses the affected focused tests and V2a/V7 to prove required
measurements survive; carry forward existing disabled-path proof when unchanged.

### Scenario 3 — Comparable regressions fail without manufacturing certainty

**Behavior.** Compatible references trigger the specified independent latency,
CPU/work and throughput/core comparisons. Proven regressions FAIL; percentage
boundaries and noise behave as specified; ambiguous precision or missing
required operands is INVALID. Cross-host/method/profiled/partial references
cannot silently judge a case. Reference promotion is explicit.

In addition to judgment arithmetic, controlled real workload regressions in
latency, CPU/work and throughput per provisioned core must reach the normal
judge and produce criterion FAIL/exit 1 against a compatible reference. The
unperturbed control must be valid and pass the compared criterion; an INVALID
run, smoke-only run or edited metric/record is not regression-detection proof.
Use the same ordinary full-run measurement rules, repetitions and sample floors,
with existing fixture/test-support controls. Cover each engine's conversion to
the judge and all three regression dimensions across the proof; a full Cartesian
matrix of engines, cases and perturbations is not required. Keep artifacts for
the control and regression, reference identity, perturbation, operands and
failing criterion. Diagnostic/calibration records never become promoted
performance references.

**RED.** Planned V3 adds throughput/P and latency regression, baseline identity,
quantization ambiguity and first-reference cases to existing comparison proof
V6. Current judgment does not cover all these dimensions.
Planned V3a adds the real-path controlled regression proof; retain an existing
passing equivalent instead of duplicating it. Choose perturbations large enough
to cross unchanged limits despite normal spread, without breaking validity.

**GREEN.** Extend existing judgment/selection and baseline metadata, preserving
original numeric values as historical evidence. Absolute SLO and correctness
misses still fail despite noisy relative results. Run V3/V6.
Run V3a through actual measured workloads, not manually supplied Repetitions.

**REFACTOR.** Reuse direction/spread arithmetic and baseline TOML structure;
avoid a new statistical framework or reference database.

### Scenario 4 — Sustained work stays owned and comparable

**Behavior.** Every L2 repetition runs the declared mixed workload/phase sequence
with real overlapping work, independent offered arrivals, matching resource
windows, bounded queues/drain/recovery and correct capture loss. Cancellation
cannot leave a scheduler admitting more work. Profile finalization does not
postpone drain or fabricate a budget overrun. Query rate makes scan sample
floors feasible. L2 calibration selects compatible writes-parquet results.

**RED.** Planned V4 injects scheduled-work/cancellation/phase-clock observations
at existing lifecycle seams and asserts no post-stop admission and complete
reconciliation. Extend the existing L2 declaration test in V1. Existing correct
behavior is verification-only, not an excuse to manufacture production churn.

**GREEN.** Fix only uncovered lifecycle/open-arrival gaps in current engine
owners, preserving declared mixes and limits. Run V1/V4 and focused affected
module tests. Supply actual sustained proof in V9 for integrated review to
assess; do not defer the first real L2 run beyond this task's evidence.

**REFACTOR.** Use established Tokio ownership/cancellation and existing profiler
lifecycle; remove detached work or duplicated cleanup rather than add another
scheduler framework.

### Scenario 5 — The documented manual workflow produces usable evidence

**Behavior.** Per-engine L1, Bifrost read/write subcommands and aggregates all run
mandatory smoke and retain individual records/exit precedence. A selected case
is partial. On clean source, full suites produce reproducible reports and allow
reviewed same-method references. Detailed tracing/profile diagnosis is optional
and explicitly differs from qualification configuration.

V7 proves all-case real smoke and persisted-output readback; exit 0 alone is
insufficient. Carry forward TASK-004-R2's unchanged selected-step/prepass failure
proof. V8 proves a useful trace/profile investigation for each engine.

**RED.** Extend V1 dispatch coverage and planned V5 to test qualification versus
partial/diagnostic records and incompatible reference promotion. A changed
README alone cannot establish runtime behavior.
Extend V5 to reject smoke as a reference even after relocation. V7 must expose
missing, mislabeled or unreadable artifacts from the real default case matrix.

**GREEN.** Close mise/CLI/record consumers, then execute V7/V8 sequentially. Report
Bifrost, Verification and Gateway results separately, including FAIL/INVALID;
retain their artifacts and diagnose failures rather than alter criteria.

**REFACTOR.** Keep one existing setup/runner/documentation route; no new setup
script, automatic commit/promotion, CI job or broad test gate is introduced.

## Acceptance Criteria

| Obligations | Observable proof |
|---|---|
| REQ-001/003/007/008/010, AC-001/009 | V1/V2 plus clean V7/V8 establish the exact full matrix, smoke, honest budgets, storage state and documented commands. |
| REQ-002/006/013/014, INV-004/005/008, AC-004/005/006 | V2/V6 and real records reconcile work/CPU/P/bytes, outcomes, windows, reports, missing data and historical schema support. |
| REQ-006/007, INV-004/005, AC-001/005/012 | V2a proves metric meaning with independent controls through each distinct real measurement path; all-case V7 maps each declaration to that proof and reads back its measurements. Wrong process/case/boundary/window controls fail, server/driver CPU remain disjoint, and mixed-workload CPU scope is explicit. |
| REQ-007/015, maintainer-directed instrumentation check | Scenario 2a's source/consumer audit accounts for timers/hooks on benchmark measurement paths, reuses matching owners, removes change-introduced duplication or unused instrumentation, and verifies benchmark-only collection is gated. Required production timers and benchmark observations remain intact; no repository-wide cleanup or per-function timer expansion. |
| REQ-011/012, AC-002/003/013 | V3/V5/V6 establish comparable references, all regression dimensions, noise/limit behavior and explicit promotion; V8 supplies real clean reference evidence. |
| REQ-020, AC-013 | V3a supplies valid unperturbed controls and real measured regressions reaching normal reference judgment: latency, CPU/work and throughput/P each FAIL without edited readings, weakened limits or INVALID standing in for detection. Each engine's measurement-to-judge path is exercised. |
| REQ-004/005/009, INV-001/002/003/007, AC-007/010 | V1/V4 and integrated V9 prove complete phase coverage, independent arrivals, cancellation/drain, correctness, audit/durability and no host-load/build contamination. |
| REQ-015/016, INV-009, AC-008/009 | Source/consumer/task inventory preserves one runner and retirement work; Linux real proof plus portability inspection covers unchanged commands/schema on supported platforms. |
| REQ-019/020, INV-006/010, AC-011/013 | V8's matched telemetry comparisons and documented trace/profile investigation preserve client/tenant policy, safe artifacts and normal measurement configuration. TASK-003 owns focused tenant timeline proof. |
| REQ-007/008/013/015, AC-012 | V7 exercises every default real L1/L2 case and phase and reproduces saved evidence/report. Production outcomes remain recorded without failing smoke; missing/inconsistent measurements or failed persistence/readback remain INVALID. Smoke cannot be a reference. TASK-004-R2/R3 supply the shared prepass, negative paths and separate controlled full-mode judging proof. |

## Implementation Reference

- `bin/capacity/query/mod.rs::l1` declares existing writes/hot/parquet cases;
  `query/isolated.rs` owns release setup and residency/settlement, and
  `query/workload.rs` / `query/telemetry.rs` own deterministic generators and
  expected answers. Extend configuration selection and existing step identities;
  do not regenerate or replace the datasets.
- `verification/mod.rs` and `verification/run.rs` own direct-verify and competing
  work; `gateway/mod.rs` / `gateway/capacity.rs` own three overhead shapes and
  capacity phases. Their existing declarations/tests are consumers, not a reason
  to invent a provider/Verifier compatibility matrix.
- `experiment.rs::judge_criterion`, baseline selection, Record/Report and
  `capacity.rs` resource readers already supply efficiency/spread/provenance.
  Extend these owners for new matching dimensions, not a second judgment engine.
- `levels.rs::execute/select/report`, capacity binary main and `mise.toml` own
  declaration selection, execution, record readback, Postgres wrappers and
  worst-exit aggregation. TASK-004-R2 replaces separate request-only smoke with
  this same normal runner in smoke mode; extend its coverage for the completed
  matrix and expand writes to two records. Correct the Gateway L2 mise
  description (payload capture) to match actual declared metadata capture rather
  than silently change workload.
- `query/competing.rs`, Verification run, Gateway scheduler/request tasks and
  `profile.rs` are the lifecycle consumers for the carried-forward review
  findings. Revalidate existing fixes; do not assume the old candidate is still
  current or redo completed work for its own sake.
- Existing `benchmarks/*/baselines.toml`, README and TESTING are the reference
  and discoverability owners. Preserve source values/history; annotate method
  incompatibility and add reviewed replacements linked to real clean records.
- `capacity.rs::Window/ResourceWindow/Population`, `release_server.rs::LocalServer`
  and `release_server/os` own measured stop, server PID/resource reads and timing
  cohorts; `driver_cpu_seconds` reads the calling process separately. Current
  engine conversions consume those readings. Extend their existing tests and
  TASK-004-R2 calibration proof for misattribution and independent controls;
  the synthetic accounting test alone cannot prove which live PID was read.
- `gateway_capture.rs::PacedUpstream/Pacing` already supplies real controlled
  buffered/streaming waits and is used by Gateway capacity's Driver. Gateway
  run's synthetic Observed tests exercise conversion only; supplement them by
  driving the actual ingress and recorder. Reuse existing Verification/Bifrost
  fixtures and test-support seams for equivalent controlled work, adding only
  missing in-owner support, not another transport or benchmark framework.
- `profile.rs::Profiler` attaches samply to declared server PIDs and records
  capture paths/notes; the experiment runner owns its lifecycle and optional
  failure behavior. Verify selected-case diagnostic use of these existing
  owners. Profiles cover their recorded capture interval; sampled traces explain
  individual flow and do not replace complete benchmark histograms.

## Verification and Evidence

All planned names are new; confirm actual selectors after adding and before RED.
Existing named selectors below were confirmed from source. Run Cargo sequentially
and never during benchmark measurements. No --allow-dirty qualification.

### V1 — Existing matrix, L2 and command regression tests

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::level_one_declares_complete_engine_coverage) | test(=tests::level_two_preserves_concurrent_work_and_recovery) | test(=tests::level_commands_preserve_records_and_exit_precedence)'
```

### V2 — Case metrics/report completeness (planned)

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::engine_cases_preserve_latency_cpu_and_core_throughput)'
```

### V3 — All relative regression dimensions (planned)

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::compatible_histogram_baselines_judge_all_regression_dimensions)'
```

### V2a / V3a — Real measurement integrity and controlled regression proof (corrected selectors)

These planned selectors belong to Postgres tests in the existing capacity binary,
using actual engine plans and the shared experiment runner. Confirm actual names
before RED. Reuse equivalent TASK-004-R2 tests by recording their exact selectors
and coverage instead of adding duplicates. Use real time for real-process proof,
not the paused clock used by declaration-only tests. Build the release server
and driver with `mise run bench:build` and provision repository prerequisites
before these manual checks; they are not new CI performance gates.

Both are `#[ignore]`d real-server proofs, like the TASK-004-R2 calibration, built in release with frame pointers:

```bash
RUSTFLAGS='-C force-frame-pointers=yes' CARGO_PROFILE_RELEASE_DEBUG=line-tables-only mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked --release --target-dir target/profiling -p wyrd-testing --bin capacity --run-ignored=only --no-capture -E 'test(=tests::pg_tests::engine_measurements_keep_boundaries_processes_and_windows)'
RUSTFLAGS='-C force-frame-pointers=yes' CARGO_PROFILE_RELEASE_DEBUG=line-tables-only mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked --release --target-dir target/profiling -p wyrd-testing --bin capacity --run-ignored=only --no-capture -E 'test(=tests::pg_tests::measured_regressions_reach_normal_reference_judgment)'
```

V2a keeps expected/observed operands and predeclared tolerances for Scenario 2a.
V3a uses valid ordinary full measurement rules against compatible references;
it must show the normal judge's criterion FAIL/exit 1 and persisted evidence,
not only a helper returning false. Existing test-support perturbations need no
new public CLI. Complete measurement-integrity checks and each engine's L1 smoke
before its V8 qualification; run L2 smoke before V9 once the required L1
calibration exists. No builds or independent tests run beside measurements.

### V4 — Owned sustained workload lifecycle (corrected selectors)

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=query::competing::tests::sustained_work_stops_admission_and_reconciles_drain) | test(=verification::run::tests::cancelled_arrivals_admit_nothing_and_abort_their_calls) | test(=gateway::capacity::tests::cancelled_scheduler_launches_nothing_and_its_calls_are_joined)'
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::profile_saving_never_delays_drain_or_budget)'
```

### V5 — Qualification/promotion exclusions (planned)

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::reference_qualification_rejects_partial_and_diagnostic_runs)'
```

### V6 — Existing judgment/accounting regressions and required checks

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::criteria_distinguish_regression_noise_and_invalidity) | test(=experiment::tests::baseline_selection_preserves_eligibility) | test(=experiment::tests::metrics_preserve_process_scope_and_units)'
mise run fmt
mise run lints
mise run check:deps
mise exec -- git diff --check
```

### V7 — Clean real-path release smoke and output readback

```bash
mise run bench:l1:bifrost -- --smoke
mise run bench:l1:verification -- --smoke
mise run bench:l1:gateway -- --smoke
mise run bench:l2:bifrost -- --smoke
mise run bench:l2:verification -- --smoke
mise run bench:l2:gateway -- --smoke
```

These --smoke invocations are supplied by TASK-004-R2. Run sequentially on the
final clean implementation, with no --steps: all four Bifrost L1 experiments,
every engine case and each L2 phase must appear in the resulting evidence.
`bench:smoke` remains a convenience aggregate of these same commands; no second
aggregate run is required. Resolve any L2 calibration prerequisite through the
normal declared compatibility rules; never invent or promote a smoke reference.

For each emitted JSON record, run the existing report reader, substituting the
printed path for `<record.json>`:

```bash
mise exec -- target/profiling/release/capacity report "<record.json>"
```

Compare the regenerated report with the original. Open its retained
`timings/<step>/<phase>.json` evidence, difference the snapshots and check counts,
percentile ranks/bounds/maxima and applicable scan facts against saved readings
and the case mapping. Check units, source/engine/experiment/step/phase identity,
CPU/work operands, required artifacts and explicit production-criterion
inapplicability with observed values retained.
All smoke output stays under `target/benchmarks/smoke/` and mode is smoke.
Record exact commands, source identity, artifact paths and observations per
plan. Exit 0 without these observations does not close V7. Smoke is PASS when
the plumbing works, including when it accurately records production errors,
dropped calls or target misses; those outcomes do not block full preparation.
A plumbing failure is INVALID and must be diagnosed and fixed before the full
run. Do not cite an older probe run. Carry forward TASK-004-R3's separate
controlled full-mode judgment/persistence/report/exit-code proof.

Use bench:build and repository prerequisites before timing, commit implementation
before qualification, then prove clean source. All new run outputs remain
untracked until intentionally reviewed/committed; preserve pre-existing records.
Do not build, test, lint or review concurrently with measurements.

### V8 — Full L1 and reference qualification

```bash
mise run bench:l1:bifrost -- --qualify-baseline
mise run bench:l1:verification -- --qualify-baseline
mise run bench:l1:gateway -- --qualify-baseline
```

Run in that order and report after each. The qualification flag is planned by this task, not an existing capability.
Repeat this exact sequence for the second clean reference suite, reviewing/committing synthetic records between runs as
needed for clean source. Do not auto-promote references. The record's measured
source SHA and method identify each repetition even if recording adds a later
commit. After reviewed reference promotion, exercise normal per-engine L1
without that flag and require real comparison operands/coverage. Preserve phase timings and fix setup/startup/settlement/drain waste if a
budget is exceeded; never change measured work or limits to obtain PASS.

For matched telemetry comparisons, use the same focused case and controlled
fixture with endpoint absent, normal operator sampling and sampled-client tenant
recording separately. Use existing configuration forwarding; record exact
resolved commands/environment symbolically in execution evidence before running,
without secrets. Do not present an invented CLI option as already implemented.
These diagnostic/control runs are not replacement references. Exercise selected
case plus existing --profile for documented diagnosis; return to normal
configuration before judging optimization.

For each engine, retain one selected-case diagnostic run using the documented
--steps/--profile and temporary operator tracing controls. Record the exact
resolved invocation after confirming real step IDs, return to normal settings
afterward, and explain one observed wait or expensive function using the record's
components, a matching request trace and a readable server profile. Confirm the
profile targets the declared server PID(s) and identify its capture interval.
This proves the artifacts are usable for investigation, not that all requests
have sampled traces or that a whole-run profile gives exact per-case CPU.
Missing/unusable profiler behavior remains nonfatal with an explicit note, but
that fallback alone cannot close the required successful Linux profile
demonstration. Diagnostic runs never qualify references.

### V9 — Full L2 evidence and once-only integrated qualification

TASK-005 supplies one actual Linux x86_64 full run of each engine's L2 on the
integrated candidate, after compatible L1 calibration is available:

```bash
mise run bench:l2:bifrost
mise run bench:l2:verification
mise run bench:l2:gateway
```

Retain records, timing evidence, report readback and phase/accounting observations
as for V7, with full durations, three repetitions and normal floors/judgments.
Report and diagnose FAIL/INVALID honestly; they do not establish a valid reference
or passing qualification. Final change review assesses these artifacts and
carries unchanged proof forward; it does not repeat valid runs without reason.

At final change review, run the integrated production
correctness/security/storage and existing Rust/Python/TypeScript client timeline
journeys required by changed surfaces, once using their owning setup lanes.
The integrated change reviewer owns this evidence closure; TASK-005 supplies
the records and exact command inventory. Carry forward unchanged passing
security/durability evidence only with explicit source/dependency justification.
The focused cross-language timeline families use the existing setup below;
these are existing selectors, not proposed new APIs or test frameworks:

```bash
mise run py:setup
mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_action_timeline.py::test_service_action_timeline_is_one_connected_tree'
mise run ts:build
mise run ts:build:testing
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/action-timeline.test.ts -t "sdk requests carry the active trace|service action timeline is one connected tree|another tenant cannot read the timeline"'
mise run check:tenant-isolation
```

TASK-003 V5/V6 supply the focused Rust timeline/tenant-negative and server
operator-rejection proofs. The storage HTTP seam, if touched dependencies
invalidate carried evidence, uses the existing local tests without cloud lanes:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features storage-emulator --test integration --test-threads=1 -E 'test(=storage_e2e::local_client_server_round_trip) | test(=storage_e2e::storage_routes_refuse_and_audit_an_unprivileged_caller) | test(=storage_e2e::local_upload_capability_rejects_raw_paths_and_bad_bytes)'"
```

This is the carried-forward real-evidence obligation, not a request to run gate
or full SDK journeys in every task. No benchmark becomes a CI/gate dependency.
Portability inspection establishes unchanged invocation/schema on macOS arm64
and Linux arm64; do not claim real runs on platforms not exercised.

Documentation uses static comparison with the actual matrix and one exercised
invocation per documented command family; no manufactured RED for prose. Keep
rustdoc on changed Rust items and precise # Errors/# Panics where applicable.

## Material Stop Conditions

Integrate TASK-004 and its TASK-004-R2/R3 remediations before execution. Revision 8
is already approved; do not reopen that approval because of its historical
header. Approved revision 10 owns the plumbing-only smoke behavior and
separate controlled judging proof. Stop for spec authority if
required cases cannot meet a declared budget without changing fixture/coverage,
if reference qualification needs different measurement boundaries, if an engine
optimization is required, or if a new dependency/feature/public API is necessary.
Do not delete failing cases, widen thresholds, relabel storage state, promote
invalid baselines or treat absent evidence as a passing regression check.

## Authority Links

- [Specification](../spec.md), approved revision 10 (revision 8's measurement decisions retained).
- [Smoke and residual timing remediation](../review/TASK-004-R2/TASK-004-R2-finish-stop-counters-and-otlp-cohorts.md).
- [Repository rules](../../../../AGENTS.md).
- [Workflow](../../../spec-driven-development.md).
- [Execution](../../../implementation-execution.md).
- [Testing](../../../../TESTING.md).
- [Bifrost authority](../../../../architecture/bifrost-design.md).
- [Trace-context authority](../../../../architecture/wyrd-design.md#trace-context--w3c-traceparent-and-tracestate).

## Implementation Evidence

### Diagnoses

**D1: the Isolated workload never created its log directory**

- **Symptom.** The first V2a run was INVALID: "drain or stop failed: No such file or directory".
- **Evidence.** `query/isolated.rs`, `Isolated::finish`, copies `server.log` into `self.logs`, and no code path creates that directory. A CLI run works only because the runner's `keep_timings` creates the experiment directory as a side effect.
- **Cause.** `Isolated::prepare` never creates its log directory, unlike `Competing` and Verification `Bench`.
- **Fix site.** `Isolated::prepare` now calls `create_dir_all` on `self.logs`. Gateway creates its directory in `shutdown`.

**D2: the release server stop blocked the runtime that drives client connections**

- **Symptom.** The second V2a run of writes-hot was INVALID. The server exited with status 70: "Bifrost shutdown deadline elapsed before role drain; shutdown was triggered by: hosted Verifier sandbox did not settle before the shutdown deadline". The CLI smoke of the same experiment passes.
- **Evidence.** With `WYRD_LOG=info,wyrd_server=debug`:
  - at `04:16:22.42`, every supervised task logs `task drained` except `id=Grpc`;
  - at `04:16:57.42` the log reads `drain deadline exceeded; aborting remaining tasks`, exactly the 35 s drain budget later;
  - `close_hosted` then runs past the deadline and is the first check to report false, so the sandbox reason in the error is a consequence, not the cause (`wyrd-server/src/app/server.rs`, around lines 776–861).
- **Cause.**
  - `LocalServer::stop` and `terminate` in `release_server.rs` polled `try_wait` with `std::thread::sleep`.
  - Each async `finish` dropped its OTLP `Channel` and then called `stop` without yielding.
  - On the test's current-thread runtime, the tonic connection task could not run, so the client never closed its connection or answered the server's graceful GOAWAY. `serve_grpc_with_listener` therefore never completed.
  - The CLI's multi-thread runtime hid this, because other workers drove the connection; even there, `stop` still blocked a worker for up to 45 s.
- **Diagnostician.** A fresh read-only diagnostician, given only the failing command, the trace and the diff, reached the same cause and fix site independently. It named the multi-thread test flavor as a stopgap that would hide the problem.
- **Fix site.**
  - `LocalServer::stop`, `terminate` and `restart` now wait with `tokio::time::sleep(..).await`.
  - The callers checked and updated are:
    - `query/isolated.rs` `Isolated::finish`
    - `query/competing.rs` `Competing::finish`
    - `verification/run.rs` `Bench::finish`
    - `gateway/mod.rs` `shutdown`
  - `Drop` keeps its synchronous kill for abnormal exits.

**D3: six release servers shared one wrapper database**

- **Symptom.** Once D2 was fixed, writes-parquet was INVALID at boot. The server reported "built-in tables could not be ensured … the location is outside the active warehouse", and the location it named was the previous run's storage root.
- **Evidence.** The Iceberg catalog rows in `wyrd` still pointed at writes-hot's warehouse. `LocalServer::start` documents that "a retry starts from a fresh database". The CLI gets one by running each experiment under its own `with-test-postgres.sh` (`levels::spawn`).
- **Cause.** The test starts all six engine runs under one wrapper invocation and one database.
- **Fix site.** In the test, `tests::pg_tests::fresh_database` recreates `wyrd` before each run as the wrapper hands it over: owned by `wyrd_platform`, with the same `scripts/postgres/test-database-setup.sql`. Production code, the four engine plans and the CLI are unchanged.

**D4: three test references were wrong, not the measurements**

- **Symptom.** With D3 fixed, every experiment ran. Three reference failures remained.
- **Diagnostician.** A fresh read-only diagnostician, working from source, confirmed every cause and verdict below.
- **Server rows above client rows.**
  - Evidence: the server rows counter read exactly `rows + late_rows + requests`. For example, otlp-spans-c1 read 53352 against 53248 = 104 × 512, and native-write-c64 read 13697860 against 13697024 = 836 × 16384.
  - Cause: every authorized write stages one `bifrost.system.audit_log` row on the Scribe outbox (`gate/mod.rs` `stage_write_decision`). The row reaches the unlabelled memtable counter, and `retained_mib` declares that it includes server-internal rows. The audit row is written after the request answers, so a later run read up to 64 rows short at the last scrape.
  - Reference now used:
    - the wrapper reads its before and after snapshots only once `outbox_pending{outbox="scribe"}` is zero, within a 10 s deadline;
    - the exact reference is `rows + late_rows + requests + late_requests`;
    - the check requires `errors == 0`, because a failed request still stages its audit row.
- **Gateway `chat-stream`.**
  - Evidence: `recorded` read exactly 2 × (requests + late_requests).
  - Cause: a stream records two boundaries, `gateway.stream.head` and `gateway.stream.end`.
  - Reference now used: each boundary's recorded sum equals the calls.
- **Derived steps (`*-peak`, `*-scaling`).**
  - Cause: these have `Status::Derived`. The runner never measures them and keeps no timing file for them.
  - Reference now used: the test checks only `Status::Measured` steps, so a measured step that never ran still fails.

**D5: writes-parquet server CPU does not follow a window's own requests**

- **Symptom.** In three consecutive runs, writes-parquet `native-write-c1` failed the server side of the spin control. Spun server CPU per work read 0.0196, 0.0223 and 0.0176, against a ±25 % reference on the plain rate of about 0.013. Every other case held. The same native path held in writes-hot.
- **Evidence.**
  - In one run the server CPU of the plain, spun and scraped windows was 1.35, 1.74 and 5.66 s for equal work. `published_files` read 0, 0 and 1. `staged_backlog_mib` at each stop read 41.4, 82.8 and 41.4.
  - So one default-size memtable staged inside the spun window, and the scraped window published.
  - A one-client native writer reaches the same rotation point in the same window on every run, which is why the failure was deterministic.
- **Cause.** Under `PARQUET_ENV` (default sizes), a window pays for the staging and publication it causes, as the declaration states. Its server CPU is therefore not proportional to its own requests. The server-side checks compare windows at the plain window's CPU per unit of work, so on writes-parquet the spin check fails falsely and the scrape check passes vacuously. The burn runs in the driver process and cannot enter the server PID's counter.
- **Fix site.**
  - `step_holds` runs the server-side spin and scrape checks only where server CPU is proportional, which is every case except writes-parquet.
  - The driver side of the spin check and every negative control still run there.
  - writes-hot proves the same server-process accounting: the same `LocalServer::cpu_usec` owner, the same transports and the same conversion.

**D6: the trimmed qualification declaration had no criterion**

- **Symptom.** The first V3a run panicked before measuring: `declaration.missing`, "no criterion is declared".
- **Evidence.** The writes-hot `no-errors` Limit names all six write steps. The test kept only criteria whose steps all equal the kept step, and `Declaration::compare` on a qualify run then dropped every relative criterion.
- **Cause.** The test's trim dropped every criterion that names the kept step together with others, instead of narrowing it to that step.
- **Fix site.** In the test, `judged` narrows each criterion's steps to the kept step and drops only those that never named it. Production code is unchanged.

**D7: the full writes-hot c64 window exhausts host memory**

- **Symptom.** The second V3a run was killed by the session's memory-pressure reaper 50 s in. Run 3 completed.
- **Evidence.** A 5 s memory log next to run 3 recorded the following:
  - `wyrd-server` RSS reached 58.9 GB, and available host memory fell to 182 MB of 91 GiB.
  - The record showed a memtable backlog of up to 45 GiB and 48.9 GiB of sampled peak memory.
- **Cause.** Under the hot settings every row stays in a memtable. At 64 clients the full L1 timing, a 5 s warmup and three 10 s windows at about 3.5 M rows/s, holds tens of GiB.
- **Fix site.** None in the test, which now judges `native-write-c1`; run 6 held at least 46 GB available. This is the same full L1 that V8 runs: a writes-hot suite on this 91 GiB host is close to the memory limit, and V8 must run with no other memory load.

**D8: the Gateway L1 driver kept every upstream request**

- **Symptom.** `mise run bench:l1:gateway -- --qualify-baseline` was stopped twice by the memory watchdog.
- **Evidence.** A 2 s sampler of process RSS showed the following:
  - The smoke child's `capacity` grew to 18.4 GB in about 40 s.
  - In the full run, the parent `capacity` grew from 0.6 GB to 21.7 GB in 2 min, from the first chat steps onwards.
  - `wyrd-server` stayed at 1–2 GB.
  - Memory returned as soon as the driver exited.
- **Cause.** The L1 upstream was a wiremock 0.6.5 `MockServer`. `MountedMock::matches` (`mounted_mock.rs:53-58`) pushes `request.clone()` for every matched request, whether or not recording is on; `disable_request_recording` turns off only the server-wide list. Each retained request also pins hyper's read buffer through its header bytes. That is about 10–35 KiB per 1 KiB chat call and hundreds of KiB per 64 KiB capture call, never freed during a run.
- **Diagnostician.** A fresh read-only diagnostician, given only the command, the trace and the diff, found the same cause from source. It confirmed that no other engine driver uses wiremock, and named the existing bounded `PacedUpstream` as the fix site.
- **Fix site.**
  - `gateway/run.rs` now serves the L1 cases from two `PacedUpstream`s with zero delays and `bound` 0: one answers 1 KiB completions and streams, the other 64 KiB completions.
  - `Tenant::provision` takes one base URL per deployment prefix. L2 passes its one upstream for all three.
  - `PacedUpstream` skips zero waits, so an instant answer costs no timer tick. L2's nonzero pacing is unchanged.

### V2a evidence

The V2a command above, run after `mise run bench:build` on a quiet host, gave PASS in 772.6 s. 40 measured L1 cases held (4+4+12+8+6+6):
- writes-hot 6
- writes-parquet 6
- hot 4
- parquet 8
- direct-verify 4
- overhead 12 (`chat`, `chat-capture`, `chat-stream` at c1, c4, c16 and c64)

| Measurement | Independent reference (fixed before execution) | Result |
|---|---|---|
| Window | 3 s real clock, ±0.05 s | PASS, all cases |
| Latency boundary | client p50 round trip ≥ server boundary p50 (upper bound) | PASS, all cases |
| Population | each timing boundary's Σ `recorded` = completions in and after the window | PASS, including both boundaries of a stream |
| Rows | server memtable rows Δ, after the Scribe outbox settles = `rows + late_rows + requests + late_requests` (one audit row per authorized request), with `errors = 0`; exact | PASS, every write window |
| Bytes | native writes have no wire bytes; OTLP wire bytes and retained Arrow bytes are both positive and distinct | PASS |
| Driver CPU | a 1 s blocking burn in the driver moves driver CPU by [0.8 s, 1 s + 0.02 s + 25 % of plain] | PASS, all one-client cases |
| Server CPU | after the burn, server CPU per work stays within ±25 % of plain; a sequential `/metrics` scrape loop adds ≥ 30 % of its busy time as server CPU at the plain rate, and less driver CPU than server CPU | PASS wherever server CPU is proportional (D5) |
| Negative controls | window read as ms; driver and server CPU swapped; client p50 / 1e3; another case's reconciliation; a plain window as the scraped one | every substitution trips its check |

Fast lanes after the change:
- `cargo nextest run --locked -p wyrd-testing --bin capacity`: 44 passed.
- `-p wyrd-testing --lib -E 'test(/release_server/) | test(/experiment::/)'`: 20 passed.
- `mise run lints` and `git diff --check`: clean.

### V3a evidence

The V3a command above, run from clean committed source (`6d4316122`) after `mise run bench:build` on a quiet host, gave PASS in 773.1 s. For each engine, `tests::pg_tests::measured_regressions_reach_normal_reference_judgment` does the following:
- runs the engine's ordinary full L1 plan: a 5 s warmup and three 10 s repetitions, the full fixtures and sample floors, unchanged limits, no `--allow-dirty`, `--smoke`, selection or profile;
- trims the declaration to one measured case, narrowing each criterion to that case (D6);
- runs two `--qualify-baseline` suites, each from a fresh database;
- promotes them with `Baselines::promote`, which checks every qualification refusal and requires the same method identity;
- runs an unperturbed normal control against those references;
- runs the same normal run with 32 sequential `/metrics` scrape loops against the server over each window.

The perturbation is real server work that competes with the measured requests. Nothing is edited after measurement: the readings, operands and limits are the runner's own. Every record is published, read back, and rendered identically.

| Engine, case | Control | Latency | CPU / work | Throughput / P | Record |
|---|---|---|---|---|---|
| Bifrost writes-hot `native-write-c1` | PASS, every reference | p95 (16.753, 16.761] ms vs 12.632: FAIL | 31.14 vs 0.443 µs/row, spread 3.52: FAIL | 15870.9 vs 17891.3 rows/s/vCPU, spread 256.5: FAIL | FAIL, exit 1 |
| Verification `direct-verify-c1` | PASS, every reference | overhead p99 (0.405, 0.406] ms vs 0.303: FAIL | 7893.0 vs 347.6 µs/verification, spread 73.2: FAIL | 61.16 vs 126.83 verifications/s/vCPU, spread 2.99: FAIL | FAIL, exit 1 |
| Gateway `chat.c1` | PASS, every reference | overhead p99 (0.41, 0.411] ms vs 0.264: FAIL | 8584.9 vs 486.3 µs/request, spread 186.6: FAIL | 52.82 vs 130.31 requests/s/vCPU, spread 4.62: FAIL | FAIL, exit 1 |

Each FAIL is the normal judge's own `Rule::Regression` reason, "regressed against reference.<experiment>.<step>.<metric>", and none is INVALID.
- **Per engine,** the test requires the control to PASS every judged dimension, the perturbed run to FAIL with exit code 1, and at least one dimension to regress.
- **Across the proof,** it requires each of the three dimensions to regress, which is what Scenario 3 requires.
- **In run 6,** all nine regressed. In run 3, Bifrost c64 latency and throughput did not: about 15 of 32 cores were busy, so the storm's server CPU did not reach the write path's latency.

The test uses `#[tokio::test(flavor = "multi_thread")]`, the CLI's runtime. D2's stop fix does not depend on the flavor.

**Findings for V8, measured unperturbed in these qualification attempts:**
- **Bifrost hot `point-c64`** misses its absolute SLO on this host: median p99 (7.496, 7.5] ms against `< 5` ms (`point-p99-slo`). A normal hot L1 FAILs on this alone.
- **Verification `direct-verify-c64`** misses its SLO: overhead p99 (1.346, 1.347] ms against `< 0.5` ms. It also refuses 50630–78677 calls per window with `WYRD_VERIFICATION_429_STAGING_FULL`, which `no-errors` counts. A normal direct-verify L1 FAILs, and the c64 case cannot qualify a reference.
- **Bifrost writes-hot c64** needs about 59 GB of server memory (D7).

These are measured product behavior, not defects in V3a. They are left for V8 to report and route, with no limit changed.

### V8 stop: two engines cannot qualify references at their declared limits

The first V8 suite was run with the task's own commands, on clean source `5174ad862`, after `mise run bench:build` on a quiet host. Each smoke prepass passed. Both full runs FAILed on unchanged absolute limits, so neither record can qualify a reference (`qualification_refusals`: verdict FAIL).

| Command | Record (untracked) | Failed criteria |
|---|---|---|
| `mise run bench:l1:verification -- --qualify-baseline` (exit 1) | `benchmarks/verification/direct-verify/20261026T071425Z-5174ad862.json` | `overhead-p99`, `direct-verify-c64`: median (1.365, 1.366] ms misses `< 0.5`. `no-errors`: 69087 `WYRD_VERIFICATION_429_STAGING_FULL` refusals in repetition 1. |
| `mise run bench:l1:bifrost:hot -- --qualify-baseline` (exit 1) | `benchmarks/bifrost/hot/20261026T071805Z-5174ad862.json` | `point-p99-slo`, `point-c64`: median (7.41, 7.414] ms misses `< 5`. |

These repeat the V3a observations: (1.346, 1.347] ms with 50630–78677 refusals per window, and (7.496, 7.5] ms.

Material Stop Conditions apply. Making either case PASS would take one of two things:
- an engine optimization: Verification staging admission and overhead at 64 clients, and the hot point-lookup tail;
- a changed limit or coverage, which this task forbids ("never change measured work or limits to obtain PASS"; "do not delete failing cases, widen thresholds").

Without two passing suites there are no Verification or hot references. The normal L1 for those engines therefore stays INVALID (no reviewed reference) or FAIL (limits).

Not run, pending the spec decision:
- V7 on the final implementation;
- the second V8 suite and the remaining V8 commands (Bifrost writes and parquet, Gateway);
- promotion, normal L1, the telemetry comparisons and diagnostics;
- V9;
- the V6 count update.

Verdict: SPEC_REVISION_REQUIRED. The decision for `$wyrd-spec`:
- whether TASK-005 must first carry the engine optimizations for these two SLOs, which the spec currently leaves out of scope;
- or whether the declared limits or coverage for `direct-verify-c64` and hot `point-c64` change on this reference host.

**The full writes-hot suite does not fit this host's memory.**
- `mise run bench:l1:bifrost:writes:hot -- --qualify-baseline` on clean `53b53f594` passed its smoke prepass. Its full run drove available memory down from 53 GB to 1.2 GB of 91 GiB (D7).
- I stopped it then, before the host's out-of-memory killer acted, so no record was written.
- Writes-parquet, parquet and Gateway were not reached.
- Every row of the declared full work stays in the server's memtables: 2M-row fixture, 5 s warmup and 3 × 10 s windows at 64 clients.
- Completing it on this host would take a smaller fixture, shorter windows or other memtable settings, which are changes to fixture or coverage. Running it on a larger host would make that host the reference host.
- The spec decision above should cover this as well.

**The rest of the first V8 suite, on clean `69f3a066d`.** Each smoke prepass passed.

| Command | Record (untracked) | Result |
|---|---|---|
| `mise run bench:l1:bifrost:writes:parquet -- --qualify-baseline` (exit 1) | `benchmarks/bifrost/writes-parquet/20261026T073045Z-69f3a066d.json` | FAIL: `otlp-spans-peak-floor`, median 15.218 wire MiB per server CPU-second misses `≥ 30`. Lowest available memory 10.2 GB. |
| `mise run bench:l1:bifrost:parquet -- --qualify-baseline` (exit 1) | `benchmarks/bifrost/parquet/20261026T073846Z-69f3a066d.json` | FAIL: `point-p99-slo` at `point-c64`, median (13.976, 13.984] ms misses `< 5`. |
| `mise run bench:l1:gateway -- --qualify-baseline` on `2840dfbf6`, after D8 (exit 1) | `benchmarks/gateway/overhead/20261026T080639Z-2840dfbf6.json` | FAIL on two criteria. **`overhead-p99`:** `chat.c64` median (0.638, 0.638] ms and `chat-capture.c64` (4.342, 4.346] ms both miss `< 0.5`. **`capture-shed`:** captures were shed at the outbox limit, 265 at `chat.c64`, 8785 at `chat-capture.c4`, 135671 at `chat-capture.c16` and 177609 at `chat-capture.c64`. The driver peaked at 307 MB RSS, against more than 21 GB before D8. |

No engine has a passing qualification suite. Declared absolute limits now block reference qualification on all three engines:
- **Bifrost:** hot and parquet point p99, the writes-parquet OTLP spans floor, and the writes-hot memory fit;
- **Verification:** overhead p99 and staging refusals at 64 clients;
- **Gateway:** overhead p99 at 64 clients and capture shedding.

The SPEC_REVISION_REQUIRED verdict above covers all of these.

### V6 evidence on `74b23897f`

The three named `experiment::tests` selectors passed (3/3). `mise run fmt`, `mise run lints`, `mise run check:deps` and `git diff --check` were all clean.

### V7 evidence: final implementation, clean `74b23897f`

`mise run bench:build` ran first. Then the six V7 commands ran in order with no `--steps`, on a quiet host, with nothing else building or measuring. Each exited 0 with verdict PASS. Every record has mode `smoke`, `dirty: false`, no reasons, and every criterion verdict `null` (inapplicable in a smoke). Lowest available memory was 33.8 GB.

| Command | Record under `target/benchmarks/smoke/` | Steps | Criteria | Timing phases |
|---|---|---|---|---|
| `bench:l1:bifrost -- --smoke` | `bifrost/writes-hot/20261026T081753Z-74b23897f.json` | 9 | 75 | warmup, repetition-1 |
| | `bifrost/writes-parquet/20261026T081901Z-74b23897f.json` | 9 | 75 | warmup, repetition-1 |
| | `bifrost/hot/20261026T082201Z-74b23897f.json` | 5 | 31 | warmup, repetition-1 |
| | `bifrost/parquet/20261026T082248Z-74b23897f.json` | 10 | 74 | warmup, repetition-1 |
| `bench:l1:verification -- --smoke` | `verification/direct-verify/20261026T082454Z-74b23897f.json` | 4 | 25 | warmup, repetition-1 |
| `bench:l1:gateway -- --smoke` | `gateway/overhead/20261026T082542Z-74b23897f.json` | 12 | 136 | warmup, repetition-1 |
| `bench:l2:bifrost -- --smoke` | `bifrost/competing/20261026T082705Z-74b23897f.json` | 1 | 30 | capacity, overload, recovery |
| `bench:l2:verification -- --smoke` | `verification/competing/20261026T082921Z-74b23897f.json` | 1 | 12 | capacity, overload, recovery |
| `bench:l2:gateway -- --smoke` | `gateway/capacity/20261026T083033Z-74b23897f.json` | 1 | 16 | capacity, overload, recovery |

**Readback.**
- **Report.** `target/profiling/release/capacity report <record.json>` exited 0 for all nine records. Each regenerated report appears verbatim, as one block, in the output its run printed.
- **Timing evidence.**
  - **Method.** For each measured repetition, I took the difference between the `after` and `before` snapshots in `timings/<step>/<phase>.json`, series by series and histogram by histogram (`elapsed`, `excluded`, `residual`). Then I recomputed, from those differences, every recorded reading that carries a rank.
  - **Counts.** No bucket went negative. Every series' count difference equals its histogram population.
  - **Rank readings.** All 264 rank-bearing readings (latency, overhead, upstream, verifier, max) reproduce exactly from their series:
    - their sample count equals the series difference, and their value equals the bucket at the recorded rank;
    - each `above` is the bucket's lower bound;
    - L2 Bifrost `write_ack_p99_ms` is the merge of the per-phase `bifrost.otlp.traces`, `bifrost.otlp.logs` and `bifrost.gate.write` success series, as `read_ingest` declares.
  - **Derived steps.** Scaling and peak steps (`point-scaling`, `span-trace-scaling`, `*-peak`) have status `derived`, no process and no timing file. Their readings come from their base steps.
- **Production outcomes recorded without failing the smoke.** `direct-verify-c64` read p99 (1.467, 1.468] ms with 11,462 refusals, and the CPU/work operands are present: 64.53 server CPU-seconds, 123,623 verifications, 521.99 µs per verification.

### V9 evidence: full L2 on clean `02f55beb8`

The three V9 commands ran in order on a quiet host, each with its smoke prepass (all PASS). Each record is mode `full`, `dirty: false`, 3 × 360 s repetitions. Lowest available memory was 21.7 GB.

| Command | Record (untracked) | Verdict | Elapsed |
|---|---|---|---|
| `mise run bench:l2:bifrost` (exit 2) | `benchmarks/bifrost/competing/20261026T083728Z-02f55beb8.json` | INVALID | 1567 s |
| `mise run bench:l2:verification` (exit 0) | `benchmarks/verification/competing/20261026T090451Z-02f55beb8.json` | PASS | 1229 s |
| `mise run bench:l2:gateway` (exit 0) | `benchmarks/gateway/capacity/20261026T092636Z-02f55beb8.json` | PASS | 1232 s |

**Readback.** It used the same method as V7, from each record's `artifacts.directory`.
- **Report.** `capacity report` exited 0 for each record and reproduced its printed report verbatim.
- **Timing evidence.**
  - All 105 rank readings reproduce from the timing snapshot differences, with no negative bucket. That is 48 for Bifrost, 21 for Verification and 36 for Gateway.
  - Bifrost `write_ack_p99_ms` reproduces from the merged per-phase write series in every repetition.
  - Every repetition's scheduled, sent and completed arrivals are recorded.

**Verification L2: PASS.**
- Direct overhead p99 was (0.395, 0.396] ms at capacity, (0.409, 0.409] in overload and (0.398, 0.399] in recovery.
- Backlog growth was at most 0.11 s, drain took 0.116 s, and there were 0 errors.
- All 270,000 scheduled arrivals per repetition were sent and completed.

**Gateway L2: PASS.**
- Overhead p99 was (0.332, 0.366] ms.
- Completion p99 was (102.1, 102.3] ms, first-token p99 (252.2, 252.3] ms and stream p99 (907.0, 908.1] ms.
- Every limit was met, with only the declared refusals.

**Bifrost L2: INVALID, honestly reported and not fixed.**
- **Calibration.** It calibrated from the newest compatible writes-parquet record, the FAIL qualification record at `69f3a066d`: 57,313 mixed rows per CPU-second × 0.7 headroom × 8 server cores = 320,955 rows/s offered.
- **What the server delivered.** Under competing queries and Forge compaction it delivered 11,884 rows per server CPU-second, about 4.8× less than the isolated calibration. It used 20.1 of 32 cores and peaked at 19.3 GiB.
- **Validity.** The driver reached its 4,096-pending bound and missed 35,022–38,083 of 181,947 scheduled ingest arrivals per repetition. Send-lateness p99 was 8.4–8.9 ms against a 5 ms limit. These make the run INVALID.
- **Failed limits:**
  - write-ack p99 (6530.5, 6534.7] ms at capacity and (6291.5, 6295.7] ms in recovery, against ≤ 100;
  - point p99 (111.3, 111.3] ms and (105.5, 105.6] ms, against < 5;
  - backlog 12.3 s, against ≤ 1;
  - 13,249 errors;
  - queue refusals: headroom 256,780, ack-delay 107,616, in-flight 86,567;
  - ack-delay drain 62.0 s, against ≤ 60.
- **Too few samples.** `million-p95` and `scan-p95` had 90–180 samples behind p95, against the required 200.
- **Cause.** The calibration takes isolated L1 capacity as the basis for 70% of contended capacity. On this host the contended server cannot absorb that rate.
- **What it would take to fix.** Making it valid would take one of three things:
  - a different calibration basis or headroom, which is a change to the measured work;
  - engine optimization of ingest under query and compaction contention;
  - a different `--server-cores` declaration.
- **Routing.** All three route to the spec decision below, not to a local fix.

### V8 evidence: telemetry comparison and per-engine diagnostics, clean `300c60a1d`

**Setup.**
- **Profiler.** `samply` 0.13.1 was installed with `cargo install --locked samply`. `kernel.perf_event_paranoid` is 1.
- **Trace sink.** A local OpenTelemetry collector (`otel/opentelemetry-collector-contrib:0.139.0`) received OTLP gRPC on `127.0.0.1:4317` and wrote to a file exporter under `target/otel-sink/`.
- **Configuration forwarding.** Configuration reached the server through the existing `WYRD_*` forwarding. Each record's `command.config` holds it, with secrets shown as `<set>`.
- **Order.** The runs below went one at a time. The record for each is under `benchmarks/` (untracked). Every one exits 2: with no reviewed references, each `reference.*` criterion is INVALID (`no reviewed reference with this id`). Each run's smoke prepass passed.

| Run | Resolved invocation | Record |
|---|---|---|
| Endpoint absent | `mise run bench:l1:verification -- --steps direct-verify-c64` | `verification/direct-verify/20261026T095136Z-300c60a1d.json` |
| Ordinary 1% export | `WYRD_OTLP_ENDPOINT=http://127.0.0.1:4317 WYRD_OTLP_SAMPLE_RATIO=0.01 mise run bench:l1:verification -- --steps direct-verify-c64` | `verification/direct-verify/20261026T095323Z-300c60a1d.json` |
| Verification diagnostic | the same two variables, plus `mise run bench:l1:verification -- --profile --steps direct-verify-c64` | `verification/direct-verify/20261026T095510Z-300c60a1d.json` |
| Bifrost diagnostic | the same two variables, plus `mise run bench:l1:bifrost:hot -- --profile --steps point-c64` | `bifrost/hot/20261026T095656Z-300c60a1d.json` |
| Gateway diagnostic | the same two variables, plus `mise run bench:l1:gateway -- --profile --steps chat-capture.c64` | `gateway/overhead/20261026T095850Z-300c60a1d.json` |

The variables applied only to those invocations, and the collector was removed afterwards. `Record::qualification_refusals` refuses every exporting or profiled record as "a diagnostic configuration" or "a profiled run", so none can become a reference.

**Matched telemetry comparison: `direct-verify-c64`, same build and fixture.** Values are medians over 3 repetitions, as recorded, with nothing subtracted.

| Configuration | Overhead p99 ms | CPU µs/verification | Verifications/s/vCPU | Cores |
|---|---|---|---|---|
| Endpoint absent | 1.335 [1.33, 1.371] | 499.4 [492.9, 511.6] | 1336 [1298, 1357] | 21.36 |
| 1% export | 1.357 [1.328, 1.378] | 502.3 [500.7, 507.7] | 1324 [1318, 1326] | 21.31 |

Ordinary 1% export cost about +0.6% CPU per verification and −0.9% throughput per vCPU. Its p99 difference lies inside the repetition spread. 167 OTLP export batches reached the collector.

The third configuration, sampled-client tenant recording, was **not run**:
- The capacity driver has no way to send sampled client trace context (`traceparent`). No existing option or configuration does this.
- Adding one would be a new driver capability, which Material Stop Conditions route to the spec. The task also forbids presenting an invented option as implemented.
- It is part of the spec decision.

**Per-engine investigation.** In each case:
- The profile is `profiles/profile-<pid>.json.gz` in the record's `artifacts.directory`, and `artifacts.profile.complete` is true.
- The samply log reads "Recording process with PID <pid>". That PID equals the server PID in every timing snapshot of the record.
- Sampling interval is 1.001 ms, and each capture spans 35 s: the 5 s warmup plus 3 × 10 s.
- I symbolicated the profiles offline with `addr2line` against `target/profiling/release/wyrd-server`, which is built with frame pointers and line tables.
- The Bifrost run overlapped my symbolication of the first profile. Diagnostic runs are not judged, so this changes no verdict.

- **Verification, `direct-verify-c64`.**
  - **Sampled traces.** 18,086 `request` spans: p50 0.330 ms, p99 1.556 ms.
  - **Slow request.** Trace `001f5ba56bb9d1a902163d08b4d7ac16` (p99) spends 1.116 ms of its 1.556 ms in `verification.engine` and 0.003 ms in `verification.load`. The p50 trace spends 0.069 ms in `verification.engine`.
  - **The wait.** Across all sampled spans, `verification.engine` is p50 0.056 ms and p99 0.919 ms. That span wraps the async `engine.run` (`components/verification/service.rs:596`). The evaluator itself (`vala_verify::assertion::evaluate_single`) is only 0.2% of profile samples. The tail is therefore waiting inside the engine future, not evaluation CPU.
  - **CPU.** Inclusive:
    - `authenticate_tenant_http`: 55.7%, wrapping the request;
    - the `verification/routes::execute` closure: 45.8%;
    - `verify_access_token`: 3.3%;
    - Scribe `execute_persistence_operation`: 6.2%;
    - audit `project_audit_event`: 5.2%;
    - `wyrd_queue::variant::append_raw`: 6.6%.

    The allocator is about 15% of self samples (`mi_*`).
  - **Refusals.** Status codes: 16,162 × 200 and 1,921 × 429, which are the staging refusals behind `no-errors`.
- **Bifrost hot, `point-c64`.**
  - **Sampled traces.** 3,080 sampled lookups: request p50 3.756 ms, p99 12.318 ms. They are slower than the unprofiled 7.4 ms, which is expected under profiling.
  - **Where the time goes.** The nested spans (`bifrost.gate.query.stream` ⊃ `bifrost.gate.query` ⊃ `bifrost.oracle.query` ⊃ `bifrost.oracle.stream` ⊃ `bifrost.oracle.source`) show most of it in `bifrost.oracle.source`: p50 2.178 ms and p99 7.092 ms, of the oracle query's 3.307 and 8.929 ms.
  - **CPU.** Self time is dominated by generic iterator `next`, `drop` and `clone` (about 30%) under `vala_bifrost_redux::oracle::exec::project_batch` (5.3% inclusive). That fits the expensive part of a memtable point lookup being per-row projection and copying, not admission: `grant_waiters` is 0.2%.
- **Gateway, `chat-capture.c64`.**
  - **Sampled traces.** 3,736 sampled requests: p50 4.194 ms and p99 9.006 ms, mostly the paced upstream exchange `gateway.attempt` (p99 6.949 ms).
  - **Server overhead.** The server-side `gateway.capture` stage is p50 0.469 ms and p99 1.871 ms, which alone exceeds the 0.5 ms overhead limit.
  - **The expensive function.** Inclusive: `gateway::capture::redacted_canonical_json` 10.6% and `wyrd_spec::gateway::record::validate_canonical_json` 10.1%. Self time includes `serde_json` string escaping (about 8.6%) and libc copies (46.1%, mostly copies of the 64 KiB bodies).
  - **Interpretation.** Each capture serializes and validates the 64 KiB body as canonical JSON twice. This is the CPU behind the capture overhead and the capture-outbox shedding seen in V8.

These runs show that the artifacts are usable for investigation. They don't sample every request, and they don't give exact per-case CPU.

### Scenario 2a appendices: case mapping and instrumentation audit at `a7d2d3438`

I inspected source read-only. Path aliases:
- `D/` is `crates/wyrd/wyrd-testing/src/bin/capacity/`
- `T/` is `crates/wyrd/wyrd-testing/src/`
- `S/` is `crates/wyrd/wyrd-server/src/`
- `tm` is `crates/shared/wyrd-telemetry/src/timing.rs`
- `gate` is `crates/vala/vala-bifrost-redux/src/gate/mod.rs`

Shared plumbing:
- **Labels.** The driver attaches `x-wyrd-benchmark-case` and `x-wyrd-benchmark-phase` through `with_benchmark_labels` or `benchmark_headers` (`crates/shared/wyrd-client/src/transport/trace.rs:72-79, 143-167`; `D/query/run.rs:685-700`; `D/gateway/workload.rs:285-288`). The server reads them in `label_request` (`tm:173-198`) when it opens the request span: HTTP at `S/http/router.rs:207`, gRPC at `crates/wyrd/wyrd-tonic/src/server/mod.rs:251`.
- **Windows.** Windows difference `server.timings()` (`T/release_server.rs:361-364`) through `Population::collect`, `expect` and `validate` (`T/capacity.rs:619-692, 850-866`). The residual histogram is elapsed − excluded, computed per request at `tm:351-392`.
- **CPU and memory.** These come from `Window` and `ResourceWindow` (`T/capacity.rs:333-516`). Per-work figures come from `Repetition::derive` (`T/experiment.rs:1643-1700`).

**Case mapping**

| Experiment / steps | Driver operation | Server boundary (where recorded) | `case` label | Reading conversion | Reconciliation |
|---|---|---|---|---|---|
| writes-hot, writes-parquet `native-write-c1/-c64`; L2 Bifrost native share | `Bifrost::write_batch`, gRPC (`D/query/run.rs:270-283`) | `bifrost.gate.write`, `GateRequestLifecycle` (`gate:1210, 199-212, 74-93`) | `native-write` (`D/query/mod.rs:314-320`) | elapsed p50/p95/p99, rows, retained and published MiB (`D/query/isolated.rs:213-300`), CPU and memory per row (`D/query/mod.rs:654-690`); L2 merged `write_ack_p99_ms` (`competing.rs:715-747`) | success count = answered (`isolated.rs:214-221`); L2 per phase = acked Ok (`competing.rs:461-477`) |
| `otlp-spans-c1/-c64`; L2 spans | `TraceServiceClient::export` (`run.rs:284-297`) | `bifrost.otlp.traces`, `timed()` (`S/grpc/otlp.rs:197, 221-238`) | `otlp-spans` | as native, plus encoded wire bytes (`isolated.rs:268-275`) | as native; wire bytes > 0; `partial_success` counts as an error |
| `otlp-logs-c1/-c64`; L2 logs | `LogsServiceClient::export` (`run.rs:298-311`) | `bifrost.otlp.logs` (`S/grpc/otlp.rs:683`) | `otlp-logs` | as spans | as spans |
| hot and parquet reads (point, aggregates, full scan, span trace, shredded, long-tail); L2 mixed queries | `Bifrost::query`, HTTP stream, exact row checks (`run.rs:598-631`) | `bifrost.query`, `QueryTiming` drop (`S/query/routes.rs:128-140, 356`); scans via `record_scan` (`oracle/mod.rs:579`) | `Case::label()` (`D/query/workload.rs:196-203`) | elapsed percentiles, scanned rows and bytes per query (`T/capacity.rs:913-950`), CPU per query (`D/query/mod.rs:633-653`); L2 per cohort (`competing.rs:600-690`) | success count = answered, including wrong results; scan count = success count; parquet INVALID without scan facts |
| `direct-verify-c1/4/16/64`; L2 direct verify | `observe.verify` (`D/verification/run.rs:1008-1018`) | `verification.direct` (`S/components/verification/service.rs:480-486` → `S/verification/telemetry.rs:214-255`); excluded = Verifier block intervals (`crates/vala/vala-verify/src/blocks/engine.rs:122-140`) | `direct-verify` (`run.rs:79`) | elapsed, residual `overhead_*`, excluded `verifier_*` (`run.rs:343-374`); L2 `{stage}.direct_overhead_p99_ms` and recovery (`:540-573`) | completed = acknowledged, late included; L2 = admitted verify Ok |
| L2 background submit | `observe.submit` (`run.rs:1003-1007`) | no registry boundary: Prometheus `wyrd_verification_submit_seconds` (`S/verification/telemetry.rs:283-288`) | `queued-verify` sent but unused | `submit_mean_ms` and `queue_wait_mean_ms` from `/metrics` deltas (`run.rs:522-529`) | backlog drain only (mean, no count reconciliation) |
| Gateway `chat.c*`, `chat-capture.c*` | POST `/v1/chat/completions` (`D/gateway/workload.rs:253-300`) | `gateway.buffered` (`S/components/gateway/telemetry.rs:185-232` via `invocation.rs:498`); excluded = upstream (`crates/wyrd/wyrd-gateway/src/engine.rs:431-480`); capture runs after the answer, outside elapsed (`invocation.rs:1296-1297`) | `buffered-metadata` / `buffered-payload` (`D/gateway/mod.rs:131-137`) | elapsed, residual overhead, excluded upstream, capture integrity from Prometheus (`D/gateway/run.rs:155-216, 298-420`) | succeeded = client completions (`run.rs:525-535`); capture staged = arrivals |
| Gateway `chat-stream.c*` | same call, SSE timed (`workload.rs:316`) | `gateway.stream.head` and `gateway.stream.end` (`telemetry.rs:160-170`, `invocation.rs:1404`) | `streamed-metadata` | head percentiles and overhead; end residual (`run.rs:361-372`) | head count = end count = completions |
| L2 Gateway `contention` | open-loop multi-tenant call (`D/gateway/capacity.rs:818-860`) | as above | mode × period (`capacity.rs:253-266, 836`) | completion, first token and stream p99, chunk lag, overhead (`capacity.rs:505-706`) | per period × mode × boundary = answered (`capacity.rs:1031-1051`) |

**Instrumentation audit**

| Location | Purpose | Consumer | Gating | Decision |
|---|---|---|---|---|
| `tm:117-134` (`enable`/`enabled`) | process switch | all registry recorders | set only by `metrics_router` (`S/app/metrics.rs:373-383`) under `WYRD_METRICS_BENCHMARK_SNAPSHOT=1` on a loopback bind (`S/config.rs:3234, 3419-3427`); the registry is allocated only on enable | retain |
| `tm:173-198` `label_request` | case and phase onto the request span | registry keys | `!enabled()` first: one atomic load, no header parsing | retain |
| `wyrd-telemetry/src/lib.rs:367-375` → `tm:214-231` `record_timing` | record a value | benchmark record | returns before labels, lock or HDR when disabled (`tm:221`) | retain |
| `tm:236-247` `record_scan` (`oracle/mod.rs:579`) | scan facts | scanned per query | gated `tm:242`; the facts are already computed for Prometheus | retain |
| `S/http/middleware/metrics.rs:39-60` `RequestArrival` | arrival instant | production `/metrics`; arrival reused by `bifrost.query`, `verification.direct`, `gateway.*` | always on | reuse, already the single arrival clock |
| `gate:74-93` (write) | `bifrost.gate.write` | benchmark and production histogram, one clock | eager `Span::current()` (`gate:76`) | retain |
| `gate:74-93` (query, `gate:788`) | `bifrost.gate.query` | production histogram; **no driver reads the registry series** | same | remove the registry record for query |
| `S/query/routes.rs:91-140` `QueryTiming` | `bifrost.query` | benchmark only | **ungated**: span clone and arrival read even when disabled (`:103-107, 128-140`) | retain; gate on `timing::enabled()` |
| `S/grpc/otlp.rs:221-238` `timed()` for traces and logs | OTLP boundaries | benchmark only | **ungated**: `Instant::now`, span clone and `grpc-status` lookup when disabled | retain; short-circuit when disabled |
| `S/grpc/otlp.rs:505` | `bifrost.otlp.metrics` | **none** | as above | remove |
| `S/verification/telemetry.rs:214-255` (direct) | `verification.direct` | benchmark and production, one clock | eager `Span::current()` (`:229`) | retain |
| same `emit`, queued (`S/verification/runner.rs:835`) | `verification.queued` | **none in the driver**; it lands unlabelled | same | remove the registry record for queued |
| `vala-verify/src/blocks/engine.rs:122-140` | Verifier block intervals | excluded wait, production histograms | always on | retain |
| `S/verification/telemetry.rs:283-288` | submit latency | production; L2 mean | always on | retain |
| `wyrd-sql/src/queries/verifier_executions.rs:695-708` | queue SQL durations | production; driver breakdown (`D/verification/run.rs:100-110`) | always on | retain |
| `S/components/gateway/telemetry.rs:95-114, 185-232` | ingress phases, Gateway points | production and benchmark, one clock | always on; eager `Span::current()` (`:199`) | retain |
| `wyrd-gateway/src/engine.rs:431-480`, `skald-providers/src/exchange.rs:66-101` | provider wait (excluded) | Gateway excluded, upstream histogram | always on | retain |
| `S/http/middleware/edge_timeout.rs:51-62` | deadline handoff | correctness | n/a | retain (not instrumentation) |
| `wyrd-client/src/transport/trace.rs` | benchmark headers | server labels | `test-support` feature only | retain |
| `T/capacity.rs:333-516`, `T/release_server/os.rs:69-82, 164-220` | wall time, CPU, 10 ms peak RSS | every record | driver only | retain |
| `D/query/run.rs:344, 477`; `D/verification/run.rs:881-975`; `D/gateway/workload.rs:254, 271, 316` | client latency, lateness, first event | `client_p*`, arrivals, chunk lag | driver only | retain |
| `D/query/competing.rs:509-513, 583-586` | client latency in `Acked`/`Asked` | **none**: only `is_ok()`/`err()` is read | driver only | remove |
| `D/gateway/workload.rs:299` `timed.headers` | response-headers instant | **none** | driver only | remove |

**Findings, verified against source:**
1. **Unread registry series.** `bifrost.gate.query`, `verification.queued` and `bifrost.otlp.metrics` are recorded but no driver reads them. In benchmark mode they still take series slots.
2. **Ungated benchmark-only work.** `QueryTiming::new` clones the current span, and OTLP `timed()` reads the clock, clones the span and inspects `grpc-status`, all with timing disabled. Production pays this per request, though it records nothing.
3. **Unread driver timers.** L2 Bifrost client latency (`Acked`/`Asked.result`) and Gateway `timed.headers` are written but never read.
4. **L2 submit evidence.** It uses a Prometheus mean with no count reconciliation. The `queued-verify` label is sent but never keyed.
5. **Doc drift.** The comments at `S/http/router.rs:183-184` and `wyrd-tonic/src/server/mod.rs:227-228`, and the declaration text at `T/capacity.rs:573`, say labels are "activated" only after authentication. In fact `label_request` attaches them when the span opens.
6. **Not duplicates.** Gateway, Verification and Gate-write timings each read one duration and send it to both the registry and Prometheus.

Remediation for the replanned task: findings 1–3 and 5 are retain/reuse/remove decisions inside the existing owners. They change server code, so they need V2a and V7 to rerun. They are listed here and not applied, so they land with the other source changes after the spec revision instead of invalidating the V7 and V9 evidence above.

### Spec revision request for `$wyrd-spec`

Verdict: SPEC_REVISION_REQUIRED. Each item below changes approved measured work, limits, coverage or capability, so this task cannot resolve it. They are listed in priority order.

1. **writes-hot runs under a configuration no deployment uses.**
   - Spec rows `spec.md:351` and `HOT_ENV` (`crates/wyrd/wyrd-testing/src/bin/capacity/query/mod.rs:139-144`) start the server with 16,384 MB memtable and WAL rotation and 86,400 s retention.
   - Production defaults are 512 MB, 512 MB and 600 s (`crates/wyrd/wyrd-server/src/config.rs:352-365`).
   - With rotation effectively off, writes-hot at 64 clients held up to 45 GiB of memtables and 59 GB of server RSS, and nearly exhausted the 91 GiB host (D7). Its write throughput has no production meaning.
   - Requested decision: drop writes-hot, or run it at the production rotation defaults. writes-parquet already measures writes at default sizes with a 30 s retention.
   - The `hot` read experiment can keep the override: its fixture is bounded at 10M events rows and 1M spans, and it peaked well inside memory.
2. **L1 is not fast turnaround.**
   - One engine command measured 2.8–7.6 min of run plus a 1–2 min smoke prepass. `bench:l1` has a 60 min budget.
   - The cost is fixed per step: 5 s warmup plus 3 × 10 s windows. Gateway has 12 steps, so its run is 7.0 min of pure measuring.
   - Requested decisions:
     - an explicit L1 turnaround target, for example at most 5 min per engine command;
     - L1 client coverage of 1 and 64, with 4 and 16 moved to an opt-in sweep;
     - 3 × 5 s repetitions where every case keeps at least 1,000 samples behind p99;
     - a way to skip the smoke prepass when a passing smoke exists for the same commit.
3. **Declared limits that no engine meets on the reference host.** The V8 records show:
   - Verification `direct-verify-c64`: p99 1.33–1.37 ms against < 0.5 ms, plus staging 429s;
   - hot and parquet `point-c64`: p99 7.4 ms and 14.0 ms against < 5 ms;
   - writes-parquet OTLP spans: 15.2 MiB per CPU-second against a floor of ≥ 30;
   - Gateway `chat.c64`: overhead 0.64 ms against < 0.5 ms; `chat-capture.c64`: 4.34 ms, plus capture shedding.

   Requested decision: engine optimization in scope (the V8 diagnostics name the expensive stages), revised limits or coverage, or a different reference host.
4. **Bifrost L2 calibration.** 70% of isolated writes-parquet capacity × 8 cores overloads the contended server. It delivered about 4.8× fewer rows per CPU-second under contention, making the run INVALID. Requested decision: a different calibration basis, headroom or `--server-cores`.
5. **Sampled-client tenant recording.** REQ-020's third telemetry configuration needs the driver to send sampled `traceparent`, and no existing option does this. Requested decision: approve that driver capability, or drop the comparison.

Blocked until these are decided:
- the second qualification suite;
- reference promotion and the V6 committed baseline count update;
- normal L1 against references.

### Maintainer decisions on the spec revision request (2026-10-26)

1. **writes-hot:** drop the experiment. writes-parquet covers writes at the default rotation. The 16,384 MB / 86,400 s override stays only for the bounded `hot` read experiment.
2. **L1 turnaround:** at most 5 min per engine command.
   - Measure only 1 and 64 clients at L1.
   - Use 3 × 5 s repetitions where every case keeps its sample floors.
   - Skip the smoke prepass when a passing smoke exists for the same commit.
3. **Unmet limits:** keep the declared limits and bring engine optimization into this change. The targets are:
   - the Verification engine wait at 64 clients;
   - Bifrost point-lookup projection, for both memtable and Parquet;
   - the writes-parquet OTLP spans floor;
   - Gateway overhead and the double canonical-JSON capture.

   References are qualified only after the limits pass.
4. **Bifrost L2 calibration:** calibrate under contention. Before measuring, L2 finds the ingest rate the server sustains with competing queries and compaction running, then measures at the declared headroom of that rate.
5. **Sampled-client tenant recording:** add a driver option that sends a sampled `traceparent` on a fraction of requests, and measure that configuration in the telemetry comparison.

These go to `$wyrd-spec` as a draft revision for approval.
