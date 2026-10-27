# TASK-001 first acceptance audit

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments
Base: 0739bbf38af7cc05bbfefb09850490c55432ec2d
Candidate: aaa152dd4243d57752b73d3a4e9c140bccc7d171
Range: git diff <base>...<candidate>. First complete review; no prior acceptance credit.
Approved authority: changes/active/benchmark-experiments/spec.md revision 5.
Task: changes/active/benchmark-experiments/tasks/TASK-001-benchmark-experiments.md.

Source candidate remains immutable. Two pre-existing uncommitted repairs in wyrd-telemetry/src/lib.rs and wyrd-testing/src/capacity.rs are outside candidate. Read candidate files with git show when necessary; assess repairs separately, never attribute to candidate. Reports only in this directory. No reviewer Cargo commands: orchestrator owns sequential checks. Every command through mise. CodeGraph first (.codegraph exists). Root AGENTS applies; no changed directory has nested AGENTS.

## Starting navigation map (expand independently)
- experiment.rs: declaration, validity, runner lifecycle/budget, metrics, judgment, history publication and common reports; capacity main.rs/levels.rs and mise.toml call it.
- capacity.rs: Collector, Timing, Population, Window, Usage; engine run.rs and gateway/capacity.rs consume observations.
- wyrd-telemetry::record_timing -> OTLP exporter -> Collector decode -> Population -> repetition judgments. Server owners: query/routes.rs, grpc/otlp.rs, verification/telemetry.rs, gateway/engine.rs and components/gateway/telemetry.rs; Oracle exec supplies scan facts.
- release_server.rs/os.rs and profile.rs: child processes, portable CPU/memory, helper accounting, startup/shutdown, optional profiling.
- capacity/query/{mod,run,isolated,competing,workload,telemetry}.rs: Bifrost declarations, fixtures, windows, equivalent state, L1/L2 calibration. client_queue consumes competing workflow.
- capacity/verification/{mod,run}.rs; capacity/gateway/{mod,run,workload,capacity}.rs: engine declarations and repeated work.
- benchmarks/*/baselines.toml -> shared baseline selection; source mapping in research.md and baselines-source reports. benchmarks/README.md, TESTING.md and mise.toml expose behavior.
- Removed load/*, slo.rs, bifrost_forge_capacity/* and engine report/envelope owners: verify coverage retirement.
- Tests: experiment::tests, capacity::tests, capacity binary and engine-local tests. Actual L1/L2 proof required separately. No records remain from reported failed run.

## Review selection
behavior-rev: full acceptance, engine matrices, baseline provenance and actual proof.
invariant-rev: exact timing producer-to-consumer and resource-accounting invariants.
maintainer-rev: workflow ownership/reuse and substantial abstraction/layout changes.
repo-rev: mandatory repository rules across all changed surfaces.
system-rev: runner/server/helper lifecycle, cancellation, failure propagation and budget feasibility.
Fresh ponytail-rev validates proposals after discovery. No dedicated security/data reviewer: existing security/storage boundaries are used; production invariants remain behavior/system coverage.

Human report: L1 had no usable samples, parser rejected non-integer durations, Bifrost exceeded budget; raw timings deleted. Treat diagnosis as a claim to inspect. Human-directed fast release smoke and sequential runs remain distinct from independent findings.
