---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-benchmark-experiments
spec_revision: 8
requirements: [REQ-002, REQ-003, REQ-006, REQ-007, REQ-013, REQ-014, REQ-015, INV-004, INV-005, INV-006, INV-008, INV-010, AC-004, AC-005, AC-006, AC-008, AC-012]
depends_on: [TASK-003]
---

# Server benchmark timing independent of trace export

## Outcome and Value

Every real server operation contributes latency evidence even when operator
tracing is disabled or drops spans. Engineers receive bounded high-resolution
latency distributions, trustworthy server-overhead measurements and matching
CPU/throughput accounting without exporting unrelated diagnostic spans.

For example, a Gateway request with a 100 ms provider wait retains its actual
server overhead rather than treating provider time as Wyrd latency. Turning off
operator export does not remove that measurement.

## What Changes and What Must Stay True

Keep existing monotonic timers and per-request excluded-wait calculations. Record
integer durations into bounded HDR distributions at the owning server boundary,
not by interpreting span timestamps or subtracting percentile summaries.
Three-significant-digit quantization is explicit; invalid residuals, population
mismatches, reset, overflow and threshold-bound ambiguity cannot pass.

The existing metrics listener gains only the revision-8 opt-in local read-only
snapshot. Ordinary /metrics behavior and the global Prometheus recorder remain
intact. Enabled snapshot mode refuses non-loopback binding and exposes no
payload, tenant, request ID or arbitrary query/reset. Reuse the server metrics
lifecycle and LocalServer client, not a helper service or second launcher.
Use the already installed HDR dependency in the narrow runtime owner; importing
wyrd-bench into production would violate dependency rules. Private in-process
accumulator/locking and readout encoding remain implementation-owned.

Snapshots retain full cumulative distributions with consistent generation,
process, clock, precision, operation/outcome and count metadata. Interval deltas
operate on bucket counts. Histogram recording, missing components and overflow
have explicit accounting; no missing data becomes zero. Preserve zero-duration
counts. Bound series, duration range, response size and diagnostics.

Drain warmup before measurement. Freeze CPU and completed work on the same
measurement clock; keep drain completions separate. Finish the measured cohort
under a bounded drain before finalizing its latency distribution. Record the
cohort's admission/completion counts and the two windows explicitly. Per-query
scan distributions remain associated with the same case/query-class, repetition,
phase and outcome population. Use the specified closed benchmark metadata through
authenticated request context; ordinary deployments ignore it and unknown
benchmark classifications invalidate evidence. Interleaved L2 query classes must
retain independent counts/distributions without per-request IDs in snapshots;
no unrelated native probe can satisfy offered work's expected counts.

Emit version-2 records and retain version-1 read/report support without rewriting
old records. Include timing method, precision, boundaries, count reconciliation
and tracing configuration. Retire mandatory timing-span collection only after
replacement proof passes; retain useful production spans, tests and diagnostics.

## Approach

1. Extend the existing timer/recorder ownership with bounded HDR observation and
   component/outcome accounting while preserving operational metrics.
2. Extend the existing local metrics readout and benchmark client for consistent
   cumulative snapshots; prove privacy, opt-in exposure and interval arithmetic.
3. Wire the existing Verification, Gateway and Bifrost boundary timers and scan
   facts to the same collection contract.
4. Make `--smoke` run the normal L1/L2 workflow at reduced scale (one warmup,
   one repetition, every selected step and phase) and validate histograms and
   CPU/work through normal validity, independent of tracing; full commands run
   an isolated smoke prepass before full-size preparation.
5. Adapt shared accounting, records and report readers to version 2; preserve
   legacy evidence and remove the obsolete mandatory raw-span collection path.

## Ordered Implementation Scenarios

### Scenario 1 — Bounded distributions produce auditable interval latency

**Behavior.** Known observations produce matching counts, interval distributions,
percentiles and quantization bounds under concurrent recording. Zero is counted;
range overflow, generation/reset or missing observations yields INVALID. Strict
limits pass/fail only when the entire value bound is on the corresponding side.

**RED.** Planned V1 adds known concurrent observations and before/after snapshots,
checks interval count conservation and threshold-edge classifications. Include
interleaved query classes, warmup admission/completion, admission-stop snapshot
and the final cohort snapshot; verify each observation belongs exactly once
and that class/phase populations cannot satisfy each other's counts. The
current raw-span collection has no such distribution/snapshot contract.

**GREEN.** Implement the minimum bounded recording/snapshot arithmetic and typed
validity reasons. Include p95/p99 rank semantics and loss/overflow counters;
never use saturation, percentile subtraction or a best-effort timed flush.

**REFACTOR.** Reuse the installed HDR primitives; remove redundant percentile
code only after all current callers use the new representation.

### Scenario 2 — Server overhead excludes exactly the declared waits

**Behavior.** Verification excludes the union of Verifier waits once. Gateway
excludes retry/overlapping provider waits through the appropriate buffered,
head or final-frame boundary. Bifrost includes complete query result handoff
or durable write acknowledgement. Missing/negative components are INVALID.

**RED.** Planned V2 tests the receiving metrics seam, and V2a adds the missing
engine-owner boundary assertions listed below. They use controlled clock/
component inputs and compare resulting histogram populations. Exercise overlap, zero wait,
invalid residual and streaming head/end differences; mocks cannot replace the
real boundary proof in Scenario 4.

**GREEN.** Wire existing timer results into the runtime accumulator and preserve
separate elapsed/wait/residual distributions. Rerun V1/V2 and the existing engine
telemetry owner tests in V2a; record actual exact selectors for any named additions.

**REFACTOR.** Keep duration calculations with current engine owners. Do not add
another request timing engine or reconstruct waits from trace trees.

### Scenario 3 — Local snapshot exposure is opt-in and read-only

**Behavior.** Normal deployment has no benchmark snapshot. Opt-in plus a
non-loopback metrics bind fails startup. Enabled local readout has consistent,
bounded aggregate output, no identities/payloads, no reset operation and cannot
replace or mutate the Prometheus recorder. Process replacement cannot masquerade
as a valid interval.

**RED.** Planned V3 exercises the actual metrics router/configuration path and
asserts absence, startup refusal, output bounds and repeated-read semantics.
The current router exposes no HDR snapshot. Existing recorder tests remain.

**GREEN.** Extend metrics composition and its existing client minimally. Complete
V1–V3 and V7; preserve app-route authentication, tenancy and audit.

**REFACTOR.** Reuse the existing listener/read client; remove a duplicate test
recorder or transport if the implementation introduced one.

### Scenario 4 — Every engine proves timing before expensive work

**Behavior.** `--smoke` on the normal L1/L2 commands runs the full workflow at
reduced fixture scale and windows (one warmup, one repetition, every selected
step and phase), with operator export disabled. Each boundary has
integer-duration histogram observations, positive work, valid server CPU and
available-core readings; a missing reading is INVALID under normal validity.
A smoke judges no target: every criterion is inapplicable, so a smoke is PASS
or INVALID. Records carry `mode`, smoke output stays under
`target/benchmarks/smoke/`, and a smoke record is never a baseline. Every full
direct/aggregate L1/L2 dispatch runs an isolated smoke prepass that stops it
before full-size preparation when the smoke is INVALID.

**RED.** Extend V6 so smoke mode must run the selected workflow, judge missing
histogram evidence INVALID and stay out of history. Run V8 with trace export disabled to demonstrate that the
current raw-span-only contract cannot satisfy independent timing collection.

**GREEN.** Implement smoke as a mode of the common runner and each engine's
normal prepare/execute path, including its normal publication waits; delete the
per-engine request-only smoke. Rerun V1–V3/V6 and each engine's release smoke
before long measurements.

**REFACTOR.** Delete obsolete raw-span parsing/collection and helper requirements
only after all engine consumers have moved; keep any still-used diagnostic seam
and its tests. No second smoke framework is needed.

### Scenario 5 — Accounting and persisted reports preserve the real windows

**Behavior.** A completion after the throughput/CPU window appears in cohort
latency and drain accounting, not its earlier work denominator. Reset or absent
CPU/P is INVALID. Version-2 reports reproduce stored bounds and all resource
normalizations; legacy-v1 reports keep their original meaning and unknown
versions are refused. Persistence failure cannot return PASS.

**RED.** Planned V4/V5 exercise late completion, generation change, legacy input,
new method output and report regeneration. Extend existing V6 unit regressions
where a missing assertion exposes the bug.

**GREEN.** Close runner/engine conversions, schema/readers and report consumers
together. Preserve W/C, C/W, (W/T)/P, byte units, errors and population counts.
Run V1–V7 before final release smoke.

**REFACTOR.** Keep one record/report owner and reuse existing atomic publication
and collision handling. Do not migrate historical JSON or invent a parallel
histogram-specific report framework.

## Acceptance Criteria

| Obligations | Observable proof |
|---|---|
| REQ-007, INV-005, AC-012 | V1/V2 prove quantization, wait subtraction, counts, negative/missing/overflow/reset/zero behavior; V8 proves real production boundaries without operator export. |
| REQ-002/003/006, INV-004, AC-004/005 | V4/V6 prove warmup exclusion, same-window CPU/work/P and drain reconciliation. Real smoke proves resource availability. |
| REQ-013/014, INV-008, AC-006 | V5 and existing report regressions prove schema-v2 method/bounds, legacy reading, report reproduction and persistence failure behavior. |
| REQ-015, INV-006/010, AC-008 | V3/V7 and source/dependency inspection prove local opt-in bounded snapshots, no leaked identity, no second global recorder/launcher/production test-tool dependency. |

## Implementation Reference

- `wyrd-telemetry/src/lib.rs::record_timing` currently emits ordinary sampled
  info spans. Existing engine timers are the desired source; replace this
  transport obligation once all callers migrate, not the timer boundaries.
- `wyrd-bench/src/recorder.rs::BenchmarkRecorder` has HDR three-significant-digit
  conversion, snapshot and reset, consumed by benchmark tooling. Its snapshots
  expose only summary quantiles and install a global recorder; they cannot be
  installed beside server Prometheus or subtracted for interval p99. Reuse HDR
  know-how/dependency, not the test crate as a production owner.
- `wyrd-server/src/app/metrics.rs::install_recorder` and `metrics_router` own
  Prometheus and GET /metrics; `app/server.rs` owns its dedicated listener.
  `release_server.rs::LocalServer::metrics` reads it. These inspected whole
  workflows justify the narrow added snapshot; no ready HDR transport exists.
- `verification/telemetry.rs` and `components/gateway/telemetry.rs` already emit
  per-request residual metrics; Gateway engine exchange accounting handles wait
  unions. Bifrost `query/routes.rs`, Gate write and gRPC OTLP wrappers
  own complete public timing, distinct from Oracle-only execution timers.
- `capacity.rs::Collector`, `Resources`, smoke evidence, `experiment.rs::run`,
  `Repetition`, shared judgment/read/report and `levels.rs` are existing callers.
  Migrate them cohesively; the collector is no longer required for latency.
- Existing unit targets verified from source: wyrd-testing --lib, its capacity
  binary, wyrd-server --lib, wyrd-telemetry --features test-support --lib.
  New planned test names below use those targets and need no Postgres unless
  they drive a real release process; V8 owns the release/Postgres setup.

## Verification and Evidence

Confirm each planned selector after adding its test and before RED. Record each
named existing/new engine telemetry test's exact selector as execution evidence.
Run Cargo sequentially. No builds/tests/profile work overlap measurements.

### V1 — Histogram intervals and precision (planned)

```bash
mise exec -- cargo nextest run --locked -p wyrd-telemetry --features test-support --lib -E 'test(=tests::benchmark_histograms_preserve_bounded_intervals)'
```

### V2 — Per-request timing components (planned)

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=app::metrics::tests::benchmark_timings_preserve_server_boundaries)'
```

### V2a — Actual engine boundary owners

Existing focused regressions (they do not alone prove public boundary timing):

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::telemetry::tests::union_counts_overlap_once) | test(=verification::telemetry::tests::overhead_is_the_execution_less_its_blocks) | test(=verification::telemetry::tests::abandoned_direct_execution_finishes_cancelled)'
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=engine::tests::execution_timing_separates_provider_wait_from_adapter_work)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::query_request_lifecycle_reconciles_exact_terminal_outcomes)'
```

Planned additions pin each real emission boundary rather than recreating its
calculation in the metrics layer. Confirm actual selectors after adding them:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::telemetry::tests::histogram_overhead_excludes_real_overlapping_blocks)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::telemetry::tests::histograms_preserve_head_and_final_overhead)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=query::routes::tests::histograms_cover_arrival_through_terminal_handoff)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=grpc::otlp::tests::histograms_cover_service_call_through_acknowledgement)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::histogram_write_boundary_includes_durable_append)'
```

New owner tests use the existing default-feature inline targets; if richer
fixtures require test-support, document that source-backed equivalent setup
before RED. Missing owner test infrastructure is a bounded implementation seam,
not permission to replace actual release smoke with arithmetic tests.

### V3 — Real metrics router/configuration seam (planned)

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=app::metrics::tests::benchmark_snapshots_are_local_read_only_and_opt_in)'
```

### V4 — Same-window accounting (planned)

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::histogram_cohorts_preserve_measurement_and_drain_windows)'
```

### V5 — Versioned record/read/report contract (planned)

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::histogram_records_preserve_method_and_legacy_reports)'
```

### V6 — Existing accounting, smoke and report regressions

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::metrics_preserve_process_scope_and_units) | test(=experiment::tests::repetitions_exclude_warmup_and_bound_execution) | test(=experiment::tests::reports_regenerate_from_persisted_records) | test(=capacity::tests::shared_measurements_preserve_workload_deltas) | test(=experiment::tests::smoke_runs_the_selected_workflow_and_stays_out_of_history)'
```

### V7 — Existing metrics owner and required checks

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^app::metrics::tests::/)'
mise run fmt
mise run lints
mise run check:deps
mise exec -- git diff --check
```

Documentation/schema descriptions have static proof against actual output;
no generated artifacts are hand-edited. Run codegen only if consumed generated
contracts change. The internal snapshot must not become a public SDK API.

### V8 — Mandatory real release smoke

```bash
mise run bench:smoke
```

Use existing bench build/Postgres prerequisites before the smoke clock. Run
only after committing the source/packet to a clean tree, without --allow-dirty.
Record the actual resolved manifest showing operator export disabled, per-engine
histogram counts, CPU/P and work counters; do not report the command as executed
until artifacts exist. Rerun after changing collection, timing or population
logic and once on the final integrated source.

## Material Stop Conditions

Stop if a complete server boundary cannot be measured, snapshots cannot be
bounded/consistent without changing the specified exposure contract, a new
public endpoint/SDK/dependency/feature is needed, or production tenancy/audit
would be bypassed. Do not fall back to client latency, coarse buckets or 100%
trace export. Revision 8 approval and TASK-003 integration are prerequisites.

## Authority Links

- [Specification](../spec.md), revision 8.
- [Repository rules](../../../../AGENTS.md).
- [Workflow](../../../spec-driven-development.md).
- [Execution](../../../implementation-execution.md).
- [Testing](../../../../TESTING.md).
- [Bifrost telemetry meanings](../../../../architecture/bifrost-design.md#measurement-meanings).
- [Security posture](../../../../architecture/wyrd-security-posture.md).

## Execution Evidence

Commits `b321c388c`..`2e6272c0d` on `change/benchmark-experiments`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-007, INV-005, AC-012 | `wyrd-telemetry/src/timing.rs` bounded HDR series, `since()` intervals; `record_timing` records histograms only (`wyrd.timing` span retired); boundary owners in verification `hosted/blocks.rs`, gateway telemetry, query routes, gRPC OTLP, Bifrost gate | V1; V2; V2a (all 8 selectors below); V8 smoke counts per boundary with operator span export at its default (disabled) | PASS |
| REQ-002/003/006, INV-004, AC-004/005 | `capacity.rs::Population::collect` takes expected counts per window; L2 Bifrost phases from boundary snapshots (`query/competing.rs`), gateway capacity/recovery populations (`gateway/capacity.rs`) | V4; V6; V8 CPU, logical CPUs and work counters per engine | PASS |
| REQ-013/014, INV-008, AC-006 | `experiment.rs` `SCHEMA_VERSION = 2`, reads legacy 1, refuses others; report prints `timing (schema N)` method | V5; V6 `reports_regenerate_from_persisted_records` | PASS |
| REQ-015, INV-006/010, AC-008 | Snapshot opt-in by `WYRD_METRICS_BENCHMARK_SNAPSHOT=1` on the loopback metrics listener; no new dependency; wyrd-testing stays a dev/bench tool | V3; V7 `app::metrics::tests::*`; `check:deps` | PASS |

Actual V2a selectors: the verification addition is
`verification::hosted::blocks::tests::histogram_overhead_excludes_real_overlapping_blocks`
(the overlapping-block owner lives in `hosted/blocks.rs`); the other planned
selectors are unchanged.

Commands run on the final source (all passed):

```bash
mise exec -- cargo nextest run --locked -p wyrd-telemetry --features test-support --lib -E 'test(=tests::benchmark_histograms_preserve_bounded_intervals)'   # 1 passed
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E '<V2, V2a server selectors, V3, test(/^app::metrics::tests::/)>'   # 16 passed
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=engine::tests::execution_timing_separates_provider_wait_from_adapter_work)'   # 1 passed
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::query_request_lifecycle_reconciles_exact_terminal_outcomes) | test(=gate::tests::histogram_write_boundary_includes_durable_append)'   # 2 passed
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E '<V4, V5, V6 selectors>'   # 7 passed
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --lib --bins'   # 91 passed
mise run fmt && mise run lints && mise run check:deps && mise exec -- git diff --check
mise run bench:smoke   # exit 0, committed tree, no --allow-dirty
```

V8 resolved manifest (`capacity.rs` timing controls): server env
`WYRD_METRICS_BENCHMARK_SNAPSHOT=1` only; operator span export at its default
(disabled). The first smoke run failed at boot because the driver passed
`"true"` to the server's 0/1 flag; fixed in `2e6272c0d`, then rerun:

| Path | Histogram counts | CPU / P | Work |
|---|---|---|---|
| L1 bifrost | 2 `otlp.traces`, 2 `otlp.logs`, 2 `gate.write`, 1 `query` | 0.050 s / 32 | 15,332,359 memtable bytes |
| L1 verification | 4337 `verification.direct` | 1.270 s / 32 | 4337 verdicts, 38,016 component observations |
| L1 gateway | 1811 `buffered`, 1193 `stream.head`, 1193 `stream.end` | 1.970 s / 32 | 3004 validated calls, 3004 staged captures, 379 outbox writes |
| L2 bifrost | 2 `otlp.traces`, 2 `otlp.logs`, 2 `gate.write`, 1 `query` | 0.050 s / 32 | 15,339,863 memtable bytes |
| L2 verification | 3971 `verification.direct` | 1.260 s / 32 | 3971 verdicts, 35,158 component observations |
| L2 gateway | 5 `buffered`, 1 `stream.head`, 1 `stream.end` | 0.040 s / 32 | 6 validated calls, 6 staged captures, 19 outbox writes |

Material limits:

- Native Bifrost writes share one stream and carry no case label, so L2
  `write_ack_p99` includes the client-queue lanes' writes and its
  reconciliation counts are lower bounds.
- Tenants are never a snapshot label, so gateway L2 capacity SLOs pool
  tenants; chunk lag stays client-side and worst-tenant.
- Spec revision 8 is still a draft; this work followed the maintainer's direct
  instruction.

Non-goals stayed excluded. No generated artifact, public SDK API or dependency
changed. Untracked `benchmarks/**` records predate this task and were left alone.
