---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-benchmark-experiments
spec_revision: 8
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-019, REQ-020, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-013]
depends_on: [TASK-004]
---

# Complete manual L1 regression suite and sustained L2 coverage

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

## What Changes and What Must Stay True

Keep the existing shared experiment runner, production fixtures, public SDK/OTLP
workloads, correctness checks, clean-source rules, portability, profile option
and append-only evidence. No engine optimization, harness rewrite, new workload
DSL, optional dead-helper cleanup, baseline-format migration or CI dependency.

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
ownership are proved; no waits for publication in the smoke or acknowledgement
window. No shared mutable fixture across independently owned databases.

Every case reports server latency, CPU seconds, C/T busy cores, provisioned P,
W/T, (W/T)/P, W/C, C/W, peak memory and all outcome counts; write cases also report
rows and uncompressed/OTLP-wire bytes with explicit units. Query cases retain
scan facts and scaling; capture cases retain capture bytes/accounting. Phase
histograms guide diagnosis. Resources and work use matching windows; drain work
is separate. No favorable metric is inferred from an absent observation.

Preserve 5 s + 3 x 10 s L1 windows, four Bifrost per-experiment budgets from the
spec, and the existing 15-minute budgets for other engines. Existing sample
floors still govern judged percentiles. Never reduce coverage, durations or
sample floors to fit a runtime budget; use recorded phase timings to fix setup
or lifecycle waste. Building and service prerequisites precede the experiment
clock; smoke, fixtures, pre/post host checks and drain remain inside it.

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
   all required dimensions with deterministic negative/noise cases.
4. Update mise descriptions and the existing README/TESTING/configuration docs;
   prove direct and aggregate coverage, smoke and exit precedence.
5. On clean integrated source, run smoke, full L1 one engine at a time, repeat
   for replacement references and perform matched tracing overhead comparisons.
6. Hand integrated L2 and all required production/language-surface proof to final
   change review; do not repeat broad suites after every task scenario.

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

### Scenario 3 — Comparable regressions fail without manufacturing certainty

**Behavior.** Compatible references trigger the specified independent latency,
CPU/work and throughput/core comparisons. Proven regressions FAIL; percentage
boundaries and noise behave as specified; ambiguous precision or missing
required operands is INVALID. Cross-host/method/profiled/partial references
cannot silently judge a case. Reference promotion is explicit.

**RED.** Planned V3 adds throughput/P and latency regression, baseline identity,
quantization ambiguity and first-reference cases to existing comparison proof
V6. Current judgment does not cover all these dimensions.

**GREEN.** Extend existing judgment/selection and baseline metadata, preserving
original numeric values as historical evidence. Absolute SLO and correctness
misses still fail despite noisy relative results. Run V3/V6.

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
module tests. Actual sustained proof runs once at integrated review (V9).

**REFACTOR.** Use established Tokio ownership/cancellation and existing profiler
lifecycle; remove detached work or duplicated cleanup rather than add another
scheduler framework.

### Scenario 5 — The documented manual workflow produces usable evidence

**Behavior.** Per-engine L1, Bifrost read/write subcommands and aggregates all run
mandatory smoke and retain individual records/exit precedence. A selected case
is partial. On clean source, full suites produce reproducible reports and allow
reviewed same-method references. Detailed tracing/profile diagnosis is optional
and explicitly differs from qualification configuration.

**RED.** Extend V1 dispatch coverage and planned V5 to test qualification versus
partial/diagnostic records and incompatible reference promotion. A changed
README alone cannot establish runtime behavior.

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
| REQ-011/012, AC-002/003/013 | V3/V5/V6 establish comparable references, all regression dimensions, noise/limit behavior and explicit promotion; V8 supplies real clean reference evidence. |
| REQ-004/005/009, INV-001/002/003/007, AC-007/010 | V1/V4 and integrated V9 prove complete phase coverage, independent arrivals, cancellation/drain, correctness, audit/durability and no host-load/build contamination. |
| REQ-015/016, INV-009, AC-008/009 | Source/consumer/task inventory preserves one runner and retirement work; Linux real proof plus portability inspection covers unchanged commands/schema on supported platforms. |
| REQ-019/020, INV-006/010, AC-011/013 | V8's matched telemetry comparisons and documented trace/profile investigation preserve client/tenant policy, safe artifacts and normal measurement configuration. TASK-003 owns focused tenant timeline proof. |

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
- `levels.rs`, capacity binary main and `mise.toml` already run smoke, separate
  experiments, Postgres wrappers and worst-exit aggregation. Expand writes to
  two records. Correct the current Gateway L2 mise description (payload capture)
  to match actual declared metadata capture rather than silently change workload.
- `query/competing.rs`, Verification run, Gateway scheduler/request tasks and
  `profile.rs` are the lifecycle consumers for the carried-forward review
  findings. Revalidate existing fixes; do not assume the old candidate is still
  current or redo completed work for its own sake.
- Existing `benchmarks/*/baselines.toml`, README and TESTING are the reference
  and discoverability owners. Preserve source values/history; annotate method
  incompatibility and add reviewed replacements linked to real clean records.

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

### V4 — Owned sustained workload lifecycle (planned)

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::sustained_work_stops_admission_and_reconciles_drain)'
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

### V7 — Clean release smoke

```bash
mise run bench:smoke
```

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

### V9 — Integrated qualification, once at change review

After all tasks integrate, run each engine's L2 command from REQ-010 sequentially
and retain real Linux x86_64 release records. Run the integrated production
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

Revision 8 must be approved and TASK-004 integrated. Stop for spec authority if
required cases cannot meet a declared budget without changing fixture/coverage,
if reference qualification needs different measurement boundaries, if an engine
optimization is required, or if a new dependency/feature/public API is necessary.
Do not delete failing cases, widen thresholds, relabel storage state, promote
invalid baselines or treat absent evidence as a passing regression check.

## Authority Links

- [Specification](../spec.md), revision 8.
- [Repository rules](../../../../AGENTS.md).
- [Workflow](../../../spec-driven-development.md).
- [Execution](../../../implementation-execution.md).
- [Testing](../../../../TESTING.md).
- [Bifrost authority](../../../../architecture/bifrost-design.md).
- [Trace-context authority](../../../../architecture/wyrd-design.md#trace-context--w3c-traceparent-and-tracestate).
