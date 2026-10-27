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

# Restore trustworthy benchmark evidence before long qualification

Route directly to `$wyrd-implement`. Approved authority is `changes/active/benchmark-experiments/spec.md`, revision 5. Original task is `changes/active/benchmark-experiments/tasks/TASK-001-benchmark-experiments.md`. Review verdict and complete validated ledger are beside this task in `verdict.md` and `findings-validation.md`.

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`, branch `change/benchmark-experiments`. Reviewed base `0739bbf38af7cc05bbfefb09850490c55432ec2d`; candidate `aaa152dd4243d57752b73d3a4e9c140bccc7d171`. This is the first remediation; no prior closure is assumed. Retain these identities and cumulative original acceptance context in later review.

Two pre-existing uncommitted repairs remain: telemetry lib.rs emits signed nanos, capacity.rs adds exported_timing_spans_parse_exactly. They were not part of reviewed candidate. Their real-export test passed during review; preserve and finish them rather than rediscovering/replacing the fix. verification.md records their diff digest and check results. Format check found only the new test's Collector startup expression needs rustfmt layout. No repair was committed or changed by review.

## Outcome and ordered work

A real request must produce usable exact server evidence before the harness starts a long experiment. Long runs must retain complete, correctly attributed metrics and honest verdicts rather than waiting for undecodable spans or publishing favorable incomplete populations.

The following smoke/commit/run instructions are **human-directed additions**, not retrospective reviewer findings:

1. Finish the existing signed-field fix and real-export regression first. Run its exact selector below, apply formatting, and commit the bounded repair using configured identity.
2. Add a seconds-scale release-server smoke for every engine, then prove it on a clean committed tree. Each engine starts the release server, sends a handful of real public-path requests and verifies nonempty parsed integer-nanosecond timing samples, available server CPU evidence and the engine's throughput/work counters. Cover the engine's required timing producers/handoffs, not a synthetic span alone. Missing evidence must identify the engine and exact missing/malformed boundary, field or counter and abort immediately.
3. Make smoke unavoidable for each `bench:l1:*` and `bench:l2:*` run and the aggregate path. An opt-in `bench:smoke` task with bench dependencies is acceptable only if all actual invocation paths reach it. Reuse existing launcher, client/workload, Collector and counter owners; keep smoke outside CI/gates and avoid full fixture loading or normal warmup/repetition durations. Build prerequisites remain outside experiment clocks. Smoke proves collection, not capacity or full-matrix coverage; keep its raw diagnostics under target rather than inventing a new historical capacity record.
4. Complete the bounded validated corrections below. Preserve repeated smoke proof after shared collection/helper changes; a passing early smoke cannot certify changed later collection. Commit coherent fixes before real benchmark runs.
5. Diagnose Bifrost budget using fresh valid artifacts, retaining timings.jsonl and existing lifecycle timestamps/logs. Attribute startup, fixture/publication, collection, measured windows and drain before optimizing the benchmark harness. Full L1 has 20 measured steps, not 12: required windows consume 700 seconds, normal collector settling about 16 seconds and host checks 20 seconds, leaving about 164 seconds for other lifecycle costs. Broken unsigned export adds four 30-second waits per step; the deleted bad run cannot prove a separate setup bottleneck. Finish well under the existing 900-second limit with measured headroom, without shortening windows, omitting coverage, weakening sample floors or optimizing production engines. Do not guess historical phase costs. If further harness cost reduction is needed, use measured cause and existing owners.
6. Run `mise run bench:l1:bifrost`, then `mise run bench:l1:verification`, then `mise run bench:l1:gateway`, one at a time, reporting the result after each. Run on a clean tree without `--allow-dirty`. Preserve each record/raw artifact and distinguish measured performance FAIL from INVALID. Account for prior generated summary outputs before declaring the next source snapshot clean; never erase evidence to clear dirtiness. Do not overlap benchmarks with builds, other benchmarks or review workloads.

Every command goes through mise. Commit often with plain messages and locally configured identity; never git config or author/committer overrides; never push. Benchmark commands never auto-commit output. Keep existing valid failures visible even beside invalidity; the broken run cannot establish a code performance regression.

## Validated diagnosis and selected corrections

### 1. Restore the shared integer producer — FIND-TASK-001-1

REQ-007 requires exact integer durations through normal operator OTLP. At candidate telemetry/lib.rs:323–344, record_timing emits u64; installed tracing-opentelemetry converts it through debug-string handling. capacity.rs:528–600 accepts OTLP integers only. All real engine timing paths therefore fail, while the candidate's stringified/direct-integer tests bypass this representation seam. Collector's count wait then adds up to 30 seconds after every warmup/repetition and materially exhausts L1.

Finish signed integer emission for both elapsed and excluded nanos at the existing shared producer, with checked/saturating representability and accurate rustdoc. Retain the strict decoder and existing timing boundaries/callers. The existing uncommitted repair is at this root. No string parser fallback, second exporter, longer budget or shorter measurement is justified. The existing real pipeline regression is the smallest credible proof because it crosses the failed seam and asserts values above u32::MAX.

### 2. Collect required Bifrost facts — FIND-TASK-001-2

REQ-006/007/014 require both CPU normalizations, byte/work operands, settled storage/memory metrics and request-specific executed scan rows/bytes alongside server time. isolated.rs:158–189 reports window-average scans and permanently missing storage ratio. competing.rs:241–340 discards scrape operands; query/mod.rs:837–980 omits required provisioned throughput, byte efficiencies, CPU/op and other applicable metrics. Shared derivation only derives declared metrics. Oracle already owns final scan facts (oracle/mod.rs:558–615), but process counters and Timing's duration-only evidence cannot reconstruct unequal requests.

Extend existing Bifrost declarations/readings and Collector projections, not a second measurement pipeline. Attach already-owned final Oracle scan rows/bytes to existing tracing context and associate them with query trace/process/window/server timing. Retain facts necessary for record-only reporting, with missing facts explicit. Use existing Residence/Scribe/storage evidence to measure published stored bytes and retained Arrow input for the same data after declared publication/compaction settlement; state excluded WAL/staging/catalog. Supply required mixed W/B operands and derive C/P/T metrics through Repetition, retaining total mixed server CPU rather than arbitrary attribution.

An omitted required reading cannot become inapplicable. Relabelling averages or missing values cannot close the obligation. No new endpoint, identity-bearing metric label, duplicate scan, generic store or per-window fixture rebuild is needed. If a new public/architecture decision actually becomes necessary, stop under the original material stop condition; this task does not authorize one.

### 3. Declare and schedule L2 query arrivals — FIND-TASK-001-3

REQ-009 requires an offered query rate capable of at least 4,000 total queries/200 scans per six-minute repetition at the unchanged mix. query/mod.rs:879–912 declares only ingest rate; competing.rs:456–488 runs 32 closed loops awaiting results. Slow completion reduces offered work, and later insufficient-sample rejection cannot replace the missing declaration.

Extend the existing mixed-query declaration/driver with fixed offered query rate and completion-independent scheduling under the unchanged 32-client bound. Use existing arrival accounting for query scheduled/sent/completed/missed/lateness and shared invalidity. Preserve harmonic ingest calibration, 60/25/10/5 mix, phases and sample floors. Existing owned scheduling mechanisms suffice; a generic replacement driver/server is unearned.

### 4. Separate helper accounting — FIND-TASK-001-4

REQ-006 explicitly separates collector/helper CPU from driver CPU. capacity.rs:647–740 starts Collector in driver Tokio tasks and declares their cost as driver cost; driver_cpu_seconds reads that combined process and experiment.rs:2087–2109 judges it for saturation. Records cannot distinguish traffic-driver saturation from collector processing.

Run the existing Collector as a separately measured declared plain-child benchmark helper, using existing process/launcher/sysinfo owners and retaining its service/decoder. Record helper identity, resource and collection health independently. Keep server instrumentation/export in measured server CPU. Helper loss, missing required resource evidence or collection loss/saturation must remain honest invalidity. The helper boundary is already approved; estimates, combined-reading relabels and another collector transport do not close it. Private child mechanics remain implementation-owned.

### 5. Bound receiver retention — FIND-TASK-001-5

The spec requires bounded collection and overflow INVALID. capacity.rs:611–640 has unbounded timing/malformed vectors; Collector startup allows usize::MAX decoding. Long windows/setup can accumulate until exhaustion, losing partial evidence. Server BSP bounds do not bound receiver accumulation across exports.

Apply finite declared incoming/retention/diagnostic bounds at existing Collector. Stop retaining excess and preserve a bounded precise overflow/loss reason attributable to affected measurement, while retaining in-limit exact/raw evidence. Do not silently clear loss on a window transition or claim completeness after dropped evidence. Conventional finite limits suffice; no new store/dependency or synthetic host saturation.

### 6. Reconcile offered writes by identity — FIND-TASK-001-6

REQ-007 forbids missing required timing being replaced by a sampled subset. competing.rs:324–359 gives successful explicit queries trace requirements but offered writes only boundary minima. Concurrent native queue lanes emit the same boundary. Collector::traced permits extra spans, so a lane span can replace a missing offered native span; read_ingest judges that substituted population.

Propagate existing request trace/correlation through native and OTLP sender owners for every offered write and reconcile its expected boundary/outcome in existing Collector. Keep incidental lane/probe timing populations distinct and preserve failure durations/outcomes. Reuse request tracing and current transport, not a second identity scheme. Exact all-writer counts are insufficient where intentional background populations differ; lower bounds cannot prove required identities. Collector owns the received-evidence completeness guard.

### 7. Align measured work with CPU/time — FIND-TASK-001-7

REQ-006 requires W/C/T to share a window. Bifrost competing.rs:241–305 freezes Window at deadline, then read_queries:544–550/read_ingest:618–624 count successful drain completions. Verification observe at run.rs:697 freezes CPU; arrivals:768–775 then records/joins late calls; verifications:503–513 credits them against fixed seconds/frozen CPU at 380–384. Verification's latency end filtering does not filter this numerator. A final in-flight success can improve efficiency with its tail CPU excluded. followup-review.md independently establishes the Verification sibling.

At existing Bifrost and Verification tally/read owners, use completion cutoffs/timestamps to credit measured successful W/B only within declared C/T. Preserve every late completion, error, full expected timing count and arrival/drain reconciliation separately. Keep fixed resource endpoints and background CPU; do not forgive errors, remove tail timings or extend repetitions arbitrarily. The same invariant is corrected in both siblings; no Gateway expansion or redesign of Verification stages/submission semantics is prescribed.

### 8. Preserve shared Gateway controls — FIND-TASK-001-8

REQ-001/007/013 require recorded timing clock/resolution/configuration/cost. levels.rs:136–146 inserts common controls[timing], but gateway/mod.rs:227–268 and 502–550 replace controls, losing it at both levels despite runtime tracing remaining active.

Extend the assembled controls with engine fields rather than replace them. Reuse the common declaration, without new configuration or a duplicate timing declaration. Assert both real Gateway declarations preserve these fields through persisted read/report regeneration.

### 9. Own all Gateway L2 tasks — FIND-TASK-001-9

The shared cancellation contract requires stopping admission and boundedly reconciling owned work before drain. Capacity::step awaits Driver::spawn's JoinHandle (gateway/capacity.rs:806–856,999–1018); cancellation detaches it. open_loop:313–349 independently detaches per-request tasks. finish:1125–1133 starts server shutdown without stopping these task levels. Supported Ctrl-C/deadline can therefore keep launching during drain.

Reuse existing Tokio owned-task/cancellation mechanisms so the scheduler and every request descendant are owned by measurement lifetime. Stop new launches and cancel or boundedly reconcile in-flight requests before server drain. Fixing only outer scheduler ownership is insufficient. Preserve normal schedule/rates, admission, upstream/capture and durability semantics; no process-manager layer.

### 10. Keep optional saving out of workload judgment — FIND-TASK-001-10

REQ-015 says an unusable profile gives a note, never a changed verdict. experiment.rs:1650–1697 waits for Profiler::finish before workload drain and budget judgment. profile.rs:134–179 can wait 60 seconds per unresponsive saver; this alone turns a within-budget workload INVALID and delays drain.

At existing runner/Profiler owners, initiate capture stop without postponing required workload stop/drain. Optional artifact-save delay must not supply workload budget judgment. Retain bounded capture ownership, missing/incomplete notes, actual finalization timestamps, real instrumentation cost during work and genuine workload overruns. Do not increase budget, suppress real overruns or replace profiler.

### 11. Batch mandatory documentation — FIND-TASK-001-11

Root AGENTS and Rust guide explicitly gate changed-item Errors/Panics and cancellation/partial effects. Eleven cited items lack that contract: Workload prepare/measure/finish (experiment.rs:1342–1357); Isolated implementations (isolated.rs:232–235,313–324); new assertion-bearing telemetry and Oracle scan tests (candidate telemetry/lib.rs:560–563, oracle/exec.rs:2087–2090); Verification Bench prepare535/measure581/finish594. Follow-up independently checked the latter bodies.

Add accurate Errors/cancellation/partial-effect/retry docs to these lifecycle items and Panics sections to cited tests. Document actual synchronous behavior inside finish where applicable, not imaginary await cancellation guarantees. Batch with functional work; no new checker or style-only round. This is a mandatory gate, not an additional asserted lifecycle defect.

### 12. Preserve original final qualification — FIND-TASK-001-12

AC-001/005/007/010 and original V13–V15 require real Linux release and production proof. No retained source-qualified record establishes it. Scripted tests and repair-only export success cannot demonstrate full duration, actual CPU arithmetic, raw matched samples, concurrent workload phases or production guarantees. This states unavailable acceptance evidence, not that every historical command was never run.

After source fixes, preserve the original final execution/change-review qualification once: exact commands and source identities, complete matrices/windows, raw timing and C/P/T/W/B recomputation, overlap timelines and persisted/regenerated report agreement, with honest verdicts. Begin with the user-ordered smoke and separate L1 runs above. Optional macOS arm64/Linux arm64 hardware runs are not new blockers.

This correction adds no new broad suite or product behavior. Do **not** copy full V13–V15/user-journey commands into focused remediation proof below. Original final qualification remains explicitly pending until run at its proper gate; a source-only fix review cannot claim original TASK-001 complete.

## Preserved behavior and non-goals

Keep approved L1/L2 coverage, fixture controls, fixed windows, three repetitions, raw nearest-rank latency and strict SLO/sample floors. Keep normal auth, tenant checks, non-blocking audit cost, acknowledged durability/publication, bounded admission and capture integrity. No engine optimization, baseline promotion/tuning, parser weakening, OS envelopes, platform branches, new dependency, public product/API or alternative transport. Baseline audit found no broken-new-run source contamination; preserve existing eligible/excluded references and arithmetic. Missing facts are not zero or inapplicable, and INVALID retains valid failures.

Reuse existing owners in the original task and validated ledger. Commit raw samples/logs/profiles nowhere; keep summary evidence attributable and append-only through normal explicit Git work. Cleanup is limited to benchmark-owned state. Keep reviewed source context and prior reports for focused remediation review.

## Focused acceptance and proof

Implement behavior scenarios one at a time under existing Red/Green/Refactor discipline. Confirm actual exact selectors before running new tests; no invented existing test names, empty selections or compiler/setup failures as RED. Use controlled observations/clocks/local OTLP/upstream, not synthetic host load. Every newly named test must have its exact mise/nextest command in implementation evidence.

| Finding | Required focused observation |
|---|---|
| 1 | Real exporter/Collector receives exact signed integer elapsed/excluded nanos above u32::MAX; residual correct; strict malformed rejection preserved |
| 2 | Unequal out-of-order request scans stay paired with exact request time; required L1/L2 metrics survive record-only regeneration; known C/P/T/W/B and settled same-data storage recompute correctly; missing required facts stay explicit |
| 3 | Slow completion cannot shift due arrivals; rate offers at least 4,000 total/200 scans at fixed mix/client bound; misses/lateness invalidate |
| 4 | Independent helper/driver identities/readings/health; helper startup/stop/loss and missing evidence honest; owned helper excluded from quiet-host without hiding unrelated work |
| 5 | Exceed a small receiver bound over local real OTLP: bounded retention/diagnostics, precise attributable overflow INVALID; in-limit exact path green |
| 6 | Missing offered-write span plus unrelated same-boundary span remains INVALID; complete out-of-order matches pass; duplicate/outcome failures explicit |
| 7 | Before-end and late successes: only first credited to measured W/B/C/T derivatives, both retained for full completion/drain/timing reconciliation; prove in Bifrost and Verification |
| 8 | Shared timing controls survive both real Gateway declarations and persisted record/read/report |
| 9 | Cancel actual scheduler before a later due request: no later launch/live descendant before finish; normal schedule counts and partial INVALID preserved |
| 10 | Near-budget workload plus delayed/unsavable capture has no-profile verdict, prompt drain and note; genuine overrun INVALID with/without profile |
| 11 | Eleven cited rustdoc blocks match complete bodies and actual lifecycle; format/lints pass, no checker/new runtime test |
| 12 | Original qualification remains tracked and runs at final gate once; no unit-only completion claim or added broad remediation suite |

Human smoke acceptance is separate: per-engine real release requests produce nonempty parsed integer server samples and CPU/work evidence in seconds, with exact missing-evidence diagnostics; every L1/L2 path requires it and aborts before long work on failure. Prove both success and missing-evidence failure. A synthetic span regression alone cannot meet this addition.

## Commands and execution evidence

Existing exact export regression (confirmed and run during review):

```text
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=capacity::tests::exported_timing_spans_parse_exactly)'
```

Existing focused owner/declaration lanes, plus confirmed exact selectors for newly added regressions:

```text
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(/^capacity::tests::/) | test(/^experiment::tests::/)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::level_one_declares_complete_engine_coverage)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::level_two_preserves_concurrent_work_and_recovery)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::level_commands_preserve_records_and_exit_precedence)'
mise run fmt
mise run lints
```

Use narrow exact owner selectors/features for any additionally changed Oracle/production instrumentation. Establish the owning test setup via mise.toml/TESTING first. No `gate`, full journey suite or broad aggregate is added to focused remediation. The human-directed release smoke and ordered L1 commands above are explicit requested execution, not substitutes for final original qualification. Never run experiments dirty or concurrently with Cargo checks.

Evidence must include the expected RED gap, GREEN and focused regression results, exact commands, immutable source/repair commits, smoke success and precise failure demonstration, valid phase-cost/budget evidence, each separate L1 outcome and preserved records/raw references. Full final qualification status must remain visible. Return to focused task review with prior candidate/verdict/ledger and the cumulative new candidate; carry forward unaffected evidence rather than restart discovery.
