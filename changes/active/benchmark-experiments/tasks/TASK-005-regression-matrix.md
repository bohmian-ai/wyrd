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
