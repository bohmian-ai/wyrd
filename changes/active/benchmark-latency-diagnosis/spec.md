---
id: SPEC-benchmark-latency-diagnosis
revision: 2
status: approved
---

# One benchmark recorder and a Gateway diagnosis

## Problem and proposed outcome

Wyrd's capacity experiments have two latency collection paths: precise HDR
histograms used by the main benchmark judgments, and production Prometheus
histograms still used by some diagnostic means and component reports. Gateway
phases were absent from the benchmark distributions. This makes attribution
incomplete and leaves readers comparing measurements with different precision
and populations.

Install the benchmark recorder as the sole metrics recorder in each benchmarked
server process across Bifrost, Verification and Gateway, at L1 and L2. It owns
latency distributions, diagnostic histograms, counters and gauges, and supplies
the benchmark snapshots consumed by the driver. No Prometheus recorder or
exporter is installed in benchmark mode. Give this owner a clear benchmark-specific
home outside `wyrd_telemetry` and activate it only for explicit benchmarking.
Ordinary deployments install the production Prometheus recorder instead and
retain their existing monitoring behavior.

Reuse the existing standard `metrics` instrumentation and HDR benchmark recording
where they fit. This is a choice between two process backends at startup, not a
Prometheus recorder with its timing observations filtered out. The benchmark
backend must not maintain a second independent timing registry for the same
observations. CPU and heap profilers remain separate diagnostic tools; neither
is a competing metrics recorder.

Then use the existing Gateway records, profiles and source to explain the c64
capture hot path and produce a concrete optimization proposal. Where the evidence
cannot establish a cause, name the uncertainty and the smallest measurement
that would distinguish the competing explanations. A slow background Scribe write
is not, by itself, a slow Gateway response.

This packet covers measurement consistency, diagnosis and an implementation
proposal. It does not implement Gateway latency optimizations. The desired c64
p50s are client completion below 1 ms and Gateway-added latency below 500 us;
these are investigation objectives, not new production SLOs or changes to the
existing benchmark p99 judgments.

## Required behavior

- **REQ-001 — Sole benchmark recorder.** The benchmark backend is the only
  process metrics recorder in benchmark mode. All benchmark timing distributions,
  diagnostic histograms, counters, gauges and resource-metric observations flow
  through that owner and its snapshots. Reuse existing standard `metrics`
  instrumentation and HDR distribution/interval machinery; do not install a
  Prometheus recorder/exporter, retain a filtered Prometheus recorder for health
  metrics, or run a parallel independent timing registry. One semantic latency
  observation is counted once even where existing instrumentation has both metric
  and explicit timing calls. Client and server measurements remain separately
  identified with process-local clocks. OS CPU and RSS sampling remains an
  external measurement source, not a second application metrics recorder.
- **REQ-002 — Exclusive startup selection.** Benchmark mode selects its recorder
  before metric observations are registered. Ordinary deployments install the
  production recorder and neither activate nor accumulate benchmark data. There
  is no live recorder switch or mixed-backend fallback. Explicit benchmark
  activation preserves the existing loopback-only access restriction. Production
  monitoring remains unchanged outside that mode. Names and documentation clearly
  distinguish benchmark recording from tracing and OTLP export.
- **REQ-003 — Trustworthy evidence.** Every distribution identifies its boundary,
  unit, precision, outcome and population. Windows reconcile counts and handle
  late completions, replaced processes, invalid labels, overflow and missing or
  negative components explicitly. Compute overhead within each request before
  aggregation. Never subtract percentiles, sum overlapping phase percentiles or
  average percentiles into a combined population. Benchmark classification remains
  bounded and conveys no authority.
- **REQ-004 — Complete consumer closure.** Migrate Bifrost, Verification and Gateway
  consumers together with their correctness bookkeeping to the benchmark-owned
  snapshots. Benchmark execution must not depend on Prometheus exposition or its
  parser for latency, counts, backlog, health or resource metrics. Preserve the
  actual meaning and population of counts that prove capture or acknowledgement,
  including histogram observation counts where appropriate. Preserve useful
  diagnostic coverage or document and justify its replacement; migration must not
  silently turn observations into zeros, erase diagnostic classes or weaken
  validity checks. Production metrics contract tests may continue using their
  production backend and parser.
- **REQ-005 — Gateway attribution.** Distinguish request admission, capture-slot
  reservation waiting, provider exchanges, answer handoff and handler resumption,
  capture projection/staging, outbox scheduling, and durable submission. Explain
  which work is on the response path, which shares its execution resources, and
  which occurs only after handoff. CPU samples establish sampled CPU work, not
  off-CPU waiting. Do not label wall time as CPU time.
- **REQ-006 — Client boundary.** Explain what the driver includes in client
  completion, including response reception and validation, and what the server
  excludes as provider exchange time. Identify local transport and scheduling
  effects that those exclusions can contain. Evaluate both p50 objectives
  separately for the existing 1 KiB metadata, 64 KiB metadata and 64 KiB payload
  cases at c64, using c1 and streaming evidence as relevant controls.
- **REQ-007 — Reuse evidence first.** Analyze the available baseline and candidate
  records before proposing any new experiment. Do not rerun the baseline. Keep
  profiled and unprofiled results distinct. Preserve raw evidence and record
  source identity, dirty diff, build identity, workload, settings and profiling
  mode. Symbolized CPU conclusions require the matching recorded binary or
  matching symbols and an identified sample interval; whole-run samples must not
  be presented as one step's profile.
- **REQ-008 — Concrete proposal.** Produce a ranked explanation separating measured
  facts, source-proven mechanisms, hypotheses and unresolved gaps. For each
  material gap, specify the existing tool or smallest additional observation,
  workload, distinguishing result and stopping condition. Propose corrections
  only for supported causes, at their existing owners, with the expected effect,
  correctness risks and verification. A gap requiring measurement is not an
  implemented performance fix.
- **REQ-009 — Recover the workspace.** Inventory the uncommitted recorder and
  instrumentation changes against this specification before further execution.
  Treat them as an unfinished candidate, not approved design or completed proof.
  Retain, rework or remove candidate changes according to the approved outcomes;
  preserve pre-existing work and record the disposition. Previously interrupted
  checks do not count as passes.

## Invariants and boundaries

- **INV-001:** authentication, permissions, tenant separation, credential
  scrubbing, audit behavior, governance, provider behavior, response contents,
  stable errors and evidence attribution do not change.
- **INV-002:** capture reservation remains before charge/dispatch, bounded by the
  existing deadline, with the existing retryable refusal. Reserved captures are
  never shed. Projection failures never change an already answered call.
- **INV-003:** measurement does not change cancellation, settlement, accepted-work
  ownership, retry, durable acknowledgement, or resource-admission semantics.
- **INV-004:** historical records remain immutable and readable. Changed
  measurement settings are declared; results from different instrumentation or
  workloads are not represented as a controlled performance improvement.

The benchmark recorder is a small shared instrumentation owner, not a test
harness dependency, tracing exporter or new durable service. Reuse the existing
monotonic timers, standard metrics instrumentation, HDR distributions, snapshot
access and experiment driver. Any required counters/gauges snapshot fields belong
to this same owner; no separate collection service is introduced.
Its exact internal module or crate placement remains an implementation choice
subject to repository dependency boundaries. `wyrd-benchmark` is the current
uncommitted candidate, not a mandated name.

Recording overhead, synchronization, cardinality and profile overhead are costs
to account for. New timing observations must not be assumed free or proposed as
latency fixes. No new scheduler, cache, allocator, transport, memory-accounting
framework, admission policy or public product workflow is selected here.

## Execution boundary

Creating and reviewing this packet performs source and existing-artifact research
only. It runs no benchmarks, starts no release builds and changes no implementation
code. Specification approval permits task planning, not automatic execution.

After task execution is explicitly requested, ordinary implementation checks may
verify the recording change. Any proposed new benchmark experiment must first be
presented as a concrete, bounded measurement with its evidence gap, cases,
profiling mode and interpretation, and receive explicit maintainer authorization.
A full benchmark suite is not a default prerequisite for diagnosis. Existing
baselines are reused throughout.

## Acceptance and proof

| Criterion | Obligations | Observable proof |
|---|---|---|
| **AC-001 — One recording workflow** | REQ-001–004; INV-001,003 | With benchmarking off, the production recorder remains installed and no benchmark observation accumulates. With it on, only the benchmark recorder is installed: counters increment, gauges update, and timing/diagnostic histograms retain their units and populations through benchmark-owned snapshots. Duplicate timing instrumentation does not double-count. Driver reconciliation, backlog and resource readings work without a Prometheus recorder, exporter or exposition parser. Cover all three engines and both benchmark levels through focused tests and consumer inspection; any live experiment requires the authorization above. |
| **AC-002 — Reliable intervals** | REQ-003,004,007; INV-004 | Focused tests preserve per-request residuals, phase classification, warmup separation, late-cohort counts, distribution bounds and invalid-evidence detection. Existing records remain readable. Missing evidence is explicit, and maximum declared workload coverage fits bounded recording capacity. |
| **AC-003 — Defensible c64 diagnosis** | REQ-005–007 | A report cites the existing records, applicable profile intervals and source mechanisms. It compares client and server p50s independently, distinguishes response latency from background work, and identifies where CPU profiles cannot explain waiting. Assertions of outbox saturation require outbox evidence; Scribe memory use alone is insufficient. |
| **AC-004 — Actionable next work** | REQ-008; INV-001–003 | The report ranks supported costs and proposed remedies, identifies the smallest falsifiable measurement for each unresolved material cause, and states what evidence would select or reject each remedy. It makes no unsupported root-cause or target-attainment claim and does not implement an optimization. |
| **AC-005 — Candidate disposition** | REQ-009; INV-001–004 | A compact change inventory identifies what is reused, reworked or removed from the unfinished candidate and which checks remain. The recording outcome has focused regression tests, formatting, lint and dependency-boundary evidence; no interrupted run is claimed as complete. |

## Existing owners and evidence

- [Benchmark workflow and measurement definitions](../../../benchmarks/README.md),
  the `bench:*` tasks in [mise.toml](../../../mise.toml), and
  `wyrd-testing::capacity::{Population, Window}` / `experiment::Record` own
  experiments, populations, resource windows, history and reports.
- `wyrd-bench::recorder::BenchmarkRecorder` already implements the standard
  `metrics::Recorder` interface for counters, gauges, peaks and HDR histograms.
  Its summary snapshots do not supply the full population/interval evidence
  required here. Reuse its instrumentation approach and the existing precise
  timing machinery in one narrow owner; do not import the test-tooling crate
  into production dependencies.
- The committed precise timing recorder was `wyrd_telemetry::timing`; the unfinished move is
  [wyrd-benchmark](../../../crates/shared/wyrd-benchmark/src/lib.rs).
  [Server metrics startup](../../../crates/wyrd/wyrd-server/src/app/metrics.rs)
  owns activation, monitoring installation and the loopback snapshot endpoint.
- [Gateway invocation](../../../crates/wyrd/wyrd-server/src/components/gateway/invocation.rs),
  its telemetry and capture modules, `GatewayEngine` and provider exchange timers,
  the shared runtime outbox and server Scribe sink are the existing attribution
  and execution owners. Extending their observations does not select a new
  capture execution policy.
- Baseline Gateway record:
  `benchmarks/history/gateway/overhead/20261108T234942Z-5508cf29f.json`.
  Candidate unprofiled record:
  `benchmarks/history/gateway/overhead/20261109T153318Z-b1cf95f2e.json`.
  Candidate profile record:
  `target/benchmarks/gateway/overhead/20261109T153854Z-b1cf95f2e.json`;
  raw profile and timing snapshots are under its `20261109T153854Z/` artifact
  directory. The baseline profile and candidate rerun named in the handoff are
  additional existing evidence. Local artifact availability and matching symbols
  must be checked before using derived profile conclusions.

## Task boundaries

Two outcomes are sufficient: (1) finish and verify the single benchmark recording
workflow, including the unfinished candidate's consumer closure; (2) deliver the
Gateway diagnosis and evidence-backed optimization/measurement proposal using
that measurement contract. Existing-artifact research can begin without new
runs. Any future optimization implementation needs its own evidence-supported,
approved scope. The implementation tasks are [sole benchmark recording](tasks/TASK-001-sole-benchmark-recorder.md) and [Gateway diagnosis and proposal](tasks/TASK-002-gateway-latency-diagnosis.md).

## Open material decisions

Revision 2 was explicitly approved by the maintainer on 2026-11-09. No new capture policy or
optimization mechanism is awaiting selection inside this packet: those are
outputs of the diagnosis, outside its implementation scope. Additional benchmark
execution remains separately controlled as described above.

## Authority and revision history

- [Wyrd workflow](../../spec-driven-development.md) and
  [bounded implementation execution](../../implementation-execution.md).
- [Principal capture contract](../../../architecture/wyrd-design.md#principal-model),
  [Bifrost durable ingest](../../../architecture/bifrost-design.md), and
  [operational indicators and overload](../../../architecture/operations/reliability-and-recovery.md).
- [Testing](../../../TESTING.md) and repository `AGENTS.md` apply.
- Revision 1, draft, 2026-11-09: captures the agreed single-recorder direction and
  restores diagnosis/proposal scope. It neither approves nor replaces the broader
  draft [Gateway latency and capture spec](../gateway-latency-capture/spec.md),
  and does not reopen or complete resource-scaling TASK-005.

- Revision 2, draft, 2026-11-09: makes the benchmark backend the sole process
  recorder for timings, counters, gauges and diagnostic histograms. Removes
  revision 1's proposed retained Prometheus counter/gauge backend and explicitly
  requires benchmark-owned snapshots, exclusive startup selection and protection
  against duplicate timing observations. No implementation change accompanies
  this revision.

- Revision 2 approved, 2026-11-09: explicit maintainer instruction to approve
  the specification and create the plan. Approval changes no requirement and
  does not authorize execution. For benchmark collection, this revision replaces
  older Gateway-capacity packet mechanics that read Prometheus exposition;
  production telemetry contracts remain unchanged.
