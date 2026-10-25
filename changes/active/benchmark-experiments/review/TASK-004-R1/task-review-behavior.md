# TASK-004 independent behavior discovery

**Result: FAIL.** Three material proposed findings need independent validation.

Subject: first review of `0b500d677...a020500e8ea0c39311d508d6b5bfa85ebe90368c`. HEAD was checked as the candidate. The original task was read at base. Revision-8 approval/identity is as recorded in scope.md. No previous TASK-004 acceptance is carried forward. No other discovery reports or implementation chat summary were read.

## Coverage and verification

Read task-review skill and scope.md first, then the original task, approved spec, root rules, spec-driven workflow, maintainer/Rust guides, TESTING, Bifrost measurement meanings and relevant security principles/isolation/privacy. Used CodeGraph before code navigation. Expanded navigation through Population/Window/ResourceWindow, all L1/L2 engine conversions to Repetition, query offer/mixed/probe/queue lanes, Gateway open_loop/Driver/phase snapshots, Verification arrivals/observe/Ended, Reading/derive/judge, Record parse/publish/render, benchmark metadata, histogram recording/snapshot/delta, HTTP/gRPC producers, config/router/listener and LocalServer. No nested AGENTS applies.

Source stayed immutable; no builds/tests/benchmarks were run by this reviewer. Central `verification.md` records independently rerun V1 (1), server owner/metrics group (16), Gateway (1), Gate (2), V4–V6 (7), Postgres lib/bins (91/91), bench:smoke (exit 0, eight dispatched invocations), format/dependency/diff checks. These are credible for their exercised paths. The defects below are not contradicted by those green checks: V4 constructs snapshots directly and does not run an actual driver across admission-stop; smoke has no normal cohort/window/artifact assertions.

## Acceptance matrix

| Requirement, criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Preserve engine monotonic boundary timers and per-request wait union | Verification emit/hosted blocks, Gateway point, query QueryTiming, Gate writes, gRPC OTLP wrapper | V2/V2a and real smoke | PASS |
| Bounded integer-ns three-digit HDR independent of operator export | timing Registry::record/Buckets/Distribution; record_timing directly invokes owner | V1/V2/V8 | PASS |
| Zero counted; missing/negative/overflow/reset invalid | IntervalSeries::problems, TimingSnapshot::since, Population::from_interval | V1/V2/V6 | PASS |
| Strict comparison uses whole quantization interval | Reading::quantile/bounded, Op::test/judge_criterion | V1/V6 threshold cases | PASS |
| Consistent cumulative process/method metadata and bucket deltas | locked Registry::snapshot and TimingSnapshot::since | V1/V3 | PASS |
| Drain warmup before next baseline | awaited L1 drivers and Gateway settle; L2 sequence/drain returns before next measure | owner source, V4 arithmetic | PASS |
| Freeze CPU/completed work at admission-stop; separate measured/drain populations/windows | L1 and Gateway L2 close after requests drain; Window CPU follows scrape | B-001; V4 does not invoke these callers | FAIL |
| Phase/class/outcome cohorts reconcile and retain final slow outcomes | Actual producers omit phase; native writers unlabelled; SLO slices by completion | B-002 | FAIL |
| Matching scan distributions remain associated with query population | Oracle emits classified scans, but consumers fold/discard full distributions | B-002/B-003 | FAIL |
| Local opt-in read-only bounded snapshot; refuse non-loopback; no identities/reset | MetricsConfig, metrics_router, bounded fixed schema | V3/V7 and source | PASS |
| Existing listener/client/Prometheus owner; no production tool dependency | existing metrics lifecycle, LocalServer extension; HDR narrow telemetry dependency | source, check:deps | PASS |
| Mandatory smoke before prepare on direct/aggregate dispatch | experiment::run smoke then prepare; levels::smoke same Workload owner | V6/V8 and dispatch source | PASS |
| Missing CPU/P/timing invalidates, no zero substitution | Smoke::usage/timings, Resources, Population::refused, repetition_reasons | V1/V3/V4/V6/V8 | PASS |
| v2 write, v1 read/report, unknown versions refused | constants and Record::parse/render; legacy optional collector CPU | V5/V6 | PASS |
| Retain auditable histogram artifacts and structured reconciliation/window metadata | Population collapses to Reading/prose; no histogram artifact writer | B-003 | FAIL |
| One record/report owner; collision safety and persistence failure exit | Record::publish; levels::execute failure/salvage | V5/V6 and source | PASS |
| Preserve normal metrics/auth/storage/audit; no unrelated rewrite/migration | producers retain old timing owners; opt-in extra readout; no application route/storage rewrite | actual diff/source | PASS |

## B-001 — INCORRECT: real runner clocks include final in-flight completions as measured work

**Authority:** task What Changes, Scenario 5; spec REQ-007 lines 255–262, REQ-006, AC-004/005/012 require admission-stop CPU/work freeze and separate latency-cohort/drain windows.

**Locations:** `query/isolated.rs:123–198`; Gateway `run.rs:553–571,656–683`; Verification `run.rs:279–345,942–968`; Gateway `capacity.rs:947–995`; shared `capacity.rs:310–362` (Window). Engine paths are under `crates/wyrd/wyrd-testing/src/bin/capacity/`; shared capacity.rs is under `crates/wyrd/wyrd-testing/src/`.

**Source path:** All three L1 drivers stop issuing calls at a deadline, await each outstanding request and join clients before returning. Only afterward does the caller close Window. Every acknowledged call/row is credited to requests/rows/verifications, and seconds includes waiting for the last response. Verification's closed driver explicitly records `late=false`. Gateway L2 open_loop waits for all semaphore permits (`capacity.rs:348–351`); Driver::drive returns after all calls finish, then Capacity::step closes traffic. Observed::repetition credits all answered calls. Its later settle only drains captures/outbox, not outstanding calls. Those work counts feed Repetition::derive W/T, W/C, C/W and (W/T)/P. Additionally, Window::close takes seconds before its closing HTTP scrape but reads server/driver CPU afterward, charging CPU outside the stated T.

**Reachable failure/consequence:** A slow query/stream/verification admitted just before a ten-second deadline and finishing two seconds later changes that measurement to twelve seconds and is counted as measured work rather than drain work. Gateway overload can leave a larger pending cohort. Neither report exposes the required split; throughput/efficiency and drain interpretation are wrong. This is an explicit requirement, not speculative timing hardening.

**Smallest correction:** Keep current drivers, resource owner and report workflow. Close admission and freeze CPU/completed work at the declared boundary independently of awaiting pending requests; retain the cohort through a bounded drain and record admitted/measured/drain counts/windows. Align Window CPU/wall endpoints without charging closing-scrape CPU to the prior window. No second timing engine is needed.

**Focused proof:** Exercise a controlled outstanding call across admission-stop through the actual owning conversion, including Gateway L2 and representative closed-loop behavior: measured W/T and W/C exclude the late completion, cohort latency includes it once, drain work counts it once and CPU stops at the same declared clock. Check all sibling callers. Existing V4 only constructs the desired snapshots manually.

## B-002 — INCORRECT: completion slices and unclassified writers cannot reconcile offered cohorts

**Authority:** task What Changes and Scenario 1 explicitly prohibit unrelated native work satisfying offered counts and require independent class/phase populations. Spec REQ-007 lines 242–262 requires closed warmup/repetition/phase metadata, separate incidental/probe classification and measured-cohort latency through final outcome.

**Locations:** `crates/shared/wyrd-client/src/transport/trace.rs:34–76`; Bifrost `query/competing.rs:328–360,423–488,529–567,576–645,668–698`; Verification `run.rs:716–762,777–804,838–881`; Gateway `capacity.rs:966–994,1030–1031`; shared `capacity.rs:529–610`.

**Source path:** The client adds only BENCHMARK_CASE, no phase. Mixed queries add a case; offered native/OTLP writes and queue-lane writers carry none. expected() deliberately counts successful client results only, then collect uses lower bounds (`false`). read_ingest selects `(none,none)` for all write boundaries, including unrelated native queue lanes. A missing offered observation can therefore be covered by another lane's excess. Case-labelled queries are separated from probes, but phase SLOs use snapshot slices ending at overload/recovery/window boundaries. A capacity query finishing during overload is removed from capacity. A slow recovery query finishing after end appears only in the final population's validation, not Windows.recovery latency. Verification Ended::overhead_p99 has the same completion-slice behavior, despite collecting the final population; Gateway phase populations also have no admission phase. Parser support for PHASES and manually labelled histogram tests do not classify actual requests.

**Reachable failure/consequence:** Boundary-straddling calls are ordinary in overload/recovery workloads. A phase can lose its slowest outcomes or absorb the prior phase's work, and native incidental work can hide missing offered timing/dilute ingest p99. This violates explicit task obligations. The execution evidence acknowledges native lower-bound mixing but cannot waive the requirement.

**Smallest correction:** Complete the existing bounded classification path at actual producers, including queued/native writer transport. Preserve offered class/repetition/admission phase and incidental classification through existing request context; use no request IDs or tenant labels. Reconcile each owned final cohort/outcome rather than accepting unrelated lower bounds; derive cohort latency from its final drained population, while retaining measurement-only completions for work. Reuse Population::only/expect and current bucket deltas. Update the series bound only as required by the actual declared closed matrix, not for arbitrary caller strings.

**Focused proof:** At producer/consumer seams interleave classes across capacity/overload/recovery with a late slow outcome; its own phase cohort must retain it after drain. Interleave offered native writes and incidental queue-lane writes, remove one offered timing while keeping an extra incidental timing, and require INVALID. Check terminal/scan counts for that same cohort. A Population-only fixture cannot prove queued producer context propagation.

## B-003 — MISSING: histogram distributions and reconciliation evidence are discarded

**Authority:** spec REQ-013 lines 565–588 requires full histogram distributions under target (summary references are optional), numeric population/expected/recorded metadata; AC-001 requires recomputation from retained interval histograms. REQ-007 requires matching scan distributions and population/CPU/drain windows. Task Scenario 5 requires v2 boundaries/count reconciliation.

**Locations:** shared `capacity.rs:519–527,583–610,716–729,741–819`; `experiment.rs:1247–1278,1541–1548,1835–1868`; Bifrost `query/isolated.rs:197–216,query/competing.rs:621–645`; sibling Gateway/Verification repetition conversions above.

**Source path:** TimingSnapshot contains full elapsed/excluded/residual/scan buckets in memory. Population::validate copies only invalid strings and a combined health string. percentiles reduces distributions to Reading bounds/sample count. scanned_per_query reduces scans to two means; L2 folds query classes together. Repetition serializes only metrics/resources/prose. Record Artifacts has a directory/profile note; neither Population nor any engine writes snapshots/intervals there. The old raw collector persistence was removed without a histogram artifact replacement. Successful expected counts are not stored numerically per boundary/class/outcome/phase; the health string only has combined totals. The server stops at finish, losing all full distributions, including failed/time-out populations.

**Reachable failure/consequence:** Every normal successful run loses the evidence. A maintainer can regenerate a summary but cannot recompute p95/p99/rank bounds or inspect failed/scan distributions from retained buckets. Optional artifact references do not make required distribution retention optional.

**Smallest correction:** Extend existing measurement/artifact ownership to save actual bounded snapshots or intervals under the existing target directory, keyed by engine/step/repetition/cohort and closed series dimensions. Keep all timing/scan outcomes, method/process/snapshot boundaries, and structured expected/admitted/measured/drain/completed counts in summaries sufficient for machine-readable reconciliation. Reuse existing JSON/artifact workflow; keep full buckets out of committed summaries and preserve v1/report-without-target behavior. Required evidence-write failure must report incomplete/INVALID evidence, not complete timing.

**Focused proof:** Persist a representative multi-outcome/scan workload; rebuild each selected distribution from the saved artifact and reproduce stored percentile bound/rank/count and matching scan distributions. Delete artifacts and verify summary report regeneration still works. Inject evidence-write failure and require explicit INVALID/incomplete evidence. No new dependency/report framework is required.

## Failure/recovery, reuse and excluded scope

Readout shares existing server lifecycle, not a new process. Read failures/process replacement invalidate populations; unknown labels, series overflow and invalid components are counted. Collection waits are bounded. Startup refuses public benchmark binds. Smoke rejects missing timing/CPU/P/work before prepare; shared runner keeps partial INVALID evidence and levels publication failure exits 2 with salvage. B-003 concerns the absent replacement artifact path, separately from correct summary persistence. No new authentication/storage/audit workflow replaces existing production owners.

The tooling BenchmarkRecorder installs a global recorder and exposes summaries, so it cannot supply full-bucket production interval snapshots alongside Prometheus; narrow runtime HDR ownership is justified. Pooled-tenant histograms are not a separate finding because approved snapshot dimensions exclude tenant identity. Do not broaden remediation to TASK-005 four-experiment/calibration/profile/cancellation work, historical migration, new public SDK APIs, per-request IDs, tenant labels or new collectors. No speculative hardening or optional style finding is proposed.

Skipped: runtime reruns/builds/measurements, owned by the orchestrator. Risk: green checks and smoke do not establish the actual runner cohort/window/artifact obligations above.
