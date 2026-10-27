# Benchmark experiments packet

Status: PLAN_READY
Authority: SPEC-benchmark-experiments, revision 5, status approved.

The maintainer rejected the prior specification, task and implementation.
Revision 5 and the single task were revised together at the maintainer's
request and approved by the maintainer; the task is ready for implementation.

One task, tasks/TASK-001-benchmark-experiments.md, proposes the complete migration
for Bifrost, Verification and Gateway. It has no task dependencies and maps
REQ-001 through REQ-016, INV-001 through INV-009, and AC-001 through AC-010.
Shared measurement, judgment, history, reporting, engine integration and
retirement remain together because the outcome is the complete replacement.

The benchmarks measure server throughput and server overhead. Every judged
latency runs from server handler entry after receipt to result ready, write
acknowledgment or response handoff, using raw per-request duration samples and
nearest-rank percentiles. Client round trip, token/stream receipt, visibility
polling and inter-chunk lag are diagnostic-only. Scheduled-time latency judgment
and its exclusive collection are removed; send lateness remains driver validity.
Internal overhead retains the declared Wyrd boundaries using
standard OpenTelemetry request tracing of the server's individual monotonic
timer observations before aggregation, with 100% recording and completeness
checks. Production metrics buckets cannot supply latency judgments; their
resolution cannot cause or repair a benchmark verdict. Exact component matching
and missing/dropped timing evidence are part of the task's TDD scenarios.

Report throughput both per provisioned vCPU ((W/T)/P, recorded logical CPUs
available to server processes) and per server CPU-second used (W/C). Writes use
rows and uncompressed MiB (2^20 bytes) in both forms. Queries use query counts
in both forms, with scanned rows/bytes per query alongside server query time.
Baseline/noise-aware regression uses W/C and C/W, not provisioned throughput;
historical decimal byte efficiencies are converted to MiB with source operands.
L2 heavy scans use p95 <2 s with ≥200 completed samples per repetition, unchanged
5% query mix and a declared offered query rate capable of sufficient samples.

Linux x86_64 supplies all required real L1/L2 evidence. Every experiment still
runs unchanged on macOS arm64 and Linux arm64; portability is required, real
runs there are optional. Three repetitions, quiet-host checks, resource
accounting, raw timing storage and tracing collection are material costs.
Production security, durability, audit, admission and cleanup guarantees stay
enabled. Benchmarks remain opt-in and outside CI and verification gates.

Historical primary-source provenance gaps retain their explicit exclusions;
no Verification or Gateway baseline is invented. Research retains the original
baseline mapping and adds measurement-method evidence. The implementation
reference distinguishes the rejected uncommitted candidate from reusable
owners; its prior test results are not acceptance evidence for revision 5.

Commands in the task are future obligations. No code, benchmark execution,
Cargo invocation, staging or commit is part of this packet revision. Approval
of spec revision 5 and its proposed task is the remaining handoff prerequisite.

The required bounded independent comprehension pass read the revision-5 spec,
research, plan and task together without author conversation or code. It reported
no material explanation gaps. The author consistency pass also removed a stale
heavy-scan p99 sufficiency note from the task's validity table and aligned real-run
proof with both throughput definitions. Actual collection completeness,
instrumentation cost and budget feasibility remain implementation verification
obligations, not results established by document review.

The uncommitted implementation needs revision of timing, declarations,
throughput/record/report fields and tests. Keep correct lifecycle, workload,
portable accounting, baseline arithmetic and retirement work; delete obsolete
client-latency judgments and heavy-scan p99 rather than discard the whole
candidate. Prior test results supply no revision-5 acceptance credit.

Revision history:
- Revision 5 — 2026-10-24: Applied maintainer server-metric decisions across the
  still-draft spec and proposed task; approval remains pending (PLAN_BLOCKED).
