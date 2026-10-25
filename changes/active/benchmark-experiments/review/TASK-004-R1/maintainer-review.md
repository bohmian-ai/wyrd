# TASK-004 maintainer review

Result: **FAIL**

Subject: `0b500d677...a020500e8ea0c39311d508d6b5bfa85ebe90368c`, first implementation review. Candidate HEAD remained `a020500e8ea0c39311d508d6b5bfa85ebe90368c` at the final source check. Specification revision 8 and its approved hash in `scope.md` govern; stale draft wording is not a blocker. No prior TASK-004 acceptance is carried forward.

Scope: ownership, abstraction, existing alternatives, and the collection/accounting replacement. I did not read other discovery reports or an intended verdict. No source changes, builds, tests, or benchmarks were performed. The orchestrator reports focused V1–V7 checks and the full `wyrd-testing --lib --bins` PG-wrapped lane passing (91/91); `bench:smoke` subsequently passed (exit 0, 85.63 s, all 8 dispatches), as did fmt/dependency/diff checks. Central execution evidence is `verification.md`. Those execution results are not independently rerun here.

## Authority and navigation

Read the shared scope map, root contributor rules, `agent_docs/index.md`, `agent_docs/rust.md`, `agent_docs/maintainer-style.md`, original TASK-004 at base, revision-8 REQ-007/REQ-015 and applicable acceptance obligations, `changes/spec-driven-development.md`, the trace-context ownership section of `architecture/wyrd-design.md`, and Bifrost scan/measurement authority. No nested contributor rule applies to these paths according to the shared authority map.

Used CodeGraph before code searches, including existing `BenchmarkRecorder` and experiment timing callers, then source/diff inspection through mise. Expanded the starting map to the old collector artifact writer and current runner artifact options and record projection: replacing collection also replaces its evidence-retention workflow.

## Changed-surface coverage

| Surface | Symbols and callers inspected | Assessment |
|---|---|---|
| Timing registry | `enable`, `enabled`, `label_request`, `labels_of`, `record`, `record_scan`, `lock`, `Labels`, `Registry::{new,record,scan,snapshot}`, `Series`, `Scan`, label/range/series constants | One coherent registry owner; existing timers and wait unions remain engine-owned. A private registry is justified by unmet existing-recorder requirements below. |
| Snapshot/arithmetic | `Method`, `Buckets`, `TimingSnapshot::since`, all `Distribution` methods, `IntervalSeries::{between,problems}`, `IntervalScan::between`, `counted`, `interval_of` | Native HDR record/subtract/add reused; arithmetic shared across engines. Retention is missing at the consumer seam (MNT-004-01). |
| Runtime/readout | `lib.rs::record_timing`, `WyrdTelemetryRuntime`, `install_recorder`, `metrics_router`, `serve_metrics`; metrics config/env/validation; `BoundServer` and standalone Forge listener callers; `LocalServer::{serve,metrics,timings}` | Existing listener and launch owners extended; no collector service or second launcher. `timings` parallels the existing metrics read. |
| Classification | Client `with_benchmark_case`, `headers`, task-local/header; HTTP/tonic `request_span`; query `Case::label`, `mixed`, `probe`, `Sender::send`/`authorized` | Existing transport/context owners extended. Offered-work/classification reconciliation is not assumed correct from helper tests. No second client introduced. |
| Boundary producers | Gate `record_gate_request`, Oracle `QueryTelemetryGuard::finish`, public `QueryTiming`, OTLP `timed`, Gateway `CallTelemetry::point`, Verification `ExecutionTelemetry::emit` | Actual timing/completion owners retained; Oracle scan facts added at existing finalization, with no second scanner. |
| Shared accounting | `Window::{open,close}`, `Usage`, `ResourceWindow::{open,freeze,finish}`, `Resources`, timing declaration; all `Population` methods, `merged`, `Smoke` | Existing resources/diagnostics retained; shared collection and projection. Complete evidence retention was removed (MNT-004-01). |
| Bifrost consumers | Isolated measurement/smoke; competing measurement, `Windows`, `expected`, `mixed`, `probe`, `server_percentile`, `read_queries`, `read_ingest`; query setup/smoke; sender changes | Shared arithmetic/population used. Different real workload schedules do not justify inventing a common scheduler. Phase/cohort correctness belongs to behavior/invariant coverage too. |
| Gateway consumers | Overhead start/measurement/smoke and `Observed::repetition`; capacity start/step, `Observed::repetition`, `slo_readings`, reduced diagnostic `Distribution`, `Driver`; workload `call`/`exchange`/`Timed` | Removes collector and identity joins; upstream/server/tenant lifetimes stay engine-owned. Populations are projected and dropped, as in sibling engines. |
| Verification consumers | `Bench` start/direct/competing/smoke; `Observed`, `Ended::{between,overhead_p99}`, `observe`, timing reader and mean projection | Reuses resources/populations; snapshots replace span-time joins. No separate report/record owner. |
| Record/dispatch | `Reading::{bounded,quantile}`, schema constants/errors, legacy collector field, validity removal, `Record::{parse,render}`, driver-cell rendering; levels assembly/execute, main collector-dispatch removal | Existing experiment writer/reader/report extended to v2 and v1 readback. The artifact directory remains available but histogram inputs disappear before publication. |
| Tests/docs/dependency | Registry labels/series tests, concurrent histogram test; actual boundary tests including hosted blocks; metrics/config tests; capacity population tests; histogram/accounting/v1-v2 experiment tests; adapted engine tests; README; telemetry manifest/lockfile | Tests exercise owning seams. Replacement tests omit retained histogram artifacts. README now explicitly describes logs/profiles only. Only installed HDR added as a runtime dependency. |

## Existing-owner and delete/reuse audit

Inspected `wyrd-bench::BenchmarkRecorder` and its module, install/snapshot/reset/register methods, histogram writer, tests, manifest and CodeGraph callers. It cannot directly supply this workflow:

- It owns the global `metrics::Recorder`; production already installs Prometheus. Installing it would violate the preserved recorder boundary.
- Its snapshot retains count/p50/p95/p99/max summaries, not cumulative buckets; summaries cannot reconstruct intervals.
- It accepts floating-point samples, drops non-positive values and ignores HDR recording failures. Integer durations, zero counts and recording-failure accounting are required here.
- Generic metrics lack a consistent elapsed/excluded/residual/count snapshot, classification, process/clock metadata and associated scan facts.
- Production cannot import the tooling crate. Rehoming a generic recorder would add abstraction while leaving the missing requirements to implement.

The installed HDR dependency is reused directly for recording, reconstruction, subtraction, addition and rank lookup. That is the first sufficient reuse rung. No generic recorder extraction is required.

Existing lifecycle owners remain sufficient and are used: metrics listener/router, `LocalServer`, engine timers/wait unions, `ResourceWindow`, and `experiment::run`/`Record`. No material duplicate launcher, collector service, judgment engine, renderer or record publisher was found. The old collector should stay deleted. Restore its still-required retention outcome on the existing measurement/artifact path.

## Proposed material finding

### MNT-004-01 — Replacement population workflow discards the full timing evidence

**Classification:** MISSING / REGRESSION.

**Changed location:** `crates/wyrd/wyrd-testing/src/capacity.rs:534` (`Population::collect`), `:555` (`wait`), `:600` (`from_interval`). Consumers include `query/isolated.rs:196`, `gateway/run.rs:563`, `gateway/capacity.rs:994`, and `verification/run.rs:295`. Projection: `experiment.rs:1117` (`Reading::quantile`). Related documentation: `benchmarks/README.md:202`.

**Obligation:** REQ-007 requires “Keep the full interval distribution under target/ for percentile regeneration”; AC-001 requires recomputation from retained interval histograms. REQ-015 assigns shared measurement to capacity and history/artifact orchestration to the existing experiment owner. Maintainer style requires putting a complete workflow with its owner.

**Source evidence:** `wait` reads `LocalServer::timings`, differences snapshots, converts to `Population`, validates counts and returns an in-memory value. `from_interval` drops the interval process/window context and keeps only series/scans/invalid reasons. Engine callers compute readings and drop the population. `Reading::quantile` keeps scaled bounds and count, not buckets. There is no production `Distribution::buckets` caller, snapshot file writer, or histogram data in `Repetition`/`Record`/`Artifacts`; the latter retains directory/profile information only. README explicitly replaces `timings.jsonl` with logs/profiles. At base, `Collector::keep` (`capacity.rs:1302`) appended each window's observations under its artifact directory, and its caller (`:1292`) invalidated failed writes. Collector deletion removed this workflow without a replacement.

**Reachable consequence:** Complete any ordinary L1 measured repetition; isolated query is enough. A v2 result can publish while its only full HDR distribution was a temporary value. Server shutdown removes cumulative state too. The engineer cannot regenerate that repetition's percentile from target artifacts or inspect failed-outcome distributions. This happens in supported runs without an unusual input or crash.

**Maintenance cost:** Investigating a surprising bound now requires reproducing the workload because only the selected summary remains. Retention is absent from the shared population path, inviting separate engine-specific artifact writers and omissions of L2 phases. The correction belongs at the shared collection/artifact seam, where the previous workflow already owned retention.

**Smallest testable correction:** Retain each measured interval's full elapsed/excluded/residual and scan buckets, histogram method, process/window metadata and case/phase/outcome identity under the existing run artifact directory before reducing it to readings. Reuse `Population`/snapshot arithmetic and the existing artifact directory owner; retain final-cohort and judged-phase distributions rather than only an unrelated final cumulative snapshot. Artifact-write failure must invalidate evidence. Preserve the single experiment record/report workflow. This needs neither a collector process, second launcher, dependency, generic recorder nor history writer.

**Focused closure proof:** Exercise the shared measured collection path with known bucket counts; finalize/drop its in-memory population, read the actual target artifact and regenerate identical percentile bounds/counts. Check phase/outcome separation and an unwritable artifact destination. Existing inline infrastructure and narrow owning lanes suffice; no new benchmark harness.

**Verification limit:** Source-proven absence of retention, traced across all three engines; no independent runtime checks in this role.

## Decision

FAIL for MNT-004-01. No optional naming, style or line-count findings. No duplicate-workflow finding is retained merely because the registry is new: existing alternatives were compared against the complete outcome.

Skipped: builds, tests and measurements, as assigned; source remained unchanged. Risk: v2 results currently lack the retained interval evidence required to audit latency bounds.
