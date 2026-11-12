# TASK-001 first acceptance review scope

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/resource-scaling`.
- Base: `8d25b13a1ededa23b9eb91973a2b489fe9224557` (parent of first implementation commit).
- Candidate: `bcbe88527eccd7be4768cc39e1ba55b855300512`.
- Review all five commits, starting with `8387f3a05fa018012d049104ffd7865074b23534`.
- First-review diff: `git diff 8d25b13a1...bcbe88527`.
- Approved authority: `changes/active/benchmark-latency-diagnosis/spec.md`, revision 2.
- Original task: `changes/active/benchmark-latency-diagnosis/tasks/TASK-001-sole-benchmark-recorder.md`; contractual task text is present at the first implementation commit. Candidate appends implementation evidence. Read original obligations and available candidate evidence separately.
- No prior task-review evidence to carry forward. Prior planning comprehension passes establish readability, not implementation acceptance.
- Initial HEAD equals candidate. Tracked tree is clean. Untracked `changes/active/gateway-latency-capture/` is outside the subject and stays untouched.
- No `.codegraph/` directory: use normal repository navigation. No generated navigation script is needed.

## Review boundary and conduct

Comprehensive first acceptance audit of TASK-001, including instrumentation, startup, recorder state, all six consumer workflows, production regression boundaries, evidence/history and repository rules. TASK-002 diagnosis/optimization work is excluded except verifying no drift into its scope.

Read the immutable range directly from Git; no diff or source archive is stored here. Read-only source: only assigned review report files may be written. No production/task/spec edits, commits, benchmarks, release builds, synthetic host load, dependency installation or service startup. Discovery reviewers do not run Cargo commands. The final independent validator may request/run only focused in-process checks, sequentially, if a concrete uncertainty justifies them. Source/verification limits must be explicit.

A candidate/HEAD or tracked-source change blocks this review. Review reports under this new directory do not change the source subject.

## Starting navigation map

This is a starting map, not a coverage limit or a conclusion. Every reviewer expands its assigned paths through actual callers and source.

| Contract/workflow | Owners and changed surfaces | Consumers and available proofs |
|---|---|---|
| Standard metrics and precise timing under one owner | `crates/shared/wyrd-benchmark/src/{lib,recorder}.rs`; former `wyrd-telemetry/src/timing.rs`; `wyrd-bench/src/recorder.rs` is an existing standard-recorder alternative to compare | `BenchmarkRecorder`, `Registry`, `TimingSnapshot`, `IntervalMetric`, `Distribution`; recorder/interval tests |
| Backend startup and local snapshot access | `wyrd-server/src/app/{metrics,mod,server}.rs`, `config.rs`; `MetricsBackend::install`; existing process telemetry/test startup | `metrics_router`, `WyrdServer`, `WyrdTestServer`, `LocalServer`; startup/local-access tests and new real-server Gateway journey |
| Duplicate instrumentation and population semantics | Bifrost `gate/mod.rs`, `scribe/{ingress,profile}.rs`; server Gateway and Verification telemetry; HTTP/gRPC/tonic classification | Direct/queued execution, ordinary/deduplicated Scribe ack, Gateway handoffs; producer tests and snapshot conversion |
| Snapshot conversion and failure evidence | `wyrd-testing/src/{release_server,capacity,experiment}.rs`; `Metrics::of_snapshot`, `Population`, windows and reporting | All engine setup, polling, stop/drain, counter/histogram means, persisted records; consumer matrix and historical evidence tests |
| Six engine/level workflows | `src/bin/capacity/query/{mod,isolated,competing}.rs`, `verification/{mod,run}.rs`, `gateway/{mod,run,capacity}.rs`, `main.rs`, `tests/pg_tests.rs` | L1/L2 output, backlog, health, resource and integrity readings; scoped unit tests and `tests::all_engine_consumers_read_benchmark_snapshots_without_prometheus` |
| Production regression, resource/capture guarantees | Unchanged invocation/capture/shared outbox ownership and changed producer timing branches; `tests/gateway/resilience.rs` | New snapshot-only functional journey plus existing production metrics/capture journeys |
| Dependency, documentation and authority closure | Root/package manifests and lockfile; `wyrd-tonic` server feature; `benchmarks/README.md`; task evidence; `agent_docs` | `check:deps`, fmt, targeted Clippy claimed in task evidence; changed Rust items require rustdoc |

Candidate task evidence contains claimed commands, diagnoses and limits; those are claims to validate, not an intended verdict. Reviewers do not receive the implementation completion summary as authority.

## Specialist applicability and selection

| Affected contract/invariant | Owners, producers, consumers | Selection and source-backed reason |
|---|---|---|
| Complete task behavior, non-goals, supported use and negative paths | All mapped workflows; REQ-001–004,007,009 and AC-001,002,005 | `behavior-rev` required for first comprehensive discovery. Covers complete acceptance, including bounded declared coverage and evidence credibility. |
| Shared metric state, cumulative intervals, counters/gauges, refused observations and reconciliation | `BenchmarkRecorder`/`Registry` to `TimingSnapshot`/`IntervalMetric` to `Population` and engine consumers | `invariant-rev` selected: new shared metric storage and producer/conversion semantics create materially complex state conservation obligations. |
| Broad tier/dependency/rustdoc/test and documentation obligations | Shared benchmark/telemetry, Vala, server, tonic and test harness; manifests and changed Rust items | `repo-rev` selected: 47 changed files span multiple owners/tiers and a new shared crate. Covers exact repository rules and failure-diagnosis/test-change requirements. |
| Owner placement, reuse, aggregation/conversion and lifecycle abstraction | `wyrd-bench` standard recorder, moved HDR recorder, `MetricsBackend`, snapshot consumers | `maintainer-rev` selected: substantial consolidation and new snapshot/conversion ownership require whole-workflow reuse and maintenance review. |
| Startup failure, missing/refused snapshot evidence, stop/drain and process replacement | Backend installation, listener routing, local server snapshot reads, population invalidity and engine setup/drain | `system-rev` selected: backend/exposure replacement changes failure propagation in deployed benchmark processes and must preserve ordinary production availability. |
| Concurrent updates and consistent snapshots under process-global recorder lifetime | Metric handles/atomics/locks, registry access, enable/install order, interval reads | `domain-rev` for concurrency selected: correctness of concurrent producer updates, snapshot consistency and lock/lifetime interactions needs focused concurrency review. It is separate from end-to-end state conservation. |
| Authentication, RBAC and tenant authorization | Existing auth shell and private loopback listener; classification headers | No separate security/RBAC domain specialist: no permission/token/tenant-policy change in the range. Local exposure, bounded non-secret labels and no authority from classification remain mandatory behavior/standards checks. Escalate if actual tracing establishes a material sensitive-boundary change. |
| Durable persistence, SQL/RLS and schema migration | Existing capture/Scribe/outbox; history reader/reporting | No separate data-layer domain specialist: no durable-write, transaction, RLS, schema or migration change. ACK/capture counts and immutable history are covered by behavior/invariant/system reviewers; escalate if callers establish a changed persistence contract. |

All selected reviewers are fresh contexts and occupy one role each. Always obtain a separate fresh `ponytail-rev` after discovery (and any justified follow-up), even with an empty proposed ledger. It independently validates applicability, coverage and source/materiality/corrections. A new validator finding requires separate focused source validation.

## Assigned reports

- `behavior-rev`: `task-review-behavior.md`.
- `invariant-rev`: `task-review-invariants.md`.
- `repo-rev`: `standards-review.md`.
- `maintainer-rev`: `maintainer-review.md`.
- `system-rev`: `system-review.md`.
- `domain-rev` (concurrency): `domain-review-concurrency.md`.
- Later independent `ponytail-rev`: `findings-validation.md`.

The orchestrator groups proposals by violated invariant and records whether a material conflict or untraced common cause requires one focused follow-up. It does not validate its own findings. Final artifacts contain only independently confirmed/revised findings.
