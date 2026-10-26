---
id: TASK-006
kind: implementation
status: proposed
spec: SPEC-benchmark-experiments
spec_revision: 11
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-020, REQ-021, REQ-022, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-008, AC-009, AC-010, AC-011, AC-012, AC-013, AC-014]
depends_on: [TASK-005]
---

# Finish fast L1 feedback and qualify references at unchanged limits

## Outcome and Value

An engineer can run any one L1 experiment within five minutes and obtain a
valid comparison of server latency, CPU/work and throughput/core. This task
removes the unbounded hot-write experiment, reduces L1 to its useful unloaded
and 64-client cases, implements sampled-client tracing for diagnosis, and fixes
the measured engine costs until the existing limits pass on the reference host.
Only then can two clean suites establish reviewed references.

Before, Verification and Gateway swept four client counts and repeated smoke
before every full run; the reference-host runs missed latency/throughput limits,
and hot writes nearly exhausted memory. After, L1 uses 1 and 64 clients, reuses
an eligible same-commit smoke PASS, and qualifies performance evidence only when
all unchanged limits, correctness and validity checks pass.

**Approval dependency:** revision 11 is currently **draft**. The maintainer
explicitly requested planning against it; this proposed task is not executable
until the maintainer approves revision 11. TASK-005 supplies the implemented
foundation and historical evidence, not a claim that its blocked qualification
or L2 obligations completed. No additional task is created by this packet.

## What Changes and What Must Stay True

This is the single revision task required by revision 11's Delivery and
supersession section. Its executable benchmark scope is **L1 only**: limit
checks, telemetry comparison and reference qualification, including the focused
measurement and regression proofs needed to trust those results.

- Remove `writes-hot` from runnable commands, plans, active docs and coverage.
  `bench:l1:bifrost:writes` runs only `writes-parquet`; the Bifrost aggregate
  runs `writes-parquet`, `hot`, `parquet` sequentially with fresh Postgres and
  existing exit precedence. No compatibility alias. Preserve historical records,
  baseline values/provenance and TASK-005 evidence; retirement is not history deletion.
- Each individual L1 experiment has a 300-second budget, including host checks,
  startup, fixture loading, warmups, measurement and drain. Bifrost's aggregate
  permits three such budgets (15 minutes). Build/prerequisites precede execution;
  smoke has its own declared clock, reported separately. An overrun stops work
  and retains INVALID evidence and actual finish time.
- Use 1 and 64 clients only. Keep the precise default matrix below, five-second
  discarded warmup and three five-second repetitions. A case uses three
  ten-second windows only when five seconds cannot meet its sample floor;
  declare that choice before measurement and identify it in the record. Never
  choose durations after seeing performance or lower sample floors to fit time.
- Keep ≥1,000 samples per judged p99 and ≥200 per judged p95 in each repetition.
  Full-scan latency remains descriptive; its correctness, CPU/query and
  throughput/core still judge. Retain exact fixture sizes, payloads, seeds,
  answer checks, storage-state and shredding/pruning checks.
- A full run may reuse only a smoke PASS under `target/benchmarks/smoke/` for
  the same clean commit, experiment and selected steps, linking its record.
  A dirty tree, different selection/commit/experiment, absent or INVALID smoke
  requires the normal isolated prepass. A failed prepass blocks full preparation.
  Smoke still tests plumbing and records production failures without judging them.
- Add `--client-trace-ratio <f>` with finite values in [0,1], default 0,
  deterministic request selection from the run seed and sampled W3C
  `traceparent`. Apply it at actual outbound request boundaries, including SDK,
  OTLP and Gateway HTTP paths. Record the control; nonzero runs are diagnostic
  and reference-ineligible. Preserve independent operator export and tenant
  authorization, scrubbing, linkage and nonblocking delivery.
- Fold in TASK-005 Scenario 2a audit findings 1–3 and 5: remove unread registry
  series and driver timers, gate benchmark-only work, and correct label-activation
  comments. Keep shared production clocks, metrics and correctness timers.
- Optimize the existing Verification, Bifrost and Gateway owners until the
  limits pass, without bypassing authentication, tenant isolation, audit,
  durability, canonical-JSON validation/redaction, exact results or admission.
  Keep required work inside the measured server and windows. No limit widening,
  failing-case removal, new reference host or special fast benchmark path.

| L1 experiment | Required measured cases |
|---|---|
| Bifrost writes-parquet | Native Arrow, OTLP spans and OTLP logs, each c1/c64: 6. Production rotation sizes, 30 s retention; durable ACK timing, actual maintenance CPU/activity/backlog. |
| Bifrost hot | Events point c1/c64, small aggregate c1, shredded span filter c1: 4. Bounded 10M events/1M spans fixture; 16,384 MB rotation and 24-hour retention only here; no staging/publication. |
| Bifrost parquet | Events point c1/c64, small aggregate c1, full scan c1, span trace lookup c1/c64, shredded and long-tail filters c1: 8. Same read fixtures and 200,000-row samples per write path; settle once to no memtable/staging/claim; storage ratios and layout checks retained. |
| Verification direct-verify | Existing four assertions over 2 KiB JSON, expected pass/fail judgments, c1/c64: 2. Expected failed judgments count as correct completed work. |
| Gateway overhead | Buffered metadata, streamed metadata and 64 KiB payload capture, each c1/c64: 6. Existing controlled provider, 1 KiB metadata shapes and 32 stream events. |

Bifrost L1 has **18 measured cases** (12 reads, 6 writes).
Setup/correctness steps remain visible separately and do not replace cases.

**Deferred:** implementing contended Bifrost L2 calibration, selecting its load,
L2 submit-evidence reconciliation (audit finding 4), and all L2 smoke/full runs.
REQ-009 and AC-007, plus L2 portions of AC-005/012 and REQ-020, remain later-task
obligations. This task does not claim whole-change completion. Removing the
explicitly unread L2 Bifrost timers is included audit cleanup, not L2 calibration
or execution. No `bench:smoke` aggregate here: it includes L2. No harness rewrite,
new runner, telemetry provider, benchmark CI/gate wiring or baseline migration.

## Approach

1. Confirm revision-11 approval and the TASK-005 foundation; retain its V6–V9
   records as history and identify which evidence source changes invalidate.
2. Close commands, declarations, docs and test consumers over the reduced L1
   matrix, timings and smoke reuse, using the shared runner.
3. Add deterministic sampled-client context through existing request transports
   and reference eligibility; prove real tenant recording and negative access.
4. Apply the bounded instrumentation audit and rerun affected integrity proof.
5. Use TASK-005's traces/profiles to optimize each existing engine owner; keep
   correctness/tenancy/durability regression proof green and diagnose remaining
   misses under ordinary L1 settings.
6. On the final clean candidate, run L1 smoke and limits, matched telemetry
   comparisons, two full qualification suites, reviewed reference selection and
   normal L1 regression checks. Report each engine separately; stop before L2.

## Ordered Implementation Scenarios

### Scenario 1 — The remaining L1 commands declare bounded, complete work

**Behavior.** Default declarations contain exactly the matrix above, c1/c64
only, a 300-second individual budget and three five-second repetitions except
predeclared floor-driven ten-second cases. `writes-hot` refuses before startup;
write and engine aggregates preserve individual records and INVALID precedence.
Insufficient samples and overrun stay INVALID; omitted cases stay explicitly partial.

**RED.** Extend existing declaration/dispatch proof [V1](#v1--l1-controls-and-dispatch)
to assert these controls and reject removed commands. Current source declares
four Bifrost experiments, four Verification/Gateway client counts and longer
windows/budgets. Add focused injected window/floor/overrun cases to V2 rather
than run a live experiment to test deadline logic.

**GREEN.** Change existing declarations/dispatch and necessary record/report,
fixture/test and mise consumers together. Move controlled real regression proof
from `writes-hot` to `writes-parquet`, keeping its real measured control and
perturbation requirements. Run V1/V2 and prior affected judgment tests.

**REFACTOR.** Remove dead experiment branches and active documentation; reuse
existing timing, declaration, validity and aggregate ownership.

### Scenario 2 — A matching clean smoke PASS avoids redundant preparation

**Behavior.** A full invocation links and reuses its eligible same-commit,
same-experiment, same-selection smoke PASS. Changed identity/selection, dirty
source, absent, unreadable or INVALID evidence causes an isolated prepass;
prepass failure stops full fixture preparation. No shared mutable smoke/full
fixture state or recursive prepass. Reports expose reuse and separate elapsed time.

**RED.** Add planned [V3](#v3--smoke-reuse) cases for a matching PASS and each
mismatch/failure through existing dispatch/prepass seams. Assert preparation
count and persisted linkage, not just a successful lookup. The current prepass
always spawns another smoke.

**GREEN.** Extend existing prepass and record/report owners; preserve their
negative paths and smoke production-criterion inapplicability. Run V3 and V1/V2.

**REFACTOR.** Reuse persisted smoke records; no new cache, index or lifecycle owner.

### Scenario 3 — Sampled-client requests exercise authorized tenant tracing

**Behavior.** Missing ratio equals 0; 0 sends no sampled context, 1 samples all
requests, and an intermediate ratio yields reproducible choices for a fixed
seed. Missing values, malformed/nonfinite and out-of-range inputs refuse before
startup. SDK HTTP/gRPC, direct OTLP and Gateway requests carry valid sampled
context when selected. Tenant traces remain authorized and linked when operator
export is absent; another tenant cannot read them. Nonzero records remain
reference-ineligible after persistence/readback or relocation; operator policy
never changes as a side effect.

**RED.** Planned [V4](#v4--sampled-client-controls-and-tenant-journey) covers parsing,
seed determinism, actual outgoing headers and the real authenticated request →
tenant readback/negative journey. Extend V5's existing qualification exclusions
for nonzero ratios. Existing transports support caller context, but the driver
cannot select or declare sampled requests.

**GREEN.** Extend shared driver controls and existing transport context paths,
including raw HTTP/OTLP consumers. Retain benchmark case/phase headers. Persist
the ratio and diagnostic status; run V4/V5 and prior scenarios.

**REFACTOR.** Reuse caller trace context and the production tracer; no alternate
exporter, tenant recording implementation or SDK API redesign.

### Scenario 4 — Instrumentation keeps consumed observations with a cheap disabled path

**Behavior.** Required timing populations, scan facts and production metrics
survive. When benchmark collection is disabled, QueryTiming and OTLP benchmark
wrapping avoid benchmark-only clocks, span cloning and status inspection.
Unread `bifrost.gate.query`, `verification.queued`, `bifrost.otlp.metrics`
registry series disappear while their production monitoring remains. Unread L2
Bifrost client latencies and Gateway response-header timestamps disappear.

**RED.** Planned [V6](#v6--instrumentation-and-engine-regression-tests) asserts the
disabled-path/retained-series behavior at existing timing seams; extend the
existing real measurement-integrity proof V7 to expose missing populations after
cleanup. Do not add a timer merely to test timer removal. Pure removal of unread
fields and comment corrections use source/consumer inspection, without fake RED.

**GREEN.** Apply audit findings 1–3 and 5 in their existing owners. Keep arrival,
Verifier-block, upstream, production histograms, deadline and scheduling clocks.
Rerun V6/V7 and all L1 smoke in V8 after server instrumentation changes.

**REFACTOR.** Keep the central benchmark enable switch and shared clocks; do not
expand the audit into unrelated instrumentation or add per-function timers.

### Scenario 5 — Verification meets its unchanged 64-client limit

**Behavior.** Real direct Verification retains correct alternating judgments,
evidence and audit with no unexpected refusals, while server overhead p99 at
64 clients is strictly below 500 µs. Engine-wait cost is reduced inside its
existing production execution owner; no persistence/admission bypass.

**RED.** Retain TASK-005's V8 limit miss and staging-refusal record as the
performance RED. Add a focused regression under V6 for the concrete cause
identified in execution, asserting preserved completion/evidence and error
behavior. Name and record its exact selector before RED; do not guess an engine
algorithm from a sampled wait profile.

**GREEN.** Fix the measured cause in the existing runtime/service; run V6/V7
and Verification's V8/V9 command until valid full L1 passes all unchanged limits.

**REFACTOR.** Reuse current execution and persistence ownership with no second
runtime or benchmark-only fast path.

### Scenario 6 — Both Bifrost lookup storage states meet the point SLO

**Behavior.** Exact point results retain schema projection, tenant isolation
and source authority in both hot and Parquet experiments. Point p99 at 64
clients is strictly below 5 ms in each record. Storage residence and required
scan evidence remain independently valid.

**RED.** TASK-005's hot/Parquet misses supply performance RED. Add V6 regression
coverage for the discovered projection cost using existing Oracle tests,
including schema/cast/missing-field behavior affected by the fix. Record actual
focused selectors before RED. A hot-only PASS cannot close this scenario.

**GREEN.** Optimize the existing Oracle/source projection path; run V6/V7 and
both hot and Parquet V8/V9 commands with the full unchanged fixtures.

**REFACTOR.** Reuse Arrow/DataFusion and current projection/source owners; no
special benchmark query route, result cache or weakened projection validation.

### Scenario 7 — OTLP spans meet the wire-throughput floor durably

**Behavior.** `writes-parquet` OTLP spans reach at least 30 uncompressed wire
MiB per server CPU-second while exact row outcomes, partial-success errors,
durable ACK semantics and actual staging/publication accounting remain intact.
Rotation stays at production defaults and retention at 30 s.

**RED.** TASK-005's 15.2 MiB/CPU-s result supplies performance RED. Locate the
cost through the existing OTLP → ingest → Scribe path, then add the smallest
V6 regression for that cause and its failure boundary. Confirm its selector
before RED; do not substitute native throughput for the spans floor.

**GREEN.** Optimize that production owner, rerun affected OTLP/durability proof
and V7, then full writes-parquet V8/V9 with wire-byte operands retained.

**REFACTOR.** Reuse ingest conversion and Scribe; keep maintenance CPU in its
actual measurement window and do not defer required work outside it.

### Scenario 8 — Gateway meets overhead limits with complete safe capture

**Behavior.** All c64 Gateway modes meet the existing strict <500 µs overhead
limits at applicable handoffs. Payload capture preserves canonical-JSON
validation, redaction, exact capture accounting and asynchronous delivery
semantics, without unexpected loss/shedding. Provider wait remains excluded
from server overhead by the existing boundary.

**RED.** TASK-005's metadata/capture overhead and shedding records supply
performance RED. Add focused V6 coverage for duplicate canonical serialization/
validation cost and its affected malformed/sensitive payload cases, retaining
production rejection/scrubbing. Record actual selectors before RED.

**GREEN.** Remove redundant work in existing capture/invocation owners without
removing trust-boundary validation; rerun V6/V7 and full Gateway V8/V9.

**REFACTOR.** Reuse the current canonical representation and capture outbox;
no capture disabling, smaller payload, new queue or measurement-boundary change.

## Acceptance Criteria

| Obligations | Owner and concrete proof |
|---|---|
| REQ-001/003/008/010/016; AC-001/009 | Existing plans/dispatch/mise/docs: V1/V2 and V8/V9 show three Bifrost experiments (18 measured cases), two Verification cases, six Gateway cases, only c1/c64, declared durations/exceptions and ≤300 s per experiment. Removed commands refuse; no active writes-hot coverage or alias remains. |
| REQ-002/004/005/006/007/013/014; INV-004/005/007/008/009; AC-004/005/006/012 (L1 portions) | Shared runner/timing/recorder: V2/V5/V7 and real L1 readback retain sample floors, exact populations, per-repetition operands and interval bounds, portable disjoint accounting, partial evidence and INVALID precedence. Build/host-load contamination, ambiguous histograms or budget overrun cannot PASS. |
| REQ-008/015; AC-008/012 (L1) | Existing prepass/records: V3 plus V8/V9 prove eligible smoke reuse, mismatches rerunning, failure before full preparation, isolated state and report linkage; smoke production failures are observations, not performance qualification. |
| REQ-017/018/022; INV-006/010; AC-011/013 (affected tracing portions) | Existing driver/transports/tenant trace owner: V4/V5 prove ratio validation/determinism, sampled headers on each transport, actual tenant recording/negative readback, operator independence and nonzero reference exclusion. |
| REQ-007/015; TASK-005 audit findings 1–3/5 | Existing timing and driver owners: V6/V7/V8 plus updated audit inventory prove consumed measurements survive, disabled benchmark work is gated, unread hooks/timers are removed and comments describe labeling at span opening. Finding 4 stays deferred with L2. |
| REQ-021; INV-001/002/003; AC-010/014 | Existing engine owners: Scenarios 5–8, V6–V9 show all unchanged limits pass with exact results, authentication, tenancy, nonblocking audit, admission and durable ACK/capture semantics preserved; diagnostic records locate each cost. No missing limit or invalid run substitutes for a passing result. |
| REQ-011/012/019/020; AC-002/003/013 (L1) | Existing judgment/reference owner: V5/V7/V9/V10 establish two clean complete same-method suites, explicit reviewed references, normal L1 against them, independent regression dimensions, noise/exact-boundary rules and matched three-configuration telemetry costs without subtraction. Failed, invalid, partial, dirty, smoke or diagnostic records never qualify. |

## Implementation Reference

Inspected sources and TASK-005's source-backed audit support extension of the
existing workflow. Paths guide ownership; they do not prescribe private helpers.

- **Plans and lifecycle:** `capacity/main.rs::dispatch`, `levels.rs::Context`,
  `prepass`, `execute` and `spawn_experiments`, `experiment.rs::Level::timing`,
  `Timing`, `Prepass`, `Record::qualification_refusals`. Current prepass always
  spawns; timing is still 3 × 10 s with old budgets. Extend these owners for
  reuse and timing controls, not a new runner/cache. Consumers include
  `mise.toml`, `benchmarks/README.md`, `TESTING.md` and contributor invocation docs.
- **Case producers:** `query/mod.rs::Experiment::{ALL,WRITES}`, `plan`, `declare`,
  `HOT_ENV`, `query/isolated.rs`, `verification/run.rs`, `gateway/mod.rs` and
  `gateway/run.rs`. Existing declaration/dispatch tests live inline in capacity
  `main.rs::tests`; they assert the obsolete experiment list and timings.
  Extend them and preserve setup/check steps, rather than deleting workload
  coverage. Keep HOT_ENV only for bounded hot reads.
- **Trace transport:** `wyrd-client/src/transport/trace.rs::with_caller_trace`,
  `outbound`, `with_benchmark_labels` and `benchmark_headers`; callers in
  `capacity/query/run.rs` (SDK and raw OTLP), `verification/run.rs` (observe),
  `gateway/workload.rs::call`/`exchange` (raw HTTP). These already carry context
  or labels; the gap is driver sampling/seed/control and raw transport closure.
  Reuse existing production tracing and TASK-003 tenant readback patterns.
- **Instrumentation:** `wyrd-server/src/query/routes.rs::QueryTiming` currently
  eagerly clones a span; `grpc/otlp.rs::timed` eagerly reads the clock/span/status.
  TASK-005 audit maps the unread series to Bifrost Gate, Verification telemetry
  and OTLP metrics; unread driver fields to `query/competing.rs` and
  `gateway/workload.rs`. Update misleading comments in HTTP router, tonic server
  and `wyrd-testing/src/capacity.rs::timing_declaration`; retain the central
  `wyrd-telemetry/src/timing.rs` switch and production monitoring consumers.
- **Engine reuse:** Verification service wraps `engine.run` in
  `verification.engine`; TASK-005 traces show wait dominates evaluator CPU.
  Bifrost `oracle/exec.rs::project_batch` is used by source projection and retains
  type/cast/missing-field checks; TASK-005 profiles identify projection/copying.
  OTLP export converts into the existing Scribe write path; its wire floor needs
  diagnosis there, not a driver normalization change. Gateway
  `components/gateway/capture.rs::redacted_canonical_json` is called for request
  and response capture; TASK-005 profiles implicate repeated serialization and
  `wyrd_spec::gateway::record::validate_canonical_json`. Extend those owners while
  preserving validation/redaction. Profiles locate candidates, not mandated fixes.
- **Evidence consumers:** capacity `tests/pg_tests.rs` owns both real measurement
  integrity and measured regression journeys under the actual CLI runtime. The
  latter currently selects `writes-hot`; port that case to `writes-parquet`.
  `experiment.rs::tests` owns qualification and regression arithmetic;
  `baseline_selection_preserves_eligibility` currently asserts historical-only
  committed counts and no eligible Verification/Gateway baseline. Update those
  assertions to the reviewed references after qualification, retaining historical
  value/provenance assertions; never weaken or delete the check to pass it.
  Baseline TOMLs remain under `benchmarks/<engine>/baselines.toml`.

## Verification and Evidence

Commands below are **future implementation instructions**, not planning-time
runs. Confirm planned selectors after adding tests and before RED; the package
`wyrd-testing`, inline `--bin capacity`/`--lib` targets and ignored release
Postgres proof setup are confirmed from current sources/manifests. New test
names below are planned. Engine-fix tests depend on the diagnosed cause: record
their exact package, target, setup and selector before RED under this section;
do not invent names or run whole journey suites as substitutes.

### V1 — L1 controls and dispatch

Extend these existing inline tests:

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::level_one_declares_complete_engine_coverage) | test(=tests::level_commands_preserve_records_and_exit_precedence)'
```

### V2 — Timing, floors and budget (planned)

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::l1_windows_preserve_floors_and_budget)'
```

Use injected observations, including below/equal/above budget and floor boundaries;
assert recorded controls, overrun cancellation and INVALID evidence, not host load.

### V3 — Smoke reuse (planned)

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::l1_reuses_only_matching_clean_smoke_pass)'
```

### V4 — Sampled-client controls and tenant journey

Planned focused parsing/header/determinism test:

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::client_trace_ratio_preserves_transport_and_policy)'
```

Planned real authenticated tenant trace journey, using the existing release-server
Postgres setup; cover SDK, OTLP and Gateway outbound paths and negative readback:

```bash
RUSTFLAGS='-C force-frame-pointers=yes' CARGO_PROFILE_RELEASE_DEBUG=line-tables-only mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked --release --target-dir target/profiling -p wyrd-testing --bin capacity --run-ignored=only --no-capture -E 'test(=tests::pg_tests::sampled_client_requests_retain_authorized_tenant_traces)'
```

### V5 — Existing qualification, arithmetic and history regression checks

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::reference_qualification_rejects_partial_and_diagnostic_runs) | test(=experiment::tests::compatible_histogram_baselines_judge_all_regression_dimensions) | test(=experiment::tests::criteria_distinguish_regression_noise_and_invalidity) | test(=experiment::tests::baseline_selection_preserves_eligibility) | test(=experiment::tests::metrics_preserve_process_scope_and_units)'
```

Extend exclusions for ratio >0. Rerun after reviewed reference TOML changes and
update exact expected counts/identities without rewriting historical provenance.

### V6 — Instrumentation and engine regression tests

Planned focused disabled-path/consumed-series test:

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::benchmark_instrumentation_retains_only_consumed_timings)'
```

Add the actual focused engine correctness/security/durability regression commands
here during execution, before RED, after diagnosing each cost. Use existing
owner tests/journeys and their actual setup; a new product surface is not being
introduced. Rerun affected existing tenant/storage/capture journeys, not whole
cross-language suites. Inspect the bounded audit's consumer map for pure
removals and comment changes; TDD is inapplicable to those static obligations.

### V7 — Rerun TASK-005 V2a and measured regression proof on remaining L1 paths

```bash
RUSTFLAGS='-C force-frame-pointers=yes' CARGO_PROFILE_RELEASE_DEBUG=line-tables-only mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked --release --target-dir target/profiling -p wyrd-testing --bin capacity --run-ignored=only --no-capture -E 'test(=tests::pg_tests::engine_measurements_keep_boundaries_processes_and_windows)'
RUSTFLAGS='-C force-frame-pointers=yes' CARGO_PROFILE_RELEASE_DEBUG=line-tables-only mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked --release --target-dir target/profiling -p wyrd-testing --bin capacity --run-ignored=only --no-capture -E 'test(=tests::pg_tests::measured_regressions_reach_normal_reference_judgment)'
```

Port removed write coverage before running. Preserve independent known-delay/
CPU/window/case/process controls and predeclared tolerances. Real valid controls
must PASS, real perturbations must reach normal reference judgment and FAIL
with exit 1 across latency, CPU/work and throughput/core, covering every engine's
conversion. Edited readings or INVALID runs cannot prove regression detection.
These are focused L1 proofs; do not invoke L2 through them.

### V8 — Final clean L1 smoke and evidence readback

Build the final release candidate and external prerequisites **before** measuring:

```bash
mise run bench:build
mise run bench:l1:bifrost -- --smoke
mise run bench:l1:verification -- --smoke
mise run bench:l1:gateway -- --smoke
```

Run sequentially without other builds, tests or measurements. Inspect all three
Bifrost records and both other engines: every default case, retained timing
snapshots, histogram population/bounds/scan facts, CPU/work operands, counters,
mode and applicable criteria must match. Regenerate each report with V10 and
compare. Accurately recorded production errors may coexist with smoke PASS;
missing/mismatched measurements remain INVALID. New source commits invalidate
reuse of earlier smoke records.

### V9 — Limits, telemetry costs, reference qualification and normal L1

On the same quiet reference Linux x86_64 host, final clean source and ordinary
telemetry (endpoint absent, ratio 0, unprofiled), run the following **twice** as
two complete qualification suites, sequentially. Report each engine immediately.
Each command must reuse its matching V8 smoke PASS and finish within 300 s.

```bash
mise run bench:l1:bifrost:writes -- --qualify-baseline
mise run bench:l1:bifrost:hot -- --qualify-baseline
mise run bench:l1:bifrost:parquet -- --qualify-baseline
mise run bench:l1:verification -- --qualify-baseline
mise run bench:l1:gateway -- --qualify-baseline
```

The first suite is also the final all-limit check; any FAIL/INVALID blocks
reference qualification/promotion. Diagnose and fix, then rebuild/reestablish
clean source and repeat affected evidence; no limit relaxation or acceptance
of a miss. Retain before/after source identities, profiles/traces and normal
records for each optimization. Only caller-authorized commits may establish a
clean candidate; do not automatically stage/commit records or source.

Before promotion, compare the same clean build/workload/fixtures with (a)
operator endpoint absent, (b) ordinary 1% operator export, and separately (c)
sampled-client tenant recording, using the owning L1 command above with
`--client-trace-ratio 0.01`. For the export configuration set the existing
`WYRD_OTLP_ENDPOINT` to the local collector and
`WYRD_OTLP_SAMPLE_RATIO=0.01`, as in TASK-005's Scenario 2a diagnostic
commands; keep endpoint absent for the separate tenant-recording comparison.
Record exact expanded invocations, seed, configuration and raw paths when run.
Use matched cases for every engine; compare CPU/work, latency and throughput
without subtraction and report repetition spread and observed recording/export.
Diagnostic/profiled/nonzero-ratio records cannot be any of the two ordinary
qualification suites. Rerun profiling only where changed code invalidates the
existing investigation; missing profiling never fabricates CPU evidence.

After V10 emits references and their TOML diff is reviewed, run normal complete
L1 against them (same commands without `--qualify-baseline`), retaining reports,
operands and regression coverage. If committing that reviewed change is needed
to restore clean source, obtain caller authorization at execution time; dirty
qualification is never acceptable. Do not run `bench:l2:*` or `bench:smoke`.

### V10 — Persisted report and reviewed references

Substitute actual emitted paths; retain their identities in execution evidence:

```bash
mise exec -- target/profiling/release/capacity report "<record.json>"
mise exec -- target/profiling/release/capacity reference "<suite-1-record.json>" "<suite-2-record.json>"
```

Use the reference command for each matching experiment pair. Verify six
repetition values, central-two mean, latency bounds, compatible method/control
identities and linked provenance. Review the resulting TOML explicitly; no
automatic promotion. Historical values/records stay intact. Qualification PASS
alone never claims regression coverage; normal L1 and V7 supply that proof.

### V11 — Static closure and owner checks

```bash
mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
mise exec -- git diff --check
```

Inspect command/docs/coverage consumers for active removed-experiment references,
old client counts/budgets, smoke reuse, trace-ratio usage, qualification and
recovery instructions. Preserve historical mentions as evidence. Inspect portable
accounting/commands for all three supported platforms; real macOS/arm64 runs are
optional, not a substitute for Linux evidence. Add only owner lanes justified
by the final write set; no broad gate, benchmark CI dependency or full journey
suite. Record commands once and link scenario evidence here.

## Material Stop Conditions

- Revision 11 has not been explicitly approved: task stays proposed; planning
  does not approve it. A changed revision requires remapping before execution.
- Meeting a limit requires different measured work, fixture/host, weakened
  sample/validity/latency rules, altered authorization/admission/durability,
  trace policy or public/persistent contract: return to specification authority.
- The declared floors and fixture/preparation work cannot fit a 300-second
  experiment budget under permitted controls: retain INVALID evidence and
  request a spec decision; do not drop coverage or silently shorten windows.
- Required telemetry, correct results, privacy, process/window identity or
  cancellation evidence is missing: repair its owner before qualification.
- L2 calibration or a full/L2 smoke run appears necessary to complete this
  packet: defer it to the later task; do not expand this task or claim AC-007.

## Authority Links

- [Spec revision 11, draft pending maintainer approval](../spec.md), especially
  Delivery and supersession, fast L1, comparable baselines, engine limits and
  sampled-client tracing. Current user direction authorizes this proposed plan
  before approval and explicitly limits execution to L1.
- [TASK-005 evidence](TASK-005-regression-matrix.md): V6–V9, Scenario 2a case
  mapping/instrumentation audit, spec revision request and maintainer decisions.
- [Spec-driven development](../../../spec-driven-development.md) and
  [implementation execution](../../../implementation-execution.md).
- [Repository rules](../../../../AGENTS.md), [testing](../../../../TESTING.md),
  [maintainer style](../../../../agent_docs/maintainer-style.md).
- [Bifrost authority](../../../../architecture/bifrost-design.md): Durability and
  visibility; Ingest: Scribe; Query: Oracle; Telemetry and measurement meanings.
- [Security authority](../../../../architecture/wyrd-security-posture.md): Tenant
  and data isolation; Audit integrity and privacy.
- [Wyrd authority](../../../../architecture/wyrd-design.md) for Verification,
  Gateway and production tracing boundaries; no contract redesign is authorized.
