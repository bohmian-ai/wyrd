# Maintainer review — TASK-001

Result: **PASS for assigned ownership and maintainability scope**.

Immutable subject: base `0739bbf38af7cc05bbfefb09850490c55432ec2d` through candidate
`aaa152dd4243d57752b73d3a4e9c140bccc7d171`. HEAD was checked unchanged.
Approved authority: `changes/active/benchmark-experiments/spec.md` revision 5.
Original task: `changes/active/benchmark-experiments/tasks/TASK-001-benchmark-experiments.md`.
First review; no prior acceptance credit. The two pre-existing working-tree timing repairs are excluded.

## Scope and method

Fresh specialist review of workflow consolidation, retained-state ownership,
reuse, abstraction burden and retirement across the actual 62-file range.
Read root AGENTS, supplied task-review skill, spec-driven-development,
maintainer-style, approved spec and original task. Scope metadata reports no
nested AGENTS in touched directories. CodeGraph was consulted before source
searching. Expanded the common map through actual implementations, callers,
test bodies and deletion consumers. No Cargo commands or source mutations.

This result does not approve timing correctness, resource-accounting semantics,
engine acceptance, mandatory documentation coverage, process recovery or actual
qualification. The other selected specialists own those questions. A coherent
owner does not establish that it works or that the failed run is resolved.

## Coverage and reuse comparison

| Changed surface | Source/caller coverage | Existing alternative, missing behavior and assessment |
|---|---|---|
| Shared lifecycle | experiment.rs Declaration/seal/refusals, Repetition/derive, Workload at 1341, run at 1552, judge/summarize/criterion helpers; levels execute and all engine Workload implementations | Removed load harness/matrix and local engine reports did not provide the required sealed declaration, per-repetition common judgment, baselines and history together. Candidate consolidates orchestration in one runner. Workload has multiple real implementations, earning its trait. Free run receives caller-owned work and holds invocation-local state. |
| Contributor dispatch/parser | main.rs dispatch/level/plan; complete levels.rs; flags diff; all engine plan/declare entry points | Context reuses existing flags owner; no replacement parser stack. Plan carries declaration and workload; every engine reaches execute/run/publish/render. Setup contribution remains engine-specific rather than becoming a second shared lifecycle. |
| Metric/baseline judgment | experiment Reading, Metric/Derive, Criterion/Rule, Repetition, judge/summarize; Baselines load/parse/criterion/convert and engine declaration consumers | Existing serde/TOML reused. Old SloGate cannot express unavailable evidence, advisory comparisons, all-repetition integrity and specified records. No extra dependency or general-purpose evaluation framework introduced. |
| History/report | Complete Record publish/read/parse/render/step_cells/unit bodies at experiment.rs 2372 onward; levels execute/report callers; history/report tests | NamedTempFile::persist_noclobber reused instead of bespoke filesystem transactions. One Record owner handles publication and rendering for fresh/read records. Large field count alone does not establish a god object: these fields and common policies are required by the approved record contract. |
| Collector/accounting | capacity.rs Window/ResourceWindow, Timing/decode_timings, Intake, Collector start/env/collect/append, Population; engine consumers | Existing telemetry test capture is in-process and cannot collect an external release server's operator OTLP export. New small tonic service reuses existing tonic/OTLP types. Collector owns receive state, assignment and raw writes. ResourceWindow extends existing launcher readings rather than introducing another OS backend. Correctness/bounds remain invariant review questions. |
| Launcher/profiler | LocalServer start/serve/stop and OperatorRun; complete os.rs and profile.rs; experiment profiling calls | Existing launcher is extended rather than duplicated. Existing process reading owner supplies portable sysinfo. Profiler replaces perf processing with external samply, retaining child ownership and diagnostic-only notes. Actual failure/grace behavior remains system-review scope. |
| Bifrost orchestration | query plan/declaration/calibration/start/Residence; isolated closed/read_outcome/prepare and Workload; competing start/Workload, phase/read helpers; Sender/Otlp/Bench/ask; fixture/telemetry diffs; client_queue Lane/Emitted changes | Public client, OTLP, fixture and exact-answer owners retained. Isolated/Competing own different required fixtures/traffic while reusing Sender/Bench, launcher, Collector and runner. Removing wrappers loses required work. Client queue becomes a workload participant rather than another reporting pipeline. Similar setup does not prove duplicate lifecycle ownership. |
| Verification | mod declarations/Schedule; Bench/Measured/direct/competing, Workload, arrivals/observe/drain/component helpers and tests | WyrdState/fixture owners retained. Engine-specific arrival/queue phases remain local; shared runner owns repetition judgments, percentile representation, accounting and reporting. No replacement client/transport. |
| Gateway | declarations/Case/shutdown; Samples/Integrity/Durable/Drain/Observed/Overhead/drive; Phases/Schedule/open_loop/Driver/Observed/Capacity and Workload; workload diff | Existing HTTP/upstream/capture fixture owners retained. Phase facts are engine inputs to shared judgments. Generic open_loop has a real Driver caller and focused tests. No justified requirement to merge different engines' schedules absent an existing complete behavior-preserving owner. |
| Production instrumentation | record_timing and test diff; QueryTiming and stream callers; ExecutionTiming attempt/union and telemetry consumers; Oracle scan-stat extension | Existing telemetry crate emits the shared timing span. QueryTiming owns request/body lifetime state through methods/Drop. Provider wait remains ExecutionTiming-owned and plan stats remain Oracle-owned. No benchmark endpoint, alternate provider runtime or replacement query path. Encoding correctness is not inferred from layout. |
| Retirement | Deleted slo/thresholds, load tree, Forge binary tree, engine report/envelope files; manifests/lib exports; complete mise range; remaining consumers | Candidate search found no SloGate/SloThreshold, removed Forge binary, wyrd_testing::load, perf report/script or ResourceScope consumers. Superseded reports are retired. General wyrd-bench report/recorder remain for separate Criterion purposes. bifrost/telemetry retains integration capture/parsing, not another experiment judgment/history owner. |
| Config/docs/data | mise task dependency changes, README/TESTING diff, baseline loader/file ownership | Shared bench:build setup; opt-in tasks reach one binary. Baseline provenance values, complete coverage and command truth remain behavior/repository questions; shared loading alone does not prove them. |

## Findings

No material ownership, abstraction, layout or reuse defect established within
this scope. Proposed finding ledger: **empty**.

No DRIFT is proposed solely from line count, similar engine setup, typed
declarations or alternative conventional structures. Deleted local reports and
harnesses are not live fallback owners; retaining them would contradict the
approved consolidation. Replacing the common runner with the old histogram
reports would lose declared validity, repetition, baseline and history behavior.
This comparison does not waive separately reviewed behavioral failures.

## Verification limits

Static review only. Inspected test bodies and consumers; no smoke run, green
Cargo result, platform measurement or successful benchmark claimed. Orchestrator
owns execution evidence. Outside-candidate timing repairs confer no acceptance.

Overall assigned-scope result: **PASS**.
