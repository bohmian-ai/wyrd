---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-benchmark-experiments
spec_revision: 5
requirements: [REQ-001, REQ-002, REQ-003, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-013, REQ-014, REQ-015, INV-001, INV-002, INV-003, INV-004, INV-005, INV-007, INV-009, AC-001, AC-004, AC-005, AC-007, AC-010]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6, FIND-TASK-001-7, FIND-TASK-001-8, FIND-TASK-001-9, FIND-TASK-001-10, FIND-TASK-001-11, FIND-TASK-001-12]
---

# Make the existing benchmark workflow trustworthy and usable

Route directly to `$wyrd-implement`. This document replaces only the instructions in `../TASK-001-R1/TASK-001-R1-restore-benchmark-evidence.md`; that earlier document remains historical context, not a second task. No source finding is closed and no new product scope is approved.

Authority: `changes/active/benchmark-experiments/spec.md`, approved revision 5. Original task: `changes/active/benchmark-experiments/tasks/TASK-001-benchmark-experiments.md`. Repository `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`; branch `change/benchmark-experiments`. Reviewed base `0739bbf38af7cc05bbfefb09850490c55432ec2d`; candidate `aaa152dd4243d57752b73d3a4e9c140bccc7d171`. Carry forward original review verdict, all reports, validated FIND-TASK-001-1 through -12 and verification under `../TASK-001-R1/`; independent instruction validation is `findings-validation.md` beside this task.

Two uncommitted repairs predate review: shared telemetry emits signed integer durations, and testing/capacity.rs adds the real-export regression. Preserve/finish them. The regression passed during review, as did nine runner tests, three focused engine tests and Clippy. Format check found one layout difference in the added test. These checks did not exercise a real release request or qualify a benchmark; no reviewed source was changed.

## Outcome and implementation boundary

An external contributor can use documented mise commands to run an experiment, obtain correctly labelled latency/resource/work measurements and inspect one saved record and its report. A short real-request smoke refuses broken measurement before a long run starts.

Keep the existing shared runner, record/report owner, release launcher, portable process readings and engine workload owners. Correct their behavior in place. Do not introduce another harness, measurement endpoint, timing transport, configuration language, generic scheduler, process-manager layer or dependency. This is one remediation with four execution stages, not four new approval/review rounds. Private helpers and local control flow remain implementation-owned.

The smoke and contributor-clarity requirements below are human-directed additions, not retrospective FIND IDs. Current approved L1/L2 coverage and profiling support remain. L2 is already separately invoked and profiling already opt-in; adding another optional mode solves nothing. Removing delivery/qualification obligations for L2, historical regression or profiling requires a separate spec revision.

## 1. Prove a real measurement before completing the rest

**FIND-TASK-001-1; REQ-007.** Candidate `crates/shared/wyrd-telemetry/src/lib.rs:323–344` records u64 nanoseconds, which the installed tracing visitor exports as strings; `crates/wyrd/wyrd-testing/src/capacity.rs:528–600` accepts OTLP integers. Every engine loses usable timing samples. Candidate tests stringify attributes or manufacture integer messages, bypassing this failed seam. Four collection waits can add 120 seconds to a nominal 35-second step.

Finish the existing signed-field repair at record_timing for elapsed and excluded components, with documented representable saturation. Retain strict integer parsing and existing server boundaries. The existing real init/exporter/Collector test, including a value above u32::MAX and component/residual assertions, crosses the actual failure; a permissive string parser or different exporter is unnecessary.

Then add and prove the mandatory release smoke on a clean committed tree. Reuse normal launch, public engine requests, production timing export, Collector decoder and resource/counter readers. Use tiny fixtures and a handful of requests through each distinct required timing producer/handoff, including required overhead components. Confirm nonempty parsed integer-nanosecond samples, available server CPU and provisioned CPU readings, and positive successful-work/counter evidence. A successful HTTP response or synthetic span alone is insufficient.

Require the same smoke before all L1/L2 execution paths, including aggregate/direct dispatch. A bench:smoke task may expose it, but dependency wiring must not leave another normal path able to omit it. Failure names the engine and exact missing/malformed timing boundary, component, resource field or counter, and aborts before long execution. Use seconds-scale work and a prompt bounded missing-evidence wait, not normal repeated 30-second collection waits. Record measured startup cost if it prevents that target; never omit a path to achieve a duration claim.

Smoke establishes measurement availability, not capacity, p99 qualification or precise short-window CPU efficiency. It does not require normal warmup/repetitions, percentile sample floors, large fixtures, historical comparisons or a complete L2 sequence. Build/prerequisite preparation remains outside smoke/experiment runtime clocks. Keep smoke diagnostics under target, clean up owned state, and do not publish it as qualified capacity history. Prove success and precise missing-evidence failure using existing mechanisms. Rerun after any later helper/collection/identity change and on final source before long runs.

## 2. Make collected evidence and CPU accounting agree

Paths in this and the next section are under `crates/wyrd/wyrd-testing/src/` unless fully qualified.

**FIND-TASK-001-4; REQ-006.** capacity.rs:647–740 starts Collector in driver Tokio tasks; driver_cpu_seconds measures the same process and shared validity judges combined cost as driver saturation. This violates the approved helper boundary and can falsely blame traffic generation. Run the existing Collector service as a separately measured declared plain-child helper using established launcher/process/sysinfo ownership. Retain its service/decoder; report independent identity/resources/health, and exclude its known owned PID from quiet-host checks without hiding unrelated processes. Server instrumentation/export remains server CPU. Missing helper readings, loss or collection saturation remain explicit invalidity. Estimated subtraction, relabelled combined readings and a second collector do not satisfy the requirement.

**FIND-TASK-001-5; REQ-007.** capacity.rs:611–640 retains unbounded sample/error vectors and startup permits usize::MAX messages. Supported windows can exhaust memory before partial evidence is saved; the required overflow-to-INVALID path does not exist. Apply finite declared incoming/retention/diagnostic bounds at this collector, stop retaining excess and preserve a bounded specific loss reason attributable to the affected measurement. Keep exact in-limit raw evidence. Window transitions must not erase or misattribute overflow. No new store/dependency or synthetic host saturation is needed.

**FIND-TASK-001-6; REQ-007.** bin/capacity/query/competing.rs:324–359 checks offered writes by boundary minima while incidental queue/probe traffic emits the same boundaries. A distinct background span can replace a missing offered span and still satisfy Collector::traced. Reuse existing request tracing through native/OTLP sender owners to associate each expected offered write with its boundary/outcome, and reconcile identities at the existing received-evidence owner. Preserve separate incidental populations and failure durations/outcomes. Lower-bound counts or a parallel identity/transport scheme do not close this gap.

**FIND-TASK-001-7; REQ-006.** Bifrost competing.rs:241–305 freezes Window before read_queries:544–550/read_ingest:618–624 count all drain completions. Verification run.rs:697 freezes CPU before arrivals:768–775 completes and verifications:503–513 sums late work against fixed seconds/CPU at 380–384. This inflates efficiency with tail CPU absent. At the existing Bifrost and Verification tally/read owners, credit measured W/B only within declared C/T endpoints. Keep every late completion/error, total expected timing and arrival/drain reconciliation separately. Preserve fixed windows and total background CPU; do not forgive late errors or discard tail evidence. No Gateway expansion or Verification stage/submission redesign is prescribed.

In records and reports, distinguish available logical CPUs P from average used cores C/T. Derive provisioned throughput (W/T)/P, efficiency W/C and CPU/operation C/W from the same declared processes/windows and preserve their operands/units. Missing/reset readings are unavailable, not favorable zero values. These are corrections to existing owners, not separate accounting frameworks.

## 3. Complete engine measurements and stop owned work correctly

**FIND-TASK-001-2; REQ-006/007/014.** Bifrost isolated.rs:158–189 substitutes window-average scan counters and permanently missing storage ratio; query/mod.rs:837–980 and competing.rs:241–340 omit required L2 byte/provisioned/CPU-operation and related facts. Shared derivation cannot invent undeclared operands. Oracle already owns final scan facts (`crates/vala/vala-bifrost-redux/src/oracle/mod.rs:558–615`), but current timing evidence cannot reconstruct request-specific scan/time triples.

Extend existing Bifrost declarations/readings and collector projections. Attach Oracle-owned final scan rows/bytes through existing trace context, associate with request/process/window/server time, and retain the summary evidence needed for record-only reporting. Use existing Residence/Scribe/storage settlement for published stored bytes and retained Arrow input bytes describing the same data; declare settlement and excluded WAL/staging/catalog. Supply required mixed W/B operands and derive both C/P/T normalizations, CPU/op and other applicable metrics through existing Repetition. Keep total mixed CPU rather than arbitrary per-class attribution. Missing required facts cannot become zero or inapplicable. No second scan/pipeline, new endpoint/identity metric label or per-window fixture rebuild is required; any genuinely new public/architecture decision invokes the original material stop condition.

**FIND-TASK-001-3; REQ-009.** query/mod.rs:879–912 declares only ingest rate and competing.rs:456–488 waits for each query before the next, so slower service silently reduces offered query load. Extend the existing mixed-query declaration/driver with a fixed offered rate sufficient for at least 4,000 queries/200 scans per six-minute repetition at the unchanged mix, independent of completion and under the unchanged 32-client bound. Use existing scheduled/sent/completed/missed/lateness validity. Preserve ingest calibration, phases and sample floors; no generic scheduler or extra option is earned.

**FIND-TASK-001-8; REQ-001/007/013.** levels.rs:136–146 supplies common timing controls, then gateway/mod.rs:227–268,502–550 replaces them. Extend shared controls with Gateway fields instead, preserving clock/resolution, collection configuration and cost attribution through both levels' records. Reuse common declaration rather than duplicate it.

**FIND-TASK-001-9; cancellation/partial-evidence contract.** Gateway Driver::spawn/Capacity::step (gateway/capacity.rs:806–856,999–1018) detach scheduling when measurement is cancelled, and open_loop:313–349 detaches per-request work. finish:1125–1133 can drain while new requests keep arriving. Give both scheduler and descendants measurement-lifetime ownership using existing Tokio owned-task/cancellation mechanisms. Stop admission and cancel or boundedly reconcile in-flight work before drain. Preserve normal rates, upstream/capture, admission and durability; stopping the outer task alone is insufficient.

**FIND-TASK-001-10; REQ-015.** experiment.rs:1650–1697 awaits optional saving before drain/budget; profile.rs:134–179 can wait 60 seconds per saver, invalidating an otherwise within-budget run. At existing runner/Profiler owners, stop capture without postponing required drain and prevent optional artifact-saving delay from determining workload budget judgment. Preserve bounded capture ownership, actual finalization timestamps, incomplete notes, measured instrumentation cost and genuine workload overruns; do not increase budget or replace profiler.

**FIND-TASK-001-11; mandatory rustdoc.** Batch accurate Errors/cancellation/partial-effect/retry docs with affected lifecycle edits: Workload prepare/measure/finish (experiment.rs:1342–1357), Isolated implementations (isolated.rs:232–235,313–324), Verification Bench prepare535/measure581/finish594; add Panics sections on cited telemetry/lib.rs:560–563 and Oracle exec.rs:2087–2090 tests. These eleven existing items fail the explicit gate. Document actual behavior, including internally synchronous finish bodies; do not promise absent guarantees. No additional test/checker or documentation-only round.

## 4. Demonstrate normal use and retain honest qualification evidence

Update the existing benchmark README/command descriptions only where needed so an external contributor can identify prerequisites, run smoke or one engine, understand expected duration, locate JSON/raw output, interpret metric units/CPU distinctions, distinguish FAIL from INVALID and regenerate the report. Reuse existing mise prerequisite/launch machinery; do not introduce a second setup script or contributor configuration surface. Verify the documented commands against actual behavior rather than adding a documentation gate.

Commit coherent fixes before experiments using configured identity and plain messages; never git config, author overrides or push. Run every command through mise, and no benchmark with --allow-dirty. Preserve prior summary outputs explicitly before declaring the next source snapshot clean; never delete evidence to clear dirtiness. No builds, other benchmarks or review workloads may overlap measurement. Raw logs/samples/profiles remain under target; benchmark commands never auto-stage/commit output.

After final clean-tree smoke, run `mise run bench:l1:bifrost`, then `mise run bench:l1:verification`, then `mise run bench:l1:gateway`, reporting after each and retaining records/artifacts. Diagnose budget from fresh valid timings.jsonl plus existing lifecycle timestamps/logs. Full Bifrost L1 has 20 measured steps: 700 seconds mandatory windows, about 16 seconds normal collection settling and 20 seconds host checking leave about 164 seconds for other lifecycle cost inside 900 seconds. Broken export adds four 30-second waits per step; deleted bad-run timings cannot identify a separate setup bottleneck. Measure remaining startup/fixture/publication/collection/drain cost before optimizing the existing harness, and demonstrate headroom without shortening windows, changing fixture/coverage, weakening sample floors or optimizing production engines. Preserve actual finish time and partial overrun evidence.

**FIND-TASK-001-12; original final qualification.** Original AC-001/005/007/010 and V13–V15 require real Linux release/production evidence, not scripted judgment inputs. No retained source-qualified evidence currently establishes those obligations. This is missing acceptance proof, not a claim that every historical command was never executed. Preserve original complete L1/L2/production qualification once at the final execution/change gate: exact source/commands, matrices/windows, raw matched timing and C/P/T/W/B recomputation, overlap timelines and persisted/regenerated report agreement. Optional macOS arm64/Linux arm64 hardware runs remain optional. Do not copy full journeys into each remediation stage or claim original completion from smoke/unit proof. Valid performance FAIL is distinct from INVALID and never permits threshold/baseline tuning.

## Focused closure evidence

Use existing inline tests/local OTLP/upstream fixtures and controlled observations/time; do not build another test framework. One focused regression may cover several properties if it asserts each. Confirm exact selectors for every newly named test before execution; compiler/setup failure or empty selection is not behavioral RED. Existing correct behavior uses verification-only evidence rather than manufactured churn.

| Finding | Minimum direct proof |
|---|---|
| 1 | Real telemetry/export/collector preserves exact integer elapsed/excluded values above u32::MAX and residual; strict malformed rejection retained |
| 2 | Distinct out-of-order scan/time triples stay paired; same-data settled bytes and known W/B/C/P/T recompute required L1/L2 values; persisted report retains facts; missing remains unavailable/INVALID |
| 3 | Slow response does not postpone due arrivals; required offered population/mix arithmetic holds; exhausted capacity/missed/lateness invalidity remains |
| 4 | Distinct helper/driver identity/readings/health, owned-PID host exclusion, start/stop/loss and missing evidence honest |
| 5 | Small real-OTLP receiver overflow leaves bounded state/specific attributable INVALID; in-limit and window-transition behavior correct |
| 6 | Missing offered span plus unrelated same-boundary span remains INVALID; complete out-of-order matching passes; duplicate/outcome failure explicit |
| 7 | Before-end and late completions in both engines credit only first to measured W/B, preserve both for total timing/drain/error accounting, keep C/T endpoints |
| 8 | Both actual Gateway declarations preserve common timing controls through persist/read/report |
| 9 | Cancel actual scheduler before later due arrival: no new launch or live descendants before finish; normal counts and partial INVALID retained |
| 10 | Delayed/unsavable capture leaves within-budget verdict/drain unchanged with note; genuine overrun INVALID with/without profile |
| 11 | Eleven cited docs match full bodies; existing format/lints; no new runtime test |
| 12 | Original final qualification remains visible and is completed once at owning gate; no source-only completion claim |

Human smoke additionally proves real success and exact missing-evidence diagnostics on every normal engine invocation, before long work, against final measurement plumbing. It is not a replacement for the focused negative cases or original qualification.

Existing focused command inventory (new owner regressions require their confirmed exact selectors/setup):

```text
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=capacity::tests::exported_timing_spans_parse_exactly)'
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(/^capacity::tests::/) | test(/^experiment::tests::/)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity
mise run fmt
mise run lints
```

Use narrow exact production-owner selectors/features for changed Oracle/timing seams; discover setup from owning mise tasks/TESTING. Record each new selector, expected RED, GREEN and focused regression results, smoke success/failure, immutable commits, real budget attribution and each requested L1 result. No gate/full journey aggregate is added to focused remediation. Carry prior accepted source evidence into follow-up review; reopen only affected paths.

## Scope discipline

Retain approved server-only timing boundaries, raw nearest-rank percentiles, fixed windows/three repetitions, full L1/L2 coverage, provenance/regression policy, existing opt-in profiling, auth/tenancy/audit, acknowledged durability/publication, bounded admission and capture integrity. Preserve valid failures beside invalidity. Cleanup stays benchmark-owned.

Do not add baseline-format migration, dead-helper cleanup, singleton resource refactor or a new declaration DSL to this task merely to reduce line count; none closes a retained material finding. Baseline review found no broken-new-run contamination. Changing required coverage, timing meaning, sample/SLO thresholds, CPU denominators or mandatory history/profile support requires the relevant spec revision, not a quieter remediation edit.

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-6 trace identity | `c6b173559` capacity.rs collect | `capacity::tests::collector_bounds_identity_and_loss`; Bifrost L1/L2 smoke | PASS |
| FIND-7 work within C/T | `7d76fd55c` competing.rs, verification/run.rs | `query::competing::tests::only_answers_before_the_window_end_are_work`, `verification::run::tests::tallies_separate_declared_admission_refusals` | PASS |
| FIND-10 profile save outside drain/budget | `108ba9f5a` experiment.rs, profile.rs | `experiment::tests::profile_saving_never_delays_drain_or_budget` (RED by inlining save) | PASS |
| FIND-8 shared timing controls | `b640e689c` gateway/mod.rs | both Gateway declaration tests assert `timing` | PASS |
| FIND-9 owned Gateway arrivals | `6d9fc5874` gateway/capacity.rs | `gateway::capacity::tests::cancelled_scheduler_launches_nothing_and_its_calls_are_joined` | PASS |
| FIND-11 rustdoc | `3d454f503` | `mise run lints` | PASS |
| FIND-3 fixed mixed-query rate | `1cd13d656` competing.rs | `query::competing::tests::mixed_queries_are_offered_at_a_fixed_rate` | PASS |
| FIND-2 per-query scans, storage ratio, L2 bytes | `bed0fe00e`, `bf2729259` oracle/mod.rs, capacity.rs, isolated.rs, competing.rs | `capacity::tests::collector_bounds_identity_and_loss` (RED: join disabled), `query::tests::parquet_bytes_count_only_the_tables_own_objects`, `oracle::published::published_cache_pruning_and_shutdown_are_production_governed` journey, Bifrost L1/L2 smoke | PASS |
| Stage 4 README/mise | `6587104e6` | commands checked against `bench:smoke` and L1 behavior | PASS |
| Clean-tree `mise run bench:smoke` | at `6587104e6` | all six paths PASS, exit 0 | PASS |
| `bench:l1:bifrost` | record `benchmarks/bifrost/isolated/20261024T161418Z-6587104e6.json` | INVALID: 911.2 s over 900 s; otlp-spans-c64 and otlp-logs steps unmeasured; also FAIL point-c64 p99 14.6 ms and W/C regressions | FAIL |
| `bench:l1:verification`, `bench:l1:gateway` | — | not run; stopped by user | BLOCKED |

Budget attribution from `timings.jsonl` of the INVALID run: host check + smoke end at 78.5 s (smoke publication settle 62 s); prepare 252 s, of which three per-path storage settles ≈ 185 s (fixed by `bf2729259`, one shared settle); 15 measured steps took 565.7 s against 525 s of windows. Every settle takes 61–63 s regardless of size: the declared `WYRD_MAX_FILE_RETENTION_TIME` of 30 s waited out twice. Projected after the fix: ≈ 145 s lifecycle + ≈ 755 s steps + 10 s host ≈ 910 s, still over 900 s. A rerun at `bf2729259` was stopped by the user at 785 s (otlp-logs-c4), so it left no record.

Diagnostic one-step records `20261024T163035Z` (INVALID: `WYRD_LOG=warn` inherited by the server suppressed its timing spans) and `20261024T163306Z` (point-c1 only) are retained untracked. Harness gap: the release server inherits the driver's `WYRD_LOG`.

FIND-12: qualification stays open; none is claimed from smoke or unit proof.

Status: BLOCKED. Fitting Bifrost L1 in 900 s with headroom needs a decision outside this task: a server flush route for the release server, a changed read fixture or tier, or a different budget. Each needs `$wyrd-spec`. The remaining L1 runs wait on that decision and on the user's go-ahead.
