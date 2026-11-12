# TASK-001 maintainer review

Result: **PASS** for the assigned maintainer scope. No material M-* findings.

Subject: base `8d25b13a1ededa23b9eb91973a2b489fe9224557`, candidate
`bcbe88527eccd7be4768cc39e1ba55b855300512`. HEAD still identifies that
candidate; the tracked tree is unchanged. This is the first cumulative review,
with no previous acceptance evidence carried forward.

Authority read: the supplied repository AGENTS instructions,
`.agents/skills/wyrd-task-review/SKILL.md`, `agent_docs/index.md`,
`agent_docs/rust.md`, `agent_docs/maintainer-style.md`,
`changes/spec-driven-development.md`, the review scope, approved spec revision
2, and original TASK-001 at `8387f3a05fa018012d049104ffd7865074b23534`.
Candidate task evidence was read separately as claimed verification.
There is no CodeGraph index. No narrower AGENTS file governs these changed
Rust directories; the SQL directory rules do not apply to this write set.

## Changed-surface coverage

| Surface and symbols inspected | Caller, test, and owner closure | Assessment |
|---|---|---|
| `wyrd-benchmark/src/lib.rs`: installation, classification, recording, `Registry`, series/scan state, snapshots, `Method`, `Buckets`, `Distribution`, intervals and reconstruction | Compared with the base `wyrd-telemetry::timing` owner and removed telemetry forwarding functions/tests. Read the moved recording tests and new shared-observation test. Traced consumers through `Population` and persisted `TimingEvidence`. | State mutation remains on `Registry`; distribution operations remain on their values. The move preserves an existing responsibility rather than introducing another harness. |
| `wyrd-benchmark/src/recorder.rs`: `Kind`, `Metric`, `Value`, registration/indexing, `Handle`, facade trait implementations, metric snapshots and interval selection | Read the entire module; compared the entire existing `wyrd-bench/src/recorder.rs` implementation, including registration, handles, reset, snapshots and tests. Traced new interval access through `BenchmarkInterval` and `Population`. | Standard metric storage joins the existing registry. The handle adapter implements the externally required `metrics` traits; it is not a speculative abstraction or single-implementation house trait. Snapshot and interval types represent different lifetimes, with conversion at the owner. |
| Server `MetricsBackend`, `WyrdTelemetryRuntime`, recorder installation, route composition and snapshot/heap helpers | Read production and benchmark startup tests, rendering/local-access tests, `app::run`, Forge worker composition, `WyrdServer::new_with_metrics`, bound-server transfer and serving installation. Enumerated callers of changed constructors and runtime installation. | The enum is the actual closed choice between backends and replaces the unconditional Prometheus handle. Existing process, server and supervision owners remain responsible for lifetimes. No replacement lifecycle framework was added. |
| Duplicate-observation branches in Gate, Scribe ingress/profile, Gateway `CallTelemetry::point`, Verification `ExecutionTelemetry::emit` | Read surrounding producer flow, relevant changed tests, direct execution block test and consumers of the precise boundaries. Also inspected Oracle scan finalization and HTTP, query and OTLP/tonic classification/timing import closure. | Selection stays at the producer that knows whether observations are equivalent. Counters and other distinct measurements remain separate. The two representations require consumer closure, which is visible rather than hidden behind a generic remapping layer. |
| `LocalServer::{metrics,timings}`, `Metrics::of_snapshot`, moved production `Histogram`/`deltas` | Read surrounding parsing, family/label selection and bucket methods. Traced snapshot access through capacity setup, polling, drain, reporting and calibration consumers. | Conversion retains the established `Metrics` consumer interface without parsing an exposition. Keeping the production histogram test utility at this existing process/testing owner does not introduce a second benchmark aggregation workflow. |
| `capacity::{Window,Usage,Population}`, declaration and removed `Selection`/`Readings`/`Telemetry` | Read open/measure/close and stop/drain code, population conversion and selection, changed invalid-evidence tests, and callers in all engine/level paths. | The obsolete scrape aggregation layer is deleted. `Window` owns resources; `Population` owns metric/timing interval evidence. New forwarding selection methods expose those values at existing consumer owners. |
| Verification `Bench::{closed,competing}`, `Observed`, `observe`, acknowledgement and component aggregation, means | Read complete changed methods/helpers, their surrounding module, direct and queued inputs, reporting calls and new diagnostic test. Traced `Ended`, backlog/loss helpers and producer names. | The new aggregation is required to preserve the former diagnostic view after direct timings replace duplicate production histograms. Existing `Bench`, `Population`, `Distribution` and report owners are reused. Helpers transform caller-owned values and do not retain a parallel service or cache. |
| Bifrost L1/L2 consumers: isolated/competing paths, setup/scanned/residence/queue polling and stop bookkeeping | Read changed call sites and surrounding workflows, expanded into unchanged `query/run.rs` metric consumers; read the write-phase regression and case/declaration closure. | Signature/import migration reaches the setup and polling consumers as well as reports. No new Bifrost driver or resource owner appears. |
| Gateway L1/L2 consumers, integrity/durable bookkeeping, settle/shutdown and backlog polling | Read changed windows/imports, their surrounding workflows and unchanged metric selection consumers; inspected capacity/overhead test closure and new real-server journey assertions. | Existing consumer owners are retained. Histogram count projection preserves the established accounting representation without a new production counter family. |
| Capacity consumer matrix and `pg_tests` snapshot reader; Gateway resilience test | Read complete new matrix and journey, affected production histogram imports, calibration reader and settling caller. | New tests use actual recorder interfaces and existing harnesses. Matrix assertions exercise shared consumer helpers; this specialist does not claim that it alone executes six live server workflows. |
| Experiment history/readings, manifests and benchmark docs | Inspected changed `Reading::quantile`, `TimingEvidence`, phase labels and historical/cohort tests; reviewed dependency closure and crate manifest. Read README measurement changes and candidate disposition/verification limits. | Serde defaults preserve the old record shape for newly added metric fields. No SDK declarations or generated contract surface changes require parity work. Documentation locates the sole recorder and distinguishes profiling/OS sources. |

## Existing-owner comparison

The nearby `wyrd-bench::recorder::BenchmarkRecorder` already provides the
standard `metrics::Recorder` approach, but its `snapshot` exports only histogram
summaries and its `reset_interval` mutates cumulative counters/histograms.
Neither operation supplies the immutable bucket evidence and process-identity
interval subtraction required by spec REQ-001/003. Importing that test-tooling
crate into production is explicitly forbidden by the approved spec and task.
Copying its entire lifecycle would also retain separate state from the existing
precise timing registry. The candidate instead follows its facade registration
approach while extending the moved `Registry` with metric values and interval
evidence. That is sufficient justification for this bounded new implementation;
an equally conventional atomics/per-series-lock implementation is not grounds
for DRIFT.

The changed backend enum removes the reason to keep an unconditional production
handle, and the existing server lifecycle carries it. The legacy benchmark
Prometheus selection/readings layer is removed instead of wrapped. Verification
component aggregation is a consumer-specific report transformation: it must
combine queued standard metrics and direct precise timing parts because the
producer deliberately records each duration once. A new generic aggregation
framework or another recorder would add machinery without removing that work.

## Findings and calibration

No material maintenance defect was established in the assigned scope. In
particular, module size, the enum's name, the split between `lib.rs` and
`recorder.rs`, and alternative choices of lock granularity do not establish a
maintenance regression or an unearned workflow replacement. No optional
refactor is requested.

This PASS covers owner placement, reuse, method/data shape, conversion and caller
closure, and test/documentation comprehensibility. It does not replace the
independent behavioral, invariant, standards, concurrency or resilience
verdicts. Runtime correctness and workload-bound coverage remain subject to
those reviews; the candidate's claimed green commands are not independent
execution evidence from this specialist.

No builds, tests, benchmarks, profilers or services were run. Only this report
was written; no source, task, spec or historical evidence was changed.
